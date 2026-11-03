use numera_task::{context::sha, signed_execution, test_execution::execution_identity};
use serde_json::{Value, json};
use std::collections::BTreeMap;

// Synthetic receipts exercise the join only; they are not cryptographic or hosted evidence.
fn fixture() -> (Value, Value, Value, Value, BTreeMap<String, String>) {
    let identity = execution_identity(
        &json!({
            "GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP", "GITHUB_EVENT_NAME":"workflow_dispatch",
            "GITHUB_SHA":"a".repeat(40), "GITHUB_REF":"refs/heads/main", "GITHUB_JOB":"signing",
            "GITHUB_RUN_ID":"7", "GITHUB_RUN_ATTEMPT":"2"
        }),
        "extracted-signed",
        "x86_64-unknown-linux-gnu",
    )
    .unwrap();
    let dependencies: BTreeMap<String, String> = BTreeMap::from([
        (
            "tests/rust/sdk_acceptance.rs".into(),
            sha(b"SDK fixture source"),
        ),
        (
            "tests/rust/media_acceptance.rs".into(),
            sha(b"media fixture source"),
        ),
    ]);
    let harnesses = dependencies.iter().map(|(path, digest)| (path.clone(), json!({
        "started":true, "count":1, "passed":["module::exact_case"], "slow_tests":[],
        "summary":"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s",
        "source_sha256":digest, "executable_sha256":"1".repeat(64),
        "stdout_sha256":"2".repeat(64), "stderr_sha256":"3".repeat(64),
        "listing_sha256":"4".repeat(64), "exit_code":0, "cleanup_verified":true
    }))).collect::<serde_json::Map<_, _>>();
    let sdk = json!({"schema_version":1, "component":"native-exact-sdk-execution",
        "identity":identity, "provenance_sha256":"5".repeat(64), "tested_binary_sha256":"6".repeat(64),
        "harnesses":harnesses, "required_failed_gates":[],
        "semantic_parity_claimed":false, "qualification_verified":false});
    let signature = json!({"schema_version":1, "component":"native-signed-signature-execution",
        "identity":identity, "provenance_sha256":"5".repeat(64), "manifest_sha256":"7".repeat(64),
        "tested_binary_sha256":"6".repeat(64), "positive_negative_verified":true,
        "publisher_inputs_absent":true, "cleanup_verified":true,
        "public_os_trust":false, "semantic_parity_claimed":false, "qualification_verified":false});
    let extraction = json!({"schema_version":1, "component":"native-signed-extraction-execution",
        "identity":identity, "provenance_sha256":"5".repeat(64), "manifest_sha256":"7".repeat(64),
        "tested_binary_sha256":"6".repeat(64), "archive_sha256":"8".repeat(64),
        "sdk_report_sha256":sha(&serde_json::to_vec(&sdk).unwrap()),
        "activation_verified":true, "cleanup_verified":true, "owner_host_changed":false,
        "public_os_trust":false, "semantic_parity_claimed":false, "qualification_verified":false});
    (identity, signature, extraction, sdk, dependencies)
}
fn bind(
    identity: &Value,
    signature: &Value,
    extraction: &Value,
    sdk: &Value,
    dependencies: &BTreeMap<String, String>,
) -> std::io::Result<Value> {
    signed_execution::bind(
        &serde_json::to_vec(signature).unwrap(),
        &serde_json::to_vec(extraction).unwrap(),
        &serde_json::to_vec(sdk).unwrap(),
        identity,
        &"5".repeat(64),
        dependencies,
    )
}

#[test]
fn signed_stages_bind_same_manifest_binary_sdk_and_original_report_bytes_without_retirement() {
    let (identity, signature, extraction, sdk, dependencies) = fixture();
    let result = bind(&identity, &signature, &extraction, &sdk, &dependencies).unwrap();
    assert_eq!(result["identity"], identity);
    assert_eq!(result["manifest_sha256"], "7".repeat(64));
    assert_eq!(result["tested_binary_sha256"], "6".repeat(64));
    assert_eq!(result["archive_sha256"], "8".repeat(64));
    assert_eq!(
        result["signature_report_sha256"],
        sha(&serde_json::to_vec(&signature).unwrap())
    );
    assert_eq!(
        result["extraction_report_sha256"],
        sha(&serde_json::to_vec(&extraction).unwrap())
    );
    assert_eq!(
        result["sdk_report_sha256"],
        sha(&serde_json::to_vec(&sdk).unwrap())
    );
    assert_eq!(result["signed_stage_execution_verified"], true);
    for field in [
        "semantic_parity_claimed",
        "qualification_verified",
        "retirement_authorized",
    ] {
        assert_eq!(result[field], false);
    }
}

#[test]
fn signed_stage_log_receipts_require_one_exact_digest_in_the_successful_native_job() {
    let (identity, signature, extraction, _, _) = fixture();
    let reports = [
        (
            "native-signed-signature-execution",
            sha(&serde_json::to_vec(&signature).unwrap()),
        ),
        (
            "native-signed-extraction-execution",
            sha(&serde_json::to_vec(&extraction).unwrap()),
        ),
    ];
    let logs = reports
        .iter()
        .map(|(component, digest)| {
            format!(
                "2026-11-02T10:00:00Z {}\n",
                json!({"component":component,"identity":identity,"report_sha256":digest})
            )
        })
        .collect::<String>();
    let job = json!({"id":99,"run_id":7,"head_sha":"a".repeat(40),"head_branch":"main",
        "name":"signing-x86_64-unknown-linux-gnu","status":"completed","conclusion":"success"});
    signed_execution::verify_job(&job, logs.as_bytes(), &identity, &reports).unwrap();
    for (key, value) in [
        ("id", json!(0)),
        ("run_id", json!(8)),
        ("head_sha", json!("b".repeat(40))),
        ("head_branch", json!("other")),
        ("name", json!("unrelated")),
        ("status", json!("in_progress")),
        ("conclusion", json!("failure")),
    ] {
        let mut changed = job.clone();
        changed[key] = value;
        assert!(
            signed_execution::verify_job(&changed, logs.as_bytes(), &identity, &reports).is_err(),
            "{key}"
        );
    }
    for logs in [
        String::new(),
        format!("{logs}{logs}"),
        logs.replace(&reports[0].1, &"9".repeat(64)),
        logs.replace("\"run_attempt\":2", "\"run_attempt\":1"),
        logs.replace("2026-11-02T10:00:00Z ", "untrusted-prefix "),
    ] {
        assert!(signed_execution::verify_job(&job, logs.as_bytes(), &identity, &reports).is_err());
    }
}

#[test]
fn signed_matrix_requires_every_native_target_and_the_same_current_execution_source() {
    let (identity, signature, extraction, sdk, dependencies) = fixture();
    let mut contexts = BTreeMap::new();
    for target in numera_task::matrix::contexts()
        .keys()
        .filter(|target| **target != "coverage")
    {
        let mut identity = identity.clone();
        identity["target"] = json!(target);
        identity["platform"] = json!(if target.contains("windows") {
            "win32"
        } else if target.contains("apple") {
            "darwin"
        } else {
            "linux"
        });
        let mut signature = signature.clone();
        let mut extraction = extraction.clone();
        let mut sdk = sdk.clone();
        signature["identity"] = identity.clone();
        extraction["identity"] = identity.clone();
        sdk["identity"] = identity.clone();
        extraction["sdk_report_sha256"] = json!(sha(&serde_json::to_vec(&sdk).unwrap()));
        contexts.insert(
            (*target).to_owned(),
            bind(&identity, &signature, &extraction, &sdk, &dependencies).unwrap(),
        );
    }
    numera_task::signed_matrix::verify_contexts(&contexts, &identity).unwrap();
    for mutation in [
        "missing",
        "extra",
        "attempt",
        "source",
        "reference",
        "target",
        "stage",
        "qualified",
        "retirement",
    ] {
        let mut changed = contexts.clone();
        match mutation {
            "missing" => {
                changed.remove("x86_64-pc-windows-msvc");
            }
            "extra" => {
                changed.insert(
                    "unrelated".into(),
                    contexts["x86_64-pc-windows-msvc"].clone(),
                );
            }
            "attempt" => {
                changed.get_mut("x86_64-pc-windows-msvc").unwrap()["identity"]["run_attempt"] =
                    json!(1)
            }
            "source" => {
                changed.get_mut("x86_64-pc-windows-msvc").unwrap()["identity"]["source_sha"] =
                    json!("b".repeat(40))
            }
            "reference" => {
                changed.get_mut("x86_64-pc-windows-msvc").unwrap()["identity"]["reference"] =
                    json!("refs/heads/other")
            }
            "target" => {
                changed.get_mut("x86_64-pc-windows-msvc").unwrap()["identity"]["target"] =
                    json!("aarch64-pc-windows-msvc")
            }
            "stage" => {
                changed.get_mut("x86_64-pc-windows-msvc").unwrap()["signed_stage_execution_verified"] =
                    json!(false)
            }
            "qualified" => {
                changed.get_mut("x86_64-pc-windows-msvc").unwrap()["qualification_verified"] =
                    json!(true)
            }
            "retirement" => {
                changed.get_mut("x86_64-pc-windows-msvc").unwrap()["retirement_authorized"] =
                    json!(true)
            }
            _ => unreachable!(),
        }
        assert!(
            numera_task::signed_matrix::verify_contexts(&changed, &identity).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn final_signed_collection_waits_for_every_signing_job_without_receiving_private_inputs() {
    use std::{fs, path::Path};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let collector = workflow
        .split_once("  signed-execution-matrix:\n")
        .unwrap()
        .1;
    for required in [
        "needs: [native, native-coverage, execution-matrix, signing]",
        "github.ref == 'refs/heads/main'",
        "inputs.operation == 'qualify'",
        "inputs.operation == 'sign-qualified'",
        "!cancelled()",
        "needs.signing.result == 'success'",
        "actions: read",
        "contents: read",
        "persist-credentials: false",
        "bounded 900000 60000 --exec ./target/debug/numera-task",
        "signed-execution-matrix",
        "signed-execution-matrix.json",
        "if-no-files-found: error",
    ] {
        assert!(collector.contains(required), "{required}");
    }
    for forbidden in [
        "secrets.",
        "environment:",
        "signing-tools",
        " sign '",
        "continue-on-error",
    ] {
        assert!(!collector.contains(forbidden), "{forbidden}");
    }
    assert!(workflow.contains("name: signing-${{ matrix.target }}"));
    for receipt in [
        "signature-execution.json",
        "signed-extraction-execution.json",
    ] {
        assert!(workflow.contains(receipt));
    }
}

#[test]
fn signed_sdk_keeps_every_original_mapped_name_from_the_authenticated_native_context() {
    let (_, signature, extraction, sdk, dependencies) = fixture();
    let identity = &sdk["identity"];
    let record = bind(identity, &signature, &extraction, &sdk, &dependencies).unwrap();
    let harnesses = sdk["harnesses"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, result)| (path.clone(), result["passed"].clone()))
        .collect::<serde_json::Map<_, _>>();
    let original = json!({"tooling_harness_execution":{"current_named_test_execution_verified":true,
        "sdk_artifact_phase_execution_verified":true,"harnesses":harnesses}});
    signed_execution::verify_sdk_names(&original, &record).unwrap();
    for mutation in ["missing", "unrelated", "extra", "unverified"] {
        let mut changed = original.clone();
        let harnesses = &mut changed["tooling_harness_execution"]["harnesses"];
        match mutation {
            "missing" => {
                harnesses
                    .as_object_mut()
                    .unwrap()
                    .remove("tests/rust/media_acceptance.rs");
            }
            "unrelated" => harnesses["tests/rust/sdk_acceptance.rs"] = json!(["unrelated_case"]),
            "extra" => harnesses["tests/rust/sdk_acceptance.rs"]
                .as_array_mut()
                .unwrap()
                .push(json!("missing_signed_case")),
            "unverified" => {
                changed["tooling_harness_execution"]["sdk_artifact_phase_execution_verified"] =
                    json!(false)
            }
            _ => unreachable!(),
        }
        assert!(
            signed_execution::verify_sdk_names(&changed, &record).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn signed_stage_join_rejects_stale_identity_missing_proof_and_self_consistent_wrong_binary() {
    let (identity, signature, extraction, sdk, dependencies) = fixture();
    for stage in ["signature", "extraction"] {
        let mutations: &[&str] = if stage == "signature" {
            &[
                "schema",
                "component",
                "identity",
                "attempt",
                "source",
                "target",
                "provenance",
                "manifest",
                "binary",
                "cleanup",
                "trust",
                "semantic",
                "qualified",
                "missing",
                "stage-proof",
            ]
        } else {
            &[
                "schema",
                "component",
                "identity",
                "attempt",
                "source",
                "target",
                "provenance",
                "manifest",
                "binary",
                "cleanup",
                "trust",
                "semantic",
                "qualified",
                "missing",
                "stage-proof",
                "sdk-digest",
                "archive",
            ]
        };
        for &mutation in mutations {
            let mut signature = signature.clone();
            let mut extraction = extraction.clone();
            let value = if stage == "signature" {
                &mut signature
            } else {
                &mut extraction
            };
            match mutation {
                "schema" => value["schema_version"] = json!(2),
                "component" => value["component"] = json!("unrelated"),
                "identity" => value["identity"]["reference"] = json!("refs/heads/other"),
                "attempt" => value["identity"]["run_attempt"] = json!(1),
                "source" => value["identity"]["source_sha"] = json!("b".repeat(40)),
                "target" => value["identity"]["target"] = json!("aarch64-unknown-linux-gnu"),
                "provenance" => value["provenance_sha256"] = json!("9".repeat(64)),
                "manifest" => value["manifest_sha256"] = json!("9".repeat(64)),
                "binary" => value["tested_binary_sha256"] = json!("9".repeat(64)),
                "cleanup" => value["cleanup_verified"] = json!(false),
                "trust" => value["public_os_trust"] = json!(true),
                "semantic" => value["semantic_parity_claimed"] = json!(true),
                "qualified" => value["qualification_verified"] = json!(true),
                "missing" => {
                    value.as_object_mut().unwrap().remove("manifest_sha256");
                }
                "stage-proof" => {
                    value[if stage == "signature" {
                        "positive_negative_verified"
                    } else {
                        "activation_verified"
                    }] = json!(false)
                }
                "sdk-digest" => value["sdk_report_sha256"] = json!("9".repeat(64)),
                "archive" => value["archive_sha256"] = Value::Null,
                _ => unreachable!(),
            }
            assert!(
                bind(&identity, &signature, &extraction, &sdk, &dependencies).is_err(),
                "{stage}/{mutation}"
            );
        }
    }
    for mutation in [
        "sdk-binary",
        "sdk-attempt",
        "sdk-cleanup",
        "host",
        "same-missing-hash",
        "wrong-phase",
    ] {
        let mut signature = signature.clone();
        let mut extraction = extraction.clone();
        let mut sdk = sdk.clone();
        let mut identity = identity.clone();
        match mutation {
            "sdk-binary" => sdk["tested_binary_sha256"] = json!("9".repeat(64)),
            "sdk-attempt" => sdk["identity"]["run_attempt"] = json!(3),
            "sdk-cleanup" => {
                sdk["harnesses"]["tests/rust/sdk_acceptance.rs"]["cleanup_verified"] = json!(false)
            }
            "host" => extraction["owner_host_changed"] = json!(true),
            "same-missing-hash" => {
                signature["manifest_sha256"] = Value::Null;
                extraction["manifest_sha256"] = Value::Null;
            }
            "wrong-phase" => {
                identity["phase"] = json!("release");
                identity["job"] = json!("native");
                signature["identity"] = identity.clone();
                extraction["identity"] = identity.clone();
                sdk["identity"] = identity.clone();
            }
            _ => unreachable!(),
        }
        extraction["sdk_report_sha256"] = json!(sha(&serde_json::to_vec(&sdk).unwrap()));
        assert!(
            bind(&identity, &signature, &extraction, &sdk, &dependencies).is_err(),
            "{mutation}"
        );
    }
}
