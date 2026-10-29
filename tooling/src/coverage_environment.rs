use crate::context::fail;
use std::{collections::BTreeMap, io, path::{Component, Path}};

fn value(raw: &str) -> io::Result<String> {
    let mut result = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in raw.chars() {
        if escaped { result.push(ch); escaped = false; continue; }
        match (quote, ch) {
            (Some('\''), '\'') => quote = None,
            (Some('\''), _) => result.push(ch),
            (Some('"'), '"') => quote = None,
            (Some('"'), '\\') | (None, '\\') => escaped = true,
            (Some('"'), _) => result.push(ch),
            (None, '\'' | '"') => quote = Some(ch),
            (None, ch) if ch.is_whitespace() || "$`;|&<>()".contains(ch) => return Err(fail("Invalid coverage environment token.")),
            _ => result.push(ch),
        }
    }
    if quote.is_some() || escaped || result.contains(['\n', '\r', '\0']) {
        return Err(fail("Unterminated coverage environment token."));
    }
    Ok(result)
}
pub fn parse(root: &Path, bytes: &[u8]) -> io::Result<BTreeMap<String, String>> {
    if bytes.len() > 65536 { return Err(fail("Coverage environment exceeds bound.")); }
    let text = std::str::from_utf8(bytes).map_err(|_|fail("Coverage environment must be UTF-8."))?;
    let mut values = BTreeMap::new();
    for line in text.lines().filter(|line|!line.is_empty()) {
        let (key, raw) = line.split_once('=').ok_or_else(||fail("Invalid coverage environment assignment."))?;
        if !key.bytes().all(|b|b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || !(key.starts_with("CARGO_LLVM_COV") || ["RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC_WRAPPER", "LLVM_PROFILE_FILE", "CARGO_ENCODED_RUSTFLAGS"].contains(&key))
            || values.insert(key.to_owned(), value(raw)?).is_some() {
            return Err(fail("Unexpected or duplicate coverage environment field."));
        }
    }
    for key in ["CARGO_LLVM_COV_TARGET_DIR", "CARGO_LLVM_COV_BUILD_DIR", "LLVM_PROFILE_FILE"] {
        let path = Path::new(values.get(key).ok_or_else(||fail("Required coverage path missing."))?);
        if !path.is_absolute() || !path.starts_with(root)
            || path.components().any(|part|matches!(part, Component::ParentDir)) {
            return Err(fail("Instrumented paths must remain inside the workspace."));
        }
    }
    if let Some(wrapper) = values.get("RUSTC_WRAPPER") {
        let path = Path::new(wrapper);
        if !path.is_absolute() || !path.starts_with(root) || path.components().any(|part|matches!(part, Component::ParentDir)) {
            return Err(fail("Coverage wrapper escapes the workspace."));
        }
    }
    let target = values["CARGO_LLVM_COV_TARGET_DIR"].clone();
    values.insert("CARGO_TARGET_DIR".into(), target);
    Ok(values)
}
