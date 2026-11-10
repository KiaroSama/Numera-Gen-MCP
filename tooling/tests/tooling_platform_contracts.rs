#[path = "common/tooling_execution.rs"]
mod tooling_fixture;

use numera_task::{context::sha, tooling_execution};
use serde_json::{Value, json};
use std::fs;
use tooling_fixture::Fixture;

const HARNESS: &str = "tooling/tests/coverage_diagnostics_contracts.rs";
const NAMES: [&str; 3] = [
    "report_observation_never_becomes_output_progress_or_weakens_idle_cleanup",
    "observation_opt_in_preserves_exit_wall_cancellation_and_original_output_limit",
    "numeric_observation_receipts_reject_partial_unknown_fields_stale_ids_and_links",
];

fn fixture(target: &str) -> Fixture {
    let mut fixture = Fixture::new(target);
    let mut manifest: Value = serde_json::from_slice(&fixture.manifest).unwrap();
    let row = manifest["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["source"] == "scripts/native-ci.py")
        .unwrap();
    let tests = row["tests"].as_array_mut().unwrap();
    if !tests.iter().any(|path| path == HARNESS) {
        tests.push(json!(HARNESS));
    }
    for symbol in NAMES {
        let mapping = json!(format!("{HARNESS}::{symbol}"));
        let named = row["named_tests"].as_array_mut().unwrap();
        if !named.contains(&mapping) {
            named.push(mapping.clone());
            row["scenario_tests"]["Compiler executable inventory and private bounded cleanup"]
                .as_array_mut()
                .unwrap()
                .push(json!(mapping));
        }
    }
    fixture.manifest = serde_json::to_vec(&manifest).unwrap();
    fixture.dependencies.insert(
        tooling_execution::MANIFEST_FILE.into(),
        sha(&fixture.manifest),
    );
    let source_hash = sha(&fs::read(fixture.root.join(HARNESS)).unwrap());
    fixture.sources.insert(HARNESS.into());
    fixture
        .dependencies
        .insert(HARNESS.into(), source_hash.clone());
    let passed = if target.contains("linux") {
        json!(NAMES)
    } else {
        json!([])
    };
    let count = passed.as_array().unwrap().len();
    fixture.report["harnesses"][HARNESS] = json!({
        "started":true, "count":count, "passed":passed, "slow_tests":[],
        "summary":format!("test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"),
        "source_sha256":source_hash, "executable_sha256":"1".repeat(64),
        "stdout_sha256":"2".repeat(64), "stderr_sha256":"3".repeat(64),
        "listing_sha256":"4".repeat(64), "exit_code":0, "cleanup_verified":true
    });
    fixture
}

#[test]
fn linux_only_tooling_bindings_require_exact_names_and_all_platform_reports() {
    for target in [
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "aarch64-apple-darwin",
    ] {
        let original = fixture(target);
        let result = original.bind().unwrap();
        let entry = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "scripts/native-ci.py")
            .unwrap();
        let harness = entry["tests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|test| test["source"] == HARNESS)
            .unwrap();
        assert_eq!(harness["applicable"], target.contains("linux"));
        for symbol in NAMES {
            let named = &entry["named_execution"][format!("{HARNESS}::{symbol}")];
            if target.contains("linux") {
                assert_eq!(named["harness"], HARNESS);
                assert_eq!(named["exact_test_name"], symbol);
            } else {
                assert_eq!(named["applicable"], false);
            }
        }
        let mut empty = original.clone();
        let report = &mut empty.report["harnesses"][HARNESS];
        report["passed"] = json!([]);
        report["count"] = json!(0);
        report["summary"] = json!(
            "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
        );
        assert_eq!(empty.bind().is_ok(), !target.contains("linux"));
        empty.report["harnesses"]
            .as_object_mut()
            .unwrap()
            .remove(HARNESS);
        assert!(
            empty.bind().is_err(),
            "zero tests never means missing report"
        );
        if target.contains("linux") {
            for symbol in NAMES {
                let mut unrelated = original.clone();
                let passed = unrelated.report["harnesses"][HARNESS]["passed"]
                    .as_array_mut()
                    .unwrap();
                passed.retain(|name| name != symbol);
                passed.push(json!("unrelated_passing_test"));
                assert!(unrelated.bind().is_err(), "missing exact {symbol}");
            }
        }
        assert_eq!(result["qualification_verified"], false);
        assert_eq!(result["retirement_authorized"], false);
    }
}
