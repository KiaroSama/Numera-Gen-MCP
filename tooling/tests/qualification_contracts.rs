use numera_task::{context::sha, fixtures::{BASELINE, SOURCE}, ledger_inputs::REFERENCE_SHA256,
    qualification, test_execution::execution_identity};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn identity(phase: &str) -> Value {
    execution_identity(&json!({"GITHUB_REPOSITORY":"KiaroSama/Numera-Gen-MCP","GITHUB_EVENT_NAME":"workflow_dispatch",
        "GITHUB_SHA":"a".repeat(40),"GITHUB_REF":"refs/heads/main","GITHUB_RUN_ID":"7","GITHUB_RUN_ATTEMPT":"1",
        "GITHUB_JOB":if phase == "coverage" {"native-coverage"} else {"native"}}), phase, "x86_64-unknown-linux-gnu").unwrap()
}
// Pure validator shape only: synthetic470/50 rows are never runtime or parity evidence.
struct Fixture {
    rebind: Value,
    dependencies: BTreeMap<String, String>,
    sources: BTreeSet<String>,
    reports: BTreeMap<String, Vec<u8>>,
}
impl Fixture {
    fn new(coverage: bool) -> Self {
        let file = "tests/rust/sdk_acceptance.rs";
        let mapping = format!("{file}::original_case");
        let dependencies = BTreeMap::from([(file.into(), sha(b"SDK entry")),
            ("tests/rust/media_acceptance.rs".into(), sha(b"media entry"))]);
        let sources = dependencies.keys().cloned().collect::<BTreeSet<_>>();
        let cases = (0..470).map(|index| json!({"id":format!("{index:064x}"),
            "historical":{"id":format!("{index:064x}"),"source":format!("tests/unit/suite-{}.test.ts",index%50),
                "source_sha256":"b".repeat(64),"name":format!("case {index}"),"status":"pending",
                "platform_modes":{"win32":"run","linux":"run","darwin":"run"},
                "native":["scripts/native-tools-acceptance.mjs::toolsAcceptance"],"assertions":[]},
            "current":{"status":"pending","assertion_review":"pending","current_execution_claimed":false,
                "proposed_native":[{"historical_mapping":"scripts/native-tools-acceptance.mjs::toolsAcceptance",
                    "proposed_mapping":mapping,"current_sha256":dependencies[file],
                    "assertion_ranges_rebound":false,"assertion_review":"pending","current_execution_claimed":false}]}})).collect::<Vec<_>>();
        let rebind = json!({"schema_version":1,"component":"native-case-source-rebind","baseline":BASELINE,
            "historical_source_sha":SOURCE,"historical_reference_sha256":REFERENCE_SHA256,"current_source_sha":"a".repeat(40),
            "original_evidence_rewritten":false,"current_assertion_review":"pending","current_execution_claimed":false,
            "acceptance_verified":false,"current_source_files":dependencies,"cases":cases});
        let phases = if coverage { vec!["coverage"] } else { vec!["built", "release", "extracted-intermediate"] };
        let reports = phases.into_iter().map(|phase| {
            let harnesses = dependencies.iter().map(|(path,digest)| (path.clone(), json!({"started":true,"count":1,
                "passed":["original_case"],"slow_tests":[],"source_sha256":digest,"executable_sha256":"1".repeat(64),
                "stdout_sha256":"2".repeat(64),"stderr_sha256":"3".repeat(64),"listing_sha256":"4".repeat(64),
                "exit_code":0,"cleanup_verified":true,
                "summary":"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"}))).collect::<serde_json::Map<_, _>>();
            let sdk = !matches!(phase,"built"|"coverage");
            let mut report = json!({"schema_version":1,"component":if sdk {"native-exact-sdk-execution"} else {"native-exact-test-execution"},
                "identity":identity(phase),"provenance_sha256":"5".repeat(64),"harnesses":harnesses,
                "required_failed_gates":[],"semantic_parity_claimed":false,"qualification_verified":false});
            if sdk {report["tested_binary_sha256"] = json!("6".repeat(64));}
            (phase.into(),serde_json::to_vec(&report).unwrap())
        }).collect();
        Self { rebind, dependencies, sources, reports }
    }
    fn bind(&self, coverage: bool) -> std::io::Result<Value> {
        qualification::bind_cases(&self.rebind, &identity(if coverage {"coverage"} else {"built"}),
            &"5".repeat(64), &self.dependencies, &self.sources, &self.reports)
    }
}

#[test]
fn every_original_case_binds_exact_names_in_each_required_context_without_semantic_promotion() {
    for coverage in [false,true] {
        let fixture = Fixture::new(coverage); let original = fixture.rebind.clone();
        let report = fixture.bind(coverage).unwrap();
        assert_eq!(report["cases"].as_array().unwrap().len(),470);
        assert_eq!(report["suites"],50); assert_eq!(report["current_named_execution_verified"],true);
        assert_eq!(report["semantic_parity_claimed"],false); assert_eq!(report["qualification_verified"],false);
        assert_eq!(report["original_evidence_rewritten"],false);
        assert_eq!(fixture.rebind,original);
        let phases = report["cases"][0]["named_execution"].as_object().unwrap();
        assert_eq!(phases.len(),if coverage {1} else {3});
        for phase in phases.values() {
            assert_eq!(phase["tests/rust/sdk_acceptance.rs::original_case"]["exact_test_name"],"original_case");
        }
    }
}

#[test]
fn missing_wrong_or_stale_case_report_or_dependency_cannot_inherit_historical_execution() {
    for mutation in ["missing-phase","other-attempt","missing-name","failed-phase","missing-case","duplicate-case",
        "missing-mapping","duplicate-mapping","wrong-origin","changed-source","stale-hash","approved","reference"] {
        let mut fixture = Fixture::new(false);
        match mutation {
            "missing-phase" => { fixture.reports.remove("extracted-intermediate"); },
            "other-attempt" | "missing-name" | "failed-phase" => {
                let mut report: Value = serde_json::from_slice(&fixture.reports["release"]).unwrap();
                match mutation {
                    "other-attempt" => report["identity"]["run_attempt"] = json!(2),
                    "missing-name" => report["harnesses"]["tests/rust/sdk_acceptance.rs"]["passed"] = json!(["different_case"]),
                    _ => report["required_failed_gates"] = json!(["release-sdk"]),
                }
                fixture.reports.insert("release".into(),serde_json::to_vec(&report).unwrap());
            },
            "missing-case" => {fixture.rebind["cases"].as_array_mut().unwrap().pop();},
            "duplicate-case" => fixture.rebind["cases"][469] = fixture.rebind["cases"][0].clone(),
            "missing-mapping" => fixture.rebind["cases"][0]["current"]["proposed_native"] = json!([]),
            "duplicate-mapping" => {
                let proposal = fixture.rebind["cases"][0]["current"]["proposed_native"][0].clone();
                fixture.rebind["cases"][0]["current"]["proposed_native"].as_array_mut().unwrap().push(proposal);
            },
            "wrong-origin" => fixture.rebind["cases"][0]["current"]["proposed_native"][0]["historical_mapping"] = json!("scripts/unrelated.mjs::test"),
            "changed-source" => { fixture.dependencies.insert("tests/rust/sdk_acceptance.rs".into(),sha(b"changed")); },
            "stale-hash" => fixture.rebind["cases"][0]["current"]["proposed_native"][0]["current_sha256"] = json!("0".repeat(64)),
            "approved" => fixture.rebind["current_assertion_review"] = json!("equivalent"),
            "reference" => fixture.rebind["historical_reference_sha256"] = json!("0".repeat(64)),
            _ => unreachable!(),
        }
        assert!(fixture.bind(false).is_err(),"{mutation}");
    }
}

#[test]
fn two_original_sdk_exports_can_share_one_actual_current_test_without_dropping_either_origin() {
    let mut fixture = Fixture::new(false);
    fixture.rebind["cases"][0]["historical"]["native"].as_array_mut().unwrap()
        .push(json!("scripts/native-two-host-acceptance.mjs::twoHostAcceptance"));
    let mut second = fixture.rebind["cases"][0]["current"]["proposed_native"][0].clone();
    second["historical_mapping"] = json!("scripts/native-two-host-acceptance.mjs::twoHostAcceptance");
    fixture.rebind["cases"][0]["current"]["proposed_native"].as_array_mut().unwrap().push(second);
    let report = fixture.bind(false).unwrap();
    assert_eq!(report["cases"][0]["historical_native"].as_array().unwrap().len(),2);
    assert_eq!(report["cases"][0]["current_native"].as_array().unwrap().len(),1);
    assert_eq!(report["cases"][0]["named_execution"].as_object().unwrap().len(),3);
}

#[test]
fn original_platform_skip_is_explicit_not_faked_as_a_successful_named_run() {
    let mut fixture = Fixture::new(true);
    fixture.rebind["cases"][0]["historical"]["platform_modes"]["linux"] = json!("skip");
    let report = fixture.bind(true).unwrap();
    assert_eq!(report["cases"][0]["platform_mode"],"skip");
    assert!(report["cases"][0]["named_execution"].as_object().unwrap().is_empty());
    assert_eq!(report["cases"][1]["platform_mode"],"run");
}
