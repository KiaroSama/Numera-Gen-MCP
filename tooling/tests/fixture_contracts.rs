use numera_task::fixtures::{relocate, validate_bundle, verify_materialized, INPUTS, BASELINE, SOURCE};
use numera_task::context::sha;
use std::{collections::BTreeMap, path::Path};
use serde_json::json;

#[test]
fn fixture_provenance_is_checked_before_explicit_path_relocation() {
    let bytes = b"{\"fixture\":true}";
    let mut manifest = json!({"schema_version":1,"source_sha":"f2535577c2e3145da03e2d5c123d7de061656860",
        "baseline":"fc7da25c3b470199949b1b4c67127d77afa0c34a","inputs":{
            "config.json":{"bytes":bytes.len(),"sha256":numera_task::context::sha(bytes)}}});
    validate_bundle(&manifest, "config.json", bytes).expect("independent input");
    assert!(validate_bundle(&manifest,"config.json",b"changed").is_err());
    manifest["source_sha"] = json!("another");
    assert!(validate_bundle(&manifest,"config.json",bytes).is_err());
    let original = json!({"file":"/old/root/.ci-work/parity/config.json", "expected":{"path":"/old/root/state"},"text":"{\"path\":\"/old/root/output\"}","unrelated":"/old/rootish/keep"});
    let result = relocate(&original, "/old/root", "/new/root").expect("explicit relocation");
    assert_eq!(result["file"], "/new/root/.ci-work/parity/config.json");
    assert_eq!(result["expected"]["path"], "/new/root/state");
    assert_eq!(result["text"], "{\"path\":\"/new/root/output\"}");
    assert_eq!(result["unrelated"], "/old/rootish/keep");
    assert_eq!(original["file"], "/old/root/.ci-work/parity/config.json");
}

#[test]
fn cross_platform_materialization_accepts_only_exact_originals_and_explicit_config_path_relocation() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let source = "a".repeat(40);
    for platform in ["windows", "linux", "macos"] {
        let mut payloads = BTreeMap::new(); let mut records = Vec::new();
        for name in INPUTS {
            let directory = if *name == "config.json" { platform } else { "common" };
            let bytes = std::fs::read(project.join(format!("tests/fixtures/oracle/{directory}/{name}"))).unwrap();
            let materialized = if *name == "config.json" {
                let config = numera_task::fixtures::parse_json(&bytes).unwrap();
                let suffix = if platform == "windows" { "\\.ci-work\\parity\\config.json" } else { "/.ci-work/parity/config.json" };
                let old = config["parse"][0]["file"].as_str().unwrap().strip_suffix(suffix).unwrap();
                serde_json::to_vec(&relocate(&config, old, if platform == "windows" { "G:\\isolated fixture" } else { "/isolated fixture" }).unwrap()).unwrap()
            } else { bytes.clone() };
            records.push(json!({"name":name,"original_sha256":sha(&bytes),"materialized_sha256":sha(&materialized)}));
            payloads.insert((*name).to_owned(),materialized);
        }
        let report = json!({"schema":1,"historical_source":SOURCE,"baseline":BASELINE,"current_source":source,
            "platform":platform,"inputs":records,
            "normalization":"Only explicit historical checkout path relocation in config.json; other bytes unchanged.",
            "expectations_generated_by_target":false});
        verify_materialized(project,platform,&source,&report,&payloads).unwrap();
        let mut changed = payloads.clone(); changed.insert("images.json".into(),b"[]".to_vec());
        assert!(verify_materialized(project,platform,&source,&report,&changed).is_err());
        let rehash = |name: &str, changed: &BTreeMap<String, Vec<u8>>| {
            let mut claimed = report.clone();
            let row = claimed["inputs"].as_array_mut().unwrap().iter_mut().find(|row| row["name"] == name).unwrap();
            row["materialized_sha256"] = json!(sha(&changed[name]));
            claimed
        };
        assert!(verify_materialized(project,platform,&source,&rehash("images.json",&changed),&changed).is_err(),
            "self-consistent replacement cannot redefine an original non-config input");
        let mut claimed = report.clone(); claimed["inputs"][0]["original_sha256"] = json!("0".repeat(64));
        assert!(verify_materialized(project,platform,&source,&claimed,&payloads).is_err());
        let mut changed = payloads.clone();
        let mut config = numera_task::fixtures::parse_json(&changed["config.json"]).unwrap();
        config["extra-unapproved-fixture"] = json!(true);
        changed.insert("config.json".into(),serde_json::to_vec(&config).unwrap());
        assert!(verify_materialized(project,platform,&source,&rehash("config.json",&changed),&changed).is_err(),
            "self-consistent config mutation cannot bypass exact permitted relocation");
        let mut changed = report.clone(); changed["inputs"][1] = changed["inputs"][0].clone();
        assert!(verify_materialized(project,platform,&source,&changed,&payloads).is_err());
    }
}

#[test]
fn original_deep_config_golden_remains_parseable_without_serde_default_depth_rejection() {
    let bytes = include_bytes!("../../tests/fixtures/oracle/windows/config.json");
    let parsed = numera_task::fixtures::parse_json(bytes).unwrap();
    assert!(parsed["parse"].as_array().unwrap().iter().any(|row|row["name"] == "nested-unknown-512"));
    for platform in ["windows", "linux", "macos"] {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("tests/fixtures/oracle").join(platform);
        let manifest = numera_task::fixtures::parse_json(&std::fs::read(base.join("provenance.json")).unwrap()).unwrap();
        validate_bundle(&manifest, "config.json", &std::fs::read(base.join("config.json")).unwrap()).unwrap();
    }
}
