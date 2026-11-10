#[path = "common/tooling_execution.rs"]
mod tooling_fixture;

use numera_task::{context::sha, tooling_execution};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use tooling_fixture::Fixture;

#[test]
fn original_tooling_dispositions_bind_current_source_and_named_harnesses_without_retirement() {
    for target in [
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "aarch64-apple-darwin",
    ] {
        let fixture = Fixture::new(target);
        let result = fixture.bind().unwrap();
        let manifest = serde_json::from_slice(&fixture.manifest).unwrap();
        let review = serde_json::from_slice(
            &fs::read(fixture.root.join(numera_task::tooling_review::REVIEW_FILE)).unwrap(),
        )
        .unwrap();
        let reviewed =
            numera_task::tooling_review::bind_execution(&fixture.root, &manifest, &review, &result)
                .unwrap();
        assert_eq!(
            reviewed["current_tooling_assertions_and_execution_verified"],
            true
        );
        assert_eq!(reviewed["qualification_verified"], false);
        let mut missing = result.clone();
        missing["entries"].as_array_mut().unwrap().pop();
        assert!(
            numera_task::tooling_review::bind_execution(
                &fixture.root,
                &manifest,
                &review,
                &missing,
            )
            .is_err()
        );
        assert_eq!(result["mapped_files"], 51);
        assert!(result.get("retained_powershell").is_none());
        assert_eq!(result["entries"].as_array().unwrap().len(), 51);
        assert_eq!(result["manifest_sha256"], sha(&fixture.manifest));
        assert_eq!(result["identity"], fixture.identity);
        assert_eq!(result["current_named_harness_execution_verified"], true);
        assert_eq!(result["scenario_assertions_verified"], false);
        assert_eq!(result["semantic_parity_claimed"], false);
        assert_eq!(result["qualification_verified"], false);
        assert_eq!(result["retirement_authorized"], false);
        let dpapi = "tests/rust/dpapi_acceptance.rs";
        let protection = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "scripts/protect-secret.ps1")
            .unwrap();
        let test = protection["tests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|test| test["source"] == dpapi)
            .unwrap();
        assert_eq!(test["applicable"], target.contains("windows"));
        let mut empty = fixture.clone();
        let harness = &mut empty.report["harnesses"][dpapi];
        harness["passed"] = json!([]);
        harness["count"] = json!(0);
        harness["summary"] = json!(
            "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
        );
        assert_eq!(empty.bind().is_ok(), !target.contains("windows"));
        empty.report["harnesses"]
            .as_object_mut()
            .unwrap()
            .remove(dpapi);
        assert!(empty.bind().is_err());
        if target.contains("windows") {
            let mapping = format!(
                "{dpapi}::protection_dry_run_preserves_input_and_output_and_enforces_private_file_invariants"
            );
            assert_eq!(
                protection["named_execution"][&mapping]["exact_test_name"],
                "windows::protection_dry_run_preserves_input_and_output_and_enforces_private_file_invariants"
            );
            let mut unrelated = fixture.clone();
            let harness = &mut unrelated.report["harnesses"][dpapi];
            harness["passed"] = json!(["windows::unrelated_passing_test"]);
            harness["count"] = json!(1);
            harness["summary"] = json!(
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
            );
            assert!(unrelated.bind().is_err());
        }
        let bounded = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "scripts/bounded.mjs")
            .unwrap();
        let unix = bounded["tests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|test| test["source"] == "native/devtools/process_unix.rs")
            .unwrap();
        assert_eq!(unix["applicable"], !target.contains("windows"));
        let unix_mapping = "native/devtools/process_unix.rs::denied_group_kill_requires_closed_leader_and_definitive_live_group_absence";
        let abrupt = "tests/rust/sdk_acceptance/http.rs::abrupt_parent_stop_contains_actual_sdk_and_http_leaf_children";
        if let Some(execution) = bounded["named_execution"].get(abrupt) {
            if target.contains("windows") {
                assert_eq!(execution["applicable"], false);
            } else {
                assert!(
                    execution["exact_test_name"].as_str().unwrap().ends_with(
                        "::abrupt_parent_stop_contains_actual_sdk_and_http_leaf_children"
                    )
                );
            }
        }
        let wide_exit = "tests/rust/devtool_supervisor.rs::actual_developer_cli_preserves_full_windows_child_exit_status";
        if target.contains("windows") {
            assert_eq!(
                bounded["named_execution"][unix_mapping]["applicable"],
                false
            );
            assert_eq!(
                bounded["named_execution"][wide_exit]["exact_test_name"],
                "actual_developer_cli_preserves_full_windows_child_exit_status"
            );
        } else {
            assert_eq!(bounded["named_execution"][wide_exit]["applicable"], false);
            assert_eq!(
                bounded["named_execution"][unix_mapping]["harness"],
                "native/lib.rs"
            );
        }
        let activation = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "scripts/native-activation-acceptance.py")
            .unwrap();
        let platform_signature = "tests/rust/artifact_signatures.rs::approved_manifest_does_not_authorize_unsigned_text_for_activation";
        if target.contains("linux") {
            assert_eq!(
                activation["named_execution"][platform_signature]["applicable"],
                false
            );
        } else {
            assert_eq!(
                activation["named_execution"][platform_signature]["harness"],
                "tests/rust/artifact_signatures.rs"
            );
        }
        let logging = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "scripts/logging.mjs")
            .unwrap();
        let sdk = logging["tests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|test| test["source"] == "tests/rust/sdk_acceptance/configuration.rs")
            .unwrap();
        assert_eq!(sdk["harness"], "tests/rust/sdk_acceptance.rs");
        assert_eq!(
            result["harnesses"]["tests/rust/sdk_acceptance.rs"],
            fixture.report["harnesses"]["tests/rust/sdk_acceptance.rs"]["passed"]
        );
        assert_eq!(result["current_named_test_execution_verified"], true);
        assert_eq!(result["declared_scenario_execution_verified"], true);
        assert_eq!(result["sdk_artifact_phase_execution_verified"], true);
        assert_eq!(result["sdk_tested_binary_sha256"], "6".repeat(64));
        for phase in ["built", "release", "extracted-intermediate"] {
            let report = if phase == "built" {
                &fixture.report
            } else {
                &fixture.artifact_reports[phase]
            };
            assert_eq!(
                result["phase_reports_sha256"][phase],
                sha(&serde_json::to_vec(report).unwrap())
            );
        }
        for entry in result["entries"].as_array().unwrap() {
            let expected = entry["named_execution"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(_, execution)| {
                    matches!(
                        execution["harness"].as_str(),
                        Some("tests/rust/sdk_acceptance.rs" | "tests/rust/media_acceptance.rs")
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let phases = entry["artifact_phase_execution"].as_object().unwrap();
            if expected.is_empty() {
                assert!(phases.is_empty());
            } else {
                assert_eq!(
                    phases.keys().map(String::as_str).collect::<BTreeSet<_>>(),
                    BTreeSet::from(["release", "extracted-intermediate"])
                );
                for executions in phases.values() {
                    assert_eq!(
                        executions
                            .as_object()
                            .unwrap()
                            .iter()
                            .collect::<BTreeMap<_, _>>(),
                        expected
                    );
                }
            }
            let scenarios = entry["scenario_execution"].as_object().unwrap();
            assert_eq!(
                scenarios.len(),
                entry["scenarios"].as_array().unwrap().len()
            );
            for scenario in entry["scenarios"].as_array().unwrap() {
                let bindings = scenarios[scenario.as_str().unwrap()].as_object().unwrap();
                assert!(!bindings.is_empty());
                for (mapping, execution) in bindings {
                    assert_eq!(execution, &entry["named_execution"][mapping]);
                }
            }
        }
        let mapped = &logging["named_execution"];
        let expected = "tests/rust/runtime_contracts.rs::log_runs_are_distinct_redacted_at_all_levels_immutable_and_closed";
        assert_eq!(
            mapped[expected]["harness"],
            "tests/rust/runtime_contracts.rs"
        );
        let helpers = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "tests/tooling/native_host_fixture_contracts.mjs")
            .unwrap();
        let diagnostic = "tests/rust/sdk_acceptance/contracts.rs::probe_diagnostics_expose_only_fixed_stages_and_nonnegative_numeric_durations";
        assert_eq!(
            helpers["named_execution"][diagnostic]["harness"],
            "tests/rust/sdk_acceptance.rs"
        );
        assert_eq!(
            helpers["named_execution"][diagnostic]["exact_test_name"],
            "probe_diagnostics_expose_only_fixed_stages_and_nonnegative_numeric_durations"
        );
        assert_eq!(
            mapped[expected]["exact_test_name"],
            "log_runs_are_distinct_redacted_at_all_levels_immutable_and_closed"
        );
    }
}

#[test]
fn retired_tree_binds_original_blobs_without_claiming_deleted_paths_as_current_dependencies() {
    let mut fixture = Fixture::new("x86_64-unknown-linux-gnu");
    let removed = fixture.originals.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(removed.len(), 51);
    let directory = fixture
        .root
        .join(".ci-work/tooling-tests")
        .join(format!("retired-bind-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for path in fixture
            .dependencies
            .keys()
            .filter(|path| !removed.contains(*path))
        {
            let target = directory.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, fs::read(fixture.root.join(path)).unwrap()).unwrap();
        }
        fixture.root = directory.canonicalize().unwrap();
        fixture
            .dependencies
            .retain(|path, _| !removed.contains(path));
        assert!(removed.iter().all(|path| !fixture.root.join(path).exists()));
        let report = fixture.bind().unwrap();
        assert!(
            removed
                .iter()
                .all(|path| report["source_files"].get(path).is_none())
        );
        assert_eq!(
            report["historical_original_files"]
                .as_object()
                .unwrap()
                .len(),
            51
        );
        assert_eq!(report["retirement_authorized"], false);
        let path = removed.first().unwrap();
        let bytes = fixture.originals.remove(path).unwrap();
        assert!(fixture.bind().is_err(), "missing historical original");
        fixture
            .originals
            .insert(path.clone(), [bytes, vec![b' ']].concat());
        assert!(fixture.bind().is_err(), "mutated historical original");
    }));
    fs::remove_dir_all(&directory).unwrap();
    result.unwrap();
}

#[test]
fn sdk_tooling_bindings_require_both_authenticated_artifact_phases_and_exact_names() {
    let original = Fixture::new("x86_64-unknown-linux-gnu");
    let source = "scripts/native-extract-check.py";
    let media = "tests/rust/media_acceptance.rs";
    let mapped_name = "real_sdk_media_kinds_modes_outputs_and_legacy_catalog";
    let result = original.bind().unwrap();
    let entry = result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["source"] == source)
        .unwrap();
    for phase in ["release", "extracted-intermediate"] {
        assert_eq!(
            entry["artifact_phase_execution"][phase][format!("{media}::{mapped_name}")]["exact_test_name"],
            mapped_name
        );
        for mutation in [
            "missing",
            "extra",
            "phase",
            "attempt",
            "source",
            "target",
            "provenance",
            "source-hash",
            "binary",
            "different-binary",
            "missing-harness",
            "extra-harness",
            "cleanup",
            "failed",
            "missing-name",
        ] {
            let mut fixture = original.clone();
            if mutation == "missing" {
                fixture.artifact_reports.remove(phase);
            } else if mutation == "extra" {
                fixture.artifact_reports.insert(
                    "extracted-signed".into(),
                    fixture.artifact_reports[phase].clone(),
                );
            } else {
                let report = fixture.artifact_reports.get_mut(phase).unwrap();
                match mutation {
                    "phase" => report["identity"]["phase"] = json!("built"),
                    "attempt" => report["identity"]["run_attempt"] = json!(2),
                    "source" => report["identity"]["source_sha"] = json!("b".repeat(40)),
                    "target" => report["identity"]["target"] = json!("aarch64-unknown-linux-gnu"),
                    "provenance" => report["provenance_sha256"] = json!("7".repeat(64)),
                    "source-hash" => {
                        report["harnesses"][media]["source_sha256"] = json!("8".repeat(64))
                    }
                    "binary" => report["tested_binary_sha256"] = Value::Null,
                    "different-binary" => report["tested_binary_sha256"] = json!("9".repeat(64)),
                    "missing-harness" => {
                        report["harnesses"].as_object_mut().unwrap().remove(media);
                    }
                    "extra-harness" => {
                        report["harnesses"]["native/lib.rs"] =
                            fixture.report["harnesses"]["native/lib.rs"].clone()
                    }
                    "cleanup" => report["harnesses"][media]["cleanup_verified"] = json!(false),
                    "failed" => report["harnesses"][media]["exit_code"] = json!(1),
                    "missing-name" => {
                        let harness = &mut report["harnesses"][media];
                        let passed = harness["passed"].as_array_mut().unwrap();
                        assert!(passed.iter().any(|name| name == mapped_name));
                        passed.retain(|name| name != mapped_name);
                        passed.push(json!("unrelated_passing_test"));
                        // Same count and valid summary: rejection must use the exact mapped name.
                    }
                    _ => unreachable!(),
                }
            }
            assert!(fixture.bind().is_err(), "{phase}/{mutation}");
        }
    }
}

#[test]
fn stale_missing_empty_failed_or_unproven_tooling_evidence_is_rejected() {
    let original = Fixture::new("x86_64-unknown-linux-gnu");
    for mutation in [
        "manifest",
        "original",
        "replacement",
        "helper",
        "missing-dependency",
        "missing-sdk",
        "sdk-pointer",
        "missing-harness",
        "empty-harness",
        "unrelated-names",
        "duplicate-mapping",
        "unregistered-mapping",
        "wrong-mapping-harness",
        "empty-mappings",
        "missing-scenario-bindings",
        "missing-scenario",
        "extra-scenario",
        "duplicate-scenario",
        "empty-scenario-tests",
        "duplicate-scenario-test",
        "unknown-scenario-test",
        "unused-named-test",
        "cleanup",
        "filtered",
        "phase",
        "promoted",
    ] {
        let mut fixture = original.clone();
        match mutation {
            "manifest" => {
                fixture.manifest.push(b' ');
            }
            "original" => {
                fixture
                    .originals
                    .get_mut("scripts/bounded.mjs")
                    .unwrap()
                    .push(b' ');
            }
            "replacement" => {
                fixture
                    .dependencies
                    .insert("native/devtools/supervisor.rs".into(), "0".repeat(64));
            }
            "helper" => {
                fixture.dependencies.insert(
                    "tests/rust/sdk_acceptance/configuration.rs".into(),
                    "0".repeat(64),
                );
            }
            "missing-dependency" => {
                fixture.dependencies.remove("native/devtools/mod.rs");
            }
            "missing-sdk" => {
                fixture
                    .dependencies
                    .remove("docs/testing/sdk-tooling-migration.json");
            }
            "sdk-pointer" => {
                let mut manifest: Value = serde_json::from_slice(&fixture.manifest).unwrap();
                manifest["sdk_scenario_manifest"] = json!("docs/testing/unrelated.json");
                fixture.manifest = serde_json::to_vec(&manifest).unwrap();
                fixture.dependencies.insert(
                    tooling_execution::MANIFEST_FILE.into(),
                    sha(&fixture.manifest),
                );
            }
            "missing-harness" => {
                fixture.report["harnesses"]
                    .as_object_mut()
                    .unwrap()
                    .remove("tooling/tests/ci_contracts.rs");
            }
            "empty-harness" => {
                let harness = &mut fixture.report["harnesses"]["tooling/tests/ci_contracts.rs"];
                harness["passed"] = json!([]);
                harness["count"] = json!(0);
                harness["summary"] = json!(
                    "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
                );
            }
            "unrelated-names" => {
                let harness = &mut fixture.report["harnesses"]["tests/rust/sdk_acceptance.rs"];
                harness["passed"] = json!(["unrelated_passing_test"]);
                harness["count"] = json!(1);
                harness["summary"] = json!(
                    "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
                );
            }
            "duplicate-mapping"
            | "unregistered-mapping"
            | "wrong-mapping-harness"
            | "empty-mappings" => {
                let mut manifest: Value = serde_json::from_slice(&fixture.manifest).unwrap();
                let mappings = &mut manifest["entries"][0]["named_tests"];
                match mutation {
                    "duplicate-mapping" => {
                        let first = mappings[0].clone();
                        mappings.as_array_mut().unwrap().push(first);
                    }
                    "unregistered-mapping" => {
                        mappings[0] = json!("tests/rust/devtool_supervisor.rs::command");
                    }
                    "wrong-mapping-harness" => {
                        mappings[0] = json!(
                            "tests/rust/config_parity.rs::source_generated_defaults_compact_advanced_and_overrides_match"
                        );
                    }
                    _ => *mappings = json!([]),
                }
                fixture.manifest = serde_json::to_vec(&manifest).unwrap();
                fixture.dependencies.insert(
                    tooling_execution::MANIFEST_FILE.into(),
                    sha(&fixture.manifest),
                );
            }
            "missing-scenario-bindings"
            | "missing-scenario"
            | "extra-scenario"
            | "duplicate-scenario"
            | "empty-scenario-tests"
            | "duplicate-scenario-test"
            | "unknown-scenario-test"
            | "unused-named-test" => {
                let mut manifest: Value = serde_json::from_slice(&fixture.manifest).unwrap();
                let row = &mut manifest["entries"][0];
                let scenario = row["scenarios"][0].as_str().unwrap().to_owned();
                match mutation {
                    "missing-scenario-bindings" => {
                        row.as_object_mut().unwrap().remove("scenario_tests");
                    }
                    "missing-scenario" => {
                        row["scenario_tests"]
                            .as_object_mut()
                            .unwrap()
                            .remove(&scenario);
                    }
                    "extra-scenario" => {
                        row["scenario_tests"]["Unrequested scenario"] =
                            row["scenario_tests"][&scenario].clone();
                    }
                    "duplicate-scenario" => {
                        row["scenarios"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!(scenario));
                    }
                    "empty-scenario-tests" => row["scenario_tests"][&scenario] = json!([]),
                    "duplicate-scenario-test" => {
                        let first = row["scenario_tests"][&scenario][0].clone();
                        row["scenario_tests"][&scenario]
                            .as_array_mut()
                            .unwrap()
                            .push(first);
                    }
                    "unknown-scenario-test" => {
                        row["scenario_tests"][&scenario][0] =
                            json!("tooling/tests/ci_contracts.rs::unrelated");
                    }
                    _ => {
                        let first = row["named_tests"][0].clone();
                        for tests in row["scenario_tests"].as_object_mut().unwrap().values_mut() {
                            *tests = json!([first]);
                        }
                    }
                }
                fixture.manifest = serde_json::to_vec(&manifest).unwrap();
                fixture.dependencies.insert(
                    tooling_execution::MANIFEST_FILE.into(),
                    sha(&fixture.manifest),
                );
            }
            "cleanup" => {
                fixture.report["harnesses"]["native/lib.rs"]["cleanup_verified"] = json!(false);
            }
            "filtered" => {
                let count = fixture.report["harnesses"]["native/lib.rs"]["count"]
                    .as_u64()
                    .unwrap();
                fixture.report["harnesses"]["native/lib.rs"]["summary"] = json!(format!(
                    "test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s"
                ));
            }
            "phase" => fixture.identity["phase"] = json!("coverage"),
            "promoted" => fixture.report["qualification_verified"] = json!(true),
            _ => unreachable!(),
        }
        assert!(fixture.bind().is_err(), "{mutation}");
    }
}
