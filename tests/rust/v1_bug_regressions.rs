//! Regression tests for v1.0.0 bug fixes (Plans 008-017, DD-02, DD-03, DD-04).
use numera_gen_mcp::{
    adapters::normalize,
    compat::value::{Value, json},
    config::{Config, ConfigReload},
    http::sse,
    jobs::{Receipt, Store},
    mcp::tools,
    media::{self, Kind, fal},
    services::{fal_image, health, polling},
};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

#[test]
fn plan010_fal_image_rejects_untranslated_fields_and_invalid_edit() {
    for bad in [
        json!({"prompt": "p", "mask_mode": "inpaint"}),
        json!({"prompt": "p", "preserve_subject": true}),
        json!({"prompt": "p", "preserve_style": true}),
        json!({"prompt": "p", "unknown_extra_field": 123}),
    ] {
        assert!(fal_image::to_media_request(&bad, "generate").is_err());
    }
    assert!(fal_image::to_media_request(&json!({"prompt": "p"}), "edit").is_err());
}

#[test]
fn plan011_sse_strict_utf8_last_event_and_gemini_metadata() {
    // Invalid UTF-8 rejected in sse::events().
    let bad_utf8 = b"data: {\"a\":\"\xff\"}\n\n";
    assert!(sse::events(bad_utf8, 1024).is_err());

    // Multiple event: lines use the last non-empty one; control chars rejected.
    let multi = b"event: first\nevent: response.completed\ndata: {\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n";
    let evs = sse::events(multi, 4096).unwrap();
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].kind, "response.completed");

    // Case-insensitive MIME + Gemini promptFeedback/usageMetadata preserved.
    let conn = json!({"adapter": "gemini"});
    let body = b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"inlineData\":{\"mimeType\":\"image/png\",\"data\":\"iVBORw0KGgo=\"}}]}}],\"usageMetadata\":{\"totalTokenCount\":42}}\n\n";
    let norm =
        normalize::response(&conn, body.to_vec(), "TEXT/EVENT-STREAM; charset=utf-8").unwrap();
    assert_eq!(norm.images.len(), 1);
    assert_eq!(norm.usage.as_ref().unwrap()["totalTokenCount"], json!(42));
}

#[test]
fn plan012_fal_outputs_accepts_null_model_urls_and_gateway_cancel_rejects_null_error() {
    let out = fal::outputs(
        "fal-ai/hunyuan3d-v3/text-to-3d",
        &serde_json::json!({
            "model_glb": {"url": "https://v3.fal.media/files/a.glb", "content_type": "model/gltf-binary"},
            "model_urls": null
        }),
    )
    .unwrap();
    assert_eq!(out.len(), 1);

    assert!(fal::gateway_cancel(&serde_json::json!({"error": null}), 404).is_err());
    assert!(matches!(
        fal::gateway_cancel(&serde_json::json!({"error": "Job not found"}), 404).unwrap(),
        fal::Cancel::NotFound
    ));
}

#[tokio::test]
async fn plan012_and_017_direct_fal_health_skips_models_and_reports_dynamic_rustc() {
    let dir = std::env::temp_dir().join(format!("numera-p012-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = numera_gen_mcp::runtime_paths::RuntimePaths::new(&dir).unwrap();
    let cfg_json = json!({
        "schemaVersion": 1,
        "defaultConnection": "fal",
        "outputDir": dir.join("out").to_str().unwrap(),
        "stateDir": dir.join("state").to_str().unwrap(),
        "logging": {"directory": dir.join("logs").to_str().unwrap()},
        "connections": {
            "fal": {
                "adapter": "fal-queue",
                "baseUrl": "https://queue.fal.run",
                "auth": {"type": "key", "apiKey": "dummy-key-never-sent"},
                "defaultModel": "fal-ai/flux/dev",
                "modelOverrides": {
                    "fal-ai/flux/dev": {
                        "outputTypes": ["image"],
                        "modes": ["text-to-image"],
                        "defaults": {},
                        "capabilities": {}
                    }
                }
            }
        }
    });
    let cfg = Config::parse(
        &cfg_json.to_string(),
        &dir.join("c.json"),
        &[],
        &HashMap::new(),
    )
    .unwrap();
    let report = health::health(
        &cfg,
        &HashMap::new(),
        &runtime,
        true,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(report["connections"][0]["reachable"].is_null());
    assert!(report["connections"][0]["error"].is_null());
    let runtime_str = report["runtime"].as_str().unwrap();
    assert!(runtime_str.starts_with("Rust "));
    assert_ne!(runtime_str, "Rust unknown");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dd02_store_update_fences_expired_lease() {
    let dir = std::env::temp_dir().join(format!("numera-dd02-{}", uuid::Uuid::new_v4()));
    let now = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(1_000_000));
    let clock_ref = now.clone();
    let mut store = Store::open_with_lease_clock(&dir, move || {
        clock_ref.load(std::sync::atomic::Ordering::SeqCst)
    })
    .unwrap();
    let (_, mut receipt) = store
        .prepare(
            Receipt::new(json!({
                "request_id": "r1", "connection": "c", "adapter": "openai-images",
                "operation": "generate", "status": "prepared", "generation_outcome": "not_submitted",
                "outputs": [], "errors": [], "warnings": [], "deviations": []
            })),
            "hash",
            "ident",
        )
        .unwrap();
    // Advance clock past lease expiry without renewal -> update MUST fail with outcome_unknown!
    now.fetch_add(Store::LEASE_MS + 10, std::sync::atomic::Ordering::SeqCst);
    receipt.value["status"] = json!("running");
    let err = store.update(&mut receipt).unwrap_err();
    assert_eq!(err.code, "outcome_unknown");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan013_config_reload_prune_unused_prevents_lockout() {
    let dir = std::env::temp_dir().join(format!("numera-p013-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let make = |n: usize| {
        json!({
            "schemaVersion": 1,
            "defaultConnection": "c",
            "outputDir": dir.join("out").to_str().unwrap(),
            "stateDir": dir.join("state").to_str().unwrap(),
            "logging": {"directory": dir.join("logs").to_str().unwrap()},
            "connections": {
                "c": {
                    "adapter": "openai-images",
                    "baseUrl": format!("https://api.example/v{n}"),
                    "auth": {"type": "none"}
                }
            }
        })
        .to_string()
    };
    let initial = Config::parse(&make(0), &dir.join("c.json"), &[], &HashMap::new()).unwrap();
    let mut reload = ConfigReload::new(initial, dir.join("c.json"), vec![], HashMap::new());
    for i in 1..=80 {
        let res = reload.refresh_text(Some(&make(i)), |_| true).unwrap();
        assert_eq!(res.event, numera_gen_mcp::config::ReloadEvent::Accepted);
        reload.prune_unused();
    }
    assert_eq!(reload.snapshots().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan017_tool_annotations_and_backoff_bounds() {
    let list = tools::definitions().unwrap();
    assert_eq!(list.len(), 13);
    let get_job = list.iter().find(|t| t.name == "get_job").unwrap();
    assert_eq!(
        get_job.annotations.as_ref().unwrap().read_only_hint,
        Some(false)
    );
    let list_conns = list.iter().find(|t| t.name == "list_connections").unwrap();
    assert_eq!(
        list_conns.annotations.as_ref().unwrap().open_world_hint,
        Some(false)
    );
    let list_jobs = list.iter().find(|t| t.name == "list_jobs").unwrap();
    assert_eq!(
        list_jobs.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    assert_eq!(
        list_jobs.annotations.as_ref().unwrap().open_world_hint,
        Some(false)
    );

    assert_eq!(polling::backoff_ms(0, 250, 2000), 250);
    assert_eq!(polling::backoff_ms(1, 250, 2000), 500);
    assert_eq!(polling::backoff_ms(2, 250, 2000), 1000);
    assert_eq!(polling::backoff_ms(3, 250, 2000), 2000);
    assert_eq!(polling::backoff_ms(20, 250, 2000), 2000);

    // Plan 017: negative_prompt enforces UTF-16 code unit cap.
    let emoji_over = "😀".repeat(16385); // 32770 UTF-16 code units, 16385 chars
    assert!(
        fal::prepare(
            "fal-ai/kling-video/v2.5-turbo/pro/text-to-video",
            "text-to-video",
            Some("hi"),
            &[],
            &serde_json::json!({"negative_prompt": emoji_over}),
        )
        .is_err()
    );
    let _ = Kind::Video;
    let _ = media::inspect(Kind::Audio, b"bad", 1024, 1024).unwrap_err();
    let _ = Value::Null;
}

#[test]
fn plan011_sse_reader_incremental_chunks_control_chars_and_limits() {
    let mut reader = sse::Reader::new(4096);
    reader.push(b"event: response.in_progress\r\n").unwrap();
    reader.push(b"data: {\"a\":1}\r\n\r\n").unwrap();
    reader.push(b": keepalive comment\n\n").unwrap();
    reader.push(b"event: done\ndata: [DONE]\n\n").unwrap();
    let bytes = reader.finish().unwrap();
    let events = sse::events(&bytes, 4096).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, "response.in_progress");
    assert_eq!(events[1].kind, "done");

    // Control char in event name rejected
    assert!(sse::events(b"event: bad\x01name\ndata: {}\n\n", 1024).is_err());

    // Size limit enforced
    let mut tiny = sse::Reader::new(8);
    assert!(tiny.push(b"data: 1234567890\n\n").is_err());
}

#[test]
fn plan024_and_032_fal_specs_and_list_jobs_pagination() {
    // Cover all SPECS outputs/contracts including mmaudio-v2 video-to-video & Rodin/Hunyuan sidecars
    assert!(fal::contract("fal-ai/mmaudio-v2", "video-to-video").is_ok());
    assert!(fal::contract("fal-ai/mmaudio-v2", "text-to-image").is_err());
    let mm_out = fal::outputs(
        "fal-ai/mmaudio-v2",
        &serde_json::json!({"video": {"url": "https://v3.fal.media/muxed.mp4", "content_type": "video/mp4"}}),
    )
    .unwrap();
    assert_eq!(mm_out.len(), 1);

    let rodin_out = fal::outputs(
        "fal-ai/hyper3d/rodin",
        &serde_json::json!({
            "model_mesh": {"url": "https://v3.fal.media/m.glb"},
            "textures": [{"url": "https://v3.fal.media/t.png"}]
        }),
    )
    .unwrap();
    assert_eq!(rodin_out.len(), 2);

    // Multi-job Store::list_jobs filtering & pagination
    let dir = std::env::temp_dir().join(format!("numera-listjobs-{}", uuid::Uuid::new_v4()));
    let mut store = Store::open(&dir).unwrap();
    for (idx, status) in ["running", "completed", "failed"].iter().enumerate() {
        let (_, mut rec) = store
            .prepare(
                Receipt::new(json!({
                    "request_id": format!("j-{idx}"),
                    "connection": "c",
                    "adapter": "openai-images",
                    "operation": "generate",
                    "status": "prepared",
                    "generation_outcome": "not_submitted",
                    "outputs": [],
                    "errors": [],
                    "warnings": [],
                    "deviations": []
                })),
                &format!("h-{idx}"),
                "id",
            )
            .unwrap();
        rec.value["status"] = json!(*status);
        store.update(&mut rec).unwrap();
    }
    assert_eq!(store.list_jobs(None, 0, 10).unwrap().len(), 3);
    // Store::list_jobs returns up to limit+1 rows so Application can compute has_more.
    assert_eq!(store.list_jobs(None, 1, 1).unwrap().len(), 2);
    assert_eq!(store.list_jobs(Some("failed"), 0, 10).unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
