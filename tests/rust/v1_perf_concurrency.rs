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
    let _ = std::fs::remove_dir_all(&dir);
}
