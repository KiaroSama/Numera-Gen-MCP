use numera_task::{context::sha, test_execution::execution_identity, tooling_execution};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

// Synthetic reports exercise binding only; they are never hosted execution or scenario evidence.
#[derive(Clone)]
pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Vec<u8>,
    pub(crate) identity: Value,
    pub(crate) dependencies: BTreeMap<String, String>,
    pub(crate) sources: BTreeSet<String>,
    pub(crate) report: Value,
    pub(crate) artifact_reports: BTreeMap<String, Value>,
    pub(crate) originals: BTreeMap<String, Vec<u8>>,
}
impl Fixture {
    pub(crate) fn new(target: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let manifest = fs::read(root.join(tooling_execution::MANIFEST_FILE)).unwrap();
        let parsed: Value = serde_json::from_slice(&manifest).unwrap();
        let context = numera_task::context::Context {
            root: root.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(
                &root.join(".ci-work/tooling-tests/logs"),
                "ERROR",
            ),
        };
        let originals = numera_task::ledger_inputs::tooling_originals(&context, &parsed).unwrap();
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
        for row in parsed["entries"].as_array().unwrap() {
            let path = row["source"].as_str().unwrap();
            if root.join(path).try_exists().unwrap() {
                files.insert(path.to_owned());
            }
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
                    "report_observation_never_becomes_output_progress_or_weakens_idle_cleanup"
                    | "observation_opt_in_preserves_exit_wall_cancellation_and_original_output_limit"
                    | "numeric_observation_receipts_reject_partial_unknown_fields_stale_ids_and_links" => {
                        target.contains("linux")
                    }
                    "abrupt_parent_stop_contains_actual_sdk_and_http_leaf_children" => {
                        !target.contains("windows")
                    }
                    "actual_developer_cli_preserves_full_windows_child_exit_status"
                    | "unsigned_update_is_rejected_before_replacing_a_locked_installation"
                    | "protection_dry_run_preserves_input_and_output_and_enforces_private_file_invariants"
                    | "exact_source_windows_dpapi_script_acl_native_roundtrip_corrupt_and_no_overwrite"
                    | "disposable_identity_recovers_exact_custody_and_refuses_overwrite_or_changed_binding"
                    | "native_protection_cli_whatif_preserves_outside_destination_and_existing_file_without_input"
                    | "private_files_accept_rooted_disk_spellings_without_weakening_rejections"
                    | "native_dpapi_fixture_preserves_exact_consumer_shape_and_refuses_overwrite" => {
                        target.contains("windows")
                    }
                    "identity_custody_refuses_unsupported_hosts_without_creating_files"
                    | "native_dpapi_fixture_is_not_a_plaintext_fallback_on_other_hosts" => {
                        !target.contains("windows")
                    }
                    "approved_manifest_does_not_authorize_unsigned_text_for_activation" => {
                        target.contains("windows") || target.contains("apple")
                    }
                    _ => file != "native/devtools/process_unix.rs" || !target.contains("windows"),
                };
                if applicable {
                    let actual = if file == "tests/rust/dpapi_acceptance.rs" {
                        format!("windows::{symbol}")
                    } else {
                        symbol.into()
                    };
                    names.entry(harness.into()).or_default().insert(actual);
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
                    if (path == "tests/rust/dpapi_acceptance.rs" && !target.contains("windows"))
                        || (path == "tooling/tests/coverage_diagnostics_contracts.rs"
                            && !target.contains("linux"))
                    {
                        BTreeSet::new()
                    } else {
                        BTreeSet::from(["binding_shape_only".into()])
                    }
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
                artifact["harnesses"]
                    .as_object_mut()
                    .unwrap()
                    .retain(|path, _| {
                        matches!(
                            path.as_str(),
                            "tests/rust/sdk_acceptance.rs" | "tests/rust/media_acceptance.rs"
                        )
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
            originals,
        }
    }
    pub(crate) fn bind(&self) -> std::io::Result<Value> {
        let mut reports = self
            .artifact_reports
            .iter()
            .map(|(phase, report)| (phase.clone(), serde_json::to_vec(report).unwrap()))
            .collect::<BTreeMap<_, _>>();
        reports.insert("built".into(), serde_json::to_vec(&self.report).unwrap());
        tooling_execution::bind_with_originals(
            &self.root,
            &self.manifest,
            &self.identity,
            &"5".repeat(64),
            &self.dependencies,
            &self.sources,
            &reports,
            &self.originals,
        )
    }
}
