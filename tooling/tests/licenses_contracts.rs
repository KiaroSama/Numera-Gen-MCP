use numera_task::{
    context::{Context, sha},
    licenses::preserve_package,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    root: PathBuf,
    package_root: PathBuf,
    output: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        Self::named("fixture", "1.0.0")
    }
    fn named(name: &str, version: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.ci-work/tooling-tests")
            .join(format!("notice {}", uuid::Uuid::new_v4()));
        let package_root = root.join(format!(".ci-cache/cargo/registry/src/fixture-registry/{name}-{version}"));
        fs::create_dir_all(&package_root).unwrap();
        let output = root.join("notices");
        Self {
            root,
            package_root,
            output,
        }
    }
    fn context(&self) -> Context {
        Context {
            root: self.root.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(&self.root.join("logs"), "ERROR"),
        }
    }
    fn package(&self) -> Value {
        json!({"name":"fixture","version":"1.0.0","manifest_path":self.package_root.join("Cargo.toml"),"license":"MIT","license_file":null,"repository":"https://example.invalid/fixture"})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            if std::thread::panicking() {
                eprintln!("Notice fixture cleanup failed: {:?}", error.kind());
            } else {
                panic!("Notice fixture cleanup failed: {:?}", error.kind());
            }
        }
    }
}

#[test]
fn actual_and_declared_nonstandard_notices_preserve_exact_bytes_and_hashes() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let mut package = fixture.package();
    let expected = b"Original copyright and conditions.\n";
    fs::write(fixture.package_root.join("LICENSE-MIT"), expected).unwrap();
    fs::write(
        fixture.package_root.join("terms.txt"),
        [expected.as_slice(), b"extra\n"].concat(),
    )
    .unwrap();
    fs::create_dir(fixture.package_root.join("legal")).unwrap();
    fs::write(fixture.package_root.join("legal/COPYRIGHT"), expected).unwrap();
    package["license_file"] = json!("terms.txt");
    let result = preserve_package(&context, &package, &fixture.output, &json!({})).unwrap();
    assert_eq!(result["notices"], json!(["LICENSE-MIT", "legal/COPYRIGHT", "terms.txt"]));
    for name in result["notices"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let original = fs::read(fixture.package_root.join(name)).unwrap();
        assert_eq!(
            fs::read(fixture.output.join("fixture-1.0.0").join(name)).unwrap(),
            original
        );
        assert_eq!(result["notice_sha256"][name], sha(&original));
    }
}

#[test]
fn unknown_missing_notice_and_escaping_declared_file_fail_closed() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let mut package = fixture.package();
    let error = preserve_package(&context, &package, &fixture.output, &json!({})).unwrap_err();
    assert_eq!(error.to_string(), "Resolved dependency notice missing.");
    assert!(!fixture.output.exists());
    let outside = fixture.root.join("outside.txt");
    fs::write(&outside, b"not a package notice").unwrap();
    package["license_file"] = json!("../../../../outside.txt");
    assert!(preserve_package(&context, &package, &fixture.output, &json!({})).is_err());
    assert!(!fixture.output.exists());
    assert_eq!(fs::read(&outside).unwrap(), b"not a package notice");
    // Also exercise an existing escaping file; a missing relative path alone proves no confinement.
    package["license_file"] = json!(outside);
    let error = preserve_package(&context, &package, &fixture.output, &json!({})).unwrap_err();
    assert_eq!(error.to_string(), "Declared notice escapes package.");
    assert!(!fixture.output.exists());
}

#[test]
fn license_declarations_require_valid_nonempty_strings_before_copying_real_notice_bytes() {
    let fixture = Fixture::new();
    let context = fixture.context();
    fs::write(fixture.package_root.join("LICENSE"), b"Original notice bytes.\n").unwrap();
    for (license, license_file, accepted) in [
        (json!("MIT"), Value::Null, true),
        (Value::Null, json!("LICENSE"), true),
        (json!(""), json!("LICENSE"), true),
        (Value::Null, Value::Null, false),
        (json!(""), Value::Null, false),
        (Value::Null, json!(""), false),
        (json!("  "), Value::Null, false),
        (json!(false), Value::Null, false),
        (json!([]), json!("LICENSE"), false),
        (json!("MIT"), json!(123), false),
        (json!("MIT"), json!(""), false),
    ] {
        let mut package = fixture.package();
        package["license"] = license;
        package["license_file"] = license_file;
        let result = preserve_package(&context, &package, &fixture.output, &json!({}));
        assert_eq!(result.is_ok(), accepted);
        if accepted {
            assert_eq!(fs::read(fixture.output.join("fixture-1.0.0/LICENSE")).unwrap(), b"Original notice bytes.\n");
            fs::remove_dir_all(&fixture.output).unwrap();
        } else {
            assert_eq!(result.unwrap_err().to_string(), "Dependency license declaration missing or invalid.");
            assert!(!fixture.output.exists());
        }
    }
}

#[test]
fn notice_depth_and_aggregate_limits_fail_before_any_destination_write() {
    for case in ["depth", "aggregate"] {
        let fixture = Fixture::new();
        let context = fixture.context();
        if case == "depth" {
            let mut directory = fixture.package_root.canonicalize().unwrap();
            for _ in 0..33 {
                directory.push("d");
                fs::create_dir(&directory).unwrap();
            }
            fs::write(directory.join("LICENSE"), b"Original nested notice.").unwrap();
        } else {
            for index in 0..17 {
                fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(fixture.package_root.join(format!("LICENSE-{index:02}")))
                    .unwrap()
                    .set_len(1024 * 1024)
                    .unwrap();
            }
        }
        let error = preserve_package(&context, &fixture.package(), &fixture.output, &json!({}))
            .unwrap_err();
        assert_eq!(error.to_string(), if case == "depth" {
            "Dependency notice nesting exceeds bound."
        } else {
            "Dependency notice aggregate exceeds bound."
        });
        assert!(!fixture.output.exists(), "{case} must fail before copying any notice");
    }
}

#[test]
fn reviewed_supplement_requires_exact_crate_revision_license_and_notice_hash() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let supplements: Value =
        serde_json::from_slice(&fs::read(project.join("docs/licenses/supplements.json")).unwrap())
            .unwrap();
    let keys = supplements
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from(["alloc-stdlib@0.3.0", "rmcp@3.5.1"])
    );
    for identity in supplements.as_object().unwrap().keys() {
        let (name, version) = identity.split_once('@').unwrap();
        let fixture = Fixture::named(name, version);
        let context = fixture.context();
        let mut supplements = supplements.clone();
        // Synthetic cache bytes test the provenance join, not a real registry download.
        supplements[identity]["crate_sha256"] = json!("131137cac86325f14df8e94bc27dc9961a8cfee82c7c6a12ed56f051cfa7e9f8");
        let supplement = &supplements[identity];
        let archive = fixture.root.join(format!(".ci-cache/cargo/registry/cache/fixture-registry/{name}-{version}.crate"));
        fs::create_dir_all(archive.parent().unwrap()).unwrap();
        fs::write(&archive, b"synthetic crate fixture; never unpacked\n").unwrap();
        assert!(!fixture.package_root.join(".cargo-checksum.json").exists());
        let mut package = fixture.package();
        let original = fs::read(project.join(supplement["file"].as_str().unwrap())).unwrap();
        let copy = fixture.root.join(supplement["file"].as_str().unwrap());
        fs::create_dir_all(copy.parent().unwrap()).unwrap();
        fs::write(&copy, &original).unwrap();
        package["name"] = json!(name);
        package["version"] = json!(version);
        package["license"] = supplement["license"].clone();
        fs::write(
            fixture.package_root.join(".cargo_vcs_info.json"),
            json!({"git":{"sha1":supplement["source_revision"]}}).to_string(),
        )
        .unwrap();
        let result = preserve_package(&context, &package, &fixture.output, &supplements).unwrap();
        assert_eq!(
            fs::read(
                fixture
                    .output
                    .join(format!("{name}-{version}"))
                    .join("LICENSE")
            )
            .unwrap(),
            original
        );
        assert_eq!(
            result["supplement"]["source_revision"],
            supplement["source_revision"]
        );
        assert_eq!(result["notice_sha256"]["LICENSE"], sha(&original));
        assert_eq!(result["supplement"]["crate_sha256"], "131137cac86325f14df8e94bc27dc9961a8cfee82c7c6a12ed56f051cfa7e9f8");
        fs::remove_dir_all(&fixture.output).unwrap();
        for field in ["crate_sha256", "source_revision", "license", "sha256"] {
            let mut changed = supplements.clone();
            changed[identity][field] = json!("changed");
            let error =
                preserve_package(&context, &package, &fixture.output, &changed).unwrap_err();
            assert_eq!(
                error.to_string(),
                if field == "sha256" {
                    "Supplement hash differs."
                } else {
                    "Supplement source identity differs."
                }
            );
            assert!(!fixture.output.exists(), "{identity}/{field} wrote unauthenticated notices");
        }
        for mutation in ["archive-changed", "archive-missing", "archive-linked", "vcs-changed", "vcs-missing", "source-missing", "crate-missing", "layout"] {
            fs::write(&archive, b"synthetic crate fixture; never unpacked\n").unwrap();
            let vcs = fixture.package_root.join(".cargo_vcs_info.json");
            fs::write(&vcs, json!({"git":{"sha1":supplement["source_revision"]}}).to_string()).unwrap();
            let mut changed = supplements.clone();
            let mut package = package.clone();
            match mutation {
                "archive-changed" => fs::write(&archive, b"changed").unwrap(),
                "archive-missing" => fs::remove_file(&archive).unwrap(),
                "archive-linked" => {
                    fs::remove_file(&archive).unwrap();
                    fs::hard_link(&copy, &archive).unwrap();
                }
                "vcs-changed" => fs::write(&vcs, br#"{"git":{"sha1":"changed"}}"#).unwrap(),
                "vcs-missing" => fs::write(&vcs, b"{}").unwrap(),
                "source-missing" => {
                    changed[identity].as_object_mut().unwrap().remove("source_revision");
                    fs::write(&vcs, b"{}").unwrap();
                }
                "crate-missing" => { changed[identity].as_object_mut().unwrap().remove("crate_sha256"); }
                "layout" => {
                    let misplaced = fixture.package_root.parent().unwrap().join("wrong-package");
                    fs::create_dir(&misplaced).unwrap();
                    fs::write(misplaced.join(".cargo_vcs_info.json"), fs::read(&vcs).unwrap()).unwrap();
                    package["manifest_path"] = json!(misplaced.join("Cargo.toml"));
                }
                _ => unreachable!(),
            }
            assert!(preserve_package(&context, &package, &fixture.output, &changed).is_err(), "{identity}/{mutation}");
            assert!(!fixture.output.exists(), "{identity}/{mutation} wrote unauthenticated notices");
            if archive.exists() { fs::remove_file(&archive).unwrap(); }
            assert_eq!(fs::read(&copy).unwrap(), original);
        }
    }
}
