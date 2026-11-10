use crate::context::{Context, fail};
use serde_json::Value;
use std::{collections::BTreeSet, fs, io, path::Path};

fn collect(
    root: &Path,
    current: &Path,
    suffix: &str,
    names: &mut BTreeSet<String>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(fail("Linked source inventory member."));
        }
        if kind.is_dir() {
            collect(root, &entry.path(), suffix, names)?;
        } else if entry.path().extension().and_then(|s| s.to_str()) == Some(suffix) {
            names.insert(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| fail("Escaping source inventory."))?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

pub const TOOLING_REVISION: &str = "a24dccd5f53cb4ccad57297b1cf8f749f42858eb";

/// Historical identities stay immutable even after the working copies are retired.
pub fn tooling_hashes(manifest: &Value) -> io::Result<std::collections::BTreeMap<String, String>> {
    if manifest["schema_version"] != 3
        || manifest["feature"] != "003-rust-tooling"
        || manifest["original_revision"] != TOOLING_REVISION
        || manifest["original_file_count"] != 51
        || manifest["rust_migration_count"] != 51
        || manifest["powershell_retained_count"] != 0
        || manifest.get("retained_powershell").is_some()
        || manifest["current_execution_claimed"] != false
    {
        return Err(fail("Original tooling migration identity differs."));
    }
    let rows = manifest["entries"].as_array().filter(|rows| rows.len() == 51)
        .ok_or_else(|| fail("Every original tooling file requires one disposition."))?;
    let mut sources = std::collections::BTreeMap::new();
    let mut powershell = 0;
    for row in rows {
        let source = row["source"].as_str().ok_or_else(|| fail("Original tooling source missing."))?;
        let extension = Path::new(source).extension().and_then(|value| value.to_str());
        let digest = row["source_sha256"].as_str().filter(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }).ok_or_else(|| fail("Original tooling source hash is invalid."))?;
        if source.len() > 512
            || source.bytes().any(|byte| byte == b'\\' || byte == b':' || byte.is_ascii_control())
            || source.split('/').any(|part| matches!(part, "" | "." | ".."))
            || !(source.starts_with("scripts/") || source.starts_with("tests/tooling/"))
            || sources.insert(source.to_owned(), digest.to_owned()).is_some()
            || !matches!(extension, Some("py" | "mjs" | "ps1"))
        {
            return Err(fail("Original tooling source is invalid or duplicated."));
        }
        powershell += usize::from(extension == Some("ps1"));
    }
    if powershell != 5 {
        return Err(fail("Historical tooling language inventory differs."));
    }
    Ok(sources)
}

pub fn tooling_manifest(root: &Path, manifest: &Value, acceptance: bool) -> io::Result<Value> {
    let hashes = tooling_hashes(manifest)?;
    let mut present = 0;
    for source in hashes.keys() {
        match fs::symlink_metadata(root.join(source)) {
            Ok(_) => present += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    if !matches!(present, 0 | 51) {
        return Err(fail("Tooling originals must be entirely retained or entirely retired."));
    }
    for row in manifest["entries"].as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        if present == 51 && hashes[source] != crate::context::sha(&crate::ledger::read(root, source)?) {
            return Err(fail("Retained original tooling bytes differ."));
        }
        if row["scenarios"].as_array().is_none_or(|items| {
            items.is_empty() || items.iter().any(|item| item.as_str().is_none_or(|text| text.trim().is_empty()))
        }) {
            return Err(fail("Tooling behavior scenarios are missing."));
        }
        for field in ["rust", "tests"] {
            let paths = row[field].as_array().filter(|items| !items.is_empty())
                .ok_or_else(|| fail("Tooling replacement or test seam missing."))?;
            for path in paths {
                let path = path.as_str().ok_or_else(|| fail("Invalid tooling replacement path."))?;
                if !path.ends_with(".rs") || !["native/", "tooling/", "tests/rust/"].iter().any(|prefix| path.starts_with(prefix)) {
                    return Err(fail("Tooling replacement must identify maintained Rust source."));
                }
                crate::ledger::read(root, path)?;
            }
        }
    }
    // The pure source map never authenticates qualification or authorizes a source transition.
    if acceptance {
        return Err(fail("Current tooling qualification is not yet bound to exact hosted execution."));
    }
    Ok(serde_json::json!({"mapped_files":51,"acceptance_verified":false}))
}

pub fn tooling_inventory(manifest: &Value, tracked: &str) -> io::Result<()> {
    let hashes = tooling_hashes(manifest)?;
    let expected = hashes.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let actual = tracked.split('\0').filter(|path| {
        Path::new(path).extension().and_then(|value| value.to_str())
            .is_some_and(|ext| matches!(ext, "py" | "mjs" | "js" | "cjs" | "ts" | "ps1" | "psm1" | "cmd" | "bat" | "sh" | "bash"))
    }).collect::<BTreeSet<_>>();
    if expected != actual && !actual.is_empty() {
        return Err(fail("Tracked tooling inventory differs from retained or exactly retired dispositions."));
    }
    Ok(())
}

pub fn validate_source(name: &str, bytes: &[u8]) -> io::Result<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| fail("Source must be strict UTF-8."))?;
    let extension = Path::new(name).extension().and_then(|value| value.to_str());
    if matches!(extension, Some("rs" | "py" | "mjs" | "ps1")) && text.lines().count() > 800 {
        return Err(fail("Source exceeds 800 lines."));
    }
    Ok(())
}

pub fn check(context: &Context, retired: bool) -> io::Result<()> {
    let manifest = crate::ledger::parse(&crate::ledger::read(
        &context.root,
        "docs/testing/tooling-migration.json",
    )?)?;
    tooling_manifest(&context.root, &manifest, false)?;
    let originals = crate::ledger_inputs::tooling_originals(context, &manifest)?;
    if originals.len() != 51 {
        return Err(fail(
            "Authenticated historical tooling inventory is incomplete.",
        ));
    }
    let bytes = context.run(
        context.command("git").args(["ls-files", "-z"]),
        10000,
        8000,
        true,
    )?;
    let names = std::str::from_utf8(&bytes).map_err(|_| fail("Tracked paths must be UTF-8."))?;
    tooling_inventory(&manifest, names)?;
    let mut declared = BTreeSet::new();
    let manifest = fs::read_to_string(context.root.join("Cargo.toml"))?;
    for line in manifest.lines() {
        if let Some(value) = line
            .trim()
            .strip_prefix("path = \"")
            .and_then(|s| s.strip_suffix('"'))
            && value.starts_with("tests/rust/")
        {
            declared.insert(value.to_owned());
        }
    }
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(context.root.join("tests/rust"))? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            actual.insert(
                path.strip_prefix(&context.root)
                    .map_err(|_| fail("Escaping native suite."))?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    if actual != declared {
        return Err(fail("Native suite discovery differs from Cargo manifest."));
    }
    let metadata = crate::ci::suite_metadata(context)?;
    let mut workspace_suites = BTreeSet::new();
    for base in [
        context.root.join("tests/rust"),
        context.root.join("tooling/tests"),
    ] {
        for entry in fs::read_dir(base)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                workspace_suites.insert(path.canonicalize()?);
            }
        }
    }
    crate::ci::verify_suite_inventory(&metadata, &workspace_suites)?;
    let mut checked = 0;
    for name in names.split('\0').filter(|s| !s.is_empty()) {
        let extension = Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if retired && matches!(extension, "py" | "mjs" | "js" | "ts" | "cjs" | "ps1" | "psm1" | "cmd" | "bat" | "sh" | "bash") {
            return Err(fail("Maintained first-party interpreted executable source remains."));
        }
        if matches!(
            extension,
            "rs" | "py" | "mjs" | "ps1" | "json" | "jsonl" | "md" | "yml" | "yaml" | "toml"
        ) {
            validate_source(name, &fs::read(context.relative(name)?)?)?;
            checked += 1;
        }
    }
    let mut rust_files = BTreeSet::new();
    collect(
        &context.root,
        &context.root.join("tooling"),
        "rs",
        &mut rust_files,
    )?;
    for name in rust_files {
        validate_source(&name, &fs::read(context.root.join(&name))?)?;
    }
    if retired {
        crate::retirement::check(context)?;
    }
    println!(
        "{}",
        serde_json::json!({"component":"native-inventory","suites":actual.len(),"utf8_files":checked,"interpreted_retirement_required":retired})
    );
    Ok(())
}

pub fn coverage_metrics(report: &Value) -> io::Result<Vec<Value>> {
    let mut metrics = Vec::new();
    let data = report["data"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| fail("Empty coverage evidence."))?;
    for (metric, floor) in [("regions", 85.0), ("branches", 80.0)] {
        let mut count = 0u64;
        let mut covered = 0u64;
        for item in data {
            count = count
                .checked_add(
                    item["totals"][metric]["count"]
                        .as_u64()
                        .ok_or_else(|| fail("Missing coverage denominator."))?,
                )
                .ok_or_else(|| fail("Coverage count overflow."))?;
            covered = covered
                .checked_add(
                    item["totals"][metric]["covered"]
                        .as_u64()
                        .ok_or_else(|| fail("Missing coverage numerator."))?,
                )
                .ok_or_else(|| fail("Coverage count overflow."))?;
        }
        if covered > count {
            return Err(fail("Invalid coverage counts."));
        }
        let percent = if count == 0 {
            0.0
        } else {
            covered as f64 / count as f64 * 100.0
        };
        metrics.push(
            serde_json::json!({"metric":metric,"count":count,"covered":covered,"percent":percent,"floor":floor}),
        );
    }
    Ok(metrics)
}

pub fn verify_coverage_metrics(metrics: &[Value]) -> io::Result<()> {
    if metrics.len() != 2 {
        return Err(fail("Native coverage metric inventory differs."));
    }
    for (metric, (name, floor)) in metrics.iter().zip([("regions", 85.0), ("branches", 80.0)]) {
        let valid = metric["count"]
            .as_u64()
            .filter(|count| *count > 0)
            .zip(metric["covered"].as_u64())
            .is_some_and(|(count, covered)| {
                let percent = covered as f64 / count as f64 * 100.0;
                covered <= count
                    && percent >= floor
                    && metric["percent"].as_f64() == Some(percent)
                    && metric["floor"].as_f64() == Some(floor)
            });
        if metric["metric"] != name || !valid {
            return Err(fail("Native coverage gates failed."));
        }
    }
    Ok(())
}

pub fn coverage(context: &Context) -> io::Result<()> {
    let report: Value = serde_json::from_slice(&fs::read(
        context.relative(".ci-work/native/coverage.json")?,
    )?)
    .map_err(|_| fail("Invalid coverage evidence."))?;
    let metrics = coverage_metrics(&report)?;
    for metric in &metrics {
        println!("{metric}");
    }
    verify_coverage_metrics(&metrics)
}
