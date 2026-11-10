use super::{Row, Setup, Snapshot, invalid};
use std::{
    collections::BTreeSet,
    ffi::CString,
    fs::{self, File},
    io::{self, BufRead, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq)]
struct Identity {
    pid: u32,
    ppid: u32,
    group: i32,
    start: u64,
}
struct Process {
    directory: File,
    identity: Identity,
}
struct Stat {
    identity: Identity,
    row: Row,
}
fn unsigned(bytes: &[u8]) -> io::Result<u64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(invalid());
    }
    std::str::from_utf8(bytes)
        .map_err(|_| invalid())?
        .parse()
        .map_err(|_| invalid())
}
fn stat(bytes: &[u8]) -> io::Result<Stat> {
    let first = bytes.iter().position(|b| *b == b'(').ok_or_else(invalid)?;
    let end = bytes
        .iter()
        .rposition(|b| *b == b')')
        .filter(|n| *n > first)
        .ok_or_else(invalid)?;
    let pid = unsigned(bytes[..first].trim_ascii())?
        .try_into()
        .map_err(|_| invalid())?;
    let fields = bytes[end + 1..]
        .split(u8::is_ascii_whitespace)
        .filter(|s| !s.is_empty())
        .take(22)
        .collect::<Vec<_>>();
    if fields.len() < 22 || fields[0].len() != 1 {
        return Err(invalid());
    }
    let state = match fields[0][0] {
        b'R' => 1,
        b'S' => 2,
        b'D' => 3,
        b'Z' => 4,
        b'T' => 5,
        b't' => 6,
        b'X' | b'x' => 7,
        b'I' => 8,
        b'P' => 9,
        _ => return Err(invalid()),
    };
    let ppid = unsigned(fields[1])?.try_into().map_err(|_| invalid())?;
    let group = unsigned(fields[2])?.try_into().map_err(|_| invalid())?;
    let start = unsigned(fields[19])?;
    let identity = Identity {
        pid,
        ppid,
        group,
        start,
    };
    Ok(Stat {
        identity,
        row: Row {
            pid,
            ppid,
            start_ticks: start,
            role: 0,
            state,
            user_ticks: unsigned(fields[11])?,
            system_ticks: unsigned(fields[12])?,
            rss_pages: unsigned(fields[21])?,
            major_faults: unsigned(fields[9])?,
            threads: unsigned(fields[17])?,
            io: None,
            wait: 0,
        },
    })
}
fn relative(directory: &File, name: &str, is_dir: bool) -> io::Result<File> {
    let name = CString::new(name).map_err(|_| invalid())?;
    // SAFETY: retained proc directory and fixed/generated ASCII relative names; owned returned FD.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | if is_dir { libc::O_DIRECTORY } else { 0 },
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful openat transferred this descriptor to one File owner.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn bounded(file: File, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid());
    }
    Ok(bytes)
}
impl Process {
    fn open(pid: u32) -> io::Result<Self> {
        let directory = File::open(format!("/proc/{pid}"))?;
        let identity = stat(&bounded(relative(&directory, "stat", false)?, 4096)?)?.identity;
        if identity.pid != pid {
            return Err(invalid());
        }
        Ok(Self {
            directory,
            identity,
        })
    }
    fn current(&self) -> io::Result<Stat> {
        let value = stat(&bounded(relative(&self.directory, "stat", false)?, 4096)?)?;
        if value.identity != self.identity {
            return Err(invalid());
        }
        Ok(value)
    }
    fn executable(&self) -> io::Result<(u64, u64)> {
        // SAFETY: proc/exe follows the kernel executable link, never a supplied path.
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                c"exe".as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful openat transferred this descriptor to one File owner.
        let file = unsafe { File::from_raw_fd(fd) };
        let metadata = file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    fn children(&self, truncated: &mut bool) -> io::Result<Vec<u32>> {
        let task = relative(&self.directory, "task", true)?;
        // This fd-relative directory names only the held process, not a global proc scan.
        let mut result = BTreeSet::new();
        for (count, entry) in
            fs::read_dir(format!("/proc/self/fd/{}", task.as_raw_fd()))?.enumerate()
        {
            if count >= 64 {
                *truncated = true;
                break;
            }
            let name = entry?.file_name();
            let Some(name) = name
                .to_str()
                .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
            else {
                continue;
            };
            let child_ids = bounded(relative(&task, &format!("{name}/children"), false)?, 4096)?;
            for child in child_ids
                .split(u8::is_ascii_whitespace)
                .filter(|s| !s.is_empty())
            {
                let pid = unsigned(child)?.try_into().map_err(|_| invalid())?;
                result.insert(pid);
                if result.len() >= 16 {
                    *truncated = true;
                    return Ok(result.into_iter().collect());
                }
            }
        }
        self.current()?;
        Ok(result.into_iter().collect())
    }
}
fn io_counts(bytes: &[u8]) -> io::Result<[u64; 3]> {
    let mut result = [None; 3];
    for line in bytes.split(|b| *b == b'\n') {
        let Some(index) = line.iter().position(|b| *b == b':') else {
            continue;
        };
        let slot = match &line[..index] {
            b"rchar" => 0,
            b"read_bytes" => 1,
            b"write_bytes" => 2,
            _ => continue,
        };
        if result[slot].is_some() {
            return Err(invalid());
        }
        result[slot] = Some(unsigned(line[index + 1..].trim_ascii())?);
    }
    Ok([
        result[0].ok_or_else(invalid)?,
        result[1].ok_or_else(invalid)?,
        result[2].ok_or_else(invalid)?,
    ])
}
fn wait_class(bytes: &[u8]) -> u8 {
    match bytes.trim_ascii() {
        b"0" | b"" => 0,
        b"do_wait" | b"kernel_wait4" | b"__do_wait" => 1,
        b"futex_wait_queue" | b"futex_wait_queue_me" | b"futex_wait" => 2,
        b"io_schedule" | b"folio_wait_bit_common" | b"wait_on_page_bit_common" => 3,
        _ => 4,
    }
}
struct Observed {
    process: Process,
    parent: usize,
    executable: Option<(u64, u64)>,
}
fn chain_current(nodes: &[Observed], mut index: usize, anchor: &Process) -> bool {
    for _ in 0..8 {
        if nodes[index].process.current().is_err() {
            return false;
        }
        if index == 0 {
            return anchor.current().is_ok();
        }
        index = nodes[index].parent;
    }
    false
}
fn collect(
    setup: &Setup,
    tools: &[Option<(u64, u64)>; 2],
    expected: Option<&[Observed]>,
) -> io::Result<(Vec<Observed>, Vec<Row>, bool, u32)> {
    let anchor = Process::open(setup.owner_pid)?;
    if anchor.identity.group != setup.group || anchor.identity.pid != setup.group as u32 {
        return Err(invalid());
    }
    let report = Process::open(setup.report_pid)?;
    if report.identity.ppid != setup.owner_pid
        || report.identity.group != setup.group
        || report.identity.start < anchor.identity.start
    {
        return Err(invalid());
    }
    let mut nodes = vec![Observed {
        executable: report.executable().ok(),
        process: report,
        parent: 0,
    }];
    let (mut truncated, mut errors) = (false, 0);
    let mut index = 0;
    while index < nodes.len() && nodes.len() < 8 {
        match nodes[index].process.children(&mut truncated) {
            Ok(children) => {
                for pid in children {
                    if nodes.iter().any(|node| node.process.identity.pid == pid) {
                        continue;
                    }
                    if let Ok(child) = Process::open(pid) {
                        if child.identity.ppid == nodes[index].process.identity.pid
                            && child.identity.group == setup.group
                            && child.identity.start >= nodes[index].process.identity.start
                            && nodes[index].process.current().is_ok()
                        {
                            let executable = child.executable().ok();
                            nodes.push(Observed {
                                process: child,
                                parent: index,
                                executable,
                            });
                            if nodes.len() == 8 {
                                truncated = true;
                                break;
                            }
                        } else {
                            errors += 1;
                        }
                    } else {
                        errors += 1;
                    }
                }
            }
            Err(_) => errors += 1,
        }
        index += 1;
    }
    let mut rows = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let Some(mut value) = node.process.current().ok() else {
            errors += 1;
            continue;
        };
        if !chain_current(&nodes, index, &anchor) {
            errors += 1;
            continue;
        }
        if let Some(expected) = expected
            && !expected.iter().any(|old| {
                old.process.identity == node.process.identity && old.executable == node.executable
            })
        {
            errors += 1;
            continue;
        }
        if let Some(identity) = node.executable {
            if node.process.executable().ok() != Some(identity) {
                errors += 1;
                continue;
            }
            value.row.role = if tools[0] == Some(identity) {
                1
            } else if tools[1] == Some(identity) {
                2
            } else {
                0
            };
        }
        value.row.io = relative(&node.process.directory, "io", false)
            .and_then(|f| bounded(f, 4096))
            .and_then(|b| io_counts(&b))
            .ok();
        value.row.wait = relative(&node.process.directory, "wchan", false)
            .and_then(|f| bounded(f, 256))
            .map(|b| wait_class(&b))
            .unwrap_or(0);
        if chain_current(&nodes, index, &anchor)
            && node.process.executable().ok() == node.executable
        {
            rows.push(value.row);
        } else {
            errors += 1;
        }
    }
    Ok((nodes, rows, truncated, errors))
}
#[cfg(test)]
#[path = "linux_snapshot_tests.rs"]
mod tests;

pub fn sample_from_stdio() -> io::Result<()> {
    let input = io::stdin();
    let mut input = input.lock();
    let mut bytes = Vec::new();
    {
        let mut limited = (&mut input).take(4097);
        limited.read_until(b'\n', &mut bytes)?;
    }
    if bytes.len() > 4096 {
        return Err(invalid());
    }
    let setup: Setup = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    // SAFETY: numeric process queries only; no signal or ownership mutation.
    if setup.owner_pid != unsafe { libc::getppid() } as u32
        || setup.group != unsafe { libc::getpgrp() }
        || setup.owner_pid != setup.group as u32
    {
        return Err(invalid());
    }
    let mut _held = Vec::new();
    let mut tools = [None, None];
    if let Some(names) = &setup.tools {
        for (index, name) in names.iter().enumerate() {
            if let Ok(file) = File::open(name)
                && let Ok(info) = file.metadata()
                && info.is_file()
            {
                tools[index] = Some((info.dev(), info.ino()));
                _held.push(file);
            }
        }
    }
    let mut output = io::stdout().lock();
    output.write_all(b"R\n")?;
    output.flush()?;
    let mut trigger = [0];
    input.read_exact(&mut trigger)?;
    if trigger != [1] {
        return Err(invalid());
    }
    let started = Instant::now();
    let (nodes, first, truncated, errors) = collect(&setup, &tools, None)?;
    std::thread::park_timeout(Duration::from_millis(100));
    let (_, second, truncated_second, errors_second) = collect(&setup, &tools, Some(&nodes))?;
    let snapshot = Snapshot {
        first,
        second,
        interval_ms: started.elapsed().as_millis() as u64,
        tools_verified: tools.iter().filter(|tool| tool.is_some()).count() as u8,
        // SAFETY: read-only numeric system properties; unsupported values stay unavailable.
        clock_ticks: u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) }).ok(),
        page_bytes: u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok(),
        truncated: truncated || truncated_second,
        errors: errors + errors_second,
    };
    let bytes = serde_json::to_vec(&snapshot).map_err(|_| invalid())?;
    if bytes.len() > 8190 {
        return Err(invalid());
    }
    output.write_all(&bytes)?;
    output.flush()
}
