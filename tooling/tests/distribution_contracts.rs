use numera_task::{
    archives,
    context::{Context, sha},
    distribution::{authenticate_archive, prefix, validate_entries, write_tree},
};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

fn fixture() -> BTreeMap<String, Vec<u8>> {
    let mut entries = BTreeMap::from([
        (
            "bin/numera-gen-mcp.exe".into(),
            b"synthetic never executed".to_vec(),
        ),
        ("bin/numera-dev.exe".into(), b"synthetic developer".to_vec()),
        (
            "docs/assets/logo.png".into(),
            numera_gen_mcp::build_identity::LOGO.to_vec(),
        ),
    ]);
    let manifest = json!({"version":"0.3.0","target":"x86_64-pc-windows-msvc","source_sha":"a".repeat(40),"publisher":"KiaroSama",
        "binary_sha256":sha(&entries["bin/numera-gen-mcp.exe"]),"files":entries.iter().map(|(name,data)|json!({"path":name,"bytes":data.len(),"sha256":sha(data)})).collect::<Vec<_>>()});
    entries.insert(
        "manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    entries
}
#[test]
fn archive_checksum_identity_and_complete_inventory_precede_staging() {
    let entries = fixture();
    let target = "x86_64-pc-windows-msvc";
    let source = "a".repeat(40);
    let prefix = prefix(target, true).unwrap();
    let name = format!("{prefix}.zip");
    let bytes = archives::encode(&prefix, &entries, true).unwrap();
    let sidecar = format!("{}  {name}\n", sha(&bytes));
    assert_eq!(
        authenticate_archive(&bytes, sidecar.as_bytes(), target, &source, true).unwrap(),
        entries
    );
    for changed in [
        format!("{}  wrong.zip\n", sha(&bytes)),
        format!("{}  {name}\n", "0".repeat(64)),
    ] {
        assert!(authenticate_archive(&bytes, changed.as_bytes(), target, &source, true).is_err());
    }
    assert!(
        authenticate_archive(&bytes, sidecar.as_bytes(), target, &"b".repeat(40), true).is_err()
    );
    let mut signed = entries.clone();
    signed.insert("manifest.sig".into(), vec![0; 384]);
    let signed = archives::encode(&prefix, &signed, true).unwrap();
    assert!(
        authenticate_archive(
            &signed,
            format!("{} {name}", sha(&signed)).as_bytes(),
            target,
            &source,
            true
        )
        .is_err()
    );
    assert!(authenticate_archive(&bytes, sidecar.as_bytes(), target, &source, false).is_err());
    for field in ["version", "target", "publisher", "binary_sha256"] {
        let mut changed = entries.clone();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&changed["manifest.json"]).unwrap();
        manifest[field] = json!("unrelated");
        changed.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let bytes = archives::encode(&prefix, &changed, true).unwrap();
        assert!(
            authenticate_archive(
                &bytes,
                format!("{}  {name}\n", sha(&bytes)).as_bytes(),
                target,
                &source,
                true
            )
            .is_err(),
            "{field}"
        );
    }
    let mut duplicate = entries.clone();
    let text = std::str::from_utf8(&entries["manifest.json"]).unwrap();
    duplicate.insert(
        "manifest.json".into(),
        format!("{{\"publisher\":\"KiaroSama\",{}", &text[1..]).into_bytes(),
    );
    let bytes = archives::encode(&prefix, &duplicate, true).unwrap();
    assert!(
        authenticate_archive(
            &bytes,
            format!("{}  {name}\n", sha(&bytes)).as_bytes(),
            target,
            &source,
            true
        )
        .is_err()
    );
}

#[test]
fn actual_stage_cli_authenticates_before_writes_and_preserves_an_existing_destination() {
    use numera_gen_mcp::devtools::supervisor::{Bounds, run};
    use std::{fs, process::Command, sync::atomic::AtomicBool};

    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.ci-work/tooling-tests")
        .join(format!("stage 日本語 {}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&parent).unwrap();
    let operation = std::panic::catch_unwind(|| {
        let target = "x86_64-pc-windows-msvc";
        let source = "a".repeat(40);
        let archive_prefix = prefix(target, true).unwrap();
        let name = format!("{archive_prefix}.zip");
        let owner = Context {
            root: parent.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(&parent.join("logs"), "ERROR"),
        };
        for case in [
            "valid",
            "checksum",
            "source",
            "publisher",
            "version",
            "binary",
            "private",
            "unlisted",
            "duplicate",
            "existing",
        ] {
            let root = parent.join(case);
            let download = root.join(".ci-work/signing-download");
            fs::create_dir_all(&download).unwrap();
            fs::create_dir(root.join("native")).unwrap();
            fs::write(root.join("Cargo.toml"), b"inert fixture; never compiled").unwrap();
            fs::write(root.join("native/lib.rs"), b"inert fixture; never compiled").unwrap();
            let mut entries = fixture();
            if matches!(case, "source" | "publisher" | "version" | "binary") {
                let mut manifest: serde_json::Value =
                    serde_json::from_slice(&entries["manifest.json"]).unwrap();
                match case {
                    "source" => manifest["source_sha"] = json!("b".repeat(40)),
                    "publisher" => manifest["publisher"] = json!("unrelated"),
                    "version" => manifest["version"] = json!("0.1.0"),
                    _ => manifest["binary_sha256"] = json!("0".repeat(64)),
                }
                entries.insert(
                    "manifest.json".into(),
                    serde_json::to_vec(&manifest).unwrap(),
                );
            }
            if case == "unlisted" {
                entries.insert("docs/extra.txt".into(), b"unlisted".to_vec());
            }
            if case == "duplicate" {
                let manifest = std::str::from_utf8(&entries["manifest.json"]).unwrap();
                entries.insert(
                    "manifest.json".into(),
                    format!("{{\"publisher\":\"KiaroSama\",{}", &manifest[1..]).into_bytes(),
                );
            }
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (path, bytes) in &entries {
                use std::io::Write;
                writer
                    .start_file(
                        format!("{archive_prefix}/{path}"),
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
                writer.write_all(bytes).unwrap();
            }
            if case == "private" {
                use std::io::Write;
                writer
                    .start_file(
                        format!("{archive_prefix}/secrets.md"),
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
                writer.write_all(b"synthetic forbidden member").unwrap();
            }
            let bytes = writer.finish().unwrap().into_inner();
            fs::write(download.join(&name), &bytes).unwrap();
            fs::write(
                download.join(format!("{name}.sha256")),
                format!(
                    "{}  {name}\n",
                    if case == "checksum" {
                        "0".repeat(64)
                    } else {
                        sha(&bytes)
                    }
                ),
            )
            .unwrap();
            let destination = root.join(format!(".ci-work/prepared/{target}"));
            if case == "existing" {
                fs::create_dir_all(&destination).unwrap();
                fs::write(
                    destination.join("retained.txt"),
                    b"unrelated original bytes",
                )
                .unwrap();
            }
            let mut command = Command::new(env!("CARGO_BIN_EXE_numera-task"));
            command
                .env_clear()
                .args(["--root"])
                .arg(root.canonicalize().unwrap())
                .args(["stage", target]);
            for key in ["PATH", "SystemRoot", "WINDIR", "LLVM_PROFILE_FILE"] {
                if let Some(value) = std::env::var_os(key) {
                    command.env(key, value);
                }
            }
            command
                .env("GITHUB_ACTIONS", "true")
                .env("GITHUB_SHA", &source);
            let outcome = run(
                &mut command,
                &owner.root,
                Bounds::new(5000, 2000).unwrap(),
                &AtomicBool::new(false),
                &owner.logger,
            )
            .unwrap();
            assert!(outcome.reason.is_none(), "{case}");
            assert!(outcome.cleanup_verified, "{case}");
            assert_eq!(outcome.code, if case == "valid" { 0 } else { 1 }, "{case}");
            if case == "valid" {
                assert_eq!(
                    numera_task::distribution::read_tree(&destination, target, &source).unwrap(),
                    entries
                );
                let report: serde_json::Value = serde_json::from_slice(&outcome.stdout).unwrap();
                assert_eq!(report["intermediate_authenticated"], true);
                assert_eq!(report["signed"], false);
            } else {
                assert!(outcome.stdout.is_empty(), "{case}");
                let diagnostics = std::str::from_utf8(&outcome.stderr)
                    .unwrap()
                    .lines()
                    .filter(|line| !line.starts_with('{'))
                    .collect::<Vec<_>>();
                assert!(matches!(diagnostics.as_slice(), [line] if line.starts_with("ERROR: ")));
                if case == "existing" {
                    assert_eq!(
                        fs::read(destination.join("retained.txt")).unwrap(),
                        b"unrelated original bytes"
                    );
                    assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);
                } else {
                    assert!(!destination.exists(), "{case} wrote before authentication");
                }
            }
            assert_eq!(fs::read(download.join(&name)).unwrap(), bytes);
            assert!(!root.join(".ci-work/signing").exists());
            assert!(!root.join(".ci-work/release").exists());
            assert!(!root.join("target").exists());
        }
    });
    std::fs::remove_dir_all(&parent).unwrap();
    operation.unwrap();
}

#[test]
fn unsigned_final_archive_is_rejected_before_any_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.ci-work/tooling-tests")
        .join(format!("unsigned final {}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let result = std::panic::catch_unwind(|| {
        let context = Context {
            root: root.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(&root.join("logs"), "ERROR"),
        };
        let error = numera_task::distribution::archive(
            &context,
            &BTreeMap::from([("manifest.json".into(), b"{}".to_vec())]),
            "x86_64-unknown-linux-gnu",
            false,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Unsigned final distribution forbidden.");
        assert!(!root.join(".ci-work/release").exists());
        assert!(!root.join(".ci-work/intermediate").exists());
    });
    std::fs::remove_dir_all(root).unwrap();
    result.unwrap();
}

#[test]
fn complete_inventory_is_authenticated_before_any_staging_write() {
    let original = fixture();
    let target = "x86_64-pc-windows-msvc";
    let source = "a".repeat(40);
    validate_entries(&original, target, &source).unwrap();
    for name in ["secrets.md", "docs/extra.txt", "bin/numera-dev.exe"] {
        let mut changed = original.clone();
        changed.insert(name.into(), b"changed".to_vec());
        assert!(validate_entries(&changed, target, &source).is_err());
    }
    assert!(validate_entries(&original, target, &"b".repeat(40)).is_err());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.ci-work/tooling-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    let prepared = root.join("prepared");
    let result = std::panic::catch_unwind(|| {
        write_tree(&prepared, &original).unwrap();
        assert_eq!(
            numera_task::distribution::read_tree(&prepared, target, &source).unwrap(),
            original
        );
        assert_eq!(
            std::fs::read(prepared.join("bin/numera-dev.exe")).unwrap(),
            original["bin/numera-dev.exe"]
        );
        assert!(write_tree(&prepared, &original).is_err());
        std::fs::write(prepared.join("docs/unlisted.txt"), b"not authenticated").unwrap();
        assert!(numera_task::distribution::read_tree(&prepared, target, &source).is_err());
        std::fs::remove_file(prepared.join("docs/unlisted.txt")).unwrap();
        let excessive = prepared.join("extra");
        std::fs::create_dir(&excessive).unwrap();
        assert!(numera_task::distribution::read_tree(&prepared, target, &source).is_err());
    });
    std::fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
