use numera_task::context::Context;
#[cfg(windows)]
use std::fs;
use std::path::Path;

#[cfg(windows)]
#[test]
fn native_dpapi_fixture_preserves_exact_consumer_shape_and_refuses_overwrite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.ci-work/tooling-tests")
        .join(format!("dpapi-fixture-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join(".ci-work/parity")).unwrap();
    let result = std::panic::catch_unwind(|| {
        let context = Context {
            root: root.canonicalize().unwrap(),
            logger: numera_gen_mcp::logging::Logger::new(&root.join("logs"), "ERROR"),
        };
        numera_task::dpapi_fixture::create(&context).unwrap();
        let file = root.join(".ci-work/parity/current-user-dpapi.txt");
        let config = root.join(".ci-work/parity/config-dpapi.json");
        let bytes = fs::read(&file).unwrap();
        let record = fs::read(&config).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&record).unwrap();
        assert_eq!(
            value["file"],
            file.canonicalize().unwrap().to_str().unwrap()
        );
        assert_eq!(
            value["expected"]["Authorization"],
            "Bearer synthetic-same-user-credential"
        );
        assert!(
            !String::from_utf8(bytes.clone())
                .unwrap()
                .contains("synthetic-same-user-credential")
        );
        assert!(numera_task::dpapi_fixture::create(&context).is_err());
        assert_eq!(fs::read(file).unwrap(), bytes);
        assert_eq!(fs::read(config).unwrap(), record);
    });
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}

#[cfg(not(windows))]
#[test]
fn native_dpapi_fixture_is_not_a_plaintext_fallback_on_other_hosts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let context = Context {
        root: root.to_owned(),
        logger: numera_gen_mcp::logging::Logger::new(
            &root.join(".ci-work/tooling-tests/logs"),
            "ERROR",
        ),
    };
    assert!(numera_task::dpapi_fixture::create(&context).is_err());
}
