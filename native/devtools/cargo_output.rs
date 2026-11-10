//! Reads only first-party Cargo release outputs; package/private artifact readers stay single-link.
use std::{
    fs::{self, File, Metadata},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

const MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
pub enum Binary {
    Server,
    Developer,
}
impl Binary {
    pub fn filename(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Self::Server, false) => "numera-gen-mcp",
            (Self::Server, true) => "numera-gen-mcp.exe",
            (Self::Developer, false) => "numera-dev",
            (Self::Developer, true) => "numera-dev.exe",
        }
    }
}

fn invalid() -> io::Error {
    io::Error::other("Cargo release output is untrusted, changed or outside its fixed boundary.")
}

pub fn path(root: &Path, binary: Binary) -> PathBuf {
    root.join("target/release").join(binary.filename())
}

/// Cargo legitimately hardlinks final Linux/Windows binaries to its intermediate outputs.
/// This exception is not available for arbitrary names, prepared archives or private inputs.
pub fn read(root: &Path, binary: Binary) -> io::Result<Vec<u8>> {
    Opened::new(root, binary)?.read(MAX_BYTES)
}

struct Opened {
    root: PathBuf,
    binary: Binary,
    file: File,
    metadata: Metadata,
    directories: Vec<File>,
}
impl Opened {
    fn new(root: &Path, binary: Binary) -> io::Result<Self> {
        if !root.is_absolute()
            || root
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
            || fs::canonicalize(root)? != root
        {
            return Err(invalid());
        }
        let output = path(root, binary);
        let (file, directories) = open(&output)?;
        let metadata = file.metadata()?;
        if !regular(&metadata) || metadata.len() > MAX_BYTES as u64 {
            return Err(invalid());
        }
        Ok(Self {
            root: root.to_owned(),
            binary,
            file,
            metadata,
            directories,
        })
    }

    fn read(mut self, limit: usize) -> io::Result<Vec<u8>> {
        if limit > MAX_BYTES
            || self.metadata.len() > limit as u64
            || !unchanged(&self.metadata, &self.file.metadata()?)?
        {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut self.file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != self.metadata.len()
            || !unchanged(&self.metadata, &self.file.metadata()?)?
        {
            return Err(invalid());
        }
        // Reopen from the absolute root, not a potentially renamed held parent directory.
        let current = Self::new(&self.root, self.binary)?;
        if identity(&self.file)? != identity(&current.file)?
            || !unchanged(&self.metadata, &current.metadata)?
            || self.directories.len() != current.directories.len()
        {
            return Err(invalid());
        }
        for (before, after) in self.directories.iter().zip(&current.directories) {
            if identity(before)? != identity(after)? {
                return Err(invalid());
            }
        }
        if !unchanged(&self.metadata, &self.file.metadata()?)? {
            return Err(invalid());
        }
        Ok(bytes)
    }
}

fn regular(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    metadata.is_file() && !metadata.is_symlink()
}

fn unchanged(before: &Metadata, after: &Metadata) -> io::Result<bool> {
    if !regular(after) || before.len() != after.len() || before.modified()? != after.modified()? {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec()
            && before.mode() == after.mode()
            && before.nlink() == after.nlink())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(before.creation_time() == after.creation_time()
            && before.file_attributes() == after.file_attributes())
    }
}

#[cfg(unix)]
fn identity(file: &File) -> io::Result<(u64, u128)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), u128::from(metadata.ino())))
}
#[cfg(windows)]
fn identity(file: &File) -> io::Result<(u64, u128)> {
    let (volume, id) = crate::files::windows_identity::identity(file)?;
    Ok((volume, u128::from_le_bytes(id)))
}

#[cfg(unix)]
fn open(path: &Path) -> io::Result<(File, Vec<File>)> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    let mut directories = vec![File::open("/")?];
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate().skip(1) {
        let Component::Normal(name) = component else {
            return Err(invalid());
        };
        let name = CString::new(name.as_bytes()).map_err(|_| invalid())?;
        let directory = index + 1 < components.len();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            };
        // SAFETY: retained parent is a live directory; name is one NUL-terminated component.
        let descriptor = unsafe {
            libc::openat(
                directories.last().ok_or_else(invalid)?.as_raw_fd(),
                name.as_ptr(),
                flags,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful openat transfers this new descriptor to exactly one File owner.
        let file = unsafe { File::from_raw_fd(descriptor) };
        if !directory {
            return Ok((file, directories));
        }
        directories.push(file);
    }
    Err(invalid())
}

#[cfg(windows)]
fn open(path: &Path) -> io::Result<(File, Vec<File>)> {
    use std::os::windows::fs::OpenOptionsExt;
    // Existing primitives inspect all ancestors and pin their directory handles against deletion.
    super::private_files::verify_ancestors(path)?;
    let directories = super::private_files::lock_parents(path)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    Ok((file, directories))
}

#[cfg(test)]
#[path = "cargo_output_tests.rs"]
mod tests;
