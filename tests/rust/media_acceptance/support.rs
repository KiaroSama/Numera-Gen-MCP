use super::{fixtures as f, host::{self, Host, structured}, media_bytes, provider::{Provider, Reply}};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicUsize, Ordering}}};

pub const KEY: &str = "synthetic-media-key";
pub const IMAGE: &str = "fal-ai/flux/dev";
pub const IMAGE_EDIT: &str = "fal-ai/flux/dev/image-to-image";
pub const VIDEO: &str = "fal-ai/kling-video/v2.5-turbo/pro/text-to-video";
pub const IMAGE_VIDEO: &str = "fal-ai/kling-video/v2.1/standard/image-to-video";
pub const VIDEO_VIDEO: &str = "fal-ai/cogvideox-5b/video-to-video";
pub const AUDIO: &str = "fal-ai/stable-audio";
pub const AUDIO_AUDIO: &str = "fal-ai/stable-audio-25/audio-to-audio";
pub const MESH: &str = "fal-ai/hunyuan3d-v3/text-to-3d";
pub const IMAGE_MESH: &str = "fal-ai/trellis";
pub const RETEXTURE: &str = "fal-ai/trellis-2/retexture";

pub fn config(directory: &Path, origin: &str, endpoints: &[(&str, &str, &[&str])]) -> Value {
    let mut overrides = serde_json::Map::new();
    for (endpoint, mode, kinds) in endpoints {
        overrides.insert((*endpoint).into(), json!({"outputTypes":kinds,"modes":[mode]}));
    }
    f::config(directory, json!({"local":{
        "adapter":"fal-queue","baseUrl":origin,"auth":{"type":"key","apiKey":KEY,"origin":origin},
        "defaultModel":endpoints[0].0,"configuredModels":endpoints.iter().map(|(id,_,_)|json!({"id":id,"name":id})).collect::<Vec<_>>(),
        "modelOverrides":overrides,"requestTimeoutMs":5000,
        "maxConcurrentRequests":4
    }}))
}

pub fn data(kind: &str) -> (String, &'static str, Vec<u8>) {
    let (mime, bytes) = match kind {
        "image" => ("image/png", media_bytes::png()),
        "video" => ("video/mp4", media_bytes::mp4()),
        "audio" => ("audio/wav", media_bytes::wav()),
        "3d" => ("model/gltf-binary", media_bytes::glb()),
        _ => panic!("unknown fixture kind"),
    };
    (format!("data:{mime};base64,{}", STANDARD.encode(&bytes)), mime, bytes)
}

pub fn output(endpoint: &str, url: &str, mime: &str) -> Value {
    let file = json!({"url":url,"content_type":mime});
    match endpoint {
        IMAGE | IMAGE_EDIT => json!({"images":[file]}),
        AUDIO => json!({"audio_file":file}),
        AUDIO_AUDIO => json!({"audio":file}),
        MESH => json!({"model_glb":file,"model_urls":{}}),
        IMAGE_MESH => json!({"model_mesh":file,"timings":{}}),
        RETEXTURE => json!({"model_glb":file}),
        _ => json!({"video":file}),
    }
}

pub fn request(id: &str, endpoint: &str, mode: &str, prompt: bool, roles: &[(&str, &str)]) -> Value {
    let references: Vec<_> = roles.iter().map(|(kind,role)| json!({"kind":kind,"role":role,"data_url":data(kind).0})).collect();
    let mut value = json!({"request_id":id,"connection":"local","model":endpoint,"mode":mode,"references":references,"parameters":{},"wait":true});
    if prompt { value["prompt"] = json!("آرام — 東京 123"); }
    value
}

pub fn handle(endpoint: &str, id: &str) -> Value {
    let app = endpoint.split('/').take(2).collect::<Vec<_>>().join("/");
    let base = format!("/{app}/requests/{id}");
    json!({"status":"IN_QUEUE","request_id":id,"status_url":format!("{base}/status"),"response_url":base,"cancel_url":format!("{base}/cancel")})
}

pub fn authenticated(request: &super::provider::Request) {
    assert_eq!(request.header("authorization"), Some("Key synthetic-media-key"));
    if request.method == "POST" {
        assert_eq!(request.header("x-fal-no-retry"), Some("1"));
        assert_eq!(request.header("x-fal-store-io"), Some("0"));
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert!(request.json().get("input").is_none());
    }
}

pub async fn generate(host: &Host, request: Value) -> Value {
    let result = host.call_timeout("generate_media", request, 10000).await;
    assert_ne!(result["isError"], true, "{result}");
    structured(&result)
}

pub async fn verified(host: &Host, receipt: &Value, kind: &str, mime: &str, bytes: &[u8]) -> String {
    assert_eq!(receipt["status"], "completed", "{receipt}");
    let outputs = receipt["outputs"].as_array().expect("owned media output list");
    assert_eq!(outputs.len(), 1);
    let output = &outputs[0];
    assert_eq!(output["kind"], kind);
    assert_eq!(output["mime_type"], mime);
    assert_eq!(std::fs::read(output["path"].as_str().unwrap()).unwrap(), bytes);
    let hash = numera_gen_mcp::compat::hash::sha256(bytes);
    assert_eq!(output["sha256"], hash);
    let id = output["output_id"].as_str().unwrap();
    let info = host.call("get_media_output_info", json!({"output_id":id})).await;
    assert_ne!(info["isError"], true, "{info}");
    let info = structured(&info);
    assert_eq!(info["output_id"], id);
    assert_eq!(info["kind"], kind);
    assert_eq!(info["mime_type"], mime);
    assert_eq!(info["sha256"], hash);
    assert_eq!(info["format"], match kind {"image"=>"png","audio"=>"wav","video"=>"mp4","3d"=>"glb",_=>unreachable!()});
    assert_eq!(info["verified"], true);
    assert_eq!(std::fs::read(info["path"].as_str().unwrap()).unwrap(), bytes);
    if kind != "image" {
        assert!(!info.as_object().unwrap().contains_key("width"));
        assert!(!info.as_object().unwrap().contains_key("height"));
    }
    match kind {
        "image"=>assert_eq!(info["media"]["width"],1),
        "audio"=>assert_eq!(info["media"]["sample_rate"],8000),
        "video"=>assert_eq!(info["media"]["duration_ms"],1000),
        "3d"=>assert_eq!(info["media"]["vertex_count"],3),
        _=>unreachable!(),
    }
    id.into()
}

pub async fn normal_provider(endpoint: &'static str, output: Value, polls: Arc<AtomicUsize>) -> Provider {
    Provider::new(move |request, _| {
        let output = output.clone(); let polls = polls.clone();
        async move {
            authenticated(&request);
            if request.method == "POST" {
                assert_eq!(request.path, format!("/{endpoint}"));
                return Ok(Reply::json(handle(endpoint, "owned")));
            }
            if request.path.ends_with("/status") || request.path.contains("/status?") {
                let first = polls.fetch_add(1, Ordering::SeqCst) == 0;
                return Ok(Reply::status(json!({"request_id":"owned","status":if first { "IN_QUEUE" } else { "COMPLETED" }}), if first { 202 } else { 200 }));
            }
            assert!(request.path.ends_with("/requests/owned"));
            Ok(Reply::json(output))
        }
    }).await
}

pub async fn start(directory: &Path, config: &Value, name: &str) -> Host { f::start(directory, config, name).await }

pub fn input_file(directory: &Path, kind: &str) -> PathBuf {
    let inputs = directory.join("inputs");
    std::fs::create_dir_all(&inputs).unwrap();
    let file = inputs.join(format!("approved.{kind}"));
    std::fs::write(&file, data(kind).2).unwrap();
    file
}

pub async fn settle(host: &Host, id: &str) -> Value {
    host::bounded(10000, async {
        for _ in 0..32 {
            let job = host.job(id, true).await;
            if ["completed", "partial", "failed", "cancelled", "outcome_unknown"].contains(&job["status"].as_str().unwrap_or("")) { return job; }
            tokio::task::yield_now().await;
        }
        panic!("media job did not reach a terminal state within its bounded refresh count");
    }).await
}
