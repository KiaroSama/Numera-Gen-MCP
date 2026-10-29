use crate::context::{Context, fail, sha};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io::{self, Write}, path::{Path, PathBuf}, process::Command, time::{Duration, Instant}};

const LIMIT: usize = 16 * 1024 * 1024;
fn lines(bytes: &[u8]) -> io::Result<impl Iterator<Item = &str>> {
    if bytes.len() > LIMIT || (!bytes.is_empty() && !bytes.ends_with(b"\n")) {
        return Err(fail("Harness output is oversized or incomplete."));
    }
    Ok(std::str::from_utf8(bytes).map_err(|_| fail("Harness output must be UTF-8."))?.lines())
}
fn test_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 1024
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_:".contains(&byte))
}

pub fn listing(bytes: &[u8]) -> io::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for line in lines(bytes)? {
        if line.is_empty() { continue; }
        let name = line.strip_suffix(": test").filter(|name| test_name(name))
            .ok_or_else(|| fail("Harness listing contains an unsupported entry."))?;
        if names.len() >= 20000 || !names.insert(name.to_owned()) {
            return Err(fail("Harness listing duplicates a test or exceeds its bound."));
        }
    }
    Ok(names)
}

pub fn results(bytes: &[u8], expected: &BTreeSet<String>) -> io::Result<Value> {
    let mut started = None;
    let mut summary = None;
    let mut passed = BTreeSet::new();
    let mut slow = BTreeSet::new();
    for line in lines(bytes)? {
        if line.is_empty() { continue; }
        if let Some(count) = line.strip_prefix("running ")
            .and_then(|text| text.strip_suffix(" tests").or_else(|| text.strip_suffix(" test"))) {
            let count = count.parse::<usize>().map_err(|_| fail("Invalid harness test count."))?;
            if started.replace(count).is_some() || summary.is_some() { return Err(fail("Duplicate harness start.")); }
        } else if let Some(name) = line.strip_prefix("test ").and_then(|text| text.strip_suffix(" ... ok")) {
            let name = name.strip_suffix(" - should panic").unwrap_or(name);
            if started.is_none() || summary.is_some() || !expected.contains(name) || !passed.insert(name.to_owned()) {
                return Err(fail("Harness pass is unknown, duplicated or outside its run."));
            }
        } else if let Some(name) = line.strip_prefix("test ").and_then(|text| text.strip_suffix(" has been running for over 60 seconds")) {
            if started.is_none() || summary.is_some() || !expected.contains(name) || passed.contains(name) {
                return Err(fail("Harness slow-test observation is outside its run."));
            }
            slow.insert(name.to_owned());
        } else if line.starts_with("test result: ok. ") {
            let prefix = format!("test result: ok. {} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ", expected.len());
            let elapsed = line.strip_prefix(&prefix).and_then(|text| text.strip_suffix('s'))
                .and_then(|text| text.parse::<f64>().ok()).filter(|number| number.is_finite() && *number >= 0.0);
            if elapsed.is_none() || started.is_none() || summary.replace(line.to_owned()).is_some() {
                return Err(fail("Harness summary is incomplete, filtered or unsuccessful."));
            }
        } else { return Err(fail("Harness output is not an exact successful test report.")); }
    }
    if started != Some(expected.len()) || passed != *expected || summary.is_none() {
        return Err(fail("Every listed test requires its own successful result."));
    }
    Ok(json!({"started":true,"count":expected.len(),"passed":passed,"summary":summary,"slow_tests":slow}))
}

pub fn compiled_harnesses(root: &Path, bytes: &[u8]) -> io::Result<BTreeMap<String, PathBuf>> {
    let root = root.canonicalize()?;
    let mut harnesses = BTreeMap::new();
    let mut executables = BTreeSet::new();
    let mut finished = false;
    for line in lines(bytes)? {
        let row: Value = serde_json::from_str(line).map_err(|_| fail("Compilation inventory contains non-JSON output."))?;
        if finished { return Err(fail("Compilation inventory continues after its terminal marker.")); }
        if row["reason"] == "build-finished" {
            if row["success"] != true { return Err(fail("Harness compilation failed.")); }
            finished = true;
            continue;
        }
        if row["reason"] != "compiler-artifact" || row["profile"]["test"] != true || row["executable"].is_null() { continue; }
        let path = |value: &Value| -> io::Result<PathBuf> {
            let path = Path::new(value.as_str().ok_or_else(|| fail("Compiler source or executable path is missing."))?);
            if !path.is_absolute() { return Err(fail("Compiler paths must be absolute.")); }
            path.canonicalize().map_err(|_| fail("Compiler source or executable is missing."))
        };
        let manifest = path(&row["manifest_path"])?;
        if manifest != root.join("Cargo.toml") && manifest != root.join("tooling/Cargo.toml") {
            return Err(fail("Compiled harness belongs to an unexpected package."));
        }
        let source = path(&row["target"]["src_path"])?;
        let source = source.strip_prefix(&root).map_err(|_| fail("Compiled harness source escapes the workspace."))?
            .to_str().ok_or_else(|| fail("Harness source must be UTF-8."))?.replace('\\', "/");
        if !source.ends_with(".rs") || !["native/", "tests/rust/", "tooling/"].iter().any(|prefix| source.starts_with(prefix)) {
            return Err(fail("Compiled harness source is outside maintained test scope."));
        }
        let executable = path(&row["executable"])?;
        if !executable.starts_with(root.join("target")) || !executable.is_file() || !executables.insert(executable.clone())
            || harnesses.insert(source, executable).is_some() {
            return Err(fail("Compiled harness identity is escaping or duplicated."));
        }
    }
    if !finished || harnesses.is_empty() { return Err(fail("Successful compiled harness inventory is missing.")); }
    Ok(harnesses)
}

pub fn selected_sources(root: &Path, metadata: &Value, selected: &[&str]) -> io::Result<BTreeSet<String>> {
    let root = root.canonicalize()?;
    let wanted = selected.iter().copied().collect::<BTreeSet<_>>();
    if wanted.is_empty() || wanted.len() != selected.len() { return Err(fail("Selected harness packages are empty or duplicated.")); }
    let members = metadata["workspace_members"].as_array().ok_or_else(|| fail("Harness workspace members missing."))?;
    let packages = metadata["packages"].as_array().ok_or_else(|| fail("Harness package metadata missing."))?;
    let mut found = BTreeSet::new();
    let mut expected = BTreeSet::new();
    for package in packages.iter().filter(|package| members.contains(&package["id"])) {
        let name = package["name"].as_str().ok_or_else(|| fail("Harness package name missing."))?;
        if !wanted.contains(name) { continue; }
        if !found.insert(name) { return Err(fail("Selected harness package is duplicated.")); }
        for target in package["targets"].as_array().ok_or_else(|| fail("Harness targets missing."))? {
            let enabled = target["test"].as_bool().ok_or_else(|| fail("Harness target selection missing."))?;
            if !enabled { continue; }
            let path = Path::new(target["src_path"].as_str().ok_or_else(|| fail("Harness target source missing."))?);
            if !path.is_absolute() { return Err(fail("Harness target source must be absolute.")); }
            let path = path.canonicalize()?;
            let source = path.strip_prefix(&root).map_err(|_| fail("Harness target source escapes workspace."))?
                .to_str().ok_or_else(|| fail("Harness target source must be UTF-8."))?.replace('\\', "/");
            if !expected.insert(source) { return Err(fail("Harness target source is duplicated.")); }
        }
    }
    if found != wanted || expected.is_empty() {
        return Err(fail("Selected Cargo test targets are missing."));
    }
    Ok(expected)
}

pub fn verify_harness_inventory(root: &Path, metadata: &Value, selected: &[&str], harnesses: &BTreeMap<String, PathBuf>) -> io::Result<()> {
    if selected_sources(root, metadata, selected)? != harnesses.keys().cloned().collect() {
        return Err(fail("Compiled harnesses differ from all selected Cargo targets."));
    }
    Ok(())
}

pub fn execution_identity(environment: &Value, phase: &str, target: &str) -> io::Result<Value> {
    crate::distribution::target(target)?;
    let job = match phase {
        "built" | "release" | "extracted-intermediate" => "native",
        "coverage" if target == "x86_64-unknown-linux-gnu" => "native-coverage",
        "extracted-signed" => "signing",
        _ => return Err(fail("Unsupported exact execution phase or target.")),
    };
    let source = environment["GITHUB_SHA"].as_str().unwrap_or("");
    let reference = environment["GITHUB_REF"].as_str().unwrap_or("");
    if environment["GITHUB_REPOSITORY"] != "KiaroSama/Numera-Gen-MCP"
        || environment["GITHUB_EVENT_NAME"] != "workflow_dispatch" || environment["GITHUB_JOB"] != job
        || source.len() != 40 || !source.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || reference.strip_prefix("refs/heads/").is_none_or(|name| name.is_empty() || name.chars().any(char::is_whitespace)) {
        return Err(fail("Exact execution repository, event, source, branch or job differs."));
    }
    let number = |key: &str| -> io::Result<u64> {
        environment[key].as_str().filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<u64>().ok()).filter(|value| *value > 0)
            .ok_or_else(|| fail("Exact execution run and attempt must be positive decimal identities."))
    };
    let platform = if target.contains("windows") { "win32" } else if target.contains("apple") { "darwin" } else { "linux" };
    Ok(json!({"source_sha":source,"repository":environment["GITHUB_REPOSITORY"],"reference":reference,
        "event":"workflow_dispatch","job":job,"run_id":number("GITHUB_RUN_ID")?,
        "run_attempt":number("GITHUB_RUN_ATTEMPT")?,"target":target,"platform":platform,"phase":phase}))
}

pub fn hosted_identity(context: &Context, phase: &str) -> io::Result<Value> {
    context.hosted()?;
    let environment = ["GITHUB_REPOSITORY", "GITHUB_EVENT_NAME", "GITHUB_SHA", "GITHUB_REF",
        "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB"].into_iter()
        .map(|key| (key.to_owned(), std::env::var(key).map(Value::String).unwrap_or(Value::Null)))
        .collect::<serde_json::Map<_, _>>();
    let identity = execution_identity(&Value::Object(environment), phase, numera_gen_mcp::devtools::artifact::target())?;
    if identity["source_sha"] != context.source_sha()? { return Err(fail("Exact execution source identity differs.")); }
    Ok(identity)
}

fn remaining(deadline: Instant) -> io::Result<u64> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() { return Err(fail("Aggregate test execution wall ceiling elapsed.")); }
    Ok((duration.as_millis().min(u64::MAX as u128) as u64).max(1))
}

pub fn harness_command(context: &Context, compile: &Command, source: &str, executable: &Path) -> Command {
    let mut command = context.command(executable);
    command.current_dir(if source.starts_with("tooling/") { context.root.join("tooling") } else { context.root.clone() });
    // Only runtime instrumentation and its contained target cross the compile/execute boundary.
    for (key, value) in compile.get_envs().filter(|(key, _)| *key == "LLVM_PROFILE_FILE" || *key == "CARGO_TARGET_DIR") {
        match value { Some(value) => { command.env(key, value); }, None => { command.env_remove(key); } }
    }
    command.env_remove("NUMERA_NATIVE_BINARY").env_remove("RUST_TEST_NOCAPTURE").env_remove("RUST_TEST_TIME_UNIT")
        .env_remove("RUST_TEST_SHUFFLE").env_remove("RUST_TEST_SHUFFLE_SEED");
    command
}

pub fn execute_harness(context: &Context, mut command: impl FnMut() -> Command, source: &str, deadline: Instant, idle: u64) -> io::Result<Value> {
    context.hosted()?;
    let mut list = command();
    let executable = PathBuf::from(list.get_program());
    let source_hash = sha(&crate::ledger::read(&context.root, source)?);
    let executable_hash = sha(&crate::distribution::read_regular(&executable, crate::archives::MAX_BYTES)?);
    list.args(["--list", "--format=terse", "--color=never"]);
    let listed = context.capture(&mut list, remaining(deadline)?.min(30000), 20000, LIMIT)?;
    let expected = listing(&listed)?;
    let mut test = command();
    if test.get_program() != executable.as_os_str() || test.get_current_dir() != list.get_current_dir() {
        return Err(fail("Harness command identity changed after listing."));
    }
    test.args(["--format=pretty", "--color=never", "--test-threads=2"]);
    let executed = context.outcome(&mut test, remaining(deadline)?, idle, LIMIT)?;
    io::stdout().write_all(format!("Native harness: {source}\n").as_bytes())?;
    io::stdout().write_all(&executed.stdout)?;
    io::stderr().write_all(&executed.stderr)?;
    if executed.code != 0 || executed.reason.is_some() || !executed.cleanup_verified {
        return Err(fail("Owned harness execution failed."));
    }
    if source_hash != sha(&crate::ledger::read(&context.root, source)?)
        || executable_hash != sha(&crate::distribution::read_regular(&executable, crate::archives::MAX_BYTES)?) {
        return Err(fail("Harness source or executable changed during execution."));
    }
    let mut report = results(&executed.stdout, &expected)?;
    report["source_sha256"] = json!(source_hash);
    report["executable_sha256"] = json!(executable_hash);
    report["stdout_sha256"] = json!(sha(&executed.stdout));
    report["stderr_sha256"] = json!(sha(&executed.stderr));
    report["listing_sha256"] = json!(sha(&listed));
    report["exit_code"] = json!(executed.code);
    report["cleanup_verified"] = json!(executed.cleanup_verified);
    Ok(report)
}

/// Compile once, then execute each actual compiler-emitted harness once. Listing does not run tests.
/// Reports retain exact names and hashes; they do not assert semantic parity or release qualification.
pub fn run(context: &Context, compile: &mut Command, wall: u64, phase: &str, selected: &[&str], failures: &mut Vec<String>) -> io::Result<()> {
    let identity = hosted_identity(context, phase)?;
    let provenance = crate::evidence::snapshot(context)?;
    let deadline = Instant::now() + Duration::from_millis(wall);
    let compiled = context.outcome(compile, remaining(deadline)?, 180000, LIMIT)?;
    io::stdout().write_all(&compiled.stdout)?;
    io::stderr().write_all(&compiled.stderr)?;
    if compiled.code != 0 || compiled.reason.is_some() || !compiled.cleanup_verified {
        return Err(fail("Owned harness compilation failed; no execution was claimed."));
    }
    let harnesses = compiled_harnesses(&context.root, &compiled.stdout)?;
    let metadata = context.capture(context.command("cargo").args(["metadata", "--locked", "--no-deps", "--format-version", "1"]),
        remaining(deadline)?.min(30000), 20000, LIMIT)?;
    verify_harness_inventory(&context.root, &crate::ledger::parse(&metadata)?, selected, &harnesses)?;
    crate::ci::record_test_executables(context, &compiled.stdout)?;
    let expected_sources = harnesses.keys().cloned().collect::<BTreeSet<_>>();
    let mut records = BTreeMap::new();
    for (source, executable) in harnesses {
        let operation = execute_harness(context, || harness_command(context, compile, &source, &executable), &source, deadline, 180000);
        match operation {
            Ok(report) => { records.insert(source, report); }
            Err(error) => {
                failures.push(format!("harness:{source}"));
                records.insert(source, json!({"execution_verified":false,"error":crate::context::diagnostic(&error)}));
            }
        }
        if Instant::now() >= deadline { failures.push("test-aggregate-wall".into()); break; }
    }
    if crate::evidence::snapshot(context)? != provenance {
        failures.push("execution-provenance-changed".into());
    }
    let report = json!({"schema_version":1,"component":"native-exact-test-execution",
        "identity":identity,"provenance_sha256":provenance.sha256,"harnesses":records,
        "required_failed_gates":failures,"semantic_parity_claimed":false,"qualification_verified":false});
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| fail("Cannot encode exact execution evidence."))?;
    if failures.is_empty() {
        crate::execution_evidence::verify_report(&bytes, &identity, &provenance.sha256,
            &provenance.dependencies, &expected_sources)?;
    }
    context.write_new(".ci-work/native/test-execution.json", &bytes)
}
