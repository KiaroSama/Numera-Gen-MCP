use numera_task::ledger::{
    historical_mapping_source, mapping_source, validate, validate_execution_binding,
    validate_parity, validate_rows,
};
use serde_json::{Value, json};
use std::path::Path;

fn synthetic_collection() -> Vec<Value> {
    // Validator-only structural input. These rows are never presented as real runtime collection,
    // source bindings or native parity evidence; the fixed470/50 boundary is the public shape seam.
    (0..470).map(|index|json!({
        "id":format!("{index:064x}"),"source":format!("tests/unit/suite-{}.test.ts",index%50),
        "source_sha256":"a".repeat(64),"name":format!("explicit synthetic case {index}"),
        "native":["tests/rust/synthetic.rs::case"],"assertions":[{"native_file":"tests/rust/synthetic.rs","native_lines":[1,1]}],
        "platform_modes":{"win32":"run","linux":"run","darwin":"run"},"status":"verified"
    })).collect()
}

#[test]
fn current_frozen_ledger_has_all470_case_identities_and_all50_suite_shapes() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let directory = project.join("docs/testing/native-cases");
    let index: Value =
        serde_json::from_slice(&std::fs::read(directory.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["baseline"], numera_task::fixtures::BASELINE);
    assert_eq!(index["source_cases"], 470);
    assert_eq!(index["source_suites"], 50);
    let mut rows = Vec::new();
    for file in index["files"].as_array().unwrap() {
        for line in std::fs::read_to_string(directory.join(file.as_str().unwrap()))
            .unwrap()
            .lines()
        {
            rows.push(serde_json::from_str::<Value>(line).unwrap());
        }
    }
    validate_rows(&rows).unwrap();
    assert_eq!(rows.len(), 470);
    assert_eq!(
        rows.iter()
            .map(|r| r["source"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        50
    );
    // Shape validation deliberately makes no acceptance_verified or current_execution claim.
}

#[test]
fn duplicate_missing_case_and_suite_identity_are_rejected() {
    let original = synthetic_collection();
    validate_rows(&original).unwrap();
    let mut duplicate = original.clone();
    duplicate[469] = duplicate[0].clone();
    assert!(validate_rows(&duplicate).is_err());
    let mut missing = original.clone();
    missing.pop();
    assert!(validate_rows(&missing).is_err());
    let mut oversized = original.clone();
    oversized.push(original[0].clone());
    assert!(validate_rows(&oversized).is_err());
    let mut missing_suite = original.clone();
    for row in &mut missing_suite {
        if row["source"] == "tests/unit/suite-49.test.ts" {
            row["source"] = json!("tests/unit/suite-48.test.ts");
        }
    }
    assert!(validate_rows(&missing_suite).is_err());
    for field in ["id", "source", "source_sha256", "name"] {
        let mut missing = original.clone();
        missing[0].as_object_mut().unwrap().remove(field);
        assert!(validate_rows(&missing).is_err(), "missing {field}");
    }
    for field in ["id", "name", "source_sha256"] {
        let mut empty = original.clone();
        empty[0][field] = json!("");
        assert!(validate_rows(&empty).is_err(), "empty {field}");
    }
}

#[test]
fn absent_native_assertions_and_invalid_platform_modes_are_rejected() {
    let original = synthetic_collection();
    validate_rows(&original).unwrap();
    let mut pending = original.clone();
    pending[0]["status"] = json!("pending");
    pending[0]["native"] = json!([]);
    pending[0]["assertions"] = json!([]);
    validate_rows(&pending).unwrap();
    for field in ["native", "assertions"] {
        for invalid in [Value::Null, json!([]), json!({}), json!("not-array")] {
            let mut rows = original.clone();
            rows[0][field] = invalid;
            assert!(validate_rows(&rows).is_err(), "invalid {field}");
        }
    }
    for platform in ["win32", "linux", "darwin"] {
        for invalid in [Value::Null, json!("unknown"), json!(false), json!(1)] {
            let mut rows = original.clone();
            rows[0]["platform_modes"][platform] = invalid;
            assert!(validate_rows(&rows).is_err(), "invalid {platform}");
        }
        for allowed in ["run", "skip", "todo"] {
            let mut rows = original.clone();
            rows[0]["platform_modes"][platform] = json!(allowed);
            validate_rows(&rows).unwrap();
        }
    }
}

use numera_task::context::sha;
use std::{collections::BTreeMap, fs, path::PathBuf};
struct Fixture {
    root: PathBuf,
    source: PathBuf,
    ledger: PathBuf,
    index: Value,
    rows: Vec<Value>,
    collection: Value,
    manifest: Value,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("ledger {}", uuid::Uuid::new_v4()));
        let source = root.join("frozen");
        let ledger = root.join("ledger");
        fs::create_dir_all(source.join("tests/unit")).unwrap();
        fs::create_dir_all(&ledger).unwrap();
        fs::create_dir_all(root.join("tests/rust")).unwrap();
        let frozen = "test('original Unicode تصویر',()=>{ expect(1).toBe(1); });\n// exact immutable synthetic suite\n";
        for suite in 0..50 {
            fs::write(
                source.join(format!("tests/unit/suite-{suite}.test.ts")),
                frozen.as_bytes(),
            )
            .unwrap();
        }
        let native = "#[test]\nfn exact_case() {\n    assert_eq!(1, 1);\n}\n";
        fs::write(root.join("tests/rust/synthetic.rs"), native).unwrap();
        let mut rows = synthetic_collection();
        for (index, row) in rows.iter_mut().enumerate() {
            row["source_sha256"] = json!(sha(frozen.as_bytes()));
            row["location"] = json!({"line":1,"column":1});
            row["native"] = json!(["tests/rust/synthetic.rs::exact_case"]);
            row["assertions"] = json!([{"source_lines":[1,1],"native_file":"tests/rust/synthetic.rs","native_lines":[2,3],"claim":format!("synthetic assertion {index}")}]);
            row["status"] = json!("pending");
        }
        let cases=rows.iter().map(|row|json!({"id":row["id"],"source":row["source"],"source_sha256":row["source_sha256"],"name":row["name"],"location":row["location"],"mode":"run"})).collect::<Vec<_>>();
        let collection = json!({"baseline":numera_task::fixtures::BASELINE,"source_cases":470,"source_suites":50,"test_bodies_executed":false,"platform":"linux","cases":cases});
        let index = json!({"baseline":numera_task::fixtures::BASELINE,"source_cases":470,"source_suites":50,"mapping_complete":false,"files":["unit.jsonl","contract.jsonl","integration.jsonl"]});
        let suites=(0..50).map(|suite|json!({"source":format!("tests/unit/suite-{suite}.test.ts"),"sha256":sha(frozen.as_bytes()),"status":"verified","native":["tests/rust/synthetic.rs::exact_case"],"evidence":"historical fixture only"})).collect::<Vec<_>>();
        let manifest = json!({"baseline":numera_task::fixtures::BASELINE,"baseline_case_count":470,"case_mapping_complete":true,"suites":suites});
        let fixture = Self {
            root,
            source,
            ledger,
            index,
            rows,
            collection,
            manifest,
        };
        fixture.write();
        fixture
    }
    fn write(&self) {
        fs::write(self.ledger.join("index.json"), self.index.to_string()).unwrap();
        let lines = self
            .rows
            .iter()
            .map(|r| r.to_string() + "\n")
            .collect::<String>();
        fs::write(self.ledger.join("unit.jsonl"), lines).unwrap();
        fs::write(self.ledger.join("contract.jsonl"), b"").unwrap();
        fs::write(self.ledger.join("integration.jsonl"), b"").unwrap();
    }
    fn validate(&self) -> std::io::Result<Value> {
        validate(
            &self.root,
            &self.source,
            &self.collection,
            &self.ledger,
            false,
        )
    }
    fn verify_rows(&mut self) {
        let hash = sha(&fs::read(self.root.join("tests/rust/synthetic.rs")).unwrap());
        for row in &mut self.rows {
            row["status"] = json!("verified");
            row["gaps"] = json!([]);
            row["review"] = json!({"source_assertions":"equivalent","confirmed_gaps":0});
            row["evidence"] = json!({"sha":"a".repeat(40),"run_id":7,"run_attempt":1,"execution_binding_sha256":"b".repeat(64),"native_sha256":{"tests/rust/synthetic.rs":hash},"passed_targets":["win32","linux","darwin"]});
        }
        self.write();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            if std::thread::panicking() {
                eprintln!("Ledger fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("Ledger fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}

#[test]
fn independent_collection_binds_case_name_location_applicability_and_frozen_source_bytes() {
    let fixture = Fixture::new();
    let report = fixture.validate().unwrap();
    assert_eq!(report["cases"], 470);
    assert_eq!(report["suites"], 50);
    assert_eq!(report["pending"], 470);
    assert_eq!(report["acceptance_verified"], false);
    assert_eq!(report["current_execution_claimed"], false);
    for mutation in [
        "duplicate",
        "missing",
        "name",
        "location",
        "mode",
        "baseline",
        "count",
        "executed",
    ] {
        let mut collection = fixture.collection.clone();
        match mutation {
            "duplicate" => collection["cases"][469] = collection["cases"][0].clone(),
            "missing" => {
                collection["cases"].as_array_mut().unwrap().pop();
            }
            "name" => collection["cases"][0]["name"] = json!("changed assertion identity"),
            "location" => collection["cases"][0]["location"]["line"] = json!(2),
            "mode" => collection["cases"][0]["mode"] = json!("todo"),
            "baseline" => collection["baseline"] = json!("b".repeat(40)),
            "count" => collection["source_cases"] = json!(469),
            "executed" => collection["test_bodies_executed"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            validate(
                &fixture.root,
                &fixture.source,
                &collection,
                &fixture.ledger,
                false
            )
            .is_err(),
            "{mutation}"
        );
    }
    fs::write(
        fixture.source.join("tests/unit/suite-0.test.ts"),
        b"changed immutable source\n",
    )
    .unwrap();
    assert!(
        fixture
            .validate()
            .unwrap_err()
            .to_string()
            .contains("stale")
    );
}

#[test]
fn mapping_cache_is_per_validation_and_never_hides_changed_file_bytes() {
    let mut fixture = Fixture::new();
    fixture.verify_rows();
    fixture.validate().unwrap();
    let native = fixture.root.join("tests/rust/synthetic.rs");
    let original = fs::read(&native).unwrap();
    let mut changed = original;
    changed.push(b'\n');
    fs::write(native, changed).unwrap();
    assert!(
        fixture
            .validate()
            .unwrap_err()
            .to_string()
            .contains("stale")
    );
}

#[test]
fn mapping_requires_a_real_attributed_test_not_helper_comment_literal_or_ignored_function() {
    let fixture = Fixture::new();
    let file = fixture.root.join("tests/rust/synthetic.rs");
    for source in [
        "fn exact_case() {}\n",
        "// #[test]\n// fn exact_case() {}\n",
        "const TEXT:&str=r###\"#[test] fn exact_case() {}\"###;\n",
        "#[test]\n#[ignore]\nfn exact_case() {}\n",
        "#[test]\nfn exact_case() {}\n#[test]\nfn exact_case() {}\n",
        "fn outer(){ #[test] fn exact_case() {} }\n",
        "macro_rules! fake { () => { #[test] fn exact_case() {} }; }\n",
    ] {
        fs::write(&file, source).unwrap();
        assert!(
            mapping_source(&fixture.root, "tests/rust/synthetic.rs", "::", "exact_case").is_err(),
            "{source}"
        );
    }
    for source in [
        "#[test]\nfn exact_case() {}\n",
        "#[tokio::test(flavor=\"multi_thread\",worker_threads=2)]\nasync fn exact_case() {}\n",
        "#[cfg(test)] mod tests { #[test] fn exact_case() {} }\n",
    ] {
        fs::write(&file, source).unwrap();
        mapping_source(&fixture.root, "tests/rust/synthetic.rs", "::", "exact_case").unwrap();
    }
    for (path, separator, symbol) in [
        ("tests/rust/synthetic.rs", "", "exact_case"),
        ("tests/rust/synthetic.rs", "::", "missing"),
        ("tests/rust/synthetic.rs", "::", "exact_case::helper"),
        ("tests/integration/original.test.ts", "::", "exact_case"),
    ] {
        assert!(mapping_source(&fixture.root, path, separator, symbol).is_err());
    }
}

#[test]
fn tooling_mapping_requires_the_same_real_attributed_test_boundary() {
    let fixture = Fixture::new();
    let file = fixture.root.join("tooling/tests/synthetic.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "#[test]\nfn exact_case() {}\n").unwrap();
    mapping_source(
        &fixture.root,
        "tooling/tests/synthetic.rs",
        "::",
        "exact_case",
    )
    .unwrap();
    for source in [
        "fn exact_case() {}\n",
        "// #[test] fn exact_case() {}\n",
        "#[test]\n#[ignore]\nfn exact_case() {}\n",
        "fn outer(){ #[test] fn exact_case() {} }\n",
    ] {
        fs::write(&file, source).unwrap();
        assert!(
            mapping_source(
                &fixture.root,
                "tooling/tests/synthetic.rs",
                "::",
                "exact_case"
            )
            .is_err()
        );
    }
}

#[test]
fn assertion_source_native_ranges_claims_and_evidence_hashes_cannot_silently_drift() {
    let mut fixture = Fixture::new();
    fixture.verify_rows();
    fixture.validate().unwrap();
    let original = fixture.rows[0].clone();
    for (field, value) in [
        ("source_lines", json!([0, 1])),
        ("source_lines", json!([2, 1])),
        ("source_lines", json!([1, 999])),
        ("native_lines", json!([1, 999])),
        ("native_lines", json!([1.5, 2])),
        ("native_lines", json!([true, 2])),
        ("claim", json!("")),
        ("native_file", json!("tests/rust/unmapped.rs")),
    ] {
        fixture.rows[0] = original.clone();
        fixture.rows[0]["assertions"][0][field] = value;
        fixture.write();
        assert!(fixture.validate().is_err(), "{field}");
    }
    fixture.rows[0] = original.clone();
    fixture.rows[0]["evidence"]["native_sha256"]["tests/rust/synthetic.rs"] = json!("0".repeat(64));
    fixture.write();
    assert!(fixture.validate().is_err());
    fixture.rows[0] = original.clone();
    fixture.rows[0]["evidence"]["passed_targets"] = json!(["linux"]);
    fixture.write();
    assert!(fixture.validate().is_err());
    fixture.rows[0] = original;
    fixture.rows[0]["gaps"] = json!(["unverified behavior"]);
    fixture.write();
    assert!(fixture.validate().is_err());
}

#[test]
fn complete_boolean_cannot_bypass_pending_assertions_or_missing_exact_execution_receipts() {
    let mut fixture = Fixture::new();
    fixture.index["mapping_complete"] = json!(true);
    fixture.write();
    assert!(
        validate(
            &fixture.root,
            &fixture.source,
            &fixture.collection,
            &fixture.ledger,
            true
        )
        .unwrap_err()
        .to_string()
        .contains("pending")
    );
    fixture.verify_rows();
    assert!(
        validate(
            &fixture.root,
            &fixture.source,
            &fixture.collection,
            &fixture.ledger,
            true
        )
        .is_err(),
        "booleans and historical metadata do not replace the exact execution receipt"
    );
    let report = fixture.validate().unwrap();
    assert_eq!(report["current_execution_claimed"], false);
    assert_eq!(report["acceptance_verified"], false);
}

#[test]
fn duplicate_metadata_and_escaping_ledger_paths_are_rejected() {
    let mut fixture = Fixture::new();
    let original = fixture.index.clone();
    fs::write(fixture.ledger.join("index.json"),format!("{{\"baseline\":\"{}\",\"baseline\":\"{}\",\"source_cases\":470,\"source_suites\":50,\"files\":[\"unit.jsonl\",\"contract.jsonl\",\"integration.jsonl\"]}}",numera_task::fixtures::BASELINE,numera_task::fixtures::BASELINE)).unwrap();
    assert!(fixture.validate().is_err());
    fixture.index = original;
    fixture.index["files"] = json!(["../outside.jsonl", "contract.jsonl", "integration.jsonl"]);
    fixture.write();
    assert!(fixture.validate().is_err());
    fixture.index["files"] = json!(["unit.jsonl", "unit.jsonl", "integration.jsonl"]);
    fixture.write();
    assert!(fixture.validate().is_err());
}

#[test]
fn every_suite_is_bound_to_immutable_source_and_registered_native_tests() {
    let fixture = Fixture::new();
    validate_parity(
        &fixture.root,
        &fixture.source,
        &fixture.collection,
        &fixture.ledger,
        &fixture.manifest,
        false,
    )
    .unwrap();
    for mutation in [
        "missing",
        "duplicate",
        "hash",
        "status",
        "evidence",
        "case-total",
    ] {
        let mut manifest = fixture.manifest.clone();
        match mutation {
            "missing" => {
                manifest["suites"].as_array_mut().unwrap().pop();
            }
            "duplicate" => manifest["suites"][49] = manifest["suites"][0].clone(),
            "hash" => manifest["suites"][0]["sha256"] = json!("0".repeat(64)),
            "status" => manifest["suites"][0]["status"] = json!("pending"),
            "evidence" => manifest["suites"][0]["evidence"] = Value::Null,
            "case-total" => manifest["baseline_case_count"] = json!(469),
            _ => unreachable!(),
        };
        assert!(
            validate_parity(
                &fixture.root,
                &fixture.source,
                &fixture.collection,
                &fixture.ledger,
                &manifest,
                false
            )
            .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn historical_sdk_whitelist_does_not_claim_current_rust_execution() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root.join("scripts")).unwrap();
    let registrations = [
        ("native-tools-acceptance.mjs", "toolsAcceptance"),
        ("native-two-host-acceptance.mjs", "twoHostAcceptance"),
        ("native-compact-acceptance.mjs", "compactAcceptance"),
        ("native-env-acceptance.mjs", "envAcceptance"),
        ("native-negotiation-acceptance.mjs", "negotiationAcceptance"),
        (
            "native-config-reload-acceptance.mjs",
            "configReloadAcceptance",
        ),
        ("native-terminal-acceptance.mjs", "terminalAcceptance"),
        ("native-races-acceptance.mjs", "racesAcceptance"),
        ("native-lifecycle-acceptance.mjs", "lifecycleAcceptance"),
        ("native-transport-acceptance.mjs", "transportAcceptance"),
    ];
    let mut caller = String::new();
    for (file, symbol) in registrations {
        fs::write(
            fixture.root.join("scripts").join(file),
            format!("export async function {symbol}(context) {{}}\n"),
        )
        .unwrap();
        caller.push_str(&format!(
            "import {{ {symbol} }} from \"./{file}\";\nawait {symbol}({{}});\n"
        ));
    }
    fs::write(fixture.root.join("scripts/native-interop.mjs"), caller).unwrap();
    let path = "scripts/native-tools-acceptance.mjs";
    for file in ["native-ci.py", "native-extract-check.py"] {
        fs::write(
            fixture.root.join("scripts").join(file),
            "scripts/native-interop.mjs\n",
        )
        .unwrap();
    }
    for (file, symbol) in registrations {
        let path = format!("scripts/{file}");
        historical_mapping_source(&fixture.root, &path, "::", symbol).unwrap();
        assert!(mapping_source(&fixture.root, &path, "::", symbol).is_err());
    }
    for (path, separator, symbol) in [
        (path, "::", "missing"),
        (
            "scripts/native-compact-acceptance.mjs",
            "::",
            "twoHostAcceptance",
        ),
        ("scripts/native-interop.mjs", "::", "tool"),
        ("tests/integration/tools.test.ts", "::", "toolsAcceptance"),
        (path, "", "toolsAcceptance"),
    ] {
        assert!(historical_mapping_source(&fixture.root, path, separator, symbol).is_err());
    }
    fs::write(
        fixture.root.join("scripts/native-interop.mjs"),
        "// no actual import or execution\n",
    )
    .unwrap();
    assert!(historical_mapping_source(&fixture.root, path, "::", "toolsAcceptance").is_err());
}

#[test]
fn historical_unicode_assertion_digest_matches_the_retained_independent_binder() {
    let assertions = json!([{"source_lines":[15,35],"native_file":"tests/rust/normalization_acceptance.rs","native_lines":[39,91],"claim":"Exact Persian prompt تصویر 123, family/image model and chat-model orchestration with no size/reference input; six distinct operations, required body field, UTF-8 prompt and complete frozen source JSON body/prefix match. Named normalization-acceptance.wires row."}]);
    assert_eq!(
        numera_task::ledger::assertion_digest(&assertions).unwrap(),
        "ca858ac3a1c7debefd32685927be0c8014e9dbe91c14b220491a5502f295ebe7"
    );
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        other => other.clone(),
    }
}
fn binding(fixture: &Fixture) -> Value {
    let revision = "a".repeat(40);
    let hash = sha(&fs::read(fixture.root.join("tests/rust/synthetic.rs")).unwrap());
    let mapping = "tests/rust/synthetic.rs::exact_case";
    let frozen = fixture
        .rows
        .iter()
        .map(|r| {
            (
                r["source"].as_str().unwrap().to_owned(),
                r["source_sha256"].clone(),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let mut targets = serde_json::Map::new();
    for (label, platform, triple) in [
        ("linux-x64", "linux", "x86_64-unknown-linux-gnu"),
        ("linux-arm", "linux", "aarch64-unknown-linux-gnu"),
        ("windows-x64", "win32", "x86_64-pc-windows-msvc"),
        ("windows-arm", "win32", "aarch64-pc-windows-msvc"),
        ("mac-x64", "darwin", "x86_64-apple-darwin"),
        ("mac-arm", "darwin", "aarch64-apple-darwin"),
        ("coverage", "linux", "coverage"),
    ] {
        targets.insert(label.into(),json!({"platform":platform,"target":triple,"artifact_id":1,"download_sha256":"1".repeat(64),"payload_sha256":"2".repeat(64),"provenance_sha256":"3".repeat(64),"log_sha256":"4".repeat(64),"baseline_execution_files":frozen,"native_harnesses":{"tests/rust/synthetic.rs":{"started":true,"passed":["exact_case"],"summary":"ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"}}}));
    }
    let cases=fixture.rows.iter().map(|row|{let named=targets.keys().map(|label|(label.clone(),json!({(mapping):{"harness":"tests/rust/synthetic.rs","exact_test_name":"exact_case"}}))).collect::<serde_json::Map<_,_>>();json!({"id":row["id"],"source":row["source"],"source_sha256":row["source_sha256"],"platform_modes":row["platform_modes"],"native":row["native"],"assertions_sha256":sha(serde_json::to_string(&canonical(&row["assertions"])).unwrap().as_bytes()),"native_sha256":{"tests/rust/synthetic.rs":hash},"named_execution":named})}).collect::<Vec<_>>();
    json!({"source_sha":revision,"baseline":numera_task::fixtures::BASELINE,"run_id":7,"run_attempt":1,"canonical_promotions":0,"native_execution_locally":false,"frozen_git_files":frozen,"current_git_files":{"tests/rust/synthetic.rs":hash},"targets":targets,"cases":cases})
}
fn execution_rows(fixture: &mut Fixture, receipt: &Value) -> (Vec<u8>, String) {
    let bytes = serde_json::to_vec(receipt).unwrap();
    let digest = sha(&bytes);
    for row in &mut fixture.rows {
        row["evidence"]["execution_binding_sha256"] = json!(digest);
    }
    fixture.write();
    (bytes, digest)
}

#[test]
fn exact_execution_receipt_binds_source_assertions_hashes_targets_and_harness_qualified_names() {
    let mut fixture = Fixture::new();
    fixture.verify_rows();
    let receipt = binding(&fixture);
    let (bytes, digest) = execution_rows(&mut fixture, &receipt);
    let report = validate_execution_binding(&fixture.rows, &bytes, &digest).unwrap();
    assert_eq!(report["bound_cases"], 470);
    assert_eq!(report["current_execution_claimed"], false);
    fs::create_dir(fixture.ledger.join("execution")).unwrap();
    fs::write(
        fixture
            .ledger
            .join("execution")
            .join(format!("{digest}.json")),
        &bytes,
    )
    .unwrap();
    fixture.index["mapping_complete"] = json!(true);
    fixture.write();
    let report = validate(
        &fixture.root,
        &fixture.source,
        &fixture.collection,
        &fixture.ledger,
        true,
    )
    .unwrap();
    assert_eq!(report["acceptance_verified"], true);
    assert_eq!(report["current_execution_claimed"], false);
    assert!(validate_execution_binding(&fixture.rows, &bytes, &"0".repeat(64)).is_err());
    for mutation in [
        "run",
        "revision",
        "duplicate-case",
        "missing-case",
        "source",
        "assertion",
        "native-hash",
        "not-started",
        "wrong-harness",
        "wrong-test",
        "missing-pass",
        "target-source",
        "missing-arm",
        "log-hash",
    ] {
        let mut changed = receipt.clone();
        match mutation {
            "run" => changed["run_id"] = json!(8),
            "revision" => changed["source_sha"] = json!("b".repeat(40)),
            "duplicate-case" => changed["cases"][469] = changed["cases"][0].clone(),
            "missing-case" => {
                changed["cases"].as_array_mut().unwrap().pop();
            }
            "source" => changed["cases"][0]["source_sha256"] = json!("0".repeat(64)),
            "assertion" => changed["cases"][0]["assertions_sha256"] = json!("0".repeat(64)),
            "native-hash" => {
                changed["cases"][0]["native_sha256"]["tests/rust/synthetic.rs"] =
                    json!("0".repeat(64))
            }
            "not-started" => {
                changed["targets"]["linux-x64"]["native_harnesses"]["tests/rust/synthetic.rs"]["started"] =
                    json!(false)
            }
            "wrong-harness" => {
                changed["cases"][0]["named_execution"]["linux-x64"]["tests/rust/synthetic.rs::exact_case"]
                    ["harness"] = json!("unrelated.rs")
            }
            "wrong-test" => {
                changed["cases"][0]["named_execution"]["linux-x64"]["tests/rust/synthetic.rs::exact_case"]
                    ["exact_test_name"] = json!("unrelated_test")
            }
            "missing-pass" => {
                changed["targets"]["linux-x64"]["native_harnesses"]["tests/rust/synthetic.rs"]["passed"] =
                    json!([])
            }
            "target-source" => {
                let source = fixture.rows[0]["source"].as_str().unwrap();
                changed["targets"]["linux-x64"]["baseline_execution_files"][source] =
                    json!("0".repeat(64));
            }
            "missing-arm" => {
                changed["targets"]
                    .as_object_mut()
                    .unwrap()
                    .remove("linux-arm");
            }
            "log-hash" => changed["targets"]["linux-x64"]["log_sha256"] = json!("missing"),
            _ => unreachable!(),
        }
        let (bytes, digest) = execution_rows(&mut fixture, &changed);
        assert!(
            validate_execution_binding(&fixture.rows, &bytes, &digest).is_err(),
            "{mutation}"
        );
    }
}

#[path = "ledger_contracts/source_rebinding.rs"]
mod source_rebinding;
