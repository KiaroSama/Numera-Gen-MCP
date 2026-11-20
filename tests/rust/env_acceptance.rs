mod common;
#[path = "common/config.rs"]
mod config_fixture;

use common::Workspace;
use numera_gen_mcp::{
    compat::value::json,
    config::{Config, ConfigReload, load_config},
    error::Result,
    logging::Logger,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

// Frozen fc7da25 tests/unit/env-config.test.ts: the preamble and suffixes are exact source inputs.
const VALID: &str = "API_ENDPOINT=https://api.example/v1\nAPI_KEY=\"synthetic-test-key\"\nMODEL_1_ID=family/image\nMODEL_1_NAME=\"تصویر #1\"\n";
const INVALID_SUFFIXES: [&str; 13] = [
    "MODEL_1_ENABLED=yes\n",
    "MODEL_101_ID=overflow\n",
    "MODEL_01_ID=zero\n",
    "MODEL_1_QUALTY=typo\n",
    "PROFILEE=typo\n",
    "API_KEY=duplicate\n",
    "broken line\n",
    "MODEL_1_SIZE=\"unterminated\n",
    "MODEL_1_SIZE=\"512x512\" garbage\n",
    "EDIT_MASKS=true\n",
    "EDIT_MODE=json\nEDIT_ENCODING=images\nEDIT_MAX_REFERENCES=1.5\n",
    "EDIT_MODE=json\nEDIT_ENCODING=images\nEDIT_MAX_REFERENCES=33\n",
    "EDIT_MODE=json\nEDIT_ENCODING=images\nEDIT_MASKS=1\n",
];

fn environment(workspace: &Workspace) -> HashMap<String, String> {
    ["LOCALAPPDATA", "XDG_CONFIG_HOME", "HOME", "USERPROFILE"]
        .into_iter()
        .map(|key| {
            (
                key.to_owned(),
                workspace
                    .0
                    .to_str()
                    .expect("Unicode fixture root")
                    .to_owned(),
            )
        })
        .collect()
}

fn load_file(workspace: &Workspace, name: &str, bytes: &[u8]) -> Result<Config> {
    let file = workspace.0.join(name);
    assert!(file.is_absolute(), "Fixture config path must be absolute.");
    std::fs::write(&file, bytes).expect("write actual fixture config bytes");
    load_config(
        &[
            "--config".into(),
            file.to_str().expect("Unicode fixture path").into(),
        ],
        &environment(workspace),
        &workspace.0,
    )
}

fn invalid_configuration(result: Result<Config>) {
    let error = result.err().expect("Source-invalid file must reject.");
    assert_eq!(error.code, "invalid_configuration");
    assert!(
        !error.to_string().contains("synthetic-test-key"),
        "Configuration error Display must not contain the fixture key."
    );
}

#[test]
fn unused_numbered_slots_do_not_fail_the_filled_ones() {
    let workspace = Workspace::new();
    // The shipped template leaves high-numbered slots with only an ENABLED key. Those
    // placeholders must not be treated as models: the filled slots stay listed.
    let text = format!(
        "{VALID}MODEL_2_ENABLED=\"true\"\nMODEL_3_ENABLED=\"true\"\nMODEL_3_OUTPUT_FORMAT=\"\"\nMODEL_3_SIZE=\"\"\nMODEL_3_QUALITY=\"\"\n"
    );
    let config = load_file(&workspace, ".env", text.as_bytes()).unwrap();
    let connection = &config.value["connections"]["default"];
    assert_eq!(connection["defaultModel"], "family/image");
    assert_eq!(
        connection["configuredModels"],
        json!([{"id":"family/image","name":"تصویر #1"}])
    );
}

#[test]
fn quoted_env_slots_preserve_source_defaults_and_process_environment() {
    let workspace = Workspace::new();
    let before = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let text = format!(
        "{VALID}MODEL_1_SIZE=\nMODEL_1_QUALITY=\"\"\nMODEL_1_OUTPUT_FORMAT=\nMODEL_2_ID=disabled\nMODEL_2_NAME=Disabled\nMODEL_2_ENABLED=false\n"
    );
    let config = load_file(&workspace, ".env", text.as_bytes()).unwrap();
    let connection = &config.value["connections"]["default"];
    assert_eq!(connection["defaultModel"], "family/image");
    assert_eq!(
        connection["configuredModels"],
        json!([{"id":"family/image","name":"تصویر #1"}])
    );
    assert_eq!(connection["deniedModels"], json!(["disabled"]));
    assert_eq!(
        connection["modelOverrides"]["family/image"]["defaults"],
        json!({})
    );
    assert!(
        std::env::vars_os().collect::<BTreeMap<_, _>>() == before,
        "File loading must not mutate the process environment."
    );
}

#[test]
fn all_thirteen_invalid_env_suffixes_reject_file_loads_without_key_disclosure() {
    let workspace = Workspace::new();
    for suffix in INVALID_SUFFIXES {
        let text = format!("{VALID}{suffix}");
        invalid_configuration(load_file(&workspace, ".env", text.as_bytes()));
        invalid_configuration(load_config(
            &[
                "--config".into(),
                workspace.0.join(".env").to_str().unwrap().into(),
            ],
            &environment(&workspace),
            &workspace.0,
        ));
    }
}

#[test]
fn gemini_env_declares_exact_header_and_four_reference_edit_contract() {
    let workspace = Workspace::new();
    let text = format!(
        "{VALID}PROFILE=gemini\nEDIT_MODE=json\nEDIT_ENCODING=images\nEDIT_MAX_REFERENCES=4\nEDIT_MASKS=true\nEDIT_MASK_POLARITY=white-edit\n"
    );
    let config = load_file(&workspace, "settings.env", text.as_bytes()).unwrap();
    let connection = &config.value["connections"]["default"];
    assert_eq!(connection["adapter"], "gemini");
    assert_eq!(connection["auth"]["type"], "header");
    assert_eq!(connection["auth"]["header"], "x-goog-api-key");
    assert_eq!(
        connection["edit"],
        json!({
            "mode":"json",
            "encoding":"images",
            "maxReferences":4,
            "masks":true,
            "maskPolarity":"white-edit"
        })
    );
}

#[test]
fn env_file_loader_rejects_source_byte_limits_and_preserves_exported_multiline_name() {
    let workspace = Workspace::new();
    invalid_configuration(load_file(&workspace, ".env", &vec![b' '; 1024 * 1024 + 1]));
    // Deliberately invalid UTF-8: the frozen file-loader rejection fixture is the single byte ff.
    invalid_configuration(load_file(&workspace, ".env", &[0xff]));
    let text = VALID.replace(
        "MODEL_1_NAME=\"تصویر #1\"",
        "export MODEL_1_NAME=\"line one\nline two\" # comment",
    );
    let config = load_file(&workspace, ".env", text.as_bytes()).unwrap();
    assert_eq!(
        config.value["connections"]["default"]["configuredModels"][0]["name"],
        "line one\nline two"
    );
}

#[test]
fn file_reload_retains_initial_arc_for_unchanged_storage_change_and_snapshot_refusal() {
    let workspace = Workspace::new();
    let file = workspace.0.join("config.json");
    let mut raw = workspace.config("https://api.example/v1", "openai-images");
    raw["defaultConnection"] = json!("local");
    raw["connections"] = json!({
        "local":{
            "adapter":"openai-images",
            "baseUrl":"https://api.example/v1",
            "auth":{"type":"none"}
        }
    });
    let config = load_file(&workspace, "config.json", raw.to_string().as_bytes()).unwrap();
    let original_value = config.value.clone();
    let logger = Logger::new(&workspace.0.join("logs"), "ERROR");
    let args = vec![
        "--config".into(),
        file.to_str().expect("Unicode fixture path").into(),
    ];
    let env = environment(&workspace);
    let mut reload = ConfigReload::new(config, file.clone(), args, env);
    let original = reload.current();
    assert_eq!(original.file, file);
    assert_eq!(original.value, original_value);

    let unchanged = reload
        .refresh(&logger, |_| {
            panic!("Unchanged file must not request snapshot admission.")
        })
        .unwrap();
    assert!(Arc::ptr_eq(&unchanged, &original));
    assert_eq!(unchanged.value, original_value);

    let mut changed = original_value.clone();
    changed["stateDir"] = json!(
        workspace
            .0
            .join("different")
            .to_str()
            .expect("Unicode fixture path")
    );
    let loaded_change =
        load_file(&workspace, "config.json", changed.to_string().as_bytes()).unwrap();
    assert_eq!(loaded_change.value["stateDir"], changed["stateDir"]);
    assert_ne!(loaded_change.value["stateDir"], original_value["stateDir"]);
    let fixed_storage = reload
        .refresh(&logger, |_| {
            panic!("Fixed storage changes must reject before admission.")
        })
        .unwrap();
    assert!(Arc::ptr_eq(&fixed_storage, &original));
    assert_eq!(fixed_storage.value, original_value);

    let mut refused = original_value.clone();
    refused["returnMode"] = json!("files_and_preview");
    let loaded_refusal =
        load_file(&workspace, "config.json", refused.to_string().as_bytes()).unwrap();
    assert_eq!(loaded_refusal.value["returnMode"], "files_and_preview");
    let mut admission_checked = false;
    let snapshot_limit = reload
        .refresh(&logger, |candidate| {
            admission_checked = true;
            assert_eq!(candidate.value, loaded_refusal.value);
            false
        })
        .unwrap();
    assert!(
        admission_checked,
        "Snapshot refusal must reach the acceptance callback."
    );
    assert!(Arc::ptr_eq(&snapshot_limit, &original));
    assert_eq!(snapshot_limit.value, original_value);
    assert!(Arc::ptr_eq(&reload.current(), &original));
    assert_eq!(reload.snapshots().len(), 1);
    assert!(Arc::ptr_eq(&reload.snapshots()[0], &original));
    drop(logger);
}

const FAL_SECTION: &str = "FAL_API_ENDPOINT=https://queue.fal.run\nFAL_API_KEY=\"synthetic-fal-key\"\nFAL_MODEL_4_ID=fal-ai/flux/dev\nFAL_MODEL_4_NAME=FLUX\nFAL_MODEL_4_ENABLED=true\nFAL_MODEL_4_OUTPUT_TYPES=image\nFAL_MODEL_4_MODES=text-to-image\n";

#[test]
fn shipped_env_example_loads_as_documented() {
    let workspace = Workspace::new();
    let template = include_str!("../../.env.example");
    // The main endpoint placeholder is replaced like the media template contract does; the fal
    // section ships disabled (empty FAL_API_ENDPOINT/FAL_API_KEY) with its model list intact.
    let text = template.replace(
        "https://YOUR_API_HOST/YOUR_API_PREFIX",
        "https://example.invalid/v1",
    );
    assert_ne!(text, template, "template placeholder endpoint changed");
    let config = load_file(&workspace, ".env", text.as_bytes()).unwrap();
    assert_eq!(config.value["defaultConnection"], "default");
    assert!(config.value["connections"].get("fal").is_none());
    // Every fal endpoint of the former fal.env.example survives under FAL_MODEL_<N>_*.
    for n in 1..=11 {
        assert!(template.contains(&format!("\nFAL_MODEL_{n}_ID=\"fal-ai/")));
    }
    assert!(!template.contains("\nMODEL_4_ID=\"fal-ai/"));
}

#[test]
fn merged_env_builds_two_isolated_connections() {
    let workspace = Workspace::new();
    let config = load_file(
        &workspace,
        ".env",
        format!("{VALID}{FAL_SECTION}").as_bytes(),
    )
    .unwrap();
    assert_eq!(config.value["defaultConnection"], "default");
    let main = &config.value["connections"]["default"];
    let fal = &config.value["connections"]["fal"];
    assert_eq!(main["auth"]["type"], "bearer");
    assert_eq!(main["auth"]["apiKey"], "synthetic-test-key");
    assert_eq!(fal["adapter"], "fal-queue");
    assert_eq!(fal["auth"]["type"], "key");
    assert_eq!(fal["auth"]["apiKey"], "synthetic-fal-key");
    assert_eq!(fal["defaultModel"], "fal-ai/flux/dev");
    assert_eq!(
        fal["configuredModels"],
        json!([{"id":"fal-ai/flux/dev","name":"FLUX"}])
    );
    let (name, _) = config
        .select_for_model(None, Some(&json!("fal-ai/flux/dev")))
        .unwrap();
    assert_eq!(name, "fal");
    let (name, _) = config
        .select_for_model(None, Some(&json!("family/image")))
        .unwrap();
    assert_eq!(name, "default");
    let unknown = config
        .select_for_model(None, Some(&json!("not/configured")))
        .unwrap_err();
    assert_eq!(unknown.code, "invalid_input");
}

#[tokio::test]
async fn routed_requests_carry_only_their_own_connection_key() {
    let workspace = Workspace::new();
    let config = load_file(
        &workspace,
        ".env",
        format!("{VALID}{FAL_SECTION}").as_bytes(),
    )
    .unwrap();
    let runtime = numera_gen_mcp::runtime_paths::RuntimePaths::new(&workspace.0).unwrap();
    for (model, expected, other) in [
        (
            "fal-ai/flux/dev",
            "Key synthetic-fal-key",
            "synthetic-test-key",
        ),
        (
            "family/image",
            "Bearer synthetic-test-key",
            "synthetic-fal-key",
        ),
    ] {
        let (_, connection) = config.select_for_model(None, Some(&json!(model))).unwrap();
        let pinned =
            numera_gen_mcp::config::snapshot_credentials(&connection, &HashMap::new(), &runtime)
                .await
                .unwrap();
        let headers = pinned.headers();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers["Authorization"], expected);
        assert!(headers.values().all(|v| !v.contains(other)));
    }
}

#[test]
fn fal_only_env_makes_fal_the_default() {
    let workspace = Workspace::new();
    let text = format!("API_ENDPOINT=\nAPI_KEY=\n{FAL_SECTION}");
    let config = load_file(&workspace, ".env", text.as_bytes()).unwrap();
    assert_eq!(config.value["defaultConnection"], "fal");
    assert!(config.value["connections"].get("default").is_none());
    assert_eq!(config.select(None).unwrap().0, "fal");
}

#[test]
fn legacy_fal_profile_file_still_loads_as_default() {
    let workspace = Workspace::new();
    let text = "API_ENDPOINT=https://queue.fal.run\nAPI_KEY=synthetic-test-key\nPROFILE=fal\nMODEL_1_ID=fal-ai/flux/dev\nMODEL_1_NAME=FLUX\nMODEL_1_OUTPUT_TYPES=image\nMODEL_1_MODES=text-to-image\n";
    let config = load_file(&workspace, "fal.env", text.as_bytes()).unwrap();
    assert_eq!(
        config.value["connections"]["default"]["adapter"],
        "fal-queue"
    );
    assert!(config.value["connections"].get("fal").is_none());
}

#[test]
fn invalid_dual_provider_files_reject_without_key_disclosure() {
    let workspace = Workspace::new();
    for text in [
        format!("{VALID}{FAL_SECTION}PROFILE=fal\n"),
        format!("{VALID}FAL_API_ENDPOINT=https://queue.fal.run\n"),
        format!("{VALID}FAL_PROFILE=fal\n"),
        format!("{VALID}{FAL_SECTION}FAL_MODEL_101_ID=x\n"),
        "API_ENDPOINT=\nAPI_KEY=\nFAL_API_ENDPOINT=\nFAL_API_KEY=\n".to_owned(),
    ] {
        let error = load_file(&workspace, ".env", text.as_bytes())
            .err()
            .expect("must reject");
        assert_eq!(error.code, "invalid_configuration");
        assert!(!error.to_string().contains("synthetic-fal-key"));
        assert!(!error.to_string().contains("synthetic-test-key"));
    }
}

const COMPACT_MAIN_JSON: &str = r#"{
    "api_endpoint": "https://main.example/v1",
    "api_key": "synthetic-main-key",
    "models": [{"id": "main/model", "name": "Main"}]
}"#;

const COMPACT_INTERNAL_FAL_JSON: &str = r#"{
    "api_endpoint": "https://main.example/v1",
    "api_key": "synthetic-main-key",
    "models": [{"id": "main/model", "name": "Main"}],
    "fal": {
        "api_endpoint": "https://other.example/v1",
        "api_key": "synthetic-secondary-key",
        "models": [{"id": "secondary/model", "name": "Secondary"}]
    }
}"#;

#[test]
fn compact_json_rejects_internal_dotenv_fal_section() {
    let workspace = Workspace::new();
    let config = load_file(&workspace, "config.json", COMPACT_MAIN_JSON.as_bytes()).unwrap();
    assert_eq!(config.value["connections"].as_object().unwrap().len(), 1);
    assert_eq!(config.select(None).unwrap().0, "default");
    let error = load_file(
        &workspace,
        "config.json",
        COMPACT_INTERNAL_FAL_JSON.as_bytes(),
    )
    .err()
    .expect("Unknown compact JSON property must reject.");
    assert_eq!(error.code, "invalid_configuration");
    assert!(!error.to_string().contains("synthetic-main-key"));
    assert!(!error.to_string().contains("synthetic-secondary-key"));
}

#[test]
fn compact_json_fal_injection_keeps_last_valid_reload_snapshot() {
    let workspace = Workspace::new();
    let config = load_file(&workspace, "config.json", COMPACT_MAIN_JSON.as_bytes()).unwrap();
    let mut reload = ConfigReload::new(
        config,
        workspace.0.join("config.json"),
        Vec::new(),
        environment(&workspace),
    );
    let original = reload.current();
    let result = reload
        .refresh_text(Some(COMPACT_INTERNAL_FAL_JSON), |_| true)
        .unwrap();
    assert_eq!(result.event, numera_gen_mcp::config::ReloadEvent::Invalid);
    assert!(Arc::ptr_eq(&result.config, &original));
    assert!(Arc::ptr_eq(&reload.current(), &original));
    assert_eq!(reload.snapshots().len(), 1);
}
