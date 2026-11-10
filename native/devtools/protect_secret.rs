//! Explicit user-selected CurrentUser DPAPI protection. No plaintext command or environment input.
#[cfg(windows)]
use super::private_files;
use super::private_files::Secret;
use std::{
    io,
    path::{Path, PathBuf},
};
#[cfg(windows)]
#[path = "protect_secret_windows.rs"]
mod windows;

fn invalid() -> io::Error {
    io::Error::other(
        "Secret protection requires supported options and an absolute Windows destination.",
    )
}
#[cfg(windows)]
fn validate_destination(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(invalid());
    }
    let mut existing = path;
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => {
                private_files::verify_ancestors(existing)?;
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                existing = existing.parent().ok_or_else(invalid)?;
            }
            Err(error) => return Err(error),
        }
    }
    if existing != path && !existing.is_dir() {
        return Err(invalid());
    }
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(io::Error::other(
            "Secret file already exists; no overwrite is performed.",
        )),
    }
}

#[cfg(windows)]
fn create_missing_parents(path: &Path) -> io::Result<Vec<std::fs::File>> {
    let mut missing = Vec::new();
    let mut directory = path.parent().ok_or_else(invalid)?;
    loop {
        match std::fs::symlink_metadata(directory) {
            Ok(_) => {
                private_files::verify_ancestors(directory)?;
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(directory.to_path_buf());
                directory = directory.parent().ok_or_else(invalid)?;
            }
            Err(error) => return Err(error),
        }
    }
    let mut locks = private_files::lock_parents(&directory.join(".numera-private-anchor"))?;
    for directory in missing.into_iter().rev() {
        // CreateNew refuses a competing directory or reparse-point insertion; do not adopt it.
        private_files::create_directory(&directory)?;
        locks.extend(private_files::lock_parents(
            &directory.join(".numera-private-anchor"),
        )?);
    }
    private_files::verify_ancestors(path)?;
    Ok(locks)
}

/// Injectable input is for owned synthetic tests; the CLI always uses the attached hidden console.
pub fn protect_with(
    path: &Path,
    dry_run: bool,
    input: impl FnOnce() -> io::Result<Secret>,
) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        let _ = (path, dry_run, input);
        Err(io::Error::other(
            "CurrentUser DPAPI protection requires Windows.",
        ))
    }
    #[cfg(windows)]
    {
        validate_destination(path)?;
        if dry_run {
            return Ok(());
        }
        let secret = input()?;
        let text = std::str::from_utf8(&secret.0).map_err(|_| invalid())?;
        if text.is_empty() || text.len() > 65536 || text.contains(['\r', '\n', '\0']) {
            return Err(invalid());
        }
        let encrypted = windows::encrypt(text)?;
        validate_destination(path)?;
        let _parents = create_missing_parents(path)?;
        let root = std::env::current_dir()?.canonicalize()?;
        let logger = crate::logging::Logger::new(&root.join("logs"), "INFO");
        logger.log(
            "INFO",
            "protect-secret",
            "User-scoped secret protection requested; value and path are not logged.",
            crate::compat::value::json!({}),
        );
        // Existing parents are never chmod/ACL-modified, and no output exists before input succeeds.
        if let Err(error) = private_files::write_new(path, &encrypted) {
            logger.log("ERROR", "protect-secret", "Secret protection failed. Verify destination, current-user DPAPI and access restrictions before use.", crate::compat::value::json!({}));
            return Err(error);
        }
        logger.log(
            "INFO",
            "protect-secret",
            "DPAPI secret created for the current Windows user; value not logged.",
            crate::compat::value::json!({}),
        );
        Ok(())
    }
}

pub fn execute(args: &[String]) -> io::Result<()> {
    let mut path = None;
    let mut dry_run = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--path" | "-Path" if path.is_none() => {
                index += 1;
                path = Some(PathBuf::from(
                    args.get(index)
                        .filter(|value| !value.starts_with('-'))
                        .ok_or_else(invalid)?,
                ));
            }
            "--dry-run" | "--what-if" | "-WhatIf" if !dry_run => dry_run = true,
            _ => return Err(invalid()),
        }
        index += 1;
    }
    let path = path.ok_or_else(invalid)?;
    #[cfg(windows)]
    {
        protect_with(&path, dry_run, windows::read_hidden)
    }
    #[cfg(not(windows))]
    {
        protect_with(&path, dry_run, || Err(invalid()))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn missing_nested_parents_are_created_only_after_valid_input_and_native_encryption() {
        use super::*;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".ci-work/native-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let result = std::panic::catch_unwind(|| {
            let path = root.join("missing/nested/secret.txt");
            protect_with(&path, true, || panic!("dry-run requested input")).unwrap();
            assert!(!root.join("missing").exists());
            assert!(
                protect_with(&path, false, || Err(io::Error::other(
                    "synthetic input failure"
                )))
                .is_err()
            );
            assert!(!root.join("missing").exists());
            assert!(protect_with(&path, false, || Ok(Secret(vec![0xff]))).is_err());
            assert!(!root.join("missing").exists());
            protect_with(&path, false, || {
                Ok(Secret(
                    "synthetic missing-parent 日本語".as_bytes().to_vec(),
                ))
            })
            .unwrap();
            let encrypted = private_files::read_private(&path, 1048576).unwrap();
            assert!(!encrypted.0.is_empty() && encrypted.0.iter().all(u8::is_ascii_hexdigit));
            for directory in [root.join("missing"), root.join("missing/nested")] {
                let status = private_files::inspect_private(&directory, true).unwrap();
                assert!(status.owner_current_user && status.owner_only && status.protected);
            }
            let before = encrypted.0.clone();
            assert!(protect_with(&path, false, || panic!("overwrite requested input")).is_err());
            assert_eq!(std::fs::read(path).unwrap(), before);
        });
        std::fs::remove_dir_all(root).unwrap();
        result.unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn strict_secret_validation_never_accepts_coerced_or_oversized_plaintext() {
        use super::*;
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".ci-work/native-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&parent).unwrap();
        let result = std::panic::catch_unwind(|| {
            for bytes in [
                vec![0xff],
                vec![],
                b"secret\n".to_vec(),
                b"secret\0".to_vec(),
                vec![b'x'; 65537],
            ] {
                let file = parent.join("secret.txt");
                assert!(protect_with(&file, false, || Ok(Secret(bytes))).is_err());
                assert!(!file.exists());
            }
        });
        std::fs::remove_dir_all(parent).unwrap();
        result.unwrap();
    }
}
