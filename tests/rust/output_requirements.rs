mod common;
#[path = "common/config.rs"]
mod config_fixture;
#[path = "common/image.rs"]
mod image_fixture;
use common::Workspace;
use numera_image_gen_mcp::compat::value::{Value, json};
use numera_image_gen_mcp::{
    adapters::types::{Normalized, ResultItem},
    config::Config,
    files::images,
    jobs::{Receipt, Store},
    logging::Logger,
    runtime_paths::RuntimePaths,
    services::{finalization, generation::Generation, jobs, request, requirements},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

fn cases() -> Vec<Value> {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".ci-work/parity/output-requirements.json");
    let values: Vec<Value> = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let mut expected: HashSet<String> = [
        "original-aspect-tier",
        "opaque-aspect-tier",
        "known-missing-aspect",
        "known-unmatched-tier",
        "known-unmatched-aspect",
        "no-tier",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for model in ["gemini-3.1-flash-image", "gemini-3-pro-image"] {
        for ratio in [
            "1:1", "1:4", "1:8", "2:3", "3:2", "3:4", "4:1", "4:3", "4:5", "5:4", "8:1", "9:16",
            "16:9", "21:9",
        ] {
            if model == "gemini-3-pro-image" && ["1:4", "1:8", "4:1", "8:1"].contains(&ratio) {
                continue;
            }
            for tier in ["0.5K", "1K", "2K", "4K"] {
                if model == "gemini-3-pro-image" && tier == "0.5K" {
                    continue;
                }
                expected.insert(format!("{model}-{ratio}-{tier}"));
            }
        }
    }
    let names: HashSet<String> = values
        .iter()
        .map(|case| case["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(values.len(), 92);
    assert_eq!(
        names.len(),
        values.len(),
        "source scenario names are unique"
    );
    assert_eq!(
        names, expected,
        "six edges plus all 56 Flash and 30 Pro pairs"
    );
    values
}

#[test]
fn every_source_requirement_deviation_and_warning_matches_exact_metadata_and_json_round_trip() {
    let original = images::inspect(image_fixture::original(), &json!({}), None).unwrap();
    for case in cases() {
        let receipt = Receipt::new(case["receipt"].clone());
        let mut image = original.clone();
        image.width = case["image"]["width"].as_u64().unwrap() as u32;
        image.height = case["image"]["height"].as_u64().unwrap() as u32;
        image.mime = case["image"]["mime"].as_str().unwrap().into();
        image.alpha = case["image"]["alpha"].as_bool().unwrap();
        assert_eq!(
            json!(requirements::deviations(&receipt, &image, 0)),
            case["deviations"],
            "{}",
            case["name"]
        );
        assert_eq!(
            json!(requirements::warnings(&receipt)),
            case["warnings"],
            "{}",
            case["name"]
        );
        let round_trip = Receipt::new(Value::parse(&receipt.value.to_string()).unwrap());
        assert_eq!(
            requirements::deviations(&round_trip, &image, 0),
            requirements::deviations(&receipt, &image, 0)
        );
        assert_eq!(
            requirements::warnings(&round_trip),
            requirements::warnings(&receipt)
        );
    }
}

fn generation(workspace: &Workspace, raw: &Value) -> Generation {
    Generation {
        config: Arc::new(
            Config::parse(
                &raw.to_string(),
                &workspace.0.join("config.json"),
                &[],
                &HashMap::new(),
            )
            .unwrap(),
        ),
        store: Arc::new(Mutex::new(Store::open(&workspace.0.join("state")).unwrap())),
        logger: Arc::new(Logger::new(&workspace.0.join("logs"), "ERROR")),
        runtime: RuntimePaths::new(&workspace.0).unwrap(),
        env: HashMap::new(),
        active: Arc::new(Mutex::new(HashMap::new())),
    }
}

async fn finalized(recover: bool, name: &str) {
    let action = async {
        let workspace = Workspace::new();
        let raw = workspace.config("https://invalid.example/v1", "openai-images");
        let g = generation(&workspace, &raw);
        let oracle = cases()
            .into_iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let bytes = image_fixture::original();
        let mut request_raw = json!({"prompt": "draw", "count": 1});
        request_raw
            .as_object_mut()
            .unwrap()
            .extend(oracle["request"].as_object().unwrap().clone());
        let request = request::parse(request_raw).unwrap();
        let initial = Receipt::new(json!({
            "request_id": "requirements", "connection": "test", "adapter": "openai-images",
            "status": "prepared", "operation": "generate", "requested_model": oracle["model"],
            "outputs": [], "errors": [], "warnings": [], "deviations": [],
            "generation_outcome": "not_submitted", "storage_outcome": "not_started",
            "output_requirements": oracle["receipt"]["output_requirements"],
            "created_at": "2026-10-02T00:00:00.000Z"
        }));
        let receipt = {
            let mut store = g.store.lock().unwrap();
            let (_, receipt) = store.prepare(initial, "hash", "identity").unwrap();
            assert!(store.admit("requirements", "test", 2, 2).unwrap());
            assert!(store.claim("requirements", Some(&receipt), true).unwrap());
            store.get("requirements").unwrap()
        };
        let fault = rusqlite::Connection::open(workspace.0.join("state/receipts.sqlite")).unwrap();
        if recover {
            fault.execute_batch("CREATE TRIGGER reject_output BEFORE INSERT ON outputs BEGIN SELECT RAISE(FAIL,'Controlled commit failure'); END;").unwrap();
        }
        let signal = CancellationToken::new();
        let mut result = finalization::finalize(
            &g.config.value,
            &g.store,
            &g.logger,
            receipt,
            finalization::Finalization {
                result: Some(Normalized {
                    images: vec![ResultItem {
                        bytes: Some(bytes.clone()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                request: &request,
                cancel: &signal,
                recovery: false,
            },
        )
        .await
        .unwrap();
        if recover {
            let pending = g.store.lock().unwrap().pending("requirements").unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].bytes.as_ref().unwrap(), &bytes);
            assert_eq!(result.value["outputs"], json!([]));
            assert_eq!(result.value["errors"][0]["code"], "storage_error");
            fault.execute_batch("DROP TRIGGER reject_output").unwrap();
            let mut removed = g.config.value.clone();
            removed["connections"] = json!({});
            removed.as_object_mut().unwrap().remove("defaultConnection");
            let recovered = Generation {
                config: Arc::new(
                    Config::parse(
                        &removed.to_string(),
                        &workspace.0.join("config.json"),
                        &[],
                        &HashMap::new(),
                    )
                    .unwrap(),
                ),
                store: g.store.clone(),
                logger: g.logger.clone(),
                runtime: g.runtime.clone(),
                env: HashMap::new(),
                active: g.active.clone(),
            };
            result = jobs::get(&recovered, "requirements", true, &signal, None)
                .await
                .unwrap();
            assert_eq!(
                result.value["outputs"][0]["output_id"],
                pending[0].output_id
            );
            assert_eq!(result.value["errors"].as_array().unwrap().len(), 1);
            assert_eq!(result.value["errors"][0]["code"], "storage_error");
            assert_eq!(result.value["storage_outcome"], "partial_or_failed");
        } else {
            assert_eq!(result.value["errors"], json!([]));
            assert_eq!(result.value["storage_outcome"], "completed");
        }
        assert_eq!(result.status(), "partial");
        assert_eq!(result.value["deviations"], oracle["deviations"]);
        assert_eq!(result.value["warnings"], oracle["warnings"]);
        assert_eq!(result.value["outputs"].as_array().unwrap().len(), 1);
        assert_eq!(
            std::fs::read(result.value["outputs"][0]["path"].as_str().unwrap()).unwrap(),
            bytes
        );
        {
            let store = g.store.lock().unwrap();
            assert_eq!(store.list_outputs(0, 10).unwrap().len(), 1);
            assert!(!store.has_pending("requirements").unwrap());
            let stored = store.get("requirements").unwrap();
            assert_eq!(stored.value, result.value);
            assert_eq!(stored.value["output_requirements"]["aspect_ratio"], "16:9");
            assert_eq!(stored.value["output_requirements"]["image_size"], "4K");
        }
        assert_eq!(
            jobs::get(&g, "requirements", true, &signal, None)
                .await
                .unwrap()
                .value,
            result.value
        );
        drop(fault);
    };
    tokio::time::timeout(Duration::from_secs(8), action)
        .await
        .expect("output requirement recovery exceeded 8 seconds");
}

#[tokio::test]
async fn normal_finalization_preserves_exact_source_aspect_tier_deviations_and_original_bytes() {
    finalized(false, "original-aspect-tier").await;
}

#[tokio::test]
async fn real_sql_commit_failure_recovers_identical_source_requirements_without_network() {
    finalized(true, "original-aspect-tier").await;
}

#[tokio::test]
async fn opaque_tier_warning_is_persisted_exactly_once_through_sql_failure_and_local_recovery() {
    finalized(true, "opaque-aspect-tier").await;
}
