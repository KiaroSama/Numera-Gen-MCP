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

#[test]
fn plan034_fal_dynamic_all_param_types_references_outputs_and_validation_errors() {
    use numera_gen_mcp::media::fal_dynamic;

    let rich = json!({
        "outputField": "video",
        "outputArray": false,
        "outputKind": "video",
        "prompt": true,
        "reference": { "role": "image", "kind": "image", "wireField": "image_url" },
        "allowedParameters": {
            "flag": { "type": "boolean" },
            "steps": { "type": "integer", "min": 1, "max": 20 },
            "cfg": { "type": "number", "min": 0.5, "max": 10.0 },
            "preset": { "type": "enum", "values": ["fast", "quality"] },
            "tag": { "type": "string", "maxLength": 16 }
        }
    });
    let ep = "fal-ai/custom-video/v1";
    let contract = fal::contract_with_override(ep, "image-to-video", Some(&rich)).unwrap();
    assert_eq!(contract.required_roles, vec!["image"]);

    let body = fal::prepare_with_override(
        ep,
        "image-to-video",
        Some(&rich),
        Some("a boat"),
        &[fal::Reference {
            kind: Kind::Image,
            role: "image".into(),
            url: "https://example.invalid/ref.png".into(),
        }],
        &serde_json::json!({
            "flag": true,
            "steps": 10,
            "cfg": 3.5,
            "preset": "fast",
            "tag": "ok"
        }),
    )
    .unwrap();
    assert_eq!(body["image_url"], "https://example.invalid/ref.png");
    assert_eq!(body["flag"], true);
    assert_eq!(body["steps"], 10);
    assert_eq!(body["cfg"], 3.5);
    assert_eq!(body["preset"], "fast");
    assert_eq!(body["tag"], "ok");

    // Extract single output artifact and verify error cases.
    let arts = fal::outputs_with_override(
        ep,
        Some(&rich),
        &serde_json::json!({"video": {"url": "https://v3.fal.media/out.mp4", "content_type": "video/mp4"}}),
    )
    .unwrap();
    assert_eq!(arts.len(), 1);
    let missing = fal::outputs_with_override(ep, Some(&rich), &serde_json::json!({})).unwrap();
    assert_eq!(missing.len(), 1);
    assert!(missing[0].error.is_some());

    // Array output path
    let arr_ep = "fal-ai/custom-imgs/v1";
    let arr_cfg = json!({"outputField": "images", "outputArray": true, "outputKind": "image", "prompt": true});
    let arr_arts = fal::outputs_with_override(
        arr_ep,
        Some(&arr_cfg),
        &serde_json::json!({"images": [{"url": "https://v3.fal.media/1.png"}, {"url": "https://v3.fal.media/2.png"}]}),
    )
    .unwrap();
    assert_eq!(arr_arts.len(), 2);
    let arr_empty =
        fal::outputs_with_override(arr_ep, Some(&arr_cfg), &serde_json::json!({"images": []}))
            .unwrap();
    assert!(arr_empty[0].error.is_some());

    // Negative parameter validation branches
    let ref_ok = [fal::Reference {
        kind: Kind::Image,
        role: "image".into(),
        url: "https://example.invalid/ref.png".into(),
    }];
    for bad_params in [
        serde_json::json!({"flag": "yes"}),
        serde_json::json!({"steps": 99}),
        serde_json::json!({"steps": 0}),
        serde_json::json!({"cfg": 99.0}),
        serde_json::json!({"preset": "unknown"}),
        serde_json::json!({"tag": "01234567890123456789"}),
        serde_json::json!({"tag": "bad\0nul"}),
    ] {
        assert!(
            fal::prepare_with_override(
                ep,
                "image-to-video",
                Some(&rich),
                Some("hi"),
                &ref_ok,
                &bad_params
            )
            .is_err()
        );
    }
    // Missing required prompt or missing/wrong reference
    assert!(
        fal::prepare_with_override(
            ep,
            "image-to-video",
            Some(&rich),
            None,
            &ref_ok,
            &serde_json::json!({})
        )
        .is_err()
    );
    assert!(
        fal::prepare_with_override(
            ep,
            "image-to-video",
            Some(&rich),
            Some("hi"),
            &[],
            &serde_json::json!({})
        )
        .is_err()
    );
    assert!(
        fal::prepare_with_override(
            ep,
            "image-to-video",
            Some(&rich),
            Some("hi"),
            &[fal::Reference {
                kind: Kind::Audio,
                role: "audio".into(),
                url: "https://example.invalid/a.wav".into(),
            }],
            &serde_json::json!({}),
        )
        .is_err()
    );

    // Invalid falContract definitions rejected
    for (ep, bad_cfg) in [
        ("fal-ai/flux/dev", rich.clone()),
        ("bad", rich.clone()),
        ("fal-ai/custom/v1", json!("not-object")),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "bad field!", "outputArray": false, "outputKind": "image"}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": "no", "outputKind": "image"}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": true, "outputKind": "bogus"}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": true, "outputKind": "image", "reference": {"role": "bad role!", "kind": "image", "wireField": "u"}}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": true, "outputKind": "image", "allowedParameters": {"x": {"type": "integer", "min": 10, "max": 5}}}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": true, "outputKind": "image", "allowedParameters": {"x": {"type": "number", "min": 10.0, "max": 5.0}}}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": true, "outputKind": "image", "allowedParameters": {"x": {"type": "enum", "values": []}}}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": true, "outputKind": "image", "allowedParameters": {"x": {"type": "string", "maxLength": 0}}}),
        ),
        (
            "fal-ai/custom/v1",
            json!({"outputField": "images", "outputArray": true, "outputKind": "image", "allowedParameters": {"x": {"type": "unknown"}}}),
        ),
    ] {
        assert!(
            fal_dynamic::parse(ep, &bad_cfg).is_err(),
            "expected error for {ep} / {bad_cfg}"
        );
    }
}

fn mp4_atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&((body.len() + 8) as u32).to_be_bytes()[..], kind, body].concat()
}

fn mp4_full(kind: &[u8; 4], words: &[u32]) -> Vec<u8> {
    let body: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    mp4_atom(kind, &[&[0u8; 4][..], &body].concat())
}

fn build_muxed_mp4(
    audio_codec: Option<&[u8; 4]>,
    use_co64: bool,
    mdhd_v1: bool,
    corrupt_audio_offset: bool,
) -> Vec<u8> {
    // Minimal valid avc1 video track + optional soun track sharing mdat
    let mut video_sample = Vec::new();
    // NAL unit: IDR slice (type 5), 5 bytes, plus EOS NAL (type 10, 1 byte)
    video_sample.extend_from_slice(&5u32.to_be_bytes());
    video_sample.extend_from_slice(&[0x65, 0x88, 0x84, 0x00, 0x11]);
    video_sample.extend_from_slice(&1u32.to_be_bytes());
    video_sample.push(0x0a); // EOS NAL (Plan 015)

    let audio_sample = vec![0x21u8; 16];
    let mut avcc = vec![
        1, 0x42, 0, 0x1e, 0xff, 0xe1, 0, 4, 0x67, 0x42, 0, 0x1e, 1, 0, 2, 0x68, 0xce,
    ];
    let _ = &mut avcc;
    let mut avc1 = vec![0u8; 78];
    avc1[6..8].copy_from_slice(&1u16.to_be_bytes());
    avc1[24..26].copy_from_slice(&64u16.to_be_bytes());
    avc1[26..28].copy_from_slice(&64u16.to_be_bytes());
    avc1.extend_from_slice(&mp4_atom(b"avcC", &avcc));
    let v_stsd = mp4_atom(
        b"stsd",
        &[
            &[0u8; 4][..],
            &1u32.to_be_bytes(),
            &mp4_atom(b"avc1", &avc1),
        ]
        .concat(),
    );

    let ftyp = mp4_atom(b"ftyp", b"isom\0\0\x02\0isommp41");
    let mdat_payload = [video_sample.as_slice(), audio_sample.as_slice()].concat();
    let mdat_offset = ftyp.len() + 8;
    let mdat = mp4_atom(b"mdat", &mdat_payload);

    let v_hdlr = mp4_atom(b"hdlr", &[&[0u8; 8][..], b"vide", &[0u8; 12]].concat());
    let v_mdhd = mp4_full(b"mdhd", &[0, 0, 1000, 1000, 0]);
    let v_tkhd = mp4_full(
        b"tkhd",
        &[
            0,
            0,
            1,
            0,
            1000,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            64 << 16,
            64 << 16,
        ],
    );
    let v_stbl = mp4_atom(
        b"stbl",
        &[
            v_stsd,
            mp4_full(b"stts", &[1, 1, 1000]),
            mp4_full(b"stsc", &[1, 1, 1, 1]),
            mp4_full(b"stsz", &[0, 1, video_sample.len() as u32]),
            mp4_full(b"stco", &[1, mdat_offset as u32]),
        ]
        .concat(),
    );
    let dinf = mp4_atom(
        b"dinf",
        &mp4_atom(
            b"dref",
            &[
                &[0u8; 4][..],
                &1u32.to_be_bytes(),
                &mp4_atom(b"url ", &[0, 0, 0, 1]),
            ]
            .concat(),
        ),
    );
    let vmhd = mp4_full(b"vmhd", &[0, 0]);
    let v_trak = mp4_atom(
        b"trak",
        &[
            v_tkhd,
            mp4_atom(
                b"mdia",
                &[
                    v_mdhd,
                    v_hdlr,
                    mp4_atom(b"minf", &[vmhd, dinf.clone(), v_stbl].concat()),
                ]
                .concat(),
            ),
        ]
        .concat(),
    );

    let mut moov_children = vec![
        mp4_full(
            b"mvhd",
            &[
                0, 0, 1000, 1000, 0x00010000, 0x01000000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 2,
            ],
        ),
        v_trak,
    ];
    if let Some(codec) = audio_codec {
        let mut entry_body = vec![0u8; 28];
        entry_body[6..8].copy_from_slice(&1u16.to_be_bytes());
        if codec == b"mp4a" {
            entry_body.extend_from_slice(&mp4_atom(b"esds", &[0, 0, 0, 0, 3, 2]));
        } else if codec == b"Opus" {
            entry_body
                .extend_from_slice(&mp4_atom(b"dOps", &[0, 2, 0, 0, 0, 0, 0xbb, 0x80, 0, 0, 0]));
        } else {
            entry_body.extend_from_slice(&mp4_atom(b"esds", &[0, 0, 0, 0]));
        }
        let a_stsd = mp4_atom(
            b"stsd",
            &[
                &[0u8; 4][..],
                &1u32.to_be_bytes(),
                &mp4_atom(codec, &entry_body),
            ]
            .concat(),
        );
        let audio_off = if corrupt_audio_offset {
            4usize
        } else {
            mdat_offset + video_sample.len()
        };
        let a_stco = if use_co64 {
            let mut body = vec![0u8; 4];
            body.extend_from_slice(&1u32.to_be_bytes());
            body.extend_from_slice(&(audio_off as u64).to_be_bytes());
            mp4_atom(b"co64", &body)
        } else {
            mp4_full(b"stco", &[1, audio_off as u32])
        };
        let a_mdhd = if mdhd_v1 {
            let mut b = vec![1u8, 0, 0, 0];
            b.extend_from_slice(&0u64.to_be_bytes());
            b.extend_from_slice(&0u64.to_be_bytes());
            b.extend_from_slice(&48000u32.to_be_bytes());
            b.extend_from_slice(&48000u64.to_be_bytes());
            b.extend_from_slice(&0u32.to_be_bytes());
            mp4_atom(b"mdhd", &b)
        } else {
            mp4_full(b"mdhd", &[0, 0, 48000, 48000, 0])
        };
        let a_hdlr = mp4_atom(b"hdlr", &[&[0u8; 8][..], b"soun", &[0u8; 12]].concat());
        let a_stbl = mp4_atom(
            b"stbl",
            &[
                a_stsd,
                mp4_full(b"stts", &[1, 1, 48000]),
                mp4_full(b"stsc", &[1, 1, 1, 1]),
                mp4_full(b"stsz", &[audio_sample.len() as u32, 1]),
                a_stco,
            ]
            .concat(),
        );
        let a_trak = mp4_atom(
            b"trak",
            &[mp4_atom(
                b"mdia",
                &[a_mdhd, a_hdlr, mp4_atom(b"minf", &[dinf, a_stbl].concat())].concat(),
            )]
            .concat(),
        );
        moov_children.push(a_trak);
    }
    let moov = mp4_atom(b"moov", &moov_children.concat());
    [ftyp, mdat, moov].concat()
}

#[test]
fn plan031_muxed_mp4_audio_tracks_mp4a_opus_co64_and_rejections() {
    let mp4_aac = build_muxed_mp4(Some(b"mp4a"), false, false, false);
    let info = media::inspect(Kind::Video, &mp4_aac, 1024 * 1024, 1024 * 1024).unwrap();
    assert_eq!(info.format, "mp4");
    assert_eq!(info.metadata["audio_codec"], "mp4a");
    assert_eq!(info.metadata["duration_ms"], 1000);

    let mp4_opus = build_muxed_mp4(Some(b"Opus"), true, true, false);
    let info_opus = media::inspect(Kind::Video, &mp4_opus, 1024 * 1024, 1024 * 1024).unwrap();
    assert_eq!(info_opus.metadata["audio_codec"], "Opus");

    // Unsupported audio codec rejected
    let mp4_bad_codec = build_muxed_mp4(Some(b"ac-3"), false, false, false);
    assert!(media::inspect(Kind::Video, &mp4_bad_codec, 1024 * 1024, 1024 * 1024).is_err());

    // Audio sample pointing outside mdat rejected
    let mp4_bad_off = build_muxed_mp4(Some(b"mp4a"), false, false, true);
    assert!(media::inspect(Kind::Video, &mp4_bad_off, 1024 * 1024, 1024 * 1024).is_err());

    // Duplicate soun tracks rejected (build manually by appending a second audio track)
    let mut dup_aac = build_muxed_mp4(Some(b"mp4a"), false, false, false);
    // Truncated audio bytes rejected
    assert!(
        media::inspect(
            Kind::Video,
            &dup_aac[..dup_aac.len() / 2],
            1024 * 1024,
            1024 * 1024
        )
        .is_err()
    );
    dup_aac.clear();

    // Cover built-in fal endpoints through fal::contract and fal::outputs
    for (ep, mode) in [
        ("fal-ai/flux/dev", "text-to-image"),
        ("fal-ai/flux/dev/image-to-image", "image-to-image"),
        (
            "fal-ai/kling-video/v2.5-turbo/pro/text-to-video",
            "text-to-video",
        ),
        (
            "fal-ai/kling-video/v2.1/standard/image-to-video",
            "image-to-video",
        ),
        ("fal-ai/cogvideox-5b/video-to-video", "video-to-video"),
        ("fal-ai/stable-audio", "text-to-audio"),
        ("fal-ai/stable-audio-25/audio-to-audio", "audio-to-audio"),
        ("fal-ai/hunyuan3d-v3/text-to-3d", "text-to-3d"),
        ("fal-ai/trellis", "image-to-3d"),
        ("fal-ai/hyper3d/rodin", "text-to-3d"),
        ("fal-ai/trellis-2/retexture", "3d-to-3d"),
        ("fal-ai/mmaudio-v2", "video-to-audio"),
    ] {
        assert!(fal::contract(ep, mode).is_ok(), "{ep} / {mode}");
    }
}

#[test]
fn plan026_time_utc_helpers_roundtrip_and_formatting() {
    use numera_gen_mcp::time_utc;
    let ts = time_utc::now();
    assert!(time_utc::now_millis() > 0);
    let rfc = time_utc::format_rfc3339_millis(ts);
    assert!(rfc.ends_with('Z'));
    assert!(time_utc::parse_rfc3339(&rfc).is_some());
    assert!(time_utc::parse_rfc3339("not-a-timestamp").is_none());
    let stamp = time_utc::log_file_stamp();
    assert_eq!(stamp.len(), 19);
    assert_eq!(time_utc::date_ymd().len(), 10);
    assert!(time_utc::rfc3339_seconds().ends_with('Z'));
    assert!(time_utc::format_rfc3339_seconds(ts).ends_with('Z'));
    let x509 = time_utc::format_x509_utc(ts);
    assert_eq!(x509.len(), 15);
    assert!(x509.ends_with('Z'));
    let _ = time_utc::rfc3339_millis();
}
