use numera_task::{
    context::sha,
    fixtures::{BASELINE, SOURCE},
    ledger, ledger_inputs::REFERENCE_SHA256, source_review,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

// Validator shape only: synthetic470 rows are not source or execution evidence.
fn fixture() -> (Value, Value, BTreeMap<String, Vec<u8>>) {
    let file = "tests/rust/example.rs";
    let bytes = b"#[test]\nfn preserved() { assert_eq!(2, 2); }\n".to_vec();
    let assertion = json!({"native_file":file,"native_lines":[1,2],"source_lines":[1,2],"claim":"Original assertion"});
    let digest = ledger::assertion_digest(&json!([assertion])).unwrap();
    let historical = (0..470).map(|n| json!({"id":format!("{n:064x}"),
        "source":format!("tests/unit/suite-{}.test.ts", n % 50),"source_sha256":"b".repeat(64),
        "name":format!("case {n}"),"status":"pending","platform_modes":{"win32":"run","linux":"run","darwin":"run"},
        "native":[format!("{file}::preserved")],"assertions":[assertion]})).collect::<Vec<_>>();
    let rebind = json!({"component":"native-case-source-rebind","schema_version":1,
        "baseline":BASELINE,"historical_source_sha":SOURCE,"historical_reference_sha256":REFERENCE_SHA256,
        "current_source_sha":"a".repeat(40),"original_evidence_rewritten":false,
        "cases":historical.into_iter().map(|row| json!({"historical":row})).collect::<Vec<_>>(),
        "current_source_files":{file:sha(&bytes)},"sdk_assertion_traceability":{"bindings":[]}});
    let review = json!({"schema_version":1,"component":"native-current-assertion-review",
        "baseline":BASELINE,"historical_source_sha":SOURCE,"historical_reference_sha256":REFERENCE_SHA256,
        "reviewer":"primary","scope":"source_assertions_only","current_execution_claimed":false,
        "qualification_verified":false,
        "current_source_sha":"a".repeat(40),
        "historical_assertions_sha256":ledger::assertion_digest(&json!(
            rebind["cases"].as_array().unwrap().iter().map(|v|json!({"id":v["historical"]["id"],"assertions":v["historical"]["assertions"]})).collect::<Vec<_>>()
        )).unwrap(),
        "current_source_files":{file:sha(&bytes)},
        "assertions":[{"historical_assertion_sha256":digest,"disposition":"equivalent",
            "basis":"reviewed_source_continuity","rationale":"Original assertion tokens are preserved; helper execution is separate.",
            "ranges":[{"file":file,"lines":[1,2],"excerpt_sha256":sha(&bytes[..bytes.len()-1])}]}]});
    (rebind, review, BTreeMap::from([(file.into(), bytes)]))
}

#[test]
fn source_review_binds_every_original_obligation_and_current_excerpt_without_claiming_execution() {
    let (rebind, review, files) = fixture();
    let result = source_review::verify(&rebind, &review, &files).unwrap();
    assert_eq!(result["cases"], 470);
    assert_eq!(result["assertion_records"], 470);
    assert_eq!(result["unique_assertions"], 1);
    assert_eq!(result["current_assertion_review_verified"], true);
    assert_eq!(result["current_execution_claimed"], false);
    assert_eq!(result["qualification_verified"], false);
}

#[test]
fn committed_primary_review_covers_the_actual_historical_ledger_and_current_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let (_, rows) = ledger::load(&root.join("docs/testing/native-cases")).unwrap();
    let reference_bytes =
        std::fs::read(root.join(numera_task::ledger_inputs::REFERENCE_FILE)).unwrap();
    assert_eq!(sha(&reference_bytes), REFERENCE_SHA256);
    let reference = serde_json::from_slice(&reference_bytes).unwrap();
    let rebind =
        numera_task::ledger_inputs::current_rebind_map(root, &rows, &reference, &"a".repeat(40))
            .unwrap();
    let review = serde_json::from_slice(
        &std::fs::read(root.join(source_review::REVIEW_FILE)).unwrap(),
    )
    .unwrap();
    let files = rebind["current_source_files"]
        .as_object()
        .unwrap()
        .keys()
        .map(|path| (path.clone(), std::fs::read(root.join(path)).unwrap()))
        .collect();
    let checked = source_review::verify(&rebind, &review, &files).unwrap();
    assert_eq!(checked["cases"], 470);
    assert_eq!(checked["assertion_records"], 606);
    assert_eq!(checked["unique_assertions"], 411);
    assert_eq!(checked["current_execution_claimed"], false);
    assert_eq!(checked["qualification_verified"], false);
}

#[test]
fn content_bound_review_can_omit_a_commit_hash_before_its_own_commit_exists() {
    let (rebind, mut review, files) = fixture();
    review.as_object_mut().unwrap().remove("current_source_sha");
    source_review::verify(&rebind, &review, &files).unwrap();
    review["current_source_sha"] = Value::Null;
    assert!(source_review::verify(&rebind, &review, &files).is_err());
}

#[test]
fn source_review_rejects_missing_extra_duplicate_stale_or_invented_approval() {
    for mutation in ["missing", "extra", "duplicate", "digest", "source", "excerpt", "bounds", "path", "disposition", "basis", "rationale", "reviewer", "execution", "qualification", "original", "reference"] {
        let (mut rebind, mut review, mut files) = fixture();
        match mutation {
            "missing" => review["assertions"] = json!([]),
            "extra" => {
                let mut extra = review["assertions"][0].clone();
                extra["historical_assertion_sha256"] = json!("0".repeat(64));
                review["assertions"].as_array_mut().unwrap().push(extra);
            }
            "duplicate" => {
                let extra = review["assertions"][0].clone();
                review["assertions"].as_array_mut().unwrap().push(extra);
            }
            "digest" => review["historical_assertions_sha256"] = json!("0".repeat(64)),
            "source" => { files.get_mut("tests/rust/example.rs").unwrap().push(b' '); }
            "excerpt" => review["assertions"][0]["ranges"][0]["excerpt_sha256"] = json!("0".repeat(64)),
            "bounds" => review["assertions"][0]["ranges"][0]["lines"] = json!([0,2]),
            "path" => review["assertions"][0]["ranges"][0]["file"] = json!("../outside.rs"),
            "disposition" => review["assertions"][0]["disposition"] = json!("pending"),
            "basis" => review["assertions"][0]["basis"] = json!("tests_exist"),
            "rationale" => review["assertions"][0]["rationale"] = json!(""),
            "reviewer" => review["reviewer"] = json!("independent"),
            "execution" => review["current_execution_claimed"] = json!(true),
            "qualification" => review["qualification_verified"] = json!(true),
            "original" => rebind["cases"][0]["historical"]["assertions"][0]["claim"] = json!("Relabelled"),
            "reference" => review["historical_reference_sha256"] = json!("0".repeat(64)),
            _ => unreachable!(),
        }
        assert!(source_review::verify(&rebind, &review, &files).is_err(), "{mutation}");
    }
}

#[test]
fn a_file_review_binds_the_exact_original_obligations_without_repeating_shared_ranges() {
    let (rebind, mut review, files) = fixture();
    let assertion = &rebind["cases"][0]["historical"]["assertions"][0];
    let mut group = review["assertions"][0].clone();
    group.as_object_mut().unwrap().remove("historical_assertion_sha256");
    group["historical_native_file"] = assertion["native_file"].clone();
    group["historical_assertions_sha256"] = json!(ledger::assertion_digest(&json!([assertion])).unwrap());
    review["file_reviews"] = json!([group]);
    review["assertions"] = json!([]);
    let checked = source_review::verify(&rebind, &review, &files).unwrap();
    assert_eq!(checked["unique_assertions"], 1);
    for mutation in ["digest", "unknown", "duplicate", "missing", "basis", "partial-file"] {
        let mut changed = review.clone();
        match mutation {
            "digest" => changed["file_reviews"][0]["historical_assertions_sha256"] = json!("0".repeat(64)),
            "unknown" => changed["file_reviews"][0]["historical_native_file"] = json!("tests/rust/unrelated.rs"),
            "duplicate" => {
                let extra = changed["file_reviews"][0].clone();
                changed["file_reviews"].as_array_mut().unwrap().push(extra);
            }
            "missing" => changed["file_reviews"] = json!([]),
            "basis" => changed["file_reviews"][0]["basis"] = json!("automatic_token_match"),
            "partial-file" => {
                changed["file_reviews"][0]["ranges"][0]["lines"] = json!([1, 1]);
                changed["file_reviews"][0]["ranges"][0]["excerpt_sha256"] = json!(sha(b"#[test]"));
            }
            _ => unreachable!(),
        }
        assert!(source_review::verify(&rebind, &changed, &files).is_err(), "{mutation}");
    }
}

#[test]
fn source_review_requires_an_exact_current_revision_and_nonempty_case_obligations() {
    for mutation in ["missing-revision", "other-revision", "invalid-revision", "empty-case", "non-rust-original", "unsafe-dependency"] {
        let (mut rebind, mut review, mut files) = fixture();
        match mutation {
            "missing-revision" => {
                rebind.as_object_mut().unwrap().remove("current_source_sha");
                review.as_object_mut().unwrap().remove("current_source_sha");
            }
            "other-revision" => review["current_source_sha"] = json!("b".repeat(40)),
            "invalid-revision" => {
                rebind["current_source_sha"] = json!("not-a-revision");
                review["current_source_sha"] = json!("not-a-revision");
            }
            "empty-case" => rebind["cases"][0]["historical"]["assertions"] = json!([]),
            "non-rust-original" => {
                for case in rebind["cases"].as_array_mut().unwrap() {
                    case["historical"]["assertions"][0]["native_file"] = json!("tests/rust/example.txt");
                }
            }
            "unsafe-dependency" => {
                let bytes = b"unrelated".to_vec();
                files.insert("../outside.rs".into(), bytes.clone());
                rebind["current_source_files"]["../outside.rs"] = json!(sha(&bytes));
                review["current_source_files"]["../outside.rs"] = json!(sha(&bytes));
            }
            _ => unreachable!(),
        }
        if matches!(mutation, "empty-case" | "non-rust-original") {
            review["historical_assertions_sha256"] = json!(ledger::assertion_digest(&json!(
                rebind["cases"].as_array().unwrap().iter().map(|v|json!({"id":v["historical"]["id"],"assertions":v["historical"]["assertions"]})).collect::<Vec<_>>()
            )).unwrap());
        }
        if mutation == "non-rust-original" {
            review["assertions"][0]["historical_assertion_sha256"] = json!(ledger::assertion_digest(
                &json!([rebind["cases"][0]["historical"]["assertions"][0]])
            ).unwrap());
        }
        assert!(source_review::verify(&rebind, &review, &files).is_err(), "{mutation}");
    }
}

#[test]
fn source_review_and_named_execution_must_cover_the_same_original_cases_and_revision() {
    let (rebind, review, files) = fixture();
    let cases = rebind["cases"].as_array().unwrap().iter().map(|case| {
        let row = &case["historical"];
        json!({"id":row["id"],"source":row["source"],"source_sha256":row["source_sha256"],
            "historical_native":row["native"],
            "historical_assertions_sha256":ledger::assertion_digest(&row["assertions"]).unwrap()})
    }).collect::<Vec<_>>();
    let execution = json!({"schema_version":1,"component":"native-current-named-case-execution",
        "identity":{"source_sha":"a".repeat(40)},"baseline":BASELINE,
        "historical_source_sha":SOURCE,"historical_reference_sha256":REFERENCE_SHA256,
        "current_named_execution_verified":true,"semantic_parity_claimed":false,
        "qualification_verified":false,"original_evidence_rewritten":false,"suites":50,"cases":cases});
    let result = source_review::bind_execution(&rebind, &review, &files, &execution).unwrap();
    assert_eq!(result["current_assertion_review_verified"], true);
    assert_eq!(result["current_named_execution_verified"], true);
    assert_eq!(result["semantic_parity_claimed"], false);
    assert_eq!(result["qualification_verified"], false);
    for mutation in ["revision", "missing", "duplicate", "obligation", "source", "mapping", "unverified", "promoted"] {
        let mut changed = execution.clone();
        match mutation {
            "revision" => changed["identity"]["source_sha"] = json!("b".repeat(40)),
            "missing" => { changed["cases"].as_array_mut().unwrap().pop(); }
            "duplicate" => changed["cases"][469] = changed["cases"][0].clone(),
            "obligation" => changed["cases"][0]["historical_assertions_sha256"] = json!("0".repeat(64)),
            "source" => changed["cases"][0]["source_sha256"] = json!("0".repeat(64)),
            "mapping" => changed["cases"][0]["historical_native"] = json!(["tests/rust/unrelated.rs::pass"]),
            "unverified" => changed["current_named_execution_verified"] = json!(false),
            "promoted" => changed["qualification_verified"] = json!(true),
            _ => unreachable!(),
        }
        assert!(source_review::bind_execution(&rebind, &review, &files, &changed).is_err(), "{mutation}");
    }
}

#[test]
fn sdk_review_uses_exact_traceability_ranges_not_an_unrelated_passing_test() {
    let (mut rebind, mut review, mut files) = fixture();
    let old = "scripts/native-tools-acceptance.mjs";
    for case in rebind["cases"].as_array_mut().unwrap() {
        case["historical"]["assertions"][0]["native_file"] = json!(old);
        case["historical"]["native"] = json!([format!("{old}::toolsAcceptance")]);
    }
    let digest = ledger::assertion_digest(&json!([rebind["cases"][0]["historical"]["assertions"][0]])).unwrap();
    review["assertions"][0]["historical_assertion_sha256"] = json!(digest);
    review["assertions"][0]["basis"] = json!("reviewed_sdk_translation");
    review["historical_assertions_sha256"] = json!(ledger::assertion_digest(&json!(
        rebind["cases"].as_array().unwrap().iter().map(|v|json!({"id":v["historical"]["id"],"assertions":v["historical"]["assertions"]})).collect::<Vec<_>>()
    )).unwrap());
    rebind["sdk_assertion_traceability"]["bindings"] = json!([{
        "historical_assertion_sha256":digest,"ranges":review["assertions"][0]["ranges"]
    }]);
    source_review::verify(&rebind, &review, &files).unwrap();
    let other = "tests/rust/unrelated.rs";
    let bytes = b"assert!(true);".to_vec();
    files.insert(other.into(), bytes.clone());
    rebind["current_source_files"][other] = json!(sha(&bytes));
    review["current_source_files"][other] = json!(sha(&bytes));
    review["assertions"][0]["ranges"] = json!([{"file":other,"lines":[1,1],"excerpt_sha256":sha(&bytes)}]);
    assert!(source_review::verify(&rebind, &review, &files).is_err());
}
