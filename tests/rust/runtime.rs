use numera_image_gen_mcp::compat::value::json;
use numera_image_gen_mcp::{logging::redact, runtime_paths::RuntimePaths};
mod common;

#[test]
fn scratch_is_project_owned_and_escape_is_rejected() {
    let workspace = common::Workspace::new();
    let paths = RuntimePaths::new(&workspace.0).expect("local runtime directories");
    assert!(
        paths
            .temp
            .starts_with(std::fs::canonicalize(&workspace.0).unwrap())
    );
    assert!(
        paths
            .cache
            .starts_with(std::fs::canonicalize(&workspace.0).unwrap())
    );
    assert!(paths.verify(&workspace.0.join("../outside")).is_err());
    assert!(paths.verify(&paths.temp).is_ok());
}

#[test]
fn secrets_are_redacted_without_replacing_safe_metadata() {
    let value = redact(
        &json!({
            "prompt": "private", "Authorization": "Bearer synthetic", "count": 1,
            "nested": {"path": "private", "url": "https://example.org/image?secret=value"}
        }),
        0,
    );
    assert_eq!(value["prompt"], "[REDACTED]");
    assert_eq!(value["Authorization"], "[REDACTED]");
    assert_eq!(value["count"], 1);
    assert_eq!(value["nested"]["path"], "[REDACTED]");
    assert_eq!(value["nested"]["url"], "https://example.org/image");
}
