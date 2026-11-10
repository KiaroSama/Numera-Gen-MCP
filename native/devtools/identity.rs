use crate::logging::Logger;
use serde_json::Value;
use std::{io, path::{Path, PathBuf}};

#[cfg(windows)]
mod crypto;
#[cfg(windows)]
mod fixtures;

pub fn validate_library_image(bytes: &[u8], expected_machine: u16) -> io::Result<()> {
    let read16 = |offset: usize| bytes.get(offset..offset + 2).and_then(|value| value.try_into().ok()).map(u16::from_le_bytes).ok_or_else(failure);
    if bytes.len() < 64 || bytes.get(..2) != Some(b"MZ") { return Err(failure()); }
    let pe = u32::from_le_bytes(bytes[60..64].try_into().map_err(|_| failure())?) as usize;
    if pe > bytes.len().saturating_sub(26) || bytes.get(pe..pe + 4) != Some(b"PE\0\0")
        || read16(pe + 4)? != expected_machine || !matches!(expected_machine, 0x8664 | 0xaa64)
        || read16(pe + 22)? & 0x2000 == 0 || read16(pe + 24)? != 0x20b { return Err(failure()); }
    Ok(())
}
#[cfg(windows)]
mod custody;

pub struct Options {
    pub action: String,
    pub library: Option<PathBuf>,
    pub library_sha256: Option<String>,
    pub confirm_create: bool,
    pub dry_run: bool,
    pub write_fixtures: bool,
}
impl Options {
    pub fn parse(args: &[String]) -> io::Result<Self> {
        let action = args.first().filter(|value| matches!(value.as_str(), "inspect" | "create" | "check"))
            .ok_or_else(invalid)?;
        let mut options = Self { action: action.clone(), library: None, library_sha256: None,
            confirm_create: false, dry_run: false, write_fixtures: false };
        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--confirm-create" if !options.confirm_create => { options.confirm_create = true; index += 1; }
                "--dry-run" if !options.dry_run => { options.dry_run = true; index += 1; }
                "--write-fixtures" if !options.write_fixtures => { options.write_fixtures = true; index += 1; }
                "--libcrypto" if options.library.is_none() => {
                    let path = PathBuf::from(args.get(index + 1).ok_or_else(invalid)?);
                    if !path.is_absolute() { return Err(invalid()); }
                    options.library = Some(path);
                    index += 2;
                }
                "--libcrypto-sha256" if options.library_sha256.is_none() => {
                    let hash = args.get(index + 1).filter(|value| value.len() == 64 && value.bytes().all(|byte|
                        byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))).ok_or_else(invalid)?;
                    options.library_sha256 = Some(hash.clone());
                    index += 2;
                }
                _ => return Err(invalid()),
            }
        }
        if action == "inspect" {
            if options.library.is_some() || options.library_sha256.is_some() || options.confirm_create
                || options.dry_run || options.write_fixtures { return Err(invalid()); }
        } else if options.dry_run {
            if action != "create" || options.write_fixtures || options.confirm_create
                || options.library.is_some() || options.library_sha256.is_some() { return Err(invalid()); }
        } else if options.library.is_none() || options.library_sha256.is_none()
            || options.confirm_create != (action == "create")
            || (options.write_fixtures && action != "check") { return Err(invalid()); }
        Ok(options)
    }
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput,
        "Expected identity inspect, create --dry-run, or create/check --libcrypto ABSOLUTE_DLL --libcrypto-sha256 SHA256; create requires --confirm-create, check optionally accepts --write-fixtures.")
}
pub(super) fn failure() -> io::Error {
    io::Error::other("Identity operation failed; preserve existing custody and reconcile before retry. Private details withheld.")
}

/// Local custody only. No host registration, trust installation, remote operation or implicit rotation.
pub fn execute(root: &Path, args: &[String], logger: Option<&Logger>) -> io::Result<Value> {
    let options = Options::parse(args)?;
    if !root.is_absolute() || !root.is_dir()
        || (options.action != "inspect" && !options.dry_run && logger.is_none()) { return Err(invalid()); }
    crate::devtools::private_files::verify_ancestors(root)?;
    #[cfg(windows)]
    let result = custody::execute(root, &options);
    #[cfg(not(windows))]
    let result: io::Result<Value> = {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Identity custody requires the approved Windows host."))
    };
    if options.action != "inspect" && !options.dry_run && let Some(logger) = logger {
        logger.log(if result.is_ok() { "INFO" } else { "ERROR" }, "identity",
            if result.is_ok() { "Local identity operation completed; no trust or remote change." }
            else { "Local identity operation failed; preserve custody. Private details withheld." },
            crate::compat::value::json!({"success":result.is_ok(),"os_trust_changed":false,"remote_secret_changed":false}));
    }
    result.map_err(|_| failure())
}
