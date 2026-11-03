use crate::context::{Context, fail};
use std::{
    collections::BTreeMap,
    io::{self, Cursor, Write},
    path::Path,
};

const LIMIT: usize = 16 * 1024 * 1024;

pub fn archive(root: &Path, inventory: &[u8]) -> io::Result<Vec<u8>> {
    let inventory =
        std::str::from_utf8(inventory).map_err(|_| fail("Formatting inventory must be UTF-8."))?;
    let inventory = inventory
        .strip_suffix('\0')
        .ok_or_else(|| fail("Formatting inventory must be NUL-terminated."))?;
    let mut entries = BTreeMap::new();
    let mut total = 0usize;
    for name in inventory.split('\0') {
        if name.is_empty() {
            return Err(fail("Empty formatting inventory path."));
        }
        if !name.ends_with(".rs")
            || !["native/", "tests/rust/", "tooling/"]
                .iter()
                .any(|base| name.starts_with(base))
        {
            continue;
        }
        if name.split('/').any(|part| part.starts_with('.'))
            || entries.len() >= 4096
            || entries
                .keys()
                .any(|key: &String| key.eq_ignore_ascii_case(name))
        {
            return Err(fail(
                "Private, duplicate or oversized formatting inventory.",
            ));
        }
        let bytes = crate::ledger::read(root, name)?;
        std::str::from_utf8(&bytes).map_err(|_| fail("Formatting source must be UTF-8."))?;
        total = total
            .checked_add(bytes.len())
            .filter(|total| *total <= LIMIT)
            .ok_or_else(|| fail("Formatting source exceeds bound."))?;
        entries.insert(name.to_owned(), bytes);
    }
    if entries.is_empty() {
        return Err(fail("Formatting source inventory is empty."));
    }
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .last_modified_time(zip::DateTime::default())
                    .unix_permissions(0o644),
            )
            .map_err(|_| fail("Cannot create formatting archive."))?;
        writer.write_all(&bytes)?;
    }
    let bytes = writer
        .finish()
        .map_err(|_| fail("Cannot finish formatting archive."))?
        .into_inner();
    if bytes.len() > LIMIT {
        return Err(fail("Formatting archive exceeds bound."));
    }
    Ok(bytes)
}

pub fn retain(context: &Context) -> io::Result<()> {
    context.hosted()?;
    // Diagnostic-only mutation follows all source-bound execution checks; it never clears failure.
    context.run(
        context.command("cargo").args(["fmt", "--all"]),
        30000,
        20000,
        false,
    )?;
    let inventory = context.capture(
        context.command("git").args(["ls-files", "-z"]),
        10000,
        8000,
        1024 * 1024,
    )?;
    let bytes = archive(&context.root, &inventory)?;
    context.write_new(".ci-work/native/formatting.zip", &bytes)
}
