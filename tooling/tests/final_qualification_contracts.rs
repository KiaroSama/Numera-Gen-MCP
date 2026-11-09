use numera_task::{final_qualification, matrix, test_execution::execution_identity};
use serde_json::{Value, json};
use std::collections::BTreeMap;

// Synthetic authenticated-boundary inputs exercise the final join, never hosted proof.
fn fixture() -> (Value, Value, BTreeMap<String, Value>) {
    let environment = json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP",
        "GITHUB_EVENT_NAME":"workflow_dispatch","GITHUB_SHA":"a".repeat(40),
        "GITHUB_REF":"refs/heads/chore/tooling-media","GITHUB_RUN_ID":"7",
        "GITHUB_RUN_ATTEMPT":"1","GITHUB_JOB":"native"});
    let mut contexts = serde_json::Map::new();
    let mut signed = BTreeMap::new();
    for (label, (target, phase)) in matrix::contexts() {
        let mut original = environment.clone();
        original["GITHUB_JOB"] = json!(if phase == "coverage" {
            "native-coverage"
        } else {
            "native"
        });
        let identity = execution_identity(&original, phase, target).unwrap();
        let names = json!({"tests/rust/sdk_acceptance.rs":["sdk_case"],
            "tests/rust/media_acceptance.rs":["media_case"]});
        let coverage = if phase == "coverage" {
            json!({"report_sha256":"3".repeat(64),
                "metrics":[{"metric":"regions","count":100,"covered":85,"percent":85.0,"floor":85.0},
                    {"metric":"branches","count":100,"covered":80,"percent":80.0,"floor":80.0}]})
        } else {
            Value::Null
        };
        contexts.insert(label.into(), json!({"reviewed_named_execution":{
            "identity":identity,"cases":470,"suites":50,
            "current_assertion_review_verified":true,"current_named_execution_verified":true,
            "source_review":{"cases":470,"assertion_records":606,"unique_assertions":411,
                "current_assertion_review_verified":true,"reviewer":"primary"}},
            "tooling_harness_execution":{"current_named_test_execution_verified":true,
                "sdk_artifact_phase_execution_verified":true,"harnesses":names},
            "reviewed_tooling_execution":{"identity":identity,
                "source_review_sha256":"2".repeat(64),
                "current_tooling_assertions_and_execution_verified":true},
            "coverage_evidence":coverage}));
        if phase == "built" {
            let mut environment = environment.clone();
            environment["GITHUB_REF"] = json!("refs/heads/main");
            environment["GITHUB_RUN_ID"] = json!("8");
            environment["GITHUB_JOB"] = json!("signing");
            let identity = execution_identity(&environment, "extracted-signed", target).unwrap();
            signed.insert(target.into(), json!({"schema_version":1,
                "component":"native-signed-stage-execution","identity":identity,
                "signed_stage_execution_verified":true,"semantic_parity_claimed":false,
                "qualification_verified":false,"retirement_authorized":false,
                "sdk_harnesses":{"tests/rust/sdk_acceptance.rs":{"passed":["sdk_case"]},
                    "tests/rust/media_acceptance.rs":{"passed":["media_case"]}}}));
        }
    }
    let source = json!({"schema_version":1,"component":"native-seven-context-execution",
        "source_sha":"a".repeat(40),"run_id":7,"run_attempt":1,"contexts":contexts,
        "cases":470,"suites":50,"source_review_sha256":"1".repeat(64),
        "tooling_source_review_sha256":"2".repeat(64),
        "current_named_execution_verified":true,"current_assertion_review_verified":true,
        "semantic_parity_claimed":false,"qualification_verified":false,
        "original_evidence_rewritten":false});
    let root = signed["x86_64-unknown-linux-gnu"]["identity"].clone();
    (source, root, signed)
}

#[test]
fn final_join_requires_all_reviewed_native_coverage_and_genuine_signed_stage_contexts() {
    let (source, root, signed) = fixture();
    let result =
        final_qualification::bind(&source, &root, &signed, &"1".repeat(64), &"2".repeat(64)).unwrap();
    assert_eq!(result["qualification_verified"], true);
    assert_eq!(result["reviewer"], "primary");
    assert_eq!(result["independent_review_claimed"], false);
    assert_eq!(result["owner_host_changed"], false);
    assert_eq!(result["retirement_authorized"], false);
    for mutation in [
        "missing-native",
        "missing-signed",
        "runtime-review",
        "tooling-review",
        "review-bytes",
        "source",
        "signature",
        "sdk",
        "coverage",
        "extra-coverage",
    ] {
        let (mut source, root, mut signed) = fixture();
        match mutation {
            "missing-native" => {
                source["contexts"].as_object_mut().unwrap().remove("coverage");
            }
            "missing-signed" => {
                signed.remove("aarch64-apple-darwin");
            }
            "runtime-review" => {
                source["contexts"]["coverage"]["reviewed_named_execution"]["source_review"]["unique_assertions"] = json!(410)
            }
            "tooling-review" => {
                source["contexts"]["aarch64-apple-darwin"]["reviewed_tooling_execution"]["current_tooling_assertions_and_execution_verified"] = json!(false)
            }
            "review-bytes" => source["tooling_source_review_sha256"] = json!("4".repeat(64)),
            "source" => source["source_sha"] = json!("b".repeat(40)),
            "signature" => {
                signed.get_mut("aarch64-apple-darwin").unwrap()["signed_stage_execution_verified"] =
                    json!(false)
            }
            "sdk" => {
                signed.get_mut("aarch64-apple-darwin").unwrap()["sdk_harnesses"]["tests/rust/sdk_acceptance.rs"]["passed"] = json!(["unrelated"])
            }
            "coverage" => {
                source["contexts"]["coverage"]["coverage_evidence"]["metrics"][0]["covered"] = json!(84)
            }
            "extra-coverage" => {
                source["contexts"]["coverage"]["coverage_evidence"]["metrics"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({}));
            }
            _ => unreachable!(),
        }
        assert!(
            final_qualification::bind(&source, &root, &signed, &"1".repeat(64), &"2".repeat(64))
                .is_err(),
            "{mutation}"
        );
    }
}
