mod common;
use common::Workspace;
use numera_gen_mcp::{compat::value::json, files::output::{self, Publication}, jobs::{Receipt, Store}, media::Kind};

#[tokio::test]
async fn fal_uses_only_the_selected_key_authentication_header() {
    use numera_gen_mcp::{config::credentials, runtime_paths::RuntimePaths};
    let work=Workspace::new();let paths=RuntimePaths::new(&work.0).unwrap();
    let headers=credentials(&json!({"auth":{"type":"key","apiKey":"synthetic-fal-only"}}),&std::collections::HashMap::new(),&paths).await.unwrap();
    assert_eq!(headers.len(),1);assert_eq!(headers["Authorization"],"Key synthetic-fal-only");
}

fn wav() -> Vec<u8> {
    let mut bytes=b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x04\0\0\0".to_vec();
    bytes.extend_from_slice(&[0,0,1,0]);bytes
}
#[test]
fn typed_original_publication_is_stable_and_rejects_changed_owned_bytes() {
    let work=Workspace::new();let config=json!({"outputDir":work.0.join("outputs").to_str().unwrap(),"files":{"maxOutputBytes":1024,"maxPixels":64}});
    let mut store=Store::open(&work.0.join("state")).unwrap();
    let receipt=Receipt::new(json!({"request_id":"media-original","operation":"media","status":"prepared","outputs":[],"deviations":[],"errors":[],"generation_outcome":"not_submitted"}));
    let (_,receipt)=store.prepare(receipt,"hash","identity").unwrap();
    store.reserve("media-original",1024,2048).unwrap();let bytes=wav();store.retain("media-original",0,&bytes,Some(&receipt)).unwrap();
    let save=|store:&mut Store| output::save_media(Kind::Audio,"audio",&bytes,&config,store,Publication{id:"media-original",index:0,subdir:Some("nested 日本語"),prefix:None,deviations:&[],expected:Some(&receipt),stable:true});
    let first=save(&mut store).unwrap();let second=save(&mut store).unwrap();assert_eq!(first,second);
    assert_eq!(first["kind"],"audio");assert_eq!(first["format"],"wav");assert!(first.get("width").is_none());
    assert_eq!(first["media"]["channels"],1);assert_eq!(first["media"]["sample_count"],2);
    let (info,saved)=output::owned_media(&config,&store,first["output_id"].as_str().unwrap()).unwrap();assert_eq!(saved,bytes);assert_eq!(info,first);
    assert_eq!(store.list_outputs(0,10).unwrap().len(),1);
    std::fs::write(first["path"].as_str().unwrap(),b"changed").unwrap();assert!(output::owned_media(&config,&store,first["output_id"].as_str().unwrap()).is_err());
}

#[test]
fn sixty_four_media_originals_share_fences_capacity_and_stable_restart_publication() {
    let work = Workspace::new();
    let directory = work.0.join("state");
    let config = json!({"outputDir":work.0.join("outputs").to_str().unwrap(),"files":{"maxOutputBytes":1024,"maxPixels":64}});
    let mut first = Store::open(&directory).unwrap();
    let value = Receipt::new(json!({"request_id":"many-media","operation":"media","status":"prepared","outputs":[],"deviations":[],"errors":[],"generation_outcome":"not_submitted"}));
    let (_, original) = first.prepare(value, "media-hash", "account").unwrap();
    let bytes = wav();
    let total = (bytes.len() * 64) as i64;
    first.reserve("many-media", total, total * 2).unwrap();
    first.retain_url("many-media", 63, &"https://example.invalid/original.wav".into(), Some(&original)).unwrap();
    for index in 0..64 {
        first.retain("many-media", index, &bytes, Some(&original)).unwrap();
    }
    let retained = first.pending("many-media").unwrap();
    assert_eq!(retained.len(), 64);
    assert_eq!(retained.iter().map(|item| item.index).collect::<Vec<_>>(), (0..64).collect::<Vec<_>>());
    assert!(retained.iter().all(|item| item.bytes.as_deref() == Some(bytes.as_slice()) && item.url.is_none()));
    let ids = retained.iter().map(|item| item.output_id.clone()).collect::<Vec<_>>();
    assert_eq!(ids.iter().collect::<std::collections::HashSet<_>>().len(), 64);
    assert_eq!(first.retain("many-media", 64, &bytes, Some(&original)).unwrap_err().code, "invalid_response");
    assert_eq!(first.retain("many-media", 63, b"changed", Some(&original)).unwrap_err().code, "invalid_response");
    // Even a terminal receipt cannot hide retained overflow bytes from another request's budget.
    let mut terminal = first.get("many-media").unwrap();
    terminal.value["status"] = json!("partial");
    first.update(&mut terminal).unwrap();
    first.release("many-media", Some(&original)).unwrap();
    let mut reopened = Store::open(&directory).unwrap();
    let (_, other) = reopened.prepare(Receipt::new(json!({"request_id":"other","status":"prepared","outputs":[],"errors":[],"deviations":[]})), "other", "account").unwrap();
    assert_eq!(reopened.reserve("other", 1, total).unwrap_err().code, "rate_limited");
    reopened.release("other", Some(&other)).unwrap();
    assert!(reopened.needs_local_recovery("many-media").unwrap());
    assert!(reopened.claim("many-media", None, false).unwrap());
    let current = reopened.get("many-media").unwrap();
    assert_eq!(first.retain("many-media", 63, &bytes, Some(&original)).unwrap_err().code, "outcome_unknown");
    reopened.reserve("many-media", total, total * 2).unwrap();
    assert_eq!(reopened.pending("many-media").unwrap().iter().map(|item| item.output_id.clone()).collect::<Vec<_>>(), ids);
    for index in 0..64 {
        let save = |store: &mut Store| output::save_media(Kind::Audio, "audio", &bytes, &config, store,
            Publication { id:"many-media", index, subdir:None, prefix:None, deviations:&[], expected:Some(&current), stable:true });
        let saved = save(&mut reopened).unwrap();
        assert_eq!(saved["output_id"], ids[index as usize].as_str());
        assert_eq!(save(&mut reopened).unwrap(), saved);
        let (_, original_bytes) = output::owned_media(&config, &reopened, saved["output_id"].as_str().unwrap()).unwrap();
        assert_eq!(original_bytes, bytes);
    }
    assert!(!reopened.has_pending("many-media").unwrap());
    assert_eq!(reopened.get("many-media").unwrap().value["outputs"].as_array().unwrap().len(), 64);
    assert_eq!(reopened.list_outputs(0, 100).unwrap().len(), 64);
    reopened.release("many-media", Some(&current)).unwrap();
}

#[tokio::test]
async fn cancelled_media_overflow_recovers_originals_locally_without_a_connection() {
    use numera_gen_mcp::{config::Config, logging::Logger, runtime_paths::RuntimePaths,
        services::{generation::Generation, media_jobs}};
    use std::{collections::HashMap, sync::{Arc, Mutex}};
    use tokio_util::sync::CancellationToken;
    let work = Workspace::new();
    let mut config = Config::parse(&json!({"schemaVersion":1,"defaultConnection":"original",
        "outputDir":work.0.join("outputs").to_str().unwrap(),"stateDir":work.0.join("state").to_str().unwrap(),
        "logging":{"directory":work.0.join("logs").to_str().unwrap()},
        "connections":{"original":{"adapter":"openai-images","baseUrl":"https://example.invalid/v1","auth":{"type":"none"}}}
    }).to_string(), &work.0.join("config.json"), &[], &HashMap::new()).unwrap();
    let mut store = Store::open(&work.0.join("state")).unwrap();
    let (_, owner) = store.prepare(Receipt::new(json!({"request_id":"local-overflow","operation":"media",
        "connection":"original","status":"finalizing","outputs":[],"errors":[],"warnings":[],"deviations":[],
        "generation_outcome":"not_submitted","media_items":vec![json!({"kind":"audio","role":"audio"});11]})),
        "original-effective-request", "original-account").unwrap();
    store.reserve("local-overflow", 1024, 2048).unwrap();
    let bytes = wav();
    for index in 0..11 { store.retain("local-overflow", index, &bytes, Some(&owner)).unwrap(); }
    let ids = store.pending("local-overflow").unwrap().iter().map(|item| item.output_id.clone()).collect::<Vec<_>>();
    store.cancel("local-overflow").unwrap();
    store.release("local-overflow", Some(&owner)).unwrap();
    config.value["connections"] = json!({});
    config.value.as_object_mut().unwrap().remove("defaultConnection");
    let generation = Generation { config:Arc::new(config), store:Arc::new(Mutex::new(store)),
        logger:Arc::new(Logger::new(&work.0.join("logs"),"ERROR")), runtime:RuntimePaths::new(&work.0).unwrap(),
        env:HashMap::new().into(), active:Arc::new(Mutex::new(HashMap::new())) };
    let stopped = CancellationToken::new(); stopped.cancel();
    let unchanged = media_jobs::get(&generation,"local-overflow",true,&stopped).await.unwrap();
    assert!(unchanged.value["outputs"].as_array().unwrap().is_empty());
    let recovered = tokio::time::timeout(std::time::Duration::from_secs(3),
        media_jobs::get(&generation,"local-overflow",true,&CancellationToken::new())).await.unwrap().unwrap();
    assert_eq!(recovered.status(), "completed");
    let outputs = recovered.value["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 11);
    for (index, output) in outputs.iter().enumerate() {
        assert_eq!(output["output_id"], ids[index].as_str());
        assert_eq!(std::fs::read(output["path"].as_str().unwrap()).unwrap(), bytes);
    }
    assert!(!generation.store.lock().unwrap().has_pending("local-overflow").unwrap());
}
