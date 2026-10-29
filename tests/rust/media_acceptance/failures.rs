use super::{fixtures as f, host::{self, structured}, provider::{Provider, Reply}, support as s};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};

pub async fn run() {
    for case in ["anonymous", "malformed", "oversized", "mime-mismatch", "partial", "terminal-error", "wrong-id", "unsafe-handle"] {
        host::scenario(25000, scenario(case)).await;
    }
    validation_before_submit().await;
    configured_model_policy_precedes_provider_io().await;
    account_mismatch().await;
    many_role_originals().await;
}

async fn many_role_originals() {
    host::scenario(35000, async {
        let work = f::workspace(); let directory = f::directory(&work, "media-many-roles");
        let endpoint = "fal-ai/hyper3d/rodin";
        let (mesh, _, mesh_bytes) = s::data("3d");
        let (texture, _, texture_bytes) = s::data("image");
        let output = json!({"model_mesh":{"url":mesh,"content_type":"model/gltf-binary"},
            "textures":vec![json!({"url":texture,"content_type":"image/png"});63]});
        let wire = s::normal_provider(endpoint, output, Arc::new(AtomicUsize::new(0))).await;
        let config = s::config(&directory, &wire.origin, &[(endpoint,"text-to-3d",&["3d","image"])]);
        let mut first = s::start(&directory, &config, "media-many-roles").await;
        let done = s::generate(&first, s::request("many-roles",endpoint,"text-to-3d",true,&[])).await;
        assert_eq!(done["status"], "completed", "{done}");
        let outputs = done["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 64, "all supported primary and sidecar originals must survive");
        let mut ids = std::collections::HashSet::new();
        for (index, output) in outputs.iter().enumerate() {
            assert!(ids.insert(output["output_id"].as_str().unwrap().to_owned()));
            assert_eq!(output["kind"], if index == 0 { "3d" } else { "image" });
            assert_eq!(output["role"], if index == 0 { "model" } else { "texture" });
            let bytes = if index == 0 { &mesh_bytes } else { &texture_bytes };
            assert_eq!(&std::fs::read(output["path"].as_str().unwrap()).unwrap(), bytes);
            assert_eq!(output["sha256"], numera_gen_mcp::compat::hash::sha256(bytes));
        }
        assert_eq!(wire.posts(), 1);
        first.close().await.unwrap();
        let mut reopened = s::start(&directory, &config, "media-many-roles-reopened").await;
        let retained = reopened.job("many-roles", true).await;
        assert_eq!(retained["outputs"], done["outputs"], "restart cannot duplicate or rename originals");
        assert_eq!(wire.posts(), 1);
        let info = structured(&reopened.call("get_media_output_info",json!({"output_id":outputs[63]["output_id"]})).await);
        assert_eq!(info["verified"], true);
        assert_eq!(info["role"], "texture");
        reopened.close().await.unwrap(); wire.close().await;
    }).await;
}

async fn scenario(case: &'static str) {
    let work=f::workspace();let directory=f::directory(&work,&format!("media-failure-{case}"));
    let (_,mime,bytes)=s::data("audio");
    let served=bytes.clone();let asset_count=Arc::new(AtomicUsize::new(0));let count=asset_count.clone();
    let asset=Provider::new(move|request,_|{let bytes=served.clone();let count=count.clone();async move{
        assert_eq!(request.method,"GET");assert_eq!(request.header("authorization"),None);
        assert_eq!(request.header("x-fal-store-io"),None);count.fetch_add(1,Ordering::SeqCst);
        Ok(Reply::bytes(200,"audio/wav",bytes))
    }}).await;
    let target=format!("{}/audio.wav",asset.origin);
    let (inline,_,_)=s::data("audio");
    let output=if case=="anonymous" {s::output(s::AUDIO_AUDIO,&target,mime)} else if case=="malformed" {
        s::output(s::AUDIO,&format!("data:audio/wav;base64,{}",STANDARD.encode(b"RIFFnot a complete wave")),mime)
    } else if case=="partial" {
        json!({"model_mesh":{"url":s::data("3d").0,"content_type":"model/gltf-binary"},"textures":[{"url":false}]})
    } else {s::output(s::AUDIO,&inline,if case=="mime-mismatch"{"image/png"}else{mime})};
    let endpoint=if case=="partial" {"fal-ai/hyper3d/rodin"}else if case=="anonymous"{s::AUDIO_AUDIO}else{s::AUDIO};
    let wire=Provider::new(move|request,_|{let output=output.clone();async move{
        s::authenticated(&request);
        if request.method=="POST" {
            if case=="anonymous" {assert!(request.json()["audio_url"].as_str().is_some_and(|url|url.starts_with("data:audio/wav;base64,")));}
            let mut handle=s::handle(endpoint,"failure");
            if case=="unsafe-handle" {handle["status_url"]=json!("https://example.invalid/steal-key");}
            return Ok(Reply::json(handle));
        }
        if request.path.contains("/status") {
            return Ok(Reply::json(if case=="terminal-error" {json!({"request_id":"failure","status":"COMPLETED","error":"synthetic private echo","error_type":"runner_disconnected"})}
                else {json!({"request_id":if case=="wrong-id"{"other-job"}else{"failure"},"status":"COMPLETED"})}));
        }
        Ok(Reply::json(output))
    }}).await;
    let kind=if case=="partial"{"3d"}else{"audio"};
    let mode=if case=="partial"{"text-to-3d"}else if case=="anonymous"{"audio-to-audio"}else{"text-to-audio"};
    let kinds: &[&str]=if case=="partial"{&["3d","image"]}else{&["audio"]};
    let mut config=s::config(&directory,&wire.origin,&[(endpoint,mode,kinds)]);
    // Only this owned loopback origin/path receives the explicit anonymous asset allowance.
    config["files"]["assetAllowances"]=json!([{"origin":asset.origin,"pathPrefix":"/audio.wav"}]);
    if case=="oversized" {config["files"]["maxOutputBytes"]=json!(bytes.len()-1);}
    let mut host=s::start(&directory,&config,"media-failure").await;
    let mut request=s::request("failure",endpoint,mode,true,&[]);
    if case=="anonymous" {request["references"]=json!([{"kind":"audio","role":"audio","url":target}]);}
    let response=host.call_timeout("generate_media",request,10000).await;
    let receipt=host.job("failure",false).await;
    match case {
        "anonymous"=>{s::verified(&host,&receipt,"audio",mime,&bytes).await;assert_eq!(asset_count.load(Ordering::SeqCst),2);},
        "partial"=>{
            assert_eq!(receipt["status"],"partial");
            assert_eq!(receipt["outputs"].as_array().unwrap().len(),1);
            assert_eq!(receipt["outputs"][0]["kind"],kind);
            assert!(!receipt["errors"].as_array().unwrap().is_empty());
            assert_eq!(std::fs::read(receipt["outputs"][0]["path"].as_str().unwrap()).unwrap(),s::data("3d").2);
        },
        "malformed"|"oversized"|"mime-mismatch"|"terminal-error"=>{
            assert!(["failed","partial"].contains(&receipt["status"].as_str().unwrap()),"{receipt}");
            assert!(receipt["outputs"].as_array().unwrap().is_empty());
            assert!(!receipt["errors"].as_array().unwrap().is_empty());
        },
        "wrong-id"|"unsafe-handle"=>{
            if case=="unsafe-handle" {assert_eq!(receipt["status"],"outcome_unknown");assert_eq!(receipt["upstream_job"]["id"],"failure");}
            assert_ne!(receipt["status"],"completed");
            assert!(receipt["outputs"].as_array().unwrap().is_empty());
            assert!(response["isError"]==true||receipt["status"]=="outcome_unknown"||!receipt["errors"].as_array().unwrap().is_empty());
        },
        _=>unreachable!(),
    }
    assert_eq!(wire.posts(),1);
    if case=="unsafe-handle" {
        assert_eq!(wire.captured().len(),1,"unsafe acknowledgement must not automatically poll even its canonical handle");
        let replay=host.call("generate_media",s::request("failure",endpoint,mode,true,&[])).await;
        assert_eq!(structured(&replay)["status"],"outcome_unknown");
        assert_eq!(wire.captured().len(),1,"same-request retry returns uncertainty without new provider I/O");
    }
    let _=host.job("failure",true).await;
    assert_eq!(wire.posts(),1);
    assert!(!host.text().contains(s::KEY),"credential redaction must also hold on errors");
    host.close().await.unwrap();wire.close().await;asset.close().await;
}

async fn validation_before_submit() {
    host::scenario(25000,async{
        let work=f::workspace();let directory=f::directory(&work,"media-input-refusal");
        let wire=Provider::new(|_,_|async{Err("Validation must finish before any provider network operation".into())}).await;
        let config=s::config(&directory,&wire.origin,&[(s::AUDIO_AUDIO,"audio-to-audio",&["audio"])]);
        let mut host=s::start(&directory,&config,"media-input-refusal").await;
        let valid=s::request("valid-shape",s::AUDIO_AUDIO,"audio-to-audio",true,&[("audio","audio")]);
        let mut cases=Vec::new();
        let mut value=valid.clone();value["references"]=json!([]);cases.push(value);
        let mut value=valid.clone();value["references"][0]["role"]=json!("mask");cases.push(value);
        let mut value=valid.clone();value["references"][0]["kind"]=json!("image");cases.push(value);
        let mut value=valid.clone();value["references"][0]["data_url"]=json!("data:audio/wav;base64,AA==");cases.push(value);
        let mut value=valid.clone();value["parameters"]=json!({"audio_url":"https://example.invalid/override"});cases.push(value);
        let mut value=valid.clone();value["mode"]=json!("image-to-video");cases.push(value);
        let mut value=valid.clone();value["references"][0]["path"]=json!(directory.join("outside.wav"));cases.push(value);
        for (index,mut value) in cases.into_iter().enumerate(){
            value["request_id"]=json!(format!("refused-{index}"));
            let refused=host.call("generate_media",value).await;
            assert_eq!(refused["isError"],true,"unsupported input must refuse: {refused}");
        }
        assert!(wire.captured().is_empty());host.close().await.unwrap();wire.close().await;
    }).await;
}

async fn configured_model_policy_precedes_provider_io() {
    host::scenario(25000, async {
        let work = f::workspace(); let directory = f::directory(&work, "media-model-policy");
        let wire = Provider::new(|_, _| async { Err("Denied media model must reject before network I/O".into()) }).await;
        let mut config = s::config(&directory, &wire.origin, &[(s::AUDIO, "text-to-audio", &["audio"])]);
        config["connections"]["local"]["deniedModels"] = json!([s::AUDIO]);
        let mut host = s::start(&directory, &config, "denied-media-model").await;
        let result = host.call("generate_media", s::request("denied-media", s::AUDIO, "text-to-audio", true, &[])).await;
        assert_eq!(result["isError"], true);
        assert!(result.to_string().contains("model_unavailable"));
        assert!(wire.captured().is_empty());
        host.close().await.unwrap();
        config["connections"]["local"]["deniedModels"] = json!([]);
        config["connections"]["local"]["allowedModels"] = json!([s::VIDEO]);
        let mut host = s::start(&directory, &config, "allowlisted-media-model").await;
        let result = host.call("generate_media", s::request("unlisted-media", s::AUDIO, "text-to-audio", true, &[])).await;
        assert_eq!(result["isError"], true);
        assert!(result.to_string().contains("model_unavailable"));
        assert!(wire.captured().is_empty());
        host.close().await.unwrap(); wire.close().await;
    }).await;
}

async fn account_mismatch() {
    host::scenario(30000,async{
        let work=f::workspace();let directory=f::directory(&work,"media-account-pin");
        let status_calls=Arc::new(AtomicUsize::new(0));let calls=status_calls.clone();
        let ready=Arc::new(AtomicBool::new(false));let r=ready.clone();
        let wire=Provider::new(move|request,_|{let calls=calls.clone();let r=r.clone();async move{
            s::authenticated(&request);
            if request.method=="POST"{return Ok(Reply::json(s::handle(s::AUDIO,"account")));}
            calls.fetch_add(1,Ordering::SeqCst);
            assert!(!r.load(Ordering::SeqCst),"changed account cannot poll an old owned handle");
            Ok(Reply::status(json!({"request_id":"account","status":"IN_QUEUE"}),202))
        }}).await;
        let mut config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        let mut first=s::start(&directory,&config,"media-account-first").await;
        let mut request=s::request("account",s::AUDIO,"text-to-audio",true,&[]);request["wait"]=json!(false);
        let _=s::generate(&first,request).await;first.close().await.unwrap();
        let before=status_calls.load(Ordering::SeqCst);ready.store(true,Ordering::SeqCst);
        config["connections"]["local"]["auth"]["apiKey"]=json!("synthetic-other-account-key");
        let mut changed=s::start(&directory,&config,"media-account-changed").await;
        let response=changed.call("get_job",json!({"request_id":"account","refresh":true})).await;
        let local=changed.job("account",false).await;
        assert_ne!(local["status"],"completed");
        assert!(response["isError"]==true||structured(&response)["warnings"].as_array().is_some_and(|warnings|!warnings.is_empty()));
        assert_eq!(wire.posts(),1);assert_eq!(status_calls.load(Ordering::SeqCst),before);
        changed.close().await.unwrap();wire.close().await;
    }).await;
}
