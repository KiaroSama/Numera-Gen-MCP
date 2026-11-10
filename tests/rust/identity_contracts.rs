mod common;
#[cfg(windows)]
#[path = "identity_contracts/der.rs"]
mod der;

use numera_gen_mcp::{devtools::identity, logging::Logger};
#[cfg(windows)]
use std::{fs, path::Path};

#[test]
fn identity_options_refuse_implicit_creation_unknown_flags_and_unqualified_crypto() {
    for args in [
        vec![],
        vec!["create"],
        vec!["check", "--write-fixtures", "--unknown"],
        vec!["inspect", "--confirm-create"],
        vec!["create", "--confirm-create", "--libcrypto", "relative.dll"],
        vec!["check", "--libcrypto-sha256", "invalid"],
    ] {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert!(identity::Options::parse(&args).is_err());
    }
    assert!(identity::Options::parse(&["inspect".into()]).is_ok());
    let library = std::env::temp_dir()
        .join("not-loaded-identity.dll")
        .to_string_lossy()
        .into_owned();
    let hash = "a".repeat(64);
    let qualified = |action: &str| {
        vec![
            action.into(),
            "--libcrypto".into(),
            library.clone(),
            "--libcrypto-sha256".into(),
            hash.clone(),
        ]
    };
    let mut create = qualified("create");
    create.push("--confirm-create".into());
    assert!(identity::Options::parse(&create).unwrap().confirm_create);
    let check = qualified("check");
    assert!(!identity::Options::parse(&check).unwrap().confirm_create);
    let mut fixtures = check.clone();
    fixtures.push("--write-fixtures".into());
    assert!(identity::Options::parse(&fixtures).unwrap().write_fixtures);
    assert!(
        identity::Options::parse(&["create".into(), "--dry-run".into()])
            .unwrap()
            .dry_run
    );
    for flag in ["--confirm-create", "--dry-run", "--write-fixtures"] {
        assert!(identity::Options::parse(&["inspect".into(), flag.into()]).is_err());
        assert!(
            identity::Options::parse(&["create".into(), "--dry-run".into(), flag.into()]).is_err()
        );
        let mut invalid = check.clone();
        invalid.extend([flag.into(), flag.into()]);
        assert!(identity::Options::parse(&invalid).is_err());
    }
    for flag in ["--libcrypto", "--libcrypto-sha256"] {
        assert!(identity::Options::parse(&["check".into(), flag.into()]).is_err());
        let mut invalid = check.clone();
        invalid.extend([
            flag.into(),
            if flag == "--libcrypto" {
                library.clone()
            } else {
                hash.clone()
            },
        ]);
        assert!(identity::Options::parse(&invalid).is_err());
    }
    for digest in ["".into(), "f".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let mut invalid = check.clone();
        *invalid.last_mut().unwrap() = digest;
        assert!(identity::Options::parse(&invalid).is_err());
    }
    for mut invalid in [
        qualified("inspect"),
        qualified("create"),
        qualified("unknown"),
    ] {
        invalid.push("--write-fixtures".into());
        assert!(identity::Options::parse(&invalid).is_err());
    }
}

#[test]
fn crypto_library_architecture_is_verified_before_native_loading() {
    for machine in [0x8664u16, 0xaa64] {
        let mut bytes = vec![0; 128];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&machine.to_le_bytes());
        bytes[86..88].copy_from_slice(&0x2000u16.to_le_bytes());
        bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
        identity::validate_library_image(&bytes, machine).unwrap();
        assert!(
            identity::validate_library_image(
                &bytes,
                if machine == 0xaa64 { 0x8664 } else { 0xaa64 }
            )
            .is_err()
        );
        assert!(identity::validate_library_image(&bytes[..70], machine).is_err());
        for (offset, changed) in [
            (0, b"NO".as_slice()),
            (60, &u32::MAX.to_le_bytes()),
            (64, b"NOPE".as_slice()),
            (88, &[0u8, 0][..]),
        ] {
            let mut malformed = bytes.clone();
            malformed[offset..offset + changed.len()].copy_from_slice(changed);
            assert!(identity::validate_library_image(&malformed, machine).is_err());
        }
        assert!(identity::validate_library_image(&bytes, 0x14c).is_err());
        bytes[86] = 0;
        bytes[87] = 0;
        assert!(identity::validate_library_image(&bytes, machine).is_err());
    }
}

#[cfg(not(windows))]
#[test]
fn identity_custody_refuses_unsupported_hosts_without_creating_files() {
    let work = common::Workspace::new();
    let logger = Logger::new(&work.0.join("logs"), "ERROR");
    assert!(identity::execute(&work.0, &["inspect".into()], Some(&logger)).is_err());
    assert!(
        identity::execute(
            &work.0,
            &["create".into(), "--dry-run".into()],
            Some(&logger)
        )
        .is_err()
    );
    assert!(
        identity::execute(std::path::Path::new("relative"), &["inspect".into()], None).is_err()
    );
    assert!(identity::execute(&work.0.join("missing"), &["inspect".into()], None).is_err());
    let library = work.0.join("not-loaded.dll").to_string_lossy().into_owned();
    for action in ["create", "check"] {
        let mut args = vec![
            action.into(),
            "--libcrypto".into(),
            library.clone(),
            "--libcrypto-sha256".into(),
            "a".repeat(64),
        ];
        if action == "create" {
            args.push("--confirm-create".into());
        }
        assert!(identity::execute(&work.0, &args, None).is_err());
        assert!(identity::execute(&work.0, &args, Some(&logger)).is_err());
    }
    assert!(!work.0.join(".runtime/signing").exists());
    assert!(!work.0.join("docs/signing").exists());
}

#[cfg(windows)]
fn qualified_arguments(action: &str) -> Vec<String> {
    vec![
        action.into(),
        "--libcrypto".into(),
        std::env::var("NUMERA_IDENTITY_TEST_LIBCRYPTO")
            .expect("Hosted identity tests require the qualified libcrypto 3 DLL"),
        "--libcrypto-sha256".into(),
        std::env::var("NUMERA_IDENTITY_TEST_LIBCRYPTO_SHA256")
            .expect("Hosted identity tests require its independently verified digest"),
    ]
}

#[cfg(windows)]
#[test]
fn disposable_identity_recovers_exact_custody_and_refuses_overwrite_or_changed_binding() {
    use numera_gen_mcp::devtools::private_files;
    let work = common::Workspace::new();
    let root = work.0.canonicalize().unwrap();
    let registry = root.join("secrets.md");
    let original = b"# Disposable registry\r\n- Existing: preserve exact bytes\r\n";
    private_files::write_new(&registry, original).unwrap();
    let logger = Logger::new(&root.join("logs"), "ERROR");
    let inspected = identity::execute(&root, &["inspect".into()], Some(&logger)).unwrap();
    assert_eq!(inspected["registry_owner_only"], true);
    assert_eq!(inspected["private_identity_exists"], false);
    assert_eq!(fs::read(&registry).unwrap(), original);
    let dry =
        identity::execute(&root, &["create".into(), "--dry-run".into()], Some(&logger)).unwrap();
    assert_eq!(dry["dry_run"], true);
    assert_eq!(fs::read(&registry).unwrap(), original);
    assert!(!root.join(".runtime/signing").exists());
    let mut create = qualified_arguments("create");
    create.push("--confirm-create".into());
    let created = identity::execute(&root, &create, Some(&logger)).unwrap();
    assert_eq!(created["private_custody_created"], true);
    assert_eq!(created["os_trust_changed"], false);
    assert_eq!(created["remote_secret_changed"], false);
    let appended = private_files::read_private(&registry, 1048576).unwrap();
    assert!(appended.0.starts_with(original));
    let password =
        private_files::read_private(&root.join(".runtime/signing/publisher-password.txt"), 4095)
            .unwrap();
    assert_eq!(password.0.len(), 64);
    let pfx =
        private_files::read_private(&root.join(".runtime/signing/publisher.pfx"), 32768).unwrap();
    assert!(!pfx.0.is_empty());
    der::assert_pfx_parameters(&pfx.0);
    let checked = identity::execute(&root, &qualified_arguments("check"), Some(&logger)).unwrap();
    for name in [
        "identity_recovered",
        "public_binding",
        "unicode_roundtrip",
        "tamper_rejected",
        "wrong_signer_rejected",
        "local_recovery_verified",
    ] {
        assert_eq!(checked[name], true, "{name}");
    }
    assert_eq!(checked["artifact_signed"], false);
    assert!(identity::execute(&root, &create, Some(&logger)).is_err());
    assert_eq!(
        private_files::read_private(&registry, 1048576).unwrap().0,
        appended.0
    );
    assert_eq!(
        private_files::read_private(&root.join(".runtime/signing/publisher.pfx"), 32768)
            .unwrap()
            .0,
        pfx.0
    );
    let metadata = root.join("docs/signing/publisher.json");
    let bytes = fs::read(&metadata).unwrap();
    let mut wrong: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    wrong["certificate_sha256"] = serde_json::json!("0".repeat(64));
    fs::write(&metadata, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(identity::execute(&root, &qualified_arguments("check"), Some(&logger)).is_err());
    fs::write(metadata, bytes).unwrap();
    assert!(identity::execute(&root, &qualified_arguments("check"), Some(&logger)).is_ok());
    let fixture_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/assets/logo.png");
    fs::create_dir(root.join("docs/assets")).unwrap();
    fs::copy(fixture_source, root.join("docs/assets/logo.png")).unwrap();
    let mut write = qualified_arguments("check");
    write.push("--write-fixtures".into());
    identity::execute(&root, &write, Some(&logger)).unwrap();
    let fixture_path =
        root.join("tests/fixtures/native-signing/approved/x86_64-pc-windows-msvc.json");
    let fixture = fs::read(&fixture_path).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fixture).unwrap();
    assert_eq!(value["version"], "0.1.0");
    assert_eq!(
        value["source_sha"],
        "fc7da25c3b470199949b1b4c67127d77afa0c34a"
    );
    assert_eq!(value["files"][0]["path"], "bin/numera-image-gen-mcp.exe");
    assert!(identity::execute(&root, &write, Some(&logger)).is_err());
    assert_eq!(fs::read(&fixture_path).unwrap(), fixture);
    let mut bad = qualified_arguments("check");
    *bad.last_mut().unwrap() = "0".repeat(64);
    assert!(identity::execute(&root, &bad, Some(&logger)).is_err());
    assert!(!contains_private_output(&root.join("logs"), &password.0));
}

#[cfg(windows)]
fn contains_private_output(directory: &Path, password: &[u8]) -> bool {
    fs::read_dir(directory).unwrap().any(|entry| {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        bytes
            .windows(password.len())
            .any(|window| window == password)
    })
}
