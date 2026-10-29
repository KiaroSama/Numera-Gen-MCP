use crate::context::{Context, fail};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io, path::PathBuf, process::Command};

fn required(context: &Context, command: &mut Command, gate: &str, wall: u64, idle: u64, failures: &mut Vec<String>) {
    if context.run(command, wall, idle, false).is_err() { failures.push(gate.to_owned()); }
}
pub fn record_test_executables(context: &Context, output: &[u8]) -> io::Result<()> {
    let mut executables = serde_json::Map::new();
    for line in output.split(|b| *b == b'\n') {
        let Ok(row) = serde_json::from_slice::<Value>(line) else { continue; };
        if row["reason"] == "compiler-artifact" && row["profile"]["test"] == true
            && let (Some(name), Some(executable)) = (row["target"]["name"].as_str(), row["executable"].as_str()) {
            let path = PathBuf::from(executable).canonicalize()?;
            if !path.starts_with(&context.root) { return Err(fail("Compiled test escapes workspace.")); }
            if let Some(previous) = executables.insert(name.to_owned(), json!(path))
                && previous != json!(path) { return Err(fail("Ambiguous compiled suite identity.")); }
        }
    }
    if !["sdk_acceptance", "media_acceptance"].iter().all(|name|executables.contains_key(*name)) {
        return Err(fail("Compiled SDK or media suite was not discovered."));
    }
    context.write_new(".ci-work/native/test-executables.json", &serde_json::to_vec_pretty(&executables).map_err(|_|fail("Cannot record compiled suite inventory."))?)
}
pub fn test_executable(context: &Context, name: &str) -> io::Result<PathBuf> {
    let bytes = crate::distribution::read_regular(&context.relative(".ci-work/native/test-executables.json")?, 1024 * 1024)?;
    let rows: Value = serde_json::from_slice(&bytes).map_err(|_|fail("Invalid compiled suite inventory."))?;
    let path = PathBuf::from(rows[name].as_str().ok_or_else(||fail("Compiled suite missing."))?);
    let resolved = path.canonicalize()?;
    if !path.is_absolute() || !resolved.starts_with(&context.root) { return Err(fail("Compiled suite path escapes workspace.")); }
    Ok(resolved)
}
pub fn acceptance_prepare(context: &Context) -> io::Result<()> {
    context.hosted()?;
    crate::fixtures::materialize(context)?;
    let output = context.capture(context.command("cargo").args(["test", "--locked", "-p", "numera-gen-mcp", "--test", "sdk_acceptance", "--test", "media_acceptance", "--no-run", "--message-format=json-render-diagnostics"]), 900000, 180000, 16 * 1024 * 1024)?;
    let harnesses = crate::test_execution::compiled_harnesses(&context.root, &output)?;
    if harnesses.keys().map(String::as_str).collect::<BTreeSet<_>>() != BTreeSet::from([
        "tests/rust/sdk_acceptance.rs", "tests/rust/media_acceptance.rs"]) {
        return Err(fail("Acceptance preparation requires exactly the two selected SDK harnesses."));
    }
    record_test_executables(context, &output)
}
fn suites(context: &Context, command: &mut Command, wall: u64, phase: &str, selected: &[&str], failures: &mut Vec<String>) {
    if crate::test_execution::run(context, command, wall, phase, selected, failures).is_err() {
        failures.push("tests-or-execution-evidence".into());
    }
}
fn finish(failures: Vec<String>) -> io::Result<()> {
    if failures.is_empty() { Ok(()) } else {
        println!("{}", json!({"required_failed_gates":failures}));
        Err(fail("Required native gates failed; retained coverage or artifacts do not override execution failures."))
    }
}
pub fn stable(context: &Context) -> io::Result<()> {
    context.hosted()?; crate::inventory::check(context, false)?;
    let mut failures = Vec::new();
    required(context, context.command("cargo").args(["fmt", "--all", "--check"]), "format", 30000, 20000, &mut failures);
    required(context, context.command("cargo").args(["clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"]), "clippy", 900000, 180000, &mut failures);
    suites(context, context.command("cargo").args(["test", "--locked", "--workspace", "--all-targets", "--no-run", "--message-format=json-render-diagnostics"]), 900000, "built", &["numera-gen-mcp", "numera-task"], &mut failures);
    required(context, context.command("cargo").args(["build", "--locked", "--release", "-p", "numera-gen-mcp", "--message-format=json-render-diagnostics"]), "release", 900000, if cfg!(target_os="macos") {360000} else {180000}, &mut failures);
    let executable = context.relative(if cfg!(windows) {"target/release/numera-gen-mcp.exe"} else {"target/release/numera-gen-mcp"})?;
    if crate::acceptance::sdk(context, &executable, "release").is_err() { failures.push("release-sdk".into()); }
    if crate::evidence::verify(context).is_err() { failures.push("oracle-evidence".into()); }
    finish(failures)
}
pub fn coverage(context: &Context) -> io::Result<()> {
    context.hosted()?; crate::inventory::check(context, false)?;
    context.run(context.command("cargo").args(["install", "cargo-llvm-cov", "--version", "0.9.1", "--locked", "--root"]).arg(context.root.join(".ci-cache/cargo")), 600000, 180000, false)?;
    let raw = context.capture(context.command("cargo").args(["+nightly-2026-10-10", "llvm-cov", "show-env", "--branch", "--no-rustc-wrapper"])
        .env("CARGO_TARGET_DIR", context.root.join("target/llvm-cov-target")), 30000, 20000, 65536)?;
    let environment = crate::coverage_environment::parse(&context.root, &raw)?;
    let target = PathBuf::from(&environment["CARGO_LLVM_COV_TARGET_DIR"]);
    if target.exists() {
        let mut pending = vec![target.clone()]; let mut scanned = 0usize;
        while let Some(directory) = pending.pop() {
            for row in std::fs::read_dir(directory)? {
                let row = row?; scanned += 1;
                if scanned > 50000 || row.file_type()?.is_symlink() { return Err(fail("Coverage workspace is not a fresh bounded tree.")); }
                if row.file_type()?.is_dir() { pending.push(row.path()); }
                else if row.path().extension().is_some_and(|ext|ext == "profraw") { return Err(fail("Coverage requires a fresh workspace without prior profiles.")); }
            }
        }
    }
    let command = |args: &[&str]| {
        let mut command = context.command("cargo"); command.args(args).envs(&environment); command
    };
    let mut failures = Vec::new();
    // Only the production package is instrumented; tooling tests run in the stable matrix.
    // Real SDK suites are root integration targets and inherit LLVM_PROFILE_FILE into owned servers.
    suites(context, &mut command(&["+nightly-2026-10-10", "test", "--locked", "-p", "numera-gen-mcp", "--all-targets", "--no-run", "--message-format=json-render-diagnostics"]), 1200000, "coverage", &["numera-gen-mcp"], &mut failures);
    required(context, &mut command(&["+nightly-2026-10-10", "build", "--locked", "-p", "numera-gen-mcp"]), "instrumented-build", 600000, 180000, &mut failures);
    required(context, &mut command(&["+nightly-2026-10-10", "llvm-cov", "report", "--branch", "-p", "numera-gen-mcp", "--json", "--output-path", ".ci-work/native/coverage.json"]), "coverage-report", 60000, 30000, &mut failures);
    if crate::inventory::coverage(context).is_err() { failures.push("coverage-floors".into()); }
    if crate::evidence::verify(context).is_err() { failures.push("oracle-evidence".into()); }
    finish(failures)
}

pub fn verify_suite_inventory(metadata: &Value, actual: &BTreeSet<PathBuf>) -> io::Result<()> {
    let mut declared = BTreeSet::new();
    let members = metadata["workspace_members"].as_array().ok_or_else(||fail("Cargo workspace members missing."))?;
    let packages = metadata["packages"].as_array().ok_or_else(||fail("Cargo packages missing."))?;
    for package in packages.iter().filter(|package|members.contains(&package["id"])) {
        for target in package["targets"].as_array().ok_or_else(||fail("Cargo target inventory missing."))? {
            if target["kind"].as_array().is_some_and(|kind|kind.iter().any(|v|v == "test")) {
                let path = target["src_path"].as_str().ok_or_else(||fail("Cargo suite source missing."))?;
                declared.insert(PathBuf::from(path).canonicalize()?);
            }
        }
    }
    if declared != *actual { return Err(fail("Workspace suite files differ from Cargo discovery.")); }
    Ok(())
}

pub fn suite_metadata(context: &Context) -> io::Result<Value> {
    let bytes = context.capture(context.command("cargo").args(["metadata", "--locked", "--no-deps", "--format-version", "1"]), 30000, 20000, 16 * 1024 * 1024)?;
    serde_json::from_slice(&bytes).map_err(|_|fail("Invalid Cargo workspace metadata."))
}
