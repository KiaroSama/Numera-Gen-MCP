use numera_task::{context::sha, test_execution::execution_identity, tooling_execution};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

// Synthetic reports exercise binding only; they are never hosted execution or scenario evidence.
#[derive(Clone)]
struct Fixture {
    root: PathBuf,
    manifest: Vec<u8>,
    identity: Value,
    dependencies: BTreeMap<String, String>,
    sources: BTreeSet<String>,
    report: Value,
    artifact_reports: BTreeMap<String, Value>,
}
impl Fixture {
    fn new(target: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let manifest = fs::read(root.join(tooling_execution::MANIFEST_FILE)).unwrap();
        let parsed: Value = serde_json::from_slice(&manifest).unwrap();
        let mut files = BTreeSet::from([
            tooling_execution::MANIFEST_FILE.to_owned(),
            "native/devtools/mod.rs".into(),
            "docs/testing/sdk-tooling-migration.json".into(),
        ]);
        let mut sources = BTreeSet::from([
            "native/lib.rs".to_owned(),
            "tooling/src/lib.rs".into(),
            "tests/rust/sdk_acceptance.rs".into(),
        ]);
        let mut names = BTreeMap::<String, BTreeSet<String>>::new();
        for field in ["entries", "retained_powershell"] {
            for row in parsed[field].as_array().unwrap() {
                files.insert(row["source"].as_str().unwrap().to_owned());
                if field == "entries" {
                    for mapping in row["named_tests"].as_array().unwrap() {
                        let (file, symbol) = mapping.as_str().unwrap().split_once("::").unwrap();
                        let harness = if file.starts_with("native/") {
                            "native/lib.rs"
                        } else if file.starts_with("tests/rust/sdk_acceptance/") {
                            "tests/rust/sdk_acceptance.rs"
                        } else {
                            file
                        };
                        let applicable = match symbol {
                            "actual_developer_cli_preserves_full_windows_child_exit_status"
                            | "unsigned_update_is_rejected_before_replacing_a_locked_installation" => {
                                target.contains("windows")
                            }
                            "approved_manifest_does_not_authorize_unsigned_text_for_activation" => {
                                target.contains("windows") || target.contains("apple")
                            }
                            _ => file != "native/devtools/process_unix.rs" || !target.contains("windows"),
                        };
                        if applicable {
                            names.entry(harness.into()).or_default().insert(symbol.into());
                        }
                    }
                }
                for field in ["rust", "tests"] {
                    for path in row[field].as_array().into_iter().flatten() {
                        let path = path.as_str().unwrap();
                        files.insert(path.to_owned());
                        if field == "tests"
                            && matches!(Path::new(path).parent(), Some(parent)
                                if parent == Path::new("tests/rust") || parent == Path::new("tooling/tests"))
                        {
                            sources.insert(path.to_owned());
                        }
                    }
                }
            }
        }
        files.extend(sources.iter().cloned());
        let dependencies = files
            .into_iter()
            .map(|path| {
                let digest = sha(&fs::read(root.join(&path)).unwrap());
                (path, digest)
            })
            .collect::<BTreeMap<_, _>>();
        let identity = execution_identity(
            &json!({
                "GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP", "GITHUB_EVENT_NAME":"workflow_dispatch",
                "GITHUB_SHA":"a".repeat(40), "GITHUB_REF":"refs/heads/main",
                "GITHUB_RUN_ID":"7", "GITHUB_RUN_ATTEMPT":"1", "GITHUB_JOB":"native"
            }),
            "built",
            target,
        )
        .unwrap();
        let harnesses = sources
            .iter()
            .map(|path| {
                let passed = names.get(path).cloned().unwrap_or_else(|| {
                    BTreeSet::from(["binding_shape_only".into()])
                });
                let count = passed.len();
                (path.clone(), json!({
                    "started":true, "count":count, "passed":passed, "slow_tests":[],
                    "summary":format!("test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"),
                    "source_sha256":dependencies[path], "executable_sha256":"1".repeat(64),
                    "stdout_sha256":"2".repeat(64), "stderr_sha256":"3".repeat(64),
                    "listing_sha256":"4".repeat(64), "exit_code":0, "cleanup_verified":true
                }))
            })
            .collect::<serde_json::Map<_, _>>();
        let report = json!({"schema_version":1, "component":"native-exact-test-execution",
            "identity":identity, "provenance_sha256":"5".repeat(64), "harnesses":harnesses,
            "required_failed_gates":[], "semantic_parity_claimed":false, "qualification_verified":false});
        let artifact_reports = ["release", "extracted-intermediate"]
            .into_iter()
            .map(|phase| {
                let mut artifact = report.clone();
                artifact["component"] = json!("native-exact-sdk-execution");
                artifact["identity"]["phase"] = json!(phase);
                artifact["tested_binary_sha256"] = json!("6".repeat(64));
                artifact["harnesses"].as_object_mut().unwrap().retain(|path, _| {
                    matches!(path.as_str(), "tests/rust/sdk_acceptance.rs" | "tests/rust/media_acceptance.rs")
                });
                (phase.to_owned(), artifact)
            })
            .collect();
        Self {
            root,
            manifest,
            identity,
            dependencies,
            sources,
            report,
            artifact_reports,
        }
    }
    fn bind(&self) -> std::io::Result<Value> {
        let mut reports = self.artifact_reports.iter()
            .map(|(phase, report)| (phase.clone(), serde_json::to_vec(report).unwrap()))
            .collect::<BTreeMap<_, _>>();
        reports.insert("built".into(), serde_json::to_vec(&self.report).unwrap());
        tooling_execution::bind(
            &self.root,
            &self.manifest,
            &self.identity,
            &"5".repeat(64),
            &self.dependencies,
            &self.sources,
            &reports,
        )
    }
}

#[test]
fn original_tooling_dispositions_bind_current_source_and_named_harnesses_without_retirement() {
    for target in [
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "aarch64-apple-darwin",
    ] {
        let fixture = Fixture::new(target);
        let result = fixture.bind().unwrap();
        assert_eq!(result["mapped_python_js"], 46);
        assert_eq!(result["retained_powershell"].as_array().unwrap().len(), 5);
        assert_eq!(result["entries"].as_array().unwrap().len(), 46);
        assert_eq!(result["manifest_sha256"], sha(&fixture.manifest));
        assert_eq!(result["identity"], fixture.identity);
        assert_eq!(result["current_named_harness_execution_verified"], true);
        assert_eq!(result["scenario_assertions_verified"], false);
        assert_eq!(result["semantic_parity_claimed"], false);
        assert_eq!(result["qualification_verified"], false);
        assert_eq!(result["retirement_authorized"], false);
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
        let wide_exit = "tests/rust/devtool_supervisor.rs::actual_developer_cli_preserves_full_windows_child_exit_status";
        if target.contains("windows") {
            assert_eq!(bounded["named_execution"][unix_mapping]["applicable"], false);
            assert_eq!(
                bounded["named_execution"][wide_exit]["exact_test_name"],
                "actual_developer_cli_preserves_full_windows_child_exit_status"
            );
        } else {
            assert_eq!(bounded["named_execution"][wide_exit]["applicable"], false);
            assert_eq!(bounded["named_execution"][unix_mapping]["harness"], "native/lib.rs");
        }
        let activation = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "scripts/native-activation-acceptance.py")
            .unwrap();
        let platform_signature = "tests/rust/artifact_signatures.rs::approved_manifest_does_not_authorize_unsigned_text_for_activation";
        if target.contains("linux") {
            assert_eq!(activation["named_execution"][platform_signature]["applicable"], false);
        } else {
            assert_eq!(activation["named_execution"][platform_signature]["harness"], "tests/rust/artifact_signatures.rs");
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
            let report = if phase == "built" { &fixture.report } else { &fixture.artifact_reports[phase] };
            assert_eq!(result["phase_reports_sha256"][phase], sha(&serde_json::to_vec(report).unwrap()));
        }
        for entry in result["entries"].as_array().unwrap() {
            let expected = entry["named_execution"].as_object().unwrap().iter()
                .filter(|(_, execution)| matches!(execution["harness"].as_str(),
                    Some("tests/rust/sdk_acceptance.rs" | "tests/rust/media_acceptance.rs")))
                .collect::<BTreeMap<_, _>>();
            let phases = entry["artifact_phase_execution"].as_object().unwrap();
            if expected.is_empty() {
                assert!(phases.is_empty());
            } else {
                assert_eq!(phases.keys().map(String::as_str).collect::<BTreeSet<_>>(),
                    BTreeSet::from(["release", "extracted-intermediate"]));
                for executions in phases.values() {
                    assert_eq!(executions.as_object().unwrap().iter().collect::<BTreeMap<_, _>>(), expected);
                }
            }
            let scenarios = entry["scenario_execution"].as_object().unwrap();
            assert_eq!(scenarios.len(), entry["scenarios"].as_array().unwrap().len());
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
        assert_eq!(mapped[expected]["harness"], "tests/rust/runtime_contracts.rs");
        let helpers = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == "tests/tooling/native_host_fixture_contracts.mjs")
            .unwrap();
        let diagnostic = "tests/rust/sdk_acceptance/contracts.rs::probe_diagnostics_expose_only_fixed_stages_and_nonnegative_numeric_durations";
        assert_eq!(helpers["named_execution"][diagnostic]["harness"], "tests/rust/sdk_acceptance.rs");
        assert_eq!(helpers["named_execution"][diagnostic]["exact_test_name"], "probe_diagnostics_expose_only_fixed_stages_and_nonnegative_numeric_durations");
        assert_eq!(
            mapped[expected]["exact_test_name"],
            "log_runs_are_distinct_redacted_at_all_levels_immutable_and_closed"
        );
    }
}

#[test]
fn sdk_tooling_bindings_require_both_authenticated_artifact_phases_and_exact_names() {
    let original = Fixture::new("x86_64-unknown-linux-gnu");
    let source = "scripts/native-extract-check.py";
    let media = "tests/rust/media_acceptance.rs";
    let mapped_name = "real_sdk_media_kinds_modes_outputs_and_legacy_catalog";
    let result = original.bind().unwrap();
    let entry = result["entries"].as_array().unwrap().iter()
        .find(|entry| entry["source"] == source).unwrap();
    for phase in ["release", "extracted-intermediate"] {
        assert_eq!(entry["artifact_phase_execution"][phase][format!("{media}::{mapped_name}")]["exact_test_name"], mapped_name);
        for mutation in ["missing", "extra", "phase", "attempt", "source", "target",
            "provenance", "source-hash", "binary", "different-binary", "missing-harness", "extra-harness",
            "cleanup", "failed", "missing-name"] {
            let mut fixture = original.clone();
            if mutation == "missing" {
                fixture.artifact_reports.remove(phase);
            } else if mutation == "extra" {
                fixture.artifact_reports.insert("extracted-signed".into(), fixture.artifact_reports[phase].clone());
            } else {
                let report = fixture.artifact_reports.get_mut(phase).unwrap();
                match mutation {
                    "phase" => report["identity"]["phase"] = json!("built"),
                    "attempt" => report["identity"]["run_attempt"] = json!(2),
                    "source" => report["identity"]["source_sha"] = json!("b".repeat(40)),
                    "target" => report["identity"]["target"] = json!("aarch64-unknown-linux-gnu"),
                    "provenance" => report["provenance_sha256"] = json!("7".repeat(64)),
                    "source-hash" => report["harnesses"][media]["source_sha256"] = json!("8".repeat(64)),
                    "binary" => report["tested_binary_sha256"] = Value::Null,
                    "different-binary" => report["tested_binary_sha256"] = json!("9".repeat(64)),
                    "missing-harness" => { report["harnesses"].as_object_mut().unwrap().remove(media); }
                    "extra-harness" => report["harnesses"]["native/lib.rs"] = fixture.report["harnesses"]["native/lib.rs"].clone(),
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
                    .dependencies
                    .insert("scripts/bounded.mjs".into(), "0".repeat(64));
            }
            "replacement" => {
                fixture.dependencies.insert(
                    "native/devtools/supervisor.rs".into(),
                    "0".repeat(64),
                );
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
                harness["summary"] = json!("test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s");
            }
            "unrelated-names" => {
                let harness = &mut fixture.report["harnesses"]["tests/rust/sdk_acceptance.rs"];
                harness["passed"] = json!(["unrelated_passing_test"]);
                harness["count"] = json!(1);
                harness["summary"] = json!("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s");
            }
            "duplicate-mapping" | "unregistered-mapping" | "wrong-mapping-harness" | "empty-mappings" => {
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
                        mappings[0] = json!("tests/rust/config_parity.rs::source_generated_defaults_compact_advanced_and_overrides_match");
                    }
                    _ => *mappings = json!([]),
                }
                fixture.manifest = serde_json::to_vec(&manifest).unwrap();
                fixture.dependencies.insert(
                    tooling_execution::MANIFEST_FILE.into(),
                    sha(&fixture.manifest),
                );
            }
            "missing-scenario-bindings" | "missing-scenario" | "extra-scenario"
            | "duplicate-scenario" | "empty-scenario-tests" | "duplicate-scenario-test"
            | "unknown-scenario-test" | "unused-named-test" => {
                let mut manifest: Value = serde_json::from_slice(&fixture.manifest).unwrap();
                let row = &mut manifest["entries"][0];
                let scenario = row["scenarios"][0].as_str().unwrap().to_owned();
                match mutation {
                    "missing-scenario-bindings" => { row.as_object_mut().unwrap().remove("scenario_tests"); }
                    "missing-scenario" => { row["scenario_tests"].as_object_mut().unwrap().remove(&scenario); }
                    "extra-scenario" => {
                        row["scenario_tests"]["Unrequested scenario"] = row["scenario_tests"][&scenario].clone();
                    }
                    "duplicate-scenario" => {
                        row["scenarios"].as_array_mut().unwrap().push(json!(scenario));
                    }
                    "empty-scenario-tests" => row["scenario_tests"][&scenario] = json!([]),
                    "duplicate-scenario-test" => {
                        let first = row["scenario_tests"][&scenario][0].clone();
                        row["scenario_tests"][&scenario].as_array_mut().unwrap().push(first);
                    }
                    "unknown-scenario-test" => {
                        row["scenario_tests"][&scenario][0] = json!("tooling/tests/ci_contracts.rs::unrelated");
                    }
                    _ => {
                        let first = row["named_tests"][0].clone();
                        for tests in row["scenario_tests"].as_object_mut().unwrap().values_mut() {
                            *tests = json!([first]);
                        }
                    }
                }
                fixture.manifest = serde_json::to_vec(&manifest).unwrap();
                fixture.dependencies.insert(tooling_execution::MANIFEST_FILE.into(), sha(&fixture.manifest));
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
