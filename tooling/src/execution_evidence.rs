use crate::{context::fail, test_execution};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io};

fn hash(value: &Value) -> bool {
    value.as_str().is_some_and(|text| text.len() == 64
        && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}
fn source(path: &str) -> bool {
    path.len() <= 1024 && path.ends_with(".rs")
        && ["native/", "tests/rust/", "tooling/"].iter().any(|prefix| path.starts_with(prefix))
        && !path.bytes().any(|byte| byte == b'\\' || byte == b':' || byte.is_ascii_control())
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}
fn identity(value: &Value) -> io::Result<()> {
    let number = |key: &str| value[key].as_u64().filter(|number| *number > 0)
        .map(|number| number.to_string()).ok_or_else(|| fail("Execution identity requires positive run and attempt."));
    let environment = json!({"GITHUB_REPOSITORY":value["repository"],"GITHUB_EVENT_NAME":value["event"],
        "GITHUB_SHA":value["source_sha"],"GITHUB_REF":value["reference"],"GITHUB_JOB":value["job"],
        "GITHUB_RUN_ID":number("run_id")?,"GITHUB_RUN_ATTEMPT":number("run_attempt")?});
    let actual = test_execution::execution_identity(&environment,
        value["phase"].as_str().unwrap_or(""), value["target"].as_str().unwrap_or(""))?;
    if *value != actual { return Err(fail("Execution identity is not an exact supported hosted phase.")); }
    Ok(())
}
fn names(value: &Value) -> io::Result<BTreeSet<String>> {
    let values = value.as_array().filter(|values| values.len() <= 20000)
        .ok_or_else(|| fail("Execution names are missing or oversized."))?;
    let mut text = String::new();
    for value in values {
        let name = value.as_str().filter(|name| !name.is_empty() && name.len() <= 1024
            && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_:".contains(&byte)))
            .ok_or_else(|| fail("Execution test name is invalid."))?;
        text.push_str(name); text.push_str(": test\n");
    }
    test_execution::listing(text.as_bytes())
}

/// Validates a report against separately authenticated identity/provenance and selected inventory.
/// It cannot authenticate an artifact download or establish assertion equivalence by itself.
pub fn verify_report(bytes: &[u8], expected_identity: &Value, provenance: &str,
    dependencies: &BTreeMap<String, String>, expected_sources: &BTreeSet<String>) -> io::Result<Value> {
    identity(expected_identity)?;
    let report = crate::ledger::parse(bytes)?;
    let sdk = matches!(expected_identity["phase"].as_str(), Some("release" | "extracted-intermediate" | "extracted-signed"));
    let component = if sdk { "native-exact-sdk-execution" } else { "native-exact-test-execution" };
    if report["schema_version"] != 1 || report["component"] != component || report["identity"] != *expected_identity
        || !hash(&json!(provenance)) || report["provenance_sha256"] != provenance
        || report["required_failed_gates"].as_array().is_none_or(|values| !values.is_empty())
        || report["semantic_parity_claimed"] != false || report["qualification_verified"] != false {
        return Err(fail("Execution report identity, provenance or required gates differ."));
    }
    let harnesses = report["harnesses"].as_object().ok_or_else(|| fail("Execution harness inventory missing."))?;
    if expected_sources.is_empty() || expected_sources.len() > 4096
        || expected_sources.iter().any(|path| !source(path))
        || harnesses.keys().cloned().collect::<BTreeSet<_>>() != *expected_sources {
        return Err(fail("Execution harness inventory differs from selected targets."));
    }
    if sdk && (expected_sources.iter().map(String::as_str).collect::<BTreeSet<_>>() != BTreeSet::from([
        "tests/rust/sdk_acceptance.rs", "tests/rust/media_acceptance.rs"])
        || !hash(&report["tested_binary_sha256"])) {
        return Err(fail("SDK execution requires both exact harnesses and tested binary identity."));
    }
    for (path, harness) in harnesses {
        if harness["started"] != true || harness["exit_code"].as_i64() != Some(0)
            || harness["cleanup_verified"] != true || !hash(&harness["source_sha256"])
            || dependencies.get(path).is_none_or(|digest| harness["source_sha256"] != *digest)
            || ["executable_sha256", "stdout_sha256", "stderr_sha256", "listing_sha256"]
                .iter().any(|key| !hash(&harness[*key])) {
            return Err(fail("Execution source, output identity, exit or cleanup differs."));
        }
        let passed = names(&harness["passed"])?;
        if harness["count"].as_u64() != Some(passed.len() as u64) {
            return Err(fail("Execution count differs from exact successful names."));
        }
        let slow = names(&harness["slow_tests"])?;
        if !slow.is_subset(&passed) { return Err(fail("Slow-test observation has no successful result.")); }
        let summary = harness["summary"].as_str().ok_or_else(|| fail("Exact execution summary missing."))?;
        if summary.contains(['\r', '\n']) { return Err(fail("Execution summary must be one exact line.")); }
        let mut text = format!("running {} tests\n", passed.len());
        for name in &passed { text.push_str(&format!("test {name} ... ok\n")); }
        text.push_str(summary); text.push('\n');
        test_execution::results(text.as_bytes(), &passed)?;
    }
    Ok(report)
}

/// Select an already-validated exact result, never a substring or an unrelated harness.
pub fn named_test(report: &Value, mapping: &str) -> io::Result<Value> {
    let (file, symbol) = mapping.split_once("::").ok_or_else(|| fail("Executed mapping requires a test symbol."))?;
    if !source(file) || symbol.is_empty() || !symbol.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        return Err(fail("Executed mapping is outside the maintained Rust test contract."));
    }
    let harness = if file.starts_with("native/") { "native/lib.rs" } else { file };
    let passed = names(&report["harnesses"][harness]["passed"])?;
    let suffix = format!("::{symbol}");
    let matched = passed.iter().filter(|name| name.as_str() == symbol || name.ends_with(&suffix)).collect::<Vec<_>>();
    if matched.len() != 1 { return Err(fail("Exact mapped test is missing or ambiguous in its harness.")); }
    Ok(json!({"harness":harness,"exact_test_name":matched[0]}))
}
