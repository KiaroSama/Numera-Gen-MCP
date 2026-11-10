use super::{Receipt, Setup, Snapshot, invalid, valid_nonce};
use crate::devtools::capture::Capture;
use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    os::unix::{fs::OpenOptionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};

struct Children(Vec<Child>);
impl Children {
    fn stop(&mut self, deadline: Instant) -> io::Result<()> {
        let mut failed = false;
        for child in &mut self.0 {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => failed |= child.kill().is_err(),
                Err(_) => failed = true,
            }
        }
        loop {
            let mut live = false;
            for child in &mut self.0 {
                match child.try_wait() {
                    Ok(status) => live |= status.is_none(),
                    Err(_) => {
                        live = true;
                        failed = true;
                    }
                }
            }
            if !live {
                return if failed { Err(invalid()) } else { Ok(()) };
            }
            if Instant::now() >= deadline {
                return Err(invalid());
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        // No independent cleanup budget. The outer owner still contains this whole group.
        for child in &mut self.0 {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.try_wait();
            }
        }
    }
}
fn capture(child: &mut Child) -> io::Result<Capture> {
    let mut result = Capture::new(
        child.stdout.take().ok_or_else(invalid)?,
        child.stderr.take().ok_or_else(invalid)?,
    )?;
    result.set_limit(8192);
    Ok(result)
}
fn write_receipt(path: &Path, receipt: &Receipt) -> io::Result<()> {
    receipt.validate(&receipt.nonce)?;
    let bytes = serde_json::to_vec(receipt).map_err(|_| invalid())?;
    if bytes.len() > super::RECEIPT_LIMIT {
        return Err(invalid());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(&bytes)?;
    file.flush()
}

struct Observer {
    child: usize,
    input: ChildStdin,
    capture: Capture,
}
impl Observer {
    fn prepare(helper: &mut Command, children: &mut Children) -> io::Result<Self> {
        helper
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        children.0.push(helper.spawn()?);
        let child = children.0.len() - 1;
        let input = children.0[child].stdin.take().ok_or_else(invalid)?;
        super::super::capture::nonblocking(&input)?;
        Ok(Self {
            child,
            input,
            capture: capture(&mut children.0[child])?,
        })
    }
    fn setup(&mut self, setup: Setup) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(&setup).map_err(|_| invalid())?;
        bytes.push(b'\n');
        if bytes.len() > 4096 {
            return Err(invalid());
        }
        self.input.write_all(&bytes)
    }
    fn ready(&mut self, children: &mut Children) -> io::Result<bool> {
        self.capture.poll()?;
        Ok(!self.capture.exceeded
            && self.capture.stderr_bytes.is_empty()
            && self.capture.stdout_bytes == b"R\n"
            && children.0[self.child].try_wait()?.is_none())
    }
    fn stop_expired(&self, children: &mut Children, elapsed: Duration) -> io::Result<bool> {
        if elapsed < Duration::from_millis(500) {
            return Ok(false);
        }
        if children.0[self.child].try_wait()?.is_none() {
            children.0[self.child].kill()?;
        }
        Ok(true)
    }
    fn result(&mut self, children: &mut Children) -> io::Result<Option<Snapshot>> {
        self.capture.poll()?;
        let Some(status) = children.0[self.child].try_wait()? else {
            return Ok(None);
        };
        self.capture.poll()?;
        if !status.success()
            || self.capture.exceeded
            || !self.capture.stderr_bytes.is_empty()
            || !self.capture.stdout_bytes.starts_with(b"R\n")
        {
            return Err(invalid());
        }
        serde_json::from_slice(&self.capture.stdout_bytes[2..])
            .map(Some)
            .map_err(|_| invalid())
    }
}
fn discover(command: &mut Command, children: &mut Children) -> io::Result<Option<[String; 2]>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    children.0.push(command.spawn()?);
    let index = children.0.len() - 1;
    let mut captured = capture(&mut children.0[index])?;
    let limit = Instant::now() + Duration::from_secs(5);
    loop {
        if children.0[0].try_wait()?.is_some() {
            return Ok(None);
        }
        captured.poll()?;
        if let Some(status) = children.0[index].try_wait()? {
            captured.poll()?;
            return Ok(if status.success() && !captured.exceeded {
                tool_names(&captured.stdout_bytes)
            } else {
                None
            });
        }
        if Instant::now() >= limit {
            children.0[index].kill()?;
            return Ok(None);
        }
        std::thread::park_timeout(Duration::from_millis(5));
    }
}

pub fn run_owner(
    report: &mut Command,
    helper: &mut Command,
    receipt: &Path,
    nonce: &str,
) -> io::Result<i32> {
    run(report, helper, None, receipt, nonce)
}
pub(super) fn run(
    report: &mut Command,
    helper: &mut Command,
    discovery: Option<&mut Command>,
    receipt_path: &Path,
    nonce: &str,
) -> io::Result<i32> {
    if !valid_nonce(nonce) {
        return Err(invalid());
    }
    let owner_pid = std::process::id();
    // SAFETY: query only; the unreaped outer Child anchors this group leader.
    let group = unsafe { libc::getpgrp() };
    if group <= 0 || group as u32 != owner_pid {
        return Err(invalid());
    }
    let started = Instant::now();
    let mut children = Children(Vec::new());
    let operation = (|| -> io::Result<i32> {
        // Diagnostic setup failure must never prevent the actual report from starting.
        report
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        children.0.push(report.spawn()?);
        let report_index = 0;
        let mut observer = Observer::prepare(helper, &mut children).ok();
        let tools = discovery.and_then(|command| discover(command, &mut children).ok().flatten());
        let mut receipt = Receipt {
            schema: 1,
            nonce: nonce.to_owned(),
            owner_pid,
            group,
            report_pid: children.0[report_index].id(),
            status: 1,
            signal_ms: None,
            sample_ms: None,
            snapshot: None,
        };
        if let Some(value) = &mut observer {
            let setup = Setup {
                owner_pid,
                group,
                report_pid: receipt.report_pid,
                tools,
            };
            if value.setup(setup).is_err() {
                observer = None;
            }
        }
        let mut control = io::stdin();
        let control_available = super::super::capture::nonblocking(&control).is_ok();
        let mut signal = None;
        let mut written = false;
        loop {
            if let Some(status) = children.0[report_index].try_wait()? {
                return Ok(status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)));
            }
            let mut trigger = [0u8; 1];
            if control_available {
                match control.read(&mut trigger) {
                    Ok(1) if trigger[0] == 1 && signal.is_none() => {
                        receipt.signal_ms = Some(started.elapsed().as_millis() as u64);
                        signal = Some(Instant::now());
                        let sent = observer.as_mut().is_some_and(|value| {
                            value.ready(&mut children).unwrap_or(false)
                                && value.input.write(&[1]).ok() == Some(1)
                        });
                        if !sent {
                            receipt.status = 2;
                            observer = None;
                        }
                    }
                    Err(error)
                        if !matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        observer = None;
                    }
                    _ => {}
                }
            }
            if let Some(signalled) = signal
                && !written
            {
                if let Some(value) = &mut observer {
                    match value.result(&mut children) {
                        Ok(Some(snapshot)) => {
                            receipt.snapshot = Some(snapshot);
                            receipt.status = 0;
                        }
                        Ok(None) => {}
                        Err(_) => {
                            receipt.status = 4;
                            observer = None;
                        }
                    }
                }
                if receipt.status == 1
                    && let Some(value) = &observer
                    && value.stop_expired(&mut children, signalled.elapsed())?
                {
                    receipt.status = 5;
                }
                if receipt.status != 1 {
                    receipt.sample_ms = Some(started.elapsed().as_millis() as u64);
                    if receipt.validate(nonce).is_err() {
                        receipt.snapshot = None;
                        receipt.status = 4;
                    }
                    let _ = write_receipt(receipt_path, &receipt);
                    written = true;
                }
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
    })();
    children.stop(Instant::now() + Duration::from_secs(2))?;
    operation
}
fn tool_names(bytes: &[u8]) -> Option<[String; 2]> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if text.is_empty() || text.contains(['\n', '\r', '\0']) {
        return None;
    }
    let lib = PathBuf::from(text);
    if !lib.is_absolute() || lib.file_name()? != "lib" {
        return None;
    }
    let bin = lib.parent()?.join("bin");
    Some([
        std::env::var_os("LLVM_COV")
            .map(PathBuf::from)
            .unwrap_or_else(|| bin.join("llvm-cov"))
            .to_str()?
            .to_owned(),
        std::env::var_os("LLVM_PROFDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| bin.join("llvm-profdata"))
            .to_str()?
            .to_owned(),
    ])
}

#[cfg(test)]
#[path = "linux_snapshot_owner_tests.rs"]
mod tests;
