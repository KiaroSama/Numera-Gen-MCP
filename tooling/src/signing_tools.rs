use crate::{context::{Context, fail, sha}, distribution::{self, read_regular}, signing::validate_authority};
use serde_json::json;
use std::{collections::{BTreeMap, HashSet}, ffi::OsStr, fs, io::{self, Cursor, Read, Write}, path::{Path, PathBuf}, process::Command};

pub const TOOL_URL: &str = "https://github.com/mtrojnar/osslsigncode/releases/download/2.14/osslsigncode-2.14-windows-x64-mingw.zip";
pub const TOOL_SHA256: &str = "9a1722aaf62a27852c4eb9c35749a0248065052d0ae0a93d4ed6bb49def027f2";
pub const TOOL_MEMBERS: &[&str] = &[
    "bin/osslsigncode.exe", "bin/libcrypto-3-x64.dll", "bin/libssl-3-x64.dll", "bin/zlib1.dll",
    "share/bash-completion/completions/osslsigncode.bash", "share/doc/osslsigncode/NEWS.md",
    "share/doc/osslsigncode/README.md",
];
const ARCHIVE_LIMIT: usize = 4 * 1024 * 1024;
const MEMBER_LIMIT: usize = 8 * 1024 * 1024;

pub fn openssl3(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else { return false; };
    let Some(tail) = text.strip_prefix("OpenSSL 3.") else { return false; };
    let Some((minor, patch)) = tail.split_once('.') else { return false; };
    if minor.is_empty() || !minor.bytes().all(|byte| byte.is_ascii_digit()) { return false; }
    let count = patch.bytes().take_while(u8::is_ascii_digit).count();
    count > 0 && patch[count..].chars().next().is_none_or(|next| !next.is_alphanumeric() && next != '_')
}

pub fn archive_entries(bytes: &[u8]) -> io::Result<BTreeMap<String, Vec<u8>>> {
    if bytes.is_empty() || bytes.len() > ARCHIVE_LIMIT { return Err(fail("Signing tool archive exceeds bound.")); }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| fail("Invalid signing tool ZIP."))?;
    if archive.len() != TOOL_MEMBERS.len() { return Err(fail("Unexpected signing tool archive inventory.")); }
    let mut entries = BTreeMap::new();
    let mut total = 0usize;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|_| fail("Invalid signing tool member."))?;
        let mode = file.unix_mode().unwrap_or(0) & 0o170000;
        let name = file.name().to_owned();
        if !file.is_file() || !matches!(mode, 0 | 0o100000) || !TOOL_MEMBERS.contains(&name.as_str())
            || entries.contains_key(&name) || file.size() > MEMBER_LIMIT as u64 {
            return Err(fail("Unexpected or nonregular signing tool member."));
        }
        total = total.checked_add(file.size() as usize)
            .filter(|size| *size <= MEMBER_LIMIT * TOOL_MEMBERS.len())
            .ok_or_else(|| fail("Signing tool aggregate exceeds bound."))?;
        let size = file.size();
        let mut data = Vec::new();
        (&mut file).take(size + 1).read_to_end(&mut data)?;
        if data.len() as u64 != size { return Err(fail("Truncated signing tool member.")); }
        entries.insert(name, data);
    }
    Ok(entries)
}

fn private_command(context: &Context, program: impl AsRef<OsStr>) -> Command {
    let mut command = context.command(program);
    command.env_clear();
    for key in ["PATH", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "HOME", "USERPROFILE", "TMP", "TEMP", "TMPDIR"] {
        if let Some(value) = std::env::var_os(key) { command.env(key, value); }
    }
    command
}

fn installed(program: &str) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) { &[".exe"] } else { &[""] };
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path).filter(|directory| directory.is_absolute()) {
        for suffix in suffixes {
            let candidate = directory.join(format!("{program}{suffix}"));
            if candidate.is_file() { return Some(candidate); }
        }
    }
    None
}

fn preinstalled_openssl(context: &Context) -> io::Result<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(target_os = "macos")] {
        let brew = installed("brew").ok_or_else(|| fail("Preinstalled Homebrew OpenSSL 3 is unavailable."))?;
        let prefix = context.capture(private_command(context, brew)
            .env("HOMEBREW_NO_AUTO_UPDATE", "1").env("HOMEBREW_NO_ANALYTICS", "1")
            .args(["--prefix", "openssl@3"]), 15000, 10000, 65536)?;
        let prefix = std::str::from_utf8(&prefix).map_err(|_| fail("Invalid OpenSSL prefix encoding."))?.trim();
        if prefix.contains(['\r', '\n']) { return Err(fail("Invalid OpenSSL prefix.")); }
        candidates.push(PathBuf::from(prefix).join("bin/openssl"));
    }
    if let Some(path) = installed("openssl") { candidates.push(path); }
    if let Some(path) = std::env::var_os("OPENSSL_ROOT_DIR") {
        let path = PathBuf::from(path);
        candidates.push(if path.is_dir() { path.join(if cfg!(windows) { "bin/openssl.exe" } else { "bin/openssl" }) } else { path });
    }
    let mut seen = HashSet::new();
    for candidate in candidates {
        if !candidate.is_absolute() || !candidate.is_file() { continue; }
        let candidate = candidate.canonicalize()?;
        if !seen.insert(candidate.clone()) { continue; }
        read_regular(&candidate, 256 * 1024 * 1024)?;
        let outcome = context.outcome(private_command(context, &candidate).arg("version"), 10000, 5000, 65536)?;
        if outcome.reason.is_some() || !outcome.cleanup_verified {
            return Err(fail("OpenSSL qualification did not complete within its owned bounds."));
        }
        if outcome.code == 0 && openssl3(&outcome.stdout) { return Ok(candidate); }
    }
    Err(fail("Compatible preinstalled OpenSSL 3 is required; no legacy or install fallback."))
}

fn publish_environment(context: &Context, values: &BTreeMap<&str, PathBuf>) -> io::Result<()> {
    let path = std::env::var_os("GITHUB_ENV").map(PathBuf::from)
        .ok_or_else(|| fail("Hosted environment command file missing."))?;
    if !path.is_absolute() { return Err(fail("Hosted environment command file must be absolute.")); }
    let info = fs::symlink_metadata(&path)?;
    if !info.is_file() || info.is_symlink() || info.len() > 1024 * 1024 {
        return Err(fail("Hosted environment command file is invalid."));
    }
    read_regular(&path, 1024 * 1024)?;
    let mut text = String::new();
    for (key, path) in values {
        let value = path.to_str().ok_or_else(|| fail("Signing tool path must be Unicode."))?;
        if value.contains(['\r', '\n', '\0']) { return Err(fail("Invalid signing tool path.")); }
        text.push_str(&format!("{key}={value}\n"));
    }
    let mut options = fs::OpenOptions::new();
    options.append(true);
    #[cfg(windows)] { use std::os::windows::fs::OpenOptionsExt; options.custom_flags(0x00200000); }
    let mut file = options.open(&path)?;
    #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        if opened.dev() != info.dev() || opened.ino() != info.ino() || opened.nlink() != 1 {
            return Err(fail("Hosted environment command file changed during qualification."));
        }
    }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if file.metadata()?.file_attributes() & 0x400 != 0
            || numera_gen_mcp::files::windows_identity::links(&file).map_err(|_| fail("Hosted environment command file identity unavailable."))? != 1 {
            return Err(fail("Hosted environment command file is linked."));
        }
    }
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    context.logger.log("INFO", "signing-tools", "Qualified signing tools published to the hosted environment.", numera_gen_mcp::compat::value::json!({"tools":values.len()}));
    Ok(())
}

pub async fn tools(context: &Context, target: &str) -> io::Result<()> {
    context.hosted()?;
    distribution::target(target)?;
    if target != numera_gen_mcp::devtools::artifact::target() { return Err(fail("Signing tool qualification requires the native host target.")); }
    validate_authority(&json!({"GITHUB_ACTIONS":std::env::var("GITHUB_ACTIONS").ok(),
        "GITHUB_REPOSITORY":std::env::var("GITHUB_REPOSITORY").ok(),"GITHUB_REF":std::env::var("GITHUB_REF").ok(),
        "GITHUB_EVENT_NAME":std::env::var("GITHUB_EVENT_NAME").ok(),"GITHUB_JOB":std::env::var("GITHUB_JOB").ok(),
        "NUMERA_CI_OPERATION":std::env::var("NUMERA_CI_OPERATION").ok()}))?;
    let openssl = preinstalled_openssl(context)?;
    let mut values = BTreeMap::from([("NUMERA_OPENSSL", openssl)]);
    if target.contains("apple") {
        #[cfg(target_os = "macos")]
        crate::signing_macos::qualify()?;
        #[cfg(not(target_os = "macos"))]
        return Err(fail("Native memory-only macOS signing capability unavailable."));
    }
    if target.contains("windows") {
        let bytes = crate::fetch::github_release(TOOL_URL, ARCHIVE_LIMIT).await?;
        if bytes.is_empty() || bytes.len() > ARCHIVE_LIMIT || sha(&bytes) != TOOL_SHA256 { return Err(fail("Official signing tool digest differs.")); }
        let entries = archive_entries(&bytes)?;
        let directory = context.relative(".ci-work/signing-tools/osslsigncode-2.14")?;
        fs::create_dir_all(directory.parent().ok_or_else(|| fail("Signing tool parent missing."))?)?;
        crate::private_files::create_directory(&directory)?;
        let result = (|| {
            for (name, bytes) in &entries {
                let path = directory.join(name);
                fs::create_dir_all(path.parent().ok_or_else(|| fail("Signing tool member parent missing."))?)?;
                let mut options = fs::OpenOptions::new();
                options.create_new(true).write(true);
                #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
                let mut file = options.open(path)?;
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            let signer = directory.join("bin/osslsigncode.exe");
            let outcome = context.outcome(private_command(context, &signer).arg("--version"), 10000, 5000, 65536)?;
            let mut output = outcome.stdout;
            output.extend_from_slice(&outcome.stderr);
            if outcome.code != 0 || outcome.reason.is_some() || !outcome.cleanup_verified
                || !std::str::from_utf8(&output).is_ok_and(|text| text.starts_with("osslsigncode 2.14,")) {
                return Err(fail("Signing tool version differs from the authenticated release."));
            }
            values.insert("NUMERA_OSSLSIGNCODE", signer);
            publish_environment(context, &values)
        })();
        if result.is_err() { fs::remove_dir_all(&directory)?; }
        result?;
    } else {
        publish_environment(context, &values)?;
    }
    println!("{}", json!({"component":"native-signing-tools","tools_qualified":true,"target":target}));
    Ok(())
}
