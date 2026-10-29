use crate::context::{Context, fail};
use serde_json::Value;
use std::{collections::BTreeSet, fs, io, path::Path};

fn collect(root: &Path, current: &Path, suffix: &str, names: &mut BTreeSet<String>) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() { return Err(fail("Linked source inventory member.")); }
        if kind.is_dir() { collect(root, &entry.path(), suffix, names)?; }
        else if entry.path().extension().and_then(|s| s.to_str()) == Some(suffix) {
            names.insert(entry.path().strip_prefix(root).map_err(|_| fail("Escaping source inventory."))?.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

pub fn tooling_manifest(root: &Path, manifest: &Value, acceptance: bool) -> io::Result<Value> {
    if manifest["schema_version"] != 2 || manifest["feature"] != "003-rust-tooling"
        || manifest["original_revision"] != "a24dccd5f53cb4ccad57297b1cf8f749f42858eb"
        || manifest["original_file_count"] != 51 || manifest["rust_migration_count"] != 46
        || manifest["powershell_retained_count"] != 5 || manifest["current_execution_claimed"] != false {
        return Err(fail("Original tooling migration identity differs."));
    }
    let mut sources = BTreeSet::new();
    for (field, count, powershell) in [("entries", 46, false), ("retained_powershell", 5, true)] {
        let rows = manifest[field].as_array().filter(|rows| rows.len() == count)
            .ok_or_else(|| fail("Every original tooling file requires one disposition."))?;
        for row in rows {
            let source = row["source"].as_str().ok_or_else(|| fail("Original tooling source missing."))?;
            let extension = Path::new(source).extension().and_then(|value| value.to_str());
            if !(source.starts_with("scripts/") || source.starts_with("tests/tooling/"))
                || !sources.insert(source.to_owned())
                || (if powershell { extension != Some("ps1") } else { !matches!(extension, Some("py" | "mjs")) }) {
                return Err(fail("Original tooling source is invalid or duplicated."));
            }
            if row["source_sha256"] != crate::context::sha(&crate::ledger::read(root, source)?) {
                return Err(fail("Retained original tooling bytes differ."));
            }
            if powershell { continue; }
            if row["scenarios"].as_array().is_none_or(|items| items.is_empty()
                || items.iter().any(|item| item.as_str().is_none_or(|text| text.trim().is_empty()))) {
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
    }
    // A source map, file count or hand-set status cannot authorize retirement. The final hosted
    // execution/assertion binder is a separate unfinished gate, not a boolean in this document.
    if acceptance { return Err(fail("Current tooling qualification is not yet bound to exact hosted execution.")); }
    Ok(serde_json::json!({"mapped_python_js":46,"retained_powershell":5,"acceptance_verified":false}))
}

pub fn tooling_inventory(manifest: &Value, tracked: &str) -> io::Result<()> {
    let expected = ["entries", "retained_powershell"].iter()
        .flat_map(|field| manifest[*field].as_array().into_iter().flatten())
        .map(|row| row["source"].as_str().ok_or_else(|| fail("Tooling source identity missing.")))
        .collect::<io::Result<BTreeSet<_>>>()?;
    let actual = tracked.split('\0').filter(|path| {
        (path.starts_with("scripts/") || path.starts_with("tests/tooling/"))
            && Path::new(path).extension().and_then(|value| value.to_str())
                .is_some_and(|ext| matches!(ext, "py" | "mjs" | "ps1"))
    }).collect::<BTreeSet<_>>();
    if expected != actual { return Err(fail("Tracked tooling inventory differs from explicit original dispositions.")); }
    Ok(())
}

pub fn check(context: &Context, retired: bool) -> io::Result<()> {
    let manifest = crate::ledger::parse(&crate::ledger::read(&context.root, "docs/testing/tooling-migration.json")?)?;
    tooling_manifest(&context.root, &manifest, retired)?;
    let bytes = context.run(context.command("git").args(["ls-files", "-z"]), 10000, 8000, true)?;
    let names = std::str::from_utf8(&bytes).map_err(|_| fail("Tracked paths must be UTF-8."))?;
    tooling_inventory(&manifest, names)?;
    let mut declared = BTreeSet::new();
    let manifest = fs::read_to_string(context.root.join("Cargo.toml"))?;
    for line in manifest.lines() {
        if let Some(value) = line.trim().strip_prefix("path = \"").and_then(|s| s.strip_suffix('"'))
            && value.starts_with("tests/rust/") { declared.insert(value.to_owned()); }
    }
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(context.root.join("tests/rust"))? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            actual.insert(path.strip_prefix(&context.root).map_err(|_| fail("Escaping native suite."))?.to_string_lossy().replace('\\', "/"));
        }
    }
    if actual != declared { return Err(fail("Native suite discovery differs from Cargo manifest.")); }
    let metadata = crate::ci::suite_metadata(context)?;
    let mut workspace_suites = BTreeSet::new();
    for base in [context.root.join("tests/rust"), context.root.join("tooling/tests")] {
        for entry in fs::read_dir(base)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "rs") { workspace_suites.insert(path.canonicalize()?); }
        }
    }
    crate::ci::verify_suite_inventory(&metadata, &workspace_suites)?;
    let mut checked = 0;
    for name in names.split('\0').filter(|s| !s.is_empty()) {
        let extension = Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or("");
        if retired && matches!(extension, "py" | "mjs" | "js" | "ts" | "cjs") {
            return Err(fail("Maintained interpreted Python/JavaScript source remains."));
        }
        if matches!(extension, "rs" | "py" | "mjs" | "ps1" | "json" | "jsonl" | "md" | "yml" | "yaml" | "toml") {
            let text = fs::read_to_string(context.relative(name)?)?;
            if matches!(extension, "rs" | "py" | "mjs" | "ps1") && text.lines().count() > 800 {
                return Err(fail("Source exceeds 800 lines."));
            }
            checked += 1;
        }
    }
    let mut rust_files = BTreeSet::new();
    collect(&context.root, &context.root.join("tooling"), "rs", &mut rust_files)?;
    for name in rust_files {
        if fs::read_to_string(context.root.join(name))?.lines().count() > 800 { return Err(fail("Tooling source exceeds 800 lines.")); }
    }
    println!("{}", serde_json::json!({"component":"native-inventory","suites":actual.len(),"utf8_files":checked,"interpreted_retirement_required":retired}));
    Ok(())
}

pub fn coverage(context: &Context) -> io::Result<()> {
    let report: Value = serde_json::from_slice(&fs::read(context.relative(".ci-work/native/coverage.json")?)?)
        .map_err(|_| fail("Invalid coverage evidence."))?;
    let data = report["data"].as_array().filter(|v| !v.is_empty()).ok_or_else(|| fail("Empty coverage evidence."))?;
    let mut failed = false;
    for (metric, floor) in [("regions",85.0),("branches",80.0)] {
        let mut count = 0u64; let mut covered = 0u64;
        for item in data {
            count = count.checked_add(item["totals"][metric]["count"].as_u64().ok_or_else(|| fail("Missing coverage denominator."))?).ok_or_else(|| fail("Coverage count overflow."))?;
            covered = covered.checked_add(item["totals"][metric]["covered"].as_u64().ok_or_else(|| fail("Missing coverage numerator."))?).ok_or_else(|| fail("Coverage count overflow."))?;
        }
        if covered > count { return Err(fail("Invalid coverage counts.")); }
        let percent = if count == 0 { 0.0 } else { covered as f64 / count as f64 * 100.0 };
        println!("{}",serde_json::json!({"metric":metric,"count":count,"covered":covered,"percent":percent,"floor":floor}));
        failed |= count == 0 || percent < floor;
    }
    if failed { Err(fail("Native coverage gates failed.")) } else { Ok(()) }
}
