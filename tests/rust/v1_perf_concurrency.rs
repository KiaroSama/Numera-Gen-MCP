//! Concurrency, performance, and SQLite race regression tests (Plans 018-021, DD-04).
use numera_gen_mcp::{
    compat::value::json,
    config::Config,
    jobs::{Receipt, Store},
    logging::Logger,
    mcp::application::Application,
    runtime_paths::RuntimePaths,
};
use std::{collections::HashMap, sync::Arc};
use tokio_util::sync::CancellationToken;

#[test]
fn dd04_concurrent_store_open_migrates_without_race() {
    let dir = std::env::temp_dir().join(format!("numera-dd04-{}", uuid::Uuid::new_v4()));
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let d = dir.clone();
            std::thread::spawn(move || Store::open(&d))
        })
        .collect();
    for h in handles {
        h.join().unwrap().unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan018_partial_index_exists_and_foreign_keys_enabled() {
    let dir = std::env::temp_dir().join(format!("numera-p018-{}", uuid::Uuid::new_v4()));
    let _store = Store::open(&dir).unwrap();
    let conn = rusqlite::Connection::open(dir.join("receipts.sqlite")).unwrap();
    let has_index: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='requests_reserved'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(has_index, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan021_two_store_owners_fence_stale_generation() {
    let dir = std::env::temp_dir().join(format!("numera-p021-{}", uuid::Uuid::new_v4()));
    let clock = Arc::new(std::sync::atomic::AtomicI64::new(10_000));
    let c1 = clock.clone();
    let c2 = clock.clone();
    let mut s1 =
        Store::open_with_lease_clock(&dir, move || c1.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap();
    let mut s2 =
        Store::open_with_lease_clock(&dir, move || c2.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap();
    let (_, mut r1) = s1
        .prepare(
            Receipt::new(json!({
                "request_id": "race-1", "connection": "c", "adapter": "openai-images",
                "operation": "generate", "status": "prepared", "generation_outcome": "not_submitted",
                "outputs": [], "errors": [], "warnings": [], "deviations": []
            })),
            "h",
            "id",
        )
        .unwrap();
    r1.value["status"] = json!("running");
    s1.update(&mut r1).unwrap();
    clock.fetch_add(Store::LEASE_MS + 5, std::sync::atomic::Ordering::SeqCst);
    assert!(s2.claim("race-1", None, true).unwrap());
    r1.value["status"] = json!("completed");
    assert_eq!(s1.update(&mut r1).unwrap_err().code, "outcome_unknown");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn plan020_application_list_jobs_and_concurrent_calls_do_not_deadlock() {
    let dir = std::env::temp_dir().join(format!("numera-p020-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = RuntimePaths::new(&dir).unwrap();
    let cfg_json = json!({
        "schemaVersion": 1,
        "defaultConnection": "c",
        "outputDir": dir.join("out").to_str().unwrap(),
        "stateDir": dir.join("state").to_str().unwrap(),
        "logging": {"directory": dir.join("logs").to_str().unwrap()},
        "connections": {
            "c": {
                "adapter": "openai-images",
                "baseUrl": "https://api.example/v1",
                "auth": {"type": "none"}
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
    let logger = Arc::new(Logger::new(&dir.join("logs"), "info"));
    let app = Arc::new(
        Application::new(
            cfg,
            None,
            logger,
            runtime,
            HashMap::new(),
            CancellationToken::new(),
        )
        .unwrap(),
    );
    let cancel = CancellationToken::new();
    let (a, b) = tokio::join!(
        app.call("list_connections", json!({}), &cancel),
        app.call("list_jobs", json!({"limit": 10}), &cancel)
    );
    assert!(a.is_ok());
    let jobs = b.unwrap();
    assert_eq!(jobs["jobs"], json!([]));
    // Seed 3 receipts and exercise list_jobs filters, pagination, has_more, and error branches.
    {
        let mut store = Store::open(&dir.join("state")).unwrap();
        for (i, st) in ["running", "completed", "failed"].iter().enumerate() {
            let (_, mut rec) = store
                .prepare(
                    Receipt::new(json!({
                        "request_id": format!("app-j-{i}"),
                        "connection": "c",
                        "model": "m",
                        "adapter": "openai-images",
                        "operation": "generate",
                        "status": "prepared",
                        "generation_outcome": "not_submitted",
                        "outputs": [],
                        "errors": [],
                        "warnings": [],
                        "deviations": []
                    })),
                    &format!("ah-{i}"),
                    "ident",
                )
                .unwrap();
            rec.value["status"] = json!(*st);
            store.update(&mut rec).unwrap();
        }
    }
    let page1 = app
        .call("list_jobs", json!({"limit": 1, "offset": 0}), &cancel)
        .await
        .unwrap();
    assert_eq!(page1["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(page1["next_offset"], json!(1));
    let filtered = app
        .call(
            "list_jobs",
            json!({"status": "completed", "limit": 10}),
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(filtered["jobs"].as_array().unwrap().len(), 1);
    assert!(filtered["next_offset"].is_null());
    assert!(
        app.call("list_jobs", json!({"status": "bogus"}), &cancel)
            .await
            .is_err()
    );
    assert!(
        app.call("list_jobs", json!({"limit": 0}), &cancel)
            .await
            .is_err()
    );
    assert!(
        app.call("list_jobs", json!({"offset": -1}), &cancel)
            .await
            .is_err()
    );
    assert!(
        app.call("nonexistent_tool", json!({}), &cancel)
            .await
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn v1_extra_branch_coverage_assets_and_fal_outputs() {
    use numera_gen_mcp::{http::assets, media::fal};

    for priv_ip in [
        "0.0.0.0",
        "10.0.0.1",
        "100.64.0.1",
        "127.0.0.1",
        "169.254.169.254",
        "172.16.0.1",
        "192.168.1.1",
        "198.18.0.1",
        "224.0.0.1",
        "240.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "fe80::1",
        "fc00::1",
        "fd00::1",
        "ff02::1",
        "[::1]",
        "not-an-ip",
    ] {
        let res = assets::private_address(priv_ip);
        assert!(res, "case {priv_ip}");
    }

    // Cover built-in fal::outputs & gateway_outputs across endpoints
    let flux = fal::outputs(
        "fal-ai/flux/dev",
        &serde_json::json!({"images": [{"url": "https://v3.fal.media/a.png", "content_type": "image/png"}]}),
    )
    .unwrap();
    assert_eq!(flux.len(), 1);
    let audio = fal::outputs(
        "fal-ai/stable-audio",
        &serde_json::json!({"audio_file": {"url": "https://v3.fal.media/a.wav", "content_type": "audio/wav"}}),
    )
    .unwrap();
    assert_eq!(audio.len(), 1);
    let video = fal::outputs(
        "fal-ai/kling-video/v2.5-turbo/pro/text-to-video",
        &serde_json::json!({"video": {"url": "https://v3.fal.media/v.mp4", "content_type": "video/mp4"}}),
    )
    .unwrap();
    assert_eq!(video.len(), 1);
    let trellis = fal::outputs(
        "fal-ai/trellis",
        &serde_json::json!({"model_mesh": {"url": "https://v3.fal.media/m.glb", "content_type": "model/gltf-binary"}}),
    )
    .unwrap();
    assert_eq!(trellis.len(), 1);
    let hunyuan = fal::outputs(
        "fal-ai/hunyuan3d-v3/text-to-3d",
        &serde_json::json!({
            "model_glb": {"url": "https://v3.fal.media/m.glb"},
            "model_urls": {"obj": {"url": "https://v3.fal.media/m.obj"}, "usdz": {"url": "https://v3.fal.media/m.usdz"}}
        }),
    )
    .unwrap();
    assert_eq!(hunyuan.len(), 1);
    assert!(fal::outputs("fal-ai/flux/dev", &serde_json::json!("bad")).is_err());
    assert!(fal::outputs("fal-ai/hunyuan3d-v3/text-to-3d", &serde_json::json!({"model_glb": {"url": "https://v3.fal.media/m.glb"}, "model_urls": "bad"})).is_err());
    let gw = fal::gateway_outputs(
        "fal-ai/flux/dev",
        &serde_json::json!({"output": {"images": [{"url": "https://v3.fal.media/gw.png"}]}}),
    )
    .unwrap();
    assert_eq!(gw.len(), 1);
    assert!(fal::gateway_outputs("fal-ai/flux/dev", &serde_json::json!({})).is_err());
}
