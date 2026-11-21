//! Acceptance tests for v1.0.0 product features (Plans 030-034).
use numera_gen_mcp::{
    adapters::{self, types::Input},
    compat::value::{JsString, Value, json},
    config::Config,
    jobs::{Receipt, Store},
    media::{self, Kind, fal},
};
use std::collections::HashMap;

fn wav_extensible(subformat: [u8; 16], bits: u16, sample_bytes: &[u8]) -> Vec<u8> {
    let channels: u16 = 2;
    let rate: u32 = 48000;
    let align: u16 = channels * (bits / 8);
    let byte_rate: u32 = rate * u32::from(align);
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&0xFFFEu16.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&byte_rate.to_le_bytes());
    fmt.extend_from_slice(&align.to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    fmt.extend_from_slice(&22u16.to_le_bytes()); // cbSize
    fmt.extend_from_slice(&bits.to_le_bytes()); // valid bits
    fmt.extend_from_slice(&3u32.to_le_bytes()); // channel mask
    fmt.extend_from_slice(&subformat);
    assert_eq!(fmt.len(), 40);

    let mut body = Vec::new();
    body.extend_from_slice(b"WAVE");
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(sample_bytes.len() as u32).to_le_bytes());
    body.extend_from_slice(sample_bytes);
    if !sample_bytes.len().is_multiple_of(2) {
        body.push(0);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn minimal_glb(extensions_required: serde_json::Value) -> Vec<u8> {
    // Minimal 1-triangle valid GLB 2.0 (3 float3 positions = 36 bytes).
    let mut bin = Vec::new();
    for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    let doc = serde_json::json!({
        "asset": {"version": "2.0"},
        "extensionsRequired": extensions_required,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": bin.len()}],
        "accessors": [{
            "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
            "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]
        }]
    });
    let mut json_bytes = serde_json::to_vec(&doc).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4e4f534au32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004e4942u32.to_le_bytes());
    out.extend_from_slice(&bin);
    out
}

#[test]
fn plan030_mmaudio_v2_video_to_audio_contract_and_params() {
    let c = fal::contract("fal-ai/mmaudio-v2", "video-to-audio").unwrap();
    assert!(c.output_types.contains(&Kind::Video));
    assert!(c.output_types.contains(&Kind::Audio));
    assert_eq!(c.required_roles, vec!["video"]);

    let body = fal::prepare(
        "fal-ai/mmaudio-v2",
        "video-to-audio",
        Some("ocean waves"),
        &[fal::Reference {
            kind: Kind::Video,
            role: "video".into(),
            url: "https://example.invalid/clip.mp4".into(),
        }],
        &serde_json::json!({"duration": 8.0, "num_steps": 25, "cfg_strength": 4.5, "mask_away_clip": false}),
    )
    .unwrap();
    assert_eq!(body["video_url"], "https://example.invalid/clip.mp4");
    assert_eq!(body["duration"], 8.0);

    // Out-of-range duration rejected.
    assert!(
        fal::prepare(
            "fal-ai/mmaudio-v2",
            "video-to-audio",
            Some("ocean"),
            &[fal::Reference {
                kind: Kind::Video,
                role: "video".into(),
                url: "https://example.invalid/clip.mp4".into(),
            }],
            &serde_json::json!({"duration": 99.0}),
        )
        .is_err()
    );
}

#[test]
fn plan031_wav_extensible_and_glb_safe_extensions() {
    let pcm_guid = [
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ];
    let float_guid = [
        0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ];
    let bad_guid = [0x99u8; 16];

    let pcm_wav = wav_extensible(pcm_guid, 24, &[0u8; 12]);
    let info = media::inspect(Kind::Audio, &pcm_wav, 1024 * 1024, 1024).unwrap();
    assert_eq!(info.format, "wav");
    assert_eq!(info.metadata["bits_per_sample"], 24);

    let float_samples: Vec<u8> = [0.25f32, -0.5f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let float_wav = wav_extensible(float_guid, 32, &float_samples);
    assert!(media::inspect(Kind::Audio, &float_wav, 1024 * 1024, 1024).is_ok());

    let bad_wav = wav_extensible(bad_guid, 16, &[0u8; 8]);
    assert!(media::inspect(Kind::Audio, &bad_wav, 1024 * 1024, 1024).is_err());

    // GLB safe Khronos material extension accepted; geometry compression rejected.
    let safe_glb = minimal_glb(serde_json::json!([
        "KHR_materials_unlit",
        "KHR_materials_ior"
    ]));
    assert!(media::inspect(Kind::Model3d, &safe_glb, 1024 * 1024, 1024).is_ok());

    let draco_glb = minimal_glb(serde_json::json!(["KHR_draco_mesh_compression"]));
    assert!(media::inspect(Kind::Model3d, &draco_glb, 1024 * 1024, 1024).is_err());
}

#[test]
fn plan032_list_jobs_redaction_and_async_wait_false_preserves_store_false() {
    let dir = std::env::temp_dir().join(format!("numera-p032-{}", uuid::Uuid::new_v4()));
    let mut store = Store::open(&dir).unwrap();
    store
        .prepare(
            Receipt::new(json!({
                "request_id": "job-1",
                "connection": "openai",
                "model": "gpt-image-1",
                "adapter": "openai-responses",
                "operation": "generate",
                "status": "running",
                "prompt": "SECRET PROMPT MUST NOT APPEAR",
                "generation_outcome": "unknown",
                "upstream_job": {"id": "resp_1", "kind": "openai-responses"},
                "outputs": [],
                "errors": [],
                "warnings": [],
                "deviations": []
            })),
            "h1",
            "id1",
        )
        .unwrap();
    let list = store.list_jobs(Some("running"), 0, 20).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["request_id"], json!("job-1"));
    assert_eq!(list[0]["has_upstream_job"], json!(true));
    assert!(list[0].get("prompt").is_none());
    assert!(
        store
            .list_jobs(Some("completed"), 0, 20)
            .unwrap()
            .is_empty()
    );

    // wait=false on openai-responses sets background=true while keeping store=false.
    let conn = json!({
        "adapter": "openai-responses",
        "orchestrationModel": "gpt-5",
        "baseUrl": "https://api.openai.com/v1",
        "auth": {"type": "none"}
    });
    let built = adapters::build_request(&Input {
        connection: &conn,
        request: &json!({"prompt": "hi", "count": 1, "wait": false, "provider_options": {}}),
        model: &JsString::from("gpt-image-1"),
        operation: "generate",
        references: &[],
        mask: None,
    })
    .unwrap();
    if let numera_gen_mcp::http::api::Body::Json(body) = built.body {
        assert_eq!(body["background"], json!(true));
        assert_eq!(body["store"], json!(false));
    } else {
        panic!("expected json");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan034_opt_in_fal_contract_accepts_valid_custom_and_blocks_builtin_override() {
    let dir = std::env::temp_dir().join(format!("numera-p034-{}", uuid::Uuid::new_v4()));
    let make_cfg = |endpoint: &str, contract: Value| {
        json!({
            "schemaVersion": 1,
            "defaultConnection": "fal",
            "outputDir": dir.join("out").to_str().unwrap(),
            "stateDir": dir.join("state").to_str().unwrap(),
            "logging": {"directory": dir.join("logs").to_str().unwrap()},
            "connections": {
                "fal": {
                    "adapter": "fal-queue",
                    "baseUrl": "https://queue.fal.run",
                    "auth": {"type": "key", "apiKey": "dummy"},
                    "defaultModel": endpoint,
                    "modelOverrides": {
                        endpoint: {
                            "outputTypes": ["image"],
                            "modes": ["text-to-image"],
                            "defaults": {},
                            "capabilities": {},
                            "falContract": contract
                        }
                    }
                }
            }
        })
    };
    let good_contract = json!({
        "outputField": "images",
        "outputArray": true,
        "outputKind": "image",
        "prompt": true,
        "allowedParameters": {
            "steps": {"type": "integer", "min": 1, "max": 50}
        }
    });
    // Custom endpoint succeeds in Config::parse & fal::prepare_with_override!
    let cfg = Config::parse(
        &make_cfg("fal-ai/custom-model/v1", good_contract.clone()).to_string(),
        &dir.join("c.json"),
        &[],
        &HashMap::new(),
    )
    .unwrap();
    let fc =
        &cfg.value["connections"]["fal"]["modelOverrides"]["fal-ai/custom-model/v1"]["falContract"];
    let body = fal::prepare_with_override(
        "fal-ai/custom-model/v1",
        "text-to-image",
        Some(fc),
        Some("cat"),
        &[],
        &serde_json::json!({"steps": 20}),
    )
    .unwrap();
    assert_eq!(body["steps"], 20);
    // Undeclared parameter rejected.
    assert!(
        fal::prepare_with_override(
            "fal-ai/custom-model/v1",
            "text-to-image",
            Some(fc),
            Some("cat"),
            &[],
            &serde_json::json!({"undeclared": 1}),
        )
        .is_err()
    );
    // Built-in endpoint cannot be overridden with falContract.
    assert!(
        Config::parse(
            &make_cfg("fal-ai/flux/dev", good_contract.clone()).to_string(),
            &dir.join("c.json"),
            &[],
            &HashMap::new(),
        )
        .is_err()
    );
    // Traversal endpoint ID rejected.
    assert!(
        Config::parse(
            &make_cfg("fal-ai/../evil", good_contract).to_string(),
            &dir.join("c.json"),
            &[],
            &HashMap::new(),
        )
        .is_err()
    );
}
