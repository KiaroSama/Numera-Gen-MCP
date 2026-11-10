use numera_task::{context::sha, tooling_execution, tooling_review};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn inputs() -> (PathBuf, Value, Value) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let manifest =
        serde_json::from_slice(&fs::read(root.join(tooling_execution::MANIFEST_FILE)).unwrap())
            .unwrap();
    let review =
        serde_json::from_slice(&fs::read(root.join(tooling_review::REVIEW_FILE)).unwrap()).unwrap();
    (root, manifest, review)
}

#[test]
fn committed_tooling_review_binds_all_originals_scenarios_and_current_sources_without_execution() {
    let (root, manifest, review) = inputs();
    let checked = tooling_review::verify(&root, &manifest, &review).unwrap();
    assert_eq!(checked["files"], 51);
    let scenarios = manifest["entries"].as_array().unwrap().iter()
        .map(|entry| entry["scenarios"].as_array().unwrap().len()).sum::<usize>();
    assert!(scenarios > 92, "the original 92 scenarios and real PowerShell coverage remain required");
    assert_eq!(checked["scenarios"], scenarios);
    assert_eq!(checked["source_assertion_review_verified"], true);
    for field in [
        "current_execution_claimed",
        "qualification_verified",
        "retirement_authorized",
    ] {
        assert_eq!(checked[field], false);
    }
    for mutation in [
        "missing",
        "extra",
        "basis",
        "pending",
        "rationale",
        "source",
        "manifest",
        "revision",
        "count",
        "scenarios",
        "reviewer",
        "execution",
        "qualification",
        "retirement",
    ] {
        let mut changed = review.clone();
        let first = manifest["entries"][0]["source"].as_str().unwrap();
        match mutation {
            "missing" => {
                changed["entries"].as_object_mut().unwrap().remove(first);
            }
            "extra" => {
                changed["entries"]["scripts/unrelated.py"] = changed["entries"][first].clone()
            }
            "basis" => changed["entries"][first]["basis"] = json!("automatic_name_match"),
            "pending" => changed["entries"][first]["disposition"] = json!("pending"),
            "rationale" => changed["entries"][first]["rationale"] = json!(" "),
            "source" => changed["source_inventory_sha256"] = json!("0".repeat(64)),
            "manifest" => changed["manifest_sha256"] = json!("0".repeat(64)),
            "revision" => changed["original_revision"] = json!("a".repeat(40)),
            "count" => changed["source_file_count"] = json!(0),
            "scenarios" => changed["scenario_count"] = json!(91),
            "reviewer" => changed["reviewer"] = json!("independent"),
            "execution" => changed["current_execution_claimed"] = json!(true),
            "qualification" => changed["qualification_verified"] = json!(true),
            "retirement" => changed["retirement_authorized"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            tooling_review::verify(&root, &manifest, &changed).is_err(),
            "{mutation}"
        );
    }
    let mut changed = manifest.clone();
    changed["entries"][0]["scenario_tests"] = json!({});
    assert!(tooling_review::verify(&root, &changed, &review).is_err());
}

#[test]
fn tooling_review_never_promotes_missing_or_self_declared_execution() {
    let (root, manifest, review) = inputs();
    let execution = json!({
        "schema_version":1,"component":"native-tooling-harness-execution",
        "manifest_sha256":sha(&fs::read(root.join(tooling_execution::MANIFEST_FILE)).unwrap()),
        "mapped_files":51,"entries":manifest["entries"],
        "current_named_test_execution_verified":true,"declared_scenario_execution_verified":true,
        "sdk_artifact_phase_execution_verified":true,"scenario_assertions_verified":false,
        "semantic_parity_claimed":false,"qualification_verified":false,
        "retirement_authorized":false,"original_evidence_rewritten":false
    });
    tooling_review::bind_execution(&root, &manifest, &review, &execution).unwrap();
    for mutation in ["missing", "self-declared", "count", "legacy-count", "legacy-field", "missing-powershell"] {
        let mut changed = execution.clone();
        match mutation {
            "missing" => changed = json!({}),
            "self-declared" => changed["qualification_verified"] = json!(true),
            "count" => changed["mapped_files"] = json!(46),
            "legacy-count" => changed["mapped_python_js"] = json!(51),
            "legacy-field" => changed["retained_powershell"] = json!([]),
            "missing-powershell" => changed["entries"].as_array_mut().unwrap()
                .retain(|entry| !entry["source"].as_str().unwrap().ends_with(".ps1")),
            _ => unreachable!(),
        }
        assert!(tooling_review::bind_execution(&root, &manifest, &review, &changed).is_err(), "{mutation}");
    }
}
