use numera_task::{context::sha, execution_evidence, test_execution::execution_identity};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn identity(phase: &str) -> Value {
    let job = match phase { "coverage" => "native-coverage", "extracted-signed" => "signing", _ => "native" };
    execution_identity(&json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP",
        "GITHUB_EVENT_NAME":"workflow_dispatch","GITHUB_SHA":"a".repeat(40),
        "GITHUB_REF":"refs/heads/main","GITHUB_RUN_ID":"42","GITHUB_RUN_ATTEMPT":"2","GITHUB_JOB":job}),
        phase, "x86_64-unknown-linux-gnu").unwrap()
}
fn fixture(phase: &str) -> (Value, BTreeMap<String, String>, BTreeSet<String>) {
    let sdk = matches!(phase, "release" | "extracted-intermediate" | "extracted-signed");
    let sources = if sdk { vec!["tests/rust/sdk_acceptance.rs", "tests/rust/media_acceptance.rs"] }
        else { vec!["native/lib.rs", "tests/rust/sdk_acceptance.rs"] };
    let dependencies = sources.iter().map(|source| ((*source).to_owned(), sha(source.as_bytes())))
        .chain([("tests/rust/sdk_acceptance/host.rs".into(), sha(b"transitive SDK owner"))]).collect::<BTreeMap<_, _>>();
    let harnesses = sources.iter().map(|source| ((*source).to_owned(), json!({"started":true,"count":1,
        "passed":["module::exact_case"],"slow_tests":[],
        "summary":"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s",
        "source_sha256":dependencies[*source],"executable_sha256":"1".repeat(64),
        "stdout_sha256":"2".repeat(64),"stderr_sha256":"3".repeat(64),"listing_sha256":"4".repeat(64),
        "exit_code":0,"cleanup_verified":true}))).collect::<serde_json::Map<_, _>>();
    let mut report = json!({"schema_version":1,"component":if sdk {"native-exact-sdk-execution"} else {"native-exact-test-execution"},
        "identity":identity(phase),"provenance_sha256":"5".repeat(64),"harnesses":harnesses,
        "required_failed_gates":[],"semantic_parity_claimed":false,"qualification_verified":false});
    if sdk { report["tested_binary_sha256"] = json!("6".repeat(64)); }
    (report, dependencies, sources.into_iter().map(str::to_owned).collect())
}
fn verify(report: &Value, dependencies: &BTreeMap<String, String>, sources: &BTreeSet<String>, phase: &str) -> std::io::Result<Value> {
    execution_evidence::verify_report(&serde_json::to_vec(report).unwrap(), &identity(phase),
        &"5".repeat(64), dependencies, sources)
}

#[test]
fn exact_phase_report_binds_expected_identity_all_harnesses_and_independently_verified_provenance() {
    for phase in ["built", "coverage", "release", "extracted-intermediate", "extracted-signed"] {
        let (report, dependencies, sources) = fixture(phase);
        assert_eq!(verify(&report, &dependencies, &sources, phase).unwrap(), report);
        let mut changed = report.clone(); changed["identity"]["run_attempt"] = json!(1);
        assert!(verify(&changed, &dependencies, &sources, phase).is_err());
        changed = report.clone(); changed["provenance_sha256"] = json!("0".repeat(64));
        assert!(verify(&changed, &dependencies, &sources, phase).is_err());
        let mut missing = sources.clone(); missing.pop_first();
        assert!(verify(&report, &dependencies, &missing, phase).is_err());
        let mut changed_sources = dependencies.clone();
        changed_sources.insert("tests/rust/sdk_acceptance.rs".into(), sha(b"changed assertion"));
        assert!(verify(&report, &changed_sources, &sources, phase).is_err());
        if phase != "built" { assert!(verify(&report, &dependencies, &sources, "built").is_err()); }
    }
}

#[test]
fn missing_failed_filtered_duplicate_or_malformed_execution_never_becomes_a_pass() {
    let (report, dependencies, sources) = fixture("built");
    for mutation in ["schema", "component", "identity", "missing", "extra", "started", "count", "duplicate", "name",
        "summary", "multiline-summary", "filtered", "exit", "cleanup", "hash", "slow", "duplicate-slow", "failed-gate", "parity", "qualified"] {
        let mut changed = report.clone();
        let harness = "tests/rust/sdk_acceptance.rs";
        match mutation {
            "schema" => changed["schema_version"] = json!(2),
            "component" => changed["component"] = json!("native-exact-sdk-execution"),
            "identity" => changed["identity"]["repository"] = json!("other/project"),
            "missing" => { changed["harnesses"].as_object_mut().unwrap().remove(harness); },
            "extra" => changed["harnesses"]["unowned.rs"] = changed["harnesses"][harness].clone(),
            "started" => changed["harnesses"][harness]["started"] = json!(false),
            "count" => changed["harnesses"][harness]["count"] = json!(2),
            "duplicate" => changed["harnesses"][harness]["passed"] = json!(["module::exact_case", "module::exact_case"]),
            "name" => changed["harnesses"][harness]["passed"] = json!(["../unowned"]),
            "summary" => changed["harnesses"][harness]["summary"] = json!("ok"),
            "multiline-summary" => changed["harnesses"][harness]["summary"] = json!(format!("{}\n\nrunning 1 test", report["harnesses"][harness]["summary"].as_str().unwrap())),
            "filtered" => changed["harnesses"][harness]["summary"] = json!("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s"),
            "exit" => changed["harnesses"][harness]["exit_code"] = json!(1),
            "cleanup" => changed["harnesses"][harness]["cleanup_verified"] = json!(false),
            "hash" => { changed["harnesses"][harness].as_object_mut().unwrap().remove("listing_sha256"); },
            "slow" => changed["harnesses"][harness]["slow_tests"] = json!(["unknown"]),
            "duplicate-slow" => changed["harnesses"][harness]["slow_tests"] = json!(["module::exact_case", "module::exact_case"]),
            "failed-gate" => changed["required_failed_gates"] = json!(["format"]),
            "parity" => changed["semantic_parity_claimed"] = json!(true),
            "qualified" => changed["qualification_verified"] = json!(true),
            _ => unreachable!(),
        }
        assert!(verify(&changed, &dependencies, &sources, "built").is_err(), "{mutation}");
    }
    let duplicate = serde_json::to_string(&report).unwrap().replacen("\"schema_version\":1", "\"schema_version\":1,\"schema_version\":1", 1);
    assert!(execution_evidence::verify_report(duplicate.as_bytes(), &identity("built"), &"5".repeat(64), &dependencies, &sources).is_err());
    let mut slow = report.clone(); slow["harnesses"]["tests/rust/sdk_acceptance.rs"]["slow_tests"] = json!(["module::exact_case"]);
    verify(&slow, &dependencies, &sources, "built").unwrap();
}

#[test]
fn sdk_report_requires_both_real_harnesses_and_tested_binary_without_promoting_semantic_parity() {
    let (report, dependencies, sources) = fixture("release");
    for replacement in [Value::Null, json!("short"), json!("A".repeat(64))] {
        let mut changed = report.clone(); changed["tested_binary_sha256"] = replacement;
        assert!(verify(&changed, &dependencies, &sources, "release").is_err());
    }
    let mut wrong = sources.clone(); wrong.remove("tests/rust/media_acceptance.rs");
    let mut changed = report.clone(); changed["harnesses"].as_object_mut().unwrap().remove("tests/rust/media_acceptance.rs");
    assert!(verify(&changed, &dependencies, &wrong, "release").is_err());
    let verified = verify(&report, &dependencies, &sources, "release").unwrap();
    assert_eq!(verified["semantic_parity_claimed"], false);
    assert_eq!(verified["qualification_verified"], false);
}

#[test]
fn named_mapping_uses_exact_harness_and_unique_qualified_symbol_never_a_substring() {
    let (report, dependencies, sources) = fixture("built");
    let report = verify(&report, &dependencies, &sources, "built").unwrap();
    assert_eq!(execution_evidence::named_test(&report, "native/config/env.rs::exact_case").unwrap(),
        json!({"harness":"native/lib.rs","exact_test_name":"module::exact_case"}));
    assert_eq!(execution_evidence::named_test(&report, "tests/rust/sdk_acceptance.rs::exact_case").unwrap()["exact_test_name"], "module::exact_case");
    for mapping in ["tests/rust/missing.rs::exact_case", "tests/rust/sdk_acceptance.rs::case",
        "scripts/retired.mjs::exact_case", "native/lib.rs::", "../outside.rs::exact_case"] {
        assert!(execution_evidence::named_test(&report, mapping).is_err(), "{mapping}");
    }
    let mut ambiguous = report;
    ambiguous["harnesses"]["native/lib.rs"]["passed"] = json!(["one::exact_case", "two::exact_case"]);
    assert!(execution_evidence::named_test(&ambiguous, "native/config/env.rs::exact_case").is_err());
}
