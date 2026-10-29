use flate2::{Compression, GzBuilder, bufread::GzDecoder};
use numera_gen_mcp::files::paths::safe_relative;
use std::{collections::{BTreeMap, HashSet}, io::{self, Cursor, Read, Write}};

pub const MAX_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_MEMBERS: usize = 4096;
fn invalid() -> io::Error { io::Error::other("Invalid or oversized distribution archive.") }

pub fn validate_name(name: &str) -> io::Result<()> {
    safe_relative(name).map_err(|_| invalid())?;
    if name.contains('\\') { return Err(invalid()); }
    let parts = name.split('/').collect::<Vec<_>>();
    let allowed = ["bin", "docs", "schemas", "examples", "licenses", "README.md", "LICENSE",
        "THIRD_PARTY_NOTICES.md", "SECURITY.md", ".env.example", "manifest.json", "manifest.sig"];
    let private = ["secrets.md", "config.local.json", "state", "outputs", "logs", "node_modules", "http-token", "http.json"];
    if !allowed.contains(&parts[0])
        || (!matches!(parts[0], "bin" | "docs" | "schemas" | "examples" | "licenses") && parts.len() != 1)
        || parts.iter().any(|part| private.contains(&part.to_lowercase().as_str())
            || (part.starts_with('.') && name != ".env.example")) {
        return Err(invalid());
    }
    Ok(())
}
fn prefix_ok(prefix: &str) -> io::Result<()> {
    if prefix.is_empty() || prefix.len() > 200 || !prefix.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.') {
        return Err(invalid());
    }
    Ok(())
}
fn check(entries: &BTreeMap<String, Vec<u8>>) -> io::Result<()> {
    if entries.is_empty() || entries.len() > MAX_MEMBERS { return Err(invalid()); }
    let mut total = 0usize;
    let mut names = HashSet::new();
    for (name, bytes) in entries {
        validate_name(name)?;
        total = total.checked_add(bytes.len()).filter(|n| *n <= MAX_BYTES).ok_or_else(invalid)?;
        if !names.insert(name.to_lowercase()) { return Err(invalid()); }
    }
    Ok(())
}

pub fn encode(prefix: &str, entries: &BTreeMap<String, Vec<u8>>, zip: bool) -> io::Result<Vec<u8>> {
    prefix_ok(prefix)?;
    check(entries)?;
    if zip {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(6))
                .last_modified_time(zip::DateTime::default())
                .unix_permissions(if name.starts_with("bin/") { 0o755 } else { 0o644 });
            writer.start_file(format!("{prefix}/{name}"), options).map_err(|_| invalid())?;
            writer.write_all(bytes)?;
        }
        Ok(writer.finish().map_err(|_| invalid())?.into_inner())
    } else {
        let encoder = GzBuilder::new().mtime(0).write(Vec::new(), Compression::new(6));
        let mut writer = tar::Builder::new(encoder);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_ustar();
            header.set_size(bytes.len() as u64);
            header.set_mode(if name.starts_with("bin/") { 0o755 } else { 0o644 });
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            writer.append_data(&mut header, format!("{prefix}/{name}"), bytes.as_slice())?;
        }
        writer.into_inner()?.finish()
    }
}

fn accept<R: Read>(entries: &mut BTreeMap<String, Vec<u8>>, total: &mut usize,
    prefix: &str, name: &str, size: u64, mut reader: R) -> io::Result<()> {
    let relative = name.strip_prefix(&format!("{prefix}/")).ok_or_else(invalid)?;
    validate_name(relative)?;
    if entries.len() >= MAX_MEMBERS || size > MAX_BYTES as u64
        || entries.keys().any(|key| key.to_lowercase() == relative.to_lowercase()) { return Err(invalid()); }
    *total = total.checked_add(size as usize).filter(|n| *n <= MAX_BYTES).ok_or_else(invalid)?;
    let mut bytes = Vec::new();
    (&mut reader).take(size + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size { return Err(invalid()); }
    entries.insert(relative.to_owned(), bytes);
    Ok(())
}

pub fn decode(bytes: &[u8], prefix: &str, zip: bool) -> io::Result<BTreeMap<String, Vec<u8>>> {
    prefix_ok(prefix)?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES { return Err(invalid()); }
    let mut entries = BTreeMap::new();
    let mut total = 0;
    if zip {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
        if archive.len() > MAX_MEMBERS { return Err(invalid()); }
        for index in 0..archive.len() {
            let file = archive.by_index(index).map_err(|_| invalid())?;
            let mode = file.unix_mode().unwrap_or(0) & 0o170000;
            if !file.is_file() || !matches!(mode, 0 | 0o100000) { return Err(invalid()); }
            let name = file.name().to_owned();
            let size = file.size();
            accept(&mut entries, &mut total, prefix, &name, size, file)?;
        }
    } else {
        let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bytes)));
        for file in archive.entries()? {
            let file = file?;
            if !file.header().entry_type().is_file() { return Err(invalid()); }
            let name = file.path()?.to_str().ok_or_else(invalid)?.to_owned();
            let size = file.size();
            accept(&mut entries, &mut total, prefix, &name, size, file)?;
        }
        // Read through the gzip trailer too; TAR's first zero block is not checksum validation.
        let mut decoder = archive.into_inner();
        let mut padding = Vec::new();
        (&mut decoder).take(4097).read_to_end(&mut padding)?;
        if padding.len() > 4096 || padding.iter().any(|byte| *byte != 0)
            || decoder.into_inner().position() != bytes.len() as u64 {
            return Err(invalid());
        }
    }
    check(&entries)?;
    Ok(entries)
}
