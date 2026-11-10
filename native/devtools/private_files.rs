//! Explicit private-file primitives shared by persistent custody and transient signing inputs.
#[cfg(unix)]
use std::fs::OpenOptions;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path},
};

#[cfg(windows)]
#[path = "private_files_windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::{
    PrivateSecurity, RegistryAccess, inspect_private, lock_parents, open_registry,
    restrict_registry,
};

pub struct Secret(pub Vec<u8>);
impl Drop for Secret {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: the allocation is exclusively owned and live until after these writes.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

pub fn verify_ancestors(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(io::Error::other("Explicit absolute private path required."));
    }
    let mut current = std::path::PathBuf::new();
    let count = path.components().count();
    let mut components = path.components().enumerate().peekable();
    while let Some((index, part)) = components.next() {
        current.push(part.as_os_str());
        // Bare verbatim disk prefixes pass is_absolute; inspect after joining their RootDir.
        if matches!(part, Component::Prefix(_))
            && matches!(components.peek(), Some((_, Component::RootDir)))
        {
            continue;
        }
        if !current.is_absolute() {
            continue;
        }
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound && index + 1 == count => break,
            Err(error) => return Err(error),
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(io::Error::other("Linked private path rejected."));
            }
        }
        if metadata.is_symlink() || (index + 1 < count && !metadata.is_dir()) {
            return Err(io::Error::other(
                "Private parent is not an unlinked directory.",
            ));
        }
    }
    Ok(())
}

pub fn create_directory(path: &Path) -> io::Result<()> {
    verify_ancestors(path)?;
    #[cfg(windows)]
    {
        windows::create_directory(path)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    }
}

pub fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    verify_ancestors(path)?;
    #[cfg(windows)]
    let _parents = windows::lock_parents(path)?;
    #[cfg(windows)]
    let mut file = windows::create_file(path)?;
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?
    };
    let result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .and_then(|_| verify_file(&file, bytes.len()));
    drop(file);
    if result.is_err() {
        fs::remove_file(path)?;
    }
    result
}

pub fn verify_file(file: &File, max_bytes: usize) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > max_bytes as u64 {
        return Err(io::Error::other(
            "Private file must be bounded and regular.",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(io::Error::other("Private owner-only file required."));
        }
    }
    #[cfg(windows)]
    windows::verify_file(file)?;
    Ok(())
}

pub fn read_private(path: &Path, max_bytes: usize) -> io::Result<Secret> {
    verify_ancestors(path)?;
    #[cfg(windows)]
    let file = windows::open_file(path, false)?;
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?
    };
    verify_file(&file, max_bytes)?;
    let mut bytes = Secret(Vec::new());
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes.0)?;
    if bytes.0.len() > max_bytes {
        return Err(io::Error::other("Private file exceeds its bound."));
    }
    Ok(bytes)
}
