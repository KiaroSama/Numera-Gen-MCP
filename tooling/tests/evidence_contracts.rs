use numera_task::{context::sha, evidence, fixtures::{BASELINE, INPUTS, SOURCE}};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::{Cursor, Write}};

fn fixture() -> (String, BTreeMap<String, String>, Value, BTreeMap<String, Vec<u8>>) {
    let source = "a".repeat(40);
    let dependencies = BTreeMap::from([
        ("Cargo.toml".into(), sha(b"fixture manifest")),
        ("Cargo.lock".into(), sha(b"fixture lock")),
        ("native/main.rs".into(), sha(b"fn main() {}")),
        ("tooling/src/ci.rs".into(), sha(b"fixture task")),
        ("tests/rust/sdk_acceptance/host.rs".into(), sha(b"fixture SDK transport owner")),
        ("tests/fixtures/oracle/common/images.json".into(), sha(b"independent expected fixture")),
    ]);
    let relocation = json!({"schema":1,"current_source":source,"historical_source":SOURCE,
        "baseline":BASELINE,"expectations_generated_by_target":false});
    let payloads = INPUTS.iter().map(|name| ((*name).to_owned(), "{\"fixture\":\"تصویر 日本語\"}\n".as_bytes().to_vec())).collect();
    (source, dependencies, relocation, payloads)
}

fn rewrite(bytes: &[u8], mutate: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) -> Vec<u8> {
    let mut reader = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut entries = BTreeMap::new();
    for index in 0..reader.len() {
        use std::io::Read;
        let mut file = reader.by_index(index).unwrap(); let name = file.name().to_owned();
        let mut data = Vec::new(); file.read_to_end(&mut data).unwrap(); entries.insert(name, data);
    }
    mutate(&mut entries);
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap(); writer.write_all(&data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn exact_allowlisted_original_inputs_bind_current_source_dependencies_and_historical_materialization() {
    let (source, dependencies, relocation, payloads) = fixture();
    let bytes = evidence::bundle(&source, &dependencies, &relocation, &payloads).unwrap();
    let manifest = evidence::verify_bundle(&bytes, &source, &dependencies, &relocation, &payloads).unwrap();
    assert_eq!(manifest["source_sha"], source); assert_eq!(manifest["baseline"], BASELINE);
    assert_eq!(manifest["historical_source"], SOURCE);
    assert_eq!(manifest["expectations_generated_by_target"], false);
    assert_eq!(manifest["inputs"].as_object().unwrap().len(), 15);
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert_eq!(archive.len(), 16);
    for name in INPUTS {
        use std::io::Read;
        let mut data = Vec::new(); archive.by_name(name).unwrap().read_to_end(&mut data).unwrap();
        assert_eq!(data, payloads[*name]); assert_eq!(manifest["inputs"][*name]["sha256"], sha(&data));
    }
    for name in ["current-user-dpapi.txt", "config-dpapi.json", "secrets.md", ".env"] {
        assert!(archive.by_name(name).is_err());
        let mut extra = payloads.clone(); extra.insert(name.into(), b"excluded synthetic private fixture".to_vec());
        assert!(evidence::bundle(&source, &dependencies, &relocation, &extra).is_err());
    }
}

#[test]
fn cross_target_verification_requires_the_requested_os_and_architecture_not_the_collectors_host() {
    let (source, dependencies, relocation, payloads) = fixture();
    let bytes = evidence::bundle(&source, &dependencies, &relocation, &payloads).unwrap();
    let os = std::env::consts::OS; let arch = std::env::consts::ARCH;
    evidence::verify_platform_bundle(&bytes, &source, &dependencies, &relocation, &payloads, os, arch).unwrap();
    let other_os = if os == "windows" { "linux" } else { "windows" };
    let other_arch = if arch == "aarch64" { "x86_64" } else { "aarch64" };
    for (wrong_os, wrong_arch) in [("unsupported", arch), (os, "unsupported"), (other_os, arch), (os, other_arch)] {
        assert!(evidence::verify_platform_bundle(&bytes, &source, &dependencies, &relocation, &payloads, wrong_os, wrong_arch).is_err());
    }
    let cross_target = rewrite(&bytes, |entries| {
        let mut manifest: Value = serde_json::from_slice(&entries["provenance.json"]).unwrap();
        manifest["platform"] = json!(other_os); manifest["machine"] = json!(other_arch);
        entries.insert("provenance.json".into(), serde_json::to_vec(&manifest).unwrap());
    });
    evidence::verify_platform_bundle(&cross_target, &source, &dependencies, &relocation, &payloads, other_os, other_arch).unwrap();
    assert!(evidence::verify_bundle(&cross_target, &source, &dependencies, &relocation, &payloads).is_err());
}

#[test]
fn changed_inputs_dependencies_source_provenance_or_archive_inventory_cannot_pass() {
    let (source, dependencies, relocation, payloads) = fixture();
    let bytes = evidence::bundle(&source, &dependencies, &relocation, &payloads).unwrap();
    let mut changed = payloads.clone(); changed.insert("config.json".into(), b"{\"changed\":true}".to_vec());
    assert!(evidence::verify_bundle(&bytes, &source, &dependencies, &relocation, &changed).is_err());
    for path in ["native/main.rs", "tests/rust/sdk_acceptance/host.rs"] {
        let mut changed_dependencies = dependencies.clone();
        changed_dependencies.insert(path.into(), sha(b"changed code"));
        assert!(evidence::verify_bundle(&bytes, &source, &changed_dependencies, &relocation, &payloads).is_err(), "{path}");
    }
    assert!(evidence::verify_bundle(&bytes, &"b".repeat(40), &dependencies, &relocation, &payloads).is_err());
    for field in ["baseline", "historical_source", "current_source", "expectations_generated_by_target"] {
        let mut changed = relocation.clone(); changed[field] = json!("changed");
        assert!(evidence::bundle(&source, &dependencies, &changed, &payloads).is_err());
    }
    for mutation in ["extra", "missing", "input", "identity", "materialization", "dependencies"] {
        let corrupt = rewrite(&bytes, |entries| {
            match mutation {
                "extra" => { entries.insert("unexpected.txt".into(), b"not an input".to_vec()); },
                "missing" => { entries.remove("config.json"); },
                "input" => { entries.insert("config.json".into(), b"{\"changed\":true}".to_vec()); },
                _ => {
                    let mut manifest: Value = serde_json::from_slice(&entries["provenance.json"]).unwrap();
                    match mutation {
                        "identity" => manifest["source_sha"] = json!("b".repeat(40)),
                        "materialization" => manifest["materialization"]["expectations_generated_by_target"] = json!(true),
                        "dependencies" => manifest["dependencies"]["Cargo.lock"] = json!("0".repeat(64)),
                        _ => unreachable!(),
                    }
                    entries.insert("provenance.json".into(), serde_json::to_vec(&manifest).unwrap());
                }
            }
        });
        assert!(evidence::verify_bundle(&corrupt, &source, &dependencies, &relocation, &payloads).is_err(), "{mutation}");
    }
    let duplicate = rewrite(&bytes, |entries| {
        let manifest = String::from_utf8(entries["provenance.json"].clone()).unwrap()
            .replacen("\"schema_version\": 2", "\"schema_version\": 2, \"schema_version\": 2", 1);
        assert_eq!(manifest.matches("\"schema_version\"").count(), 2);
        entries.insert("provenance.json".into(), manifest.into_bytes());
    });
    assert!(evidence::verify_bundle(&duplicate, &source, &dependencies, &relocation, &payloads).is_err());
    assert!(evidence::verify_bundle(&bytes[..bytes.len()/2], &source, &dependencies, &relocation, &payloads).is_err());
}
