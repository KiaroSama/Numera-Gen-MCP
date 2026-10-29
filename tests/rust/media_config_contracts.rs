mod common;
#[path = "common/config.rs"]
mod config_fixture;
use common::Workspace;
use numera_gen_mcp::{
    capabilities,
    compat::value::{Value, json},
    config::{Config, parse_env_config},
};
use std::collections::HashMap;

const VIDEO: &str = "fal-ai/kling-video/v2.5-turbo/pro/text-to-video";

fn parse(workspace: &Workspace, value: &Value) -> Config {
    Config::parse(
        &value.to_string(),
        &workspace.0.join("config.json"),
        &[],
        &HashMap::new(),
    )
    .unwrap()
}

fn reject(workspace: &Workspace, value: &Value) {
    let error = Config::parse(
        &value.to_string(),
        &workspace.0.join("config.json"),
        &[],
        &HashMap::new(),
    )
    .err()
    .expect("invalid media declaration must reject");
    assert_eq!(error.code, "invalid_configuration");
    assert!(!error.message.contains("synthetic-media-key"));
}

fn fal(workspace: &Workspace) -> Value {
    let mut raw = workspace.config("https://queue.fal.run", "fal-queue");
    raw["connections"]["test"].as_object_mut().unwrap().remove("orchestrationModel");
    raw["connections"]["test"]["auth"] =
        json!({"type":"key","apiKey":"synthetic-media-key","origin":"https://queue.fal.run"});
    raw["connections"]["test"]["defaultModel"] = json!(VIDEO);
    raw["connections"]["test"]["configuredModels"] = json!([{"id":VIDEO,"name":"動画"}]);
    raw["connections"]["test"]["modelOverrides"] =
        json!({VIDEO:{"outputTypes":["video"],"modes":["text-to-video"]}});
    raw
}

#[test]
fn full_media_declarations_round_trip_without_changing_legacy_image_defaults() {
    let workspace = Workspace::new();
    let legacy = workspace.config("https://images.example/v1", "openai-images");
    let loaded = parse(&workspace, &legacy);
    let connection = &loaded.value["connections"]["test"];
    assert_eq!(connection["modelOverrides"], json!({}));
    let old_fingerprint = connection.fingerprint().unwrap();
    let again = parse(&workspace, &loaded.value);
    assert_eq!(again.value["connections"]["test"].fingerprint().unwrap(), old_fingerprint);
    let old_cap = capabilities::capabilities(connection, &"opaque/image".into());
    assert!(old_cap.get("output_types").is_none());
    assert_eq!(old_cap["gateway_forwarding"]["text_to_image"], "supported");
    let mut declared_image = legacy;
    declared_image["connections"]["test"]["modelOverrides"] =
        json!({"opaque/image":{"outputTypes":["image"],"modes":["text-to-image","image-to-image"]}});
    let image = parse(&workspace, &declared_image);
    let image_cap = capabilities::capabilities(&image.value["connections"]["test"], &"opaque/image".into());
    assert_eq!(image_cap["output_types"], json!(["image"]));
    assert_eq!(image_cap["mode_forwarding"]["text-to-image"], "supported");
    assert_eq!(image_cap["mode_forwarding"]["image-to-image"], "unknown");
    let loaded = parse(&workspace, &fal(&workspace));
    let model = &loaded.value["connections"]["test"]["modelOverrides"][VIDEO];
    assert_eq!(model["outputTypes"], json!(["video"]));
    assert_eq!(model["modes"], json!(["text-to-video"]));
    assert_eq!(model["defaults"], json!({}));
    assert_eq!(loaded.value["connections"]["test"]["configuredModels"][0], json!({"id":VIDEO,"name":"動画"}));
    let mut mesh = fal(&workspace);
    mesh["connections"]["test"]["defaultModel"] = json!("fal-ai/hyper3d/rodin");
    mesh["connections"]["test"]["configuredModels"] = json!([{"id":"fal-ai/hyper3d/rodin","name":"Rodin"}]);
    mesh["connections"]["test"]["modelOverrides"] = json!({"fal-ai/hyper3d/rodin":{"outputTypes":["3d","image"],"modes":["text-to-3d","image-to-3d"]}});
    let loaded = parse(&workspace, &mesh);
    assert_eq!(loaded.value["connections"]["test"]["modelOverrides"]["fal-ai/hyper3d/rodin"]["outputTypes"], json!(["3d","image"]));
    mesh["connections"]["test"]["modelOverrides"]["fal-ai/hyper3d/rodin"]["outputTypes"] = json!(["3d"]);
    reject(&workspace, &mesh);
}

#[test]
fn media_declarations_reject_unpaired_empty_duplicate_unknown_and_incompatible_values() {
    let workspace = Workspace::new();
    for declaration in [
        json!({"outputTypes":["video"]}),
        json!({"modes":["text-to-video"]}),
        json!({"outputTypes":null,"modes":["text-to-video"]}),
        json!({"outputTypes":[],"modes":["text-to-video"]}),
        json!({"outputTypes":["video"],"modes":[]}),
        json!({"outputTypes":["video","video"],"modes":["text-to-video"]}),
        json!({"outputTypes":["video"],"modes":["text-to-video","text-to-video"]}),
        json!({"outputTypes":["Video"],"modes":["text-to-video"]}),
        json!({"outputTypes":["video"],"modes":["text_to_video"]}),
        json!({"outputTypes":["image"],"modes":["text-to-video"]}),
        json!({"outputTypes":["audio"],"modes":["audio-to-audio"]}),
        json!({"outputTypes":["video","audio"],"modes":["text-to-video"]}),
    ] {
        let mut raw = fal(&workspace);
        raw["connections"]["test"]["modelOverrides"][VIDEO] = declaration;
        reject(&workspace, &raw);
    }
    let mut raw = fal(&workspace);
    raw["connections"]["test"]["modelOverrides"] = json!({});
    reject(&workspace, &raw);
    let mut unknown = fal(&workspace);
    unknown["connections"]["test"]["defaultModel"] = json!("unknown/video");
    unknown["connections"]["test"]["configuredModels"] = json!([{"id":"unknown/video","name":"Unknown"}]);
    unknown["connections"]["test"]["modelOverrides"] =
        json!({"unknown/video":{"outputTypes":["video"],"modes":["text-to-video"]}});
    reject(&workspace, &unknown);
    let mut types = fal(&workspace);
    types["connections"]["test"]["modelOverrides"][VIDEO]["outputTypes"] =
        json!(["image","video","audio","3d","image"]);
    reject(&workspace, &types);
    let mut raw = fal(&workspace);
    raw["connections"]["test"]["adapter"] = json!("openai-images");
    raw["connections"]["test"]["auth"] = json!({"type":"none"});
    reject(&workspace, &raw);
    let mut raw = fal(&workspace);
    raw["connections"]["test"]["baseUrl"] = json!("https://queue.fal.run/fal-ai/model");
    reject(&workspace, &raw);
    for origin in ["http://queue.fal.run", "https://example.invalid", "https://queue.fal.run:8443", "http://192.168.1.2"] {
        let mut raw = fal(&workspace);
        raw["connections"]["test"]["baseUrl"] = json!(origin);
        raw["connections"]["test"]["auth"]["origin"] = json!(origin);
        reject(&workspace, &raw);
    }
    for auth in [json!({"type":"none"}), json!({"type":"bearer","apiKey":"synthetic"})] {
        let mut raw = fal(&workspace);
        raw["connections"]["test"]["auth"] = auth;
        reject(&workspace, &raw);
    }
}

#[test]
fn compact_and_strict_env_media_declarations_keep_model_identity_and_defaults() {
    let workspace = Workspace::new();
    let text = format!(
        "API_ENDPOINT=https://queue.fal.run\nAPI_KEY=synthetic-media-key\nPROFILE=fal\nMODEL_1_ID={VIDEO}\nMODEL_1_NAME=動画\nMODEL_1_OUTPUT_TYPES=video\nMODEL_1_MODES=text-to-video\n"
    );
    let raw = parse_env_config(&text).unwrap();
    assert_eq!(raw["models"][0]["output_types"], json!(["video"]));
    assert_eq!(raw["models"][0]["modes"], json!(["text-to-video"]));
    let loaded = parse(&workspace, &raw);
    let connection = &loaded.value["connections"]["default"];
    assert_eq!(connection["adapter"], "fal-queue");
    assert_eq!(connection["auth"]["type"], "key");
    assert_eq!(connection["defaultModel"], VIDEO);
    assert_eq!(connection["modelOverrides"][VIDEO]["outputTypes"], json!(["video"]));
    assert_eq!(connection["modelOverrides"][VIDEO]["defaults"], json!({}));
    assert_eq!(connection["configuredModels"], json!([{"id":VIDEO,"name":"動画"}]));
    let empty = text.replace("MODEL_1_OUTPUT_TYPES=video", "MODEL_1_OUTPUT_TYPES=\"\"")
        .replace("MODEL_1_MODES=text-to-video", "MODEL_1_MODES=\"\"");
    let empty = parse_env_config(&empty).unwrap();
    assert!(empty["models"][0].get("output_types").is_none());
    assert!(empty["models"][0].get("modes").is_none());
    // Empty declarations preserve legacy image slots, but cannot enable an undeclared fal model.
    reject(&workspace, &empty);
    let mut template = parse_env_config(include_str!("../../.env.example")).unwrap();
    assert!(template["models"].as_array().unwrap().iter().all(|model|
        model.get("output_types").is_none() && model.get("modes").is_none()));
    template["api_endpoint"] = json!("https://example.invalid/v1");
    let loaded_template = parse(&workspace, &template);
    assert!(loaded_template.value["connections"]["default"]["modelOverrides"].as_object().unwrap().values()
        .all(|model| model.get("outputTypes").is_none() && model.get("modes").is_none()));
    for bad in ["", "video,", "video,,audio", "video,video", "Video", "video, audio"] {
        let invalid = text.replace("MODEL_1_OUTPUT_TYPES=video", &format!("MODEL_1_OUTPUT_TYPES={bad}"));
        assert_eq!(parse_env_config(&invalid).unwrap_err().code, "invalid_configuration");
    }
    for bad in ["", "text-to-video,", "text-to-video,text-to-video", "text_to_video"] {
        let invalid = text.replace("MODEL_1_MODES=text-to-video", &format!("MODEL_1_MODES={bad}"));
        assert_eq!(parse_env_config(&invalid).unwrap_err().code, "invalid_configuration");
    }
    assert_eq!(
        parse_env_config(&text.replace("MODEL_1_MODES=text-to-video\n", ""))
            .unwrap_err()
            .code,
        "invalid_configuration"
    );
    let mut unpaired = raw.clone();
    unpaired["models"][0].as_object_mut().unwrap().remove("modes");
    reject(&workspace, &unpaired);
    let mut disabled = raw;
    disabled["models"].as_array_mut().unwrap().push(json!({"id":"disabled","name":"Disabled","enabled":false,"output_types":["image","image"],"modes":["text-to-image"]}));
    reject(&workspace, &disabled);
}

#[test]
fn public_fal_example_loads_every_explicit_mode_without_reading_a_key_or_contacting_a_provider() {
    let workspace = Workspace::new();
    let loaded = Config::parse(include_str!("../../examples/fal.json"),
        &workspace.0.join("fal.json"), &[], &HashMap::new()).unwrap();
    let connection = &loaded.value["connections"]["fal"];
    assert_eq!(connection["auth"]["secretEnv"], "FAL_KEY");
    assert!(connection["auth"].get("apiKey").is_none());
    assert_eq!(connection["configuredModels"].as_array().unwrap().len(), 11);
    let mut modes = std::collections::BTreeSet::new();
    let mut kinds = std::collections::BTreeSet::new();
    for model in connection["configuredModels"].as_array().unwrap() {
        let id = model["id"].as_js_string().unwrap();
        let declaration = numera_gen_mcp::config::media::declaration(connection, id).unwrap();
        let endpoint = id.checked_str().unwrap();
        for mode in declaration["modes"].as_array().unwrap() {
            let mode = mode.as_str().unwrap();
            let contract = numera_gen_mcp::media::fal::contract(endpoint, mode).unwrap();
            assert_eq!(contract.mode, mode);
            modes.insert(mode.to_owned());
        }
        for kind in declaration["outputTypes"].as_array().unwrap() {
            kinds.insert(kind.as_str().unwrap().to_owned());
        }
    }
    assert_eq!(kinds, ["image", "video", "audio", "3d"].map(str::to_owned).into_iter().collect());
    assert_eq!(modes.len(), 10);
    assert!(!modes.contains("video-to-audio"));
    assert_eq!(connection["modelOverrides"]["fal-ai/hyper3d/rodin"]["defaults"]["geometry_file_format"], "glb");
}

#[tokio::test]
async fn configured_media_discovery_is_offline_and_keeps_evidence_and_dimensions_separate() {
    use numera_gen_mcp::{runtime_paths::RuntimePaths, services::discovery::Discovery};
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    let workspace = Workspace::new();
    let mut raw = fal(&workspace);
    // Unreachable loopback plus absent secret makes accidental discovery I/O fail this assertion.
    raw["connections"]["test"]["baseUrl"] = json!("http://127.0.0.1:1");
    raw["connections"]["test"]["auth"] = json!({"type":"key","secretEnv":"UNSET_MEDIA_TEST_KEY"});
    let config = parse(&workspace, &raw);
    let runtime = RuntimePaths::new(&workspace.0).unwrap();
    let env = HashMap::<String, String>::new();
    let cancel = CancellationToken::new();
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut discovery = Discovery::default();
        for refresh in [false, true] {
            let catalog = discovery.list(&config, &runtime, &env, None, refresh, &cancel).await.unwrap();
            assert_eq!(catalog["complete"], true);
            assert_eq!(catalog["models"][0]["output_types"], json!(["video"]));
            assert_eq!(catalog["models"][0]["modes"], json!(["text-to-video"]));
            assert_eq!(catalog["models"][0]["metadata"]["account_availability"], "unknown");
            assert_eq!(catalog["models"][0]["metadata"]["generation_verified"], false);
        }
        let cap = Discovery::model(&config, &runtime, &env, None, &VIDEO.into(), &cancel).await.unwrap();
        assert_eq!(cap["output_types"], json!(["video"]));
        assert_eq!(cap["modes"], json!(["text-to-video"]));
        assert_eq!(cap["account_availability"], "unknown");
        assert_eq!(cap["gateway_forwarding"]["text_to_image"], "unsupported");
        assert_eq!(cap["mode_forwarding"]["text-to-video"], "supported");
        assert!(cap.get("dimensions").is_none());
        assert!(cap.get("max_references").is_none());
        assert_eq!(cap["advertised"], Value::Null);
        let unknown = Discovery::model(&config, &runtime, &env, None, &"unknown/video".into(), &cancel).await.unwrap();
        assert_eq!(unknown["mode_forwarding"], json!({}));
        assert!(unknown.get("dimensions").is_none());
        assert_eq!(unknown["account_availability"], "unknown");
    }).await.unwrap();
}
