mod common;
#[path = "common/config.rs"]
mod config_fixture;
#[path = "common/image.rs"]
mod image_fixture;
use common::Workspace;
use numera_image_gen_mcp::{
    adapters::{comfy, types::Input, workflow},
    capabilities,
    compat::value::{Value, json},
    config::Config,
    files::images,
    runtime_paths::RuntimePaths,
    services::discovery::Discovery,
};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Request {
    path: String,
    headers: String,
    body: Vec<u8>,
}

async fn serve<F, Fut>(reply: impl Fn(&Request) -> Value, action: F)
where
    F: FnOnce(String, Arc<Mutex<Vec<Request>>>) -> Fut,
    Fut: Future<Output = ()>,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let stop = CancellationToken::new();
    let server_stop = stop.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let server = async {
        loop {
            let (mut socket, _) = tokio::select! {
                _ = server_stop.cancelled() => break,
                accepted = listener.accept() => accepted.unwrap(),
            };
            let operation = async {
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let (boundary, length) = loop {
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert!(read > 0, "request must finish");
                    bytes.extend_from_slice(&buffer[..read]);
                    assert!(bytes.len() <= 1048576, "bounded multipart fixture");
                    if let Some(boundary) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&bytes[..boundary]).unwrap();
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= boundary + 4 + length {
                            break (boundary, length);
                        }
                    }
                };
                let headers = String::from_utf8(bytes[..boundary].to_vec()).unwrap();
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned();
                let request = Request {
                    path,
                    headers,
                    body: bytes[boundary + 4..boundary + 4 + length].to_vec(),
                };
                let value = reply(&request);
                captured.lock().unwrap().push(request);
                let body = value.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            };
            tokio::time::timeout(Duration::from_secs(3), operation)
                .await
                .unwrap();
        }
    };
    let client = async {
        let result = tokio::time::timeout(Duration::from_secs(8), action(origin, requests)).await;
        stop.cancel();
        result.expect("bounded contract action");
    };
    tokio::time::timeout(Duration::from_secs(12), async {
        tokio::join!(server, client)
    })
    .await
    .expect("owned server/action lifetime");
}

fn config(workspace: &Workspace, raw: Value) -> Config {
    Config::parse(
        &raw.to_string(),
        &workspace.0.join("config.json"),
        &[],
        &HashMap::new(),
    )
    .unwrap()
}

fn ids(value: &Value) -> Vec<&str> {
    value["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn omniroute_specialty_catalog_is_supplemented_by_paginated_image_only_deduplication() {
    let workspace = Workspace::new();
    serve(|request| {
        if request.path == "/v1/images/generations" { json!({"data":[{"id":"specialty"}]}) }
        else if !request.path.contains("after=") {
            json!({"data":[{"id":"specialty","type":"image"},{"id":"text-model","type":"text"},{"id":"unified-1","type":"image"}],"has_more":true,"last_id":"cursor-1"})
        } else { json!({"data":[{"id":"unified-2","type":"image"}]}) }
    }, |origin, requests| async move {
        let mut raw = workspace.config(&format!("{origin}/v1"), "openai-images");
        raw["connections"]["test"]["gateway"] = json!("omniroute");
        let config = config(&workspace, raw);
        let runtime = RuntimePaths::new(&workspace.0).unwrap();
        let result = Discovery::default().list(&config, &runtime, &HashMap::new(), None, false, &CancellationToken::new()).await.unwrap();
        assert_eq!(ids(&result), ["specialty", "unified-1", "unified-2"]);
        assert_eq!(result["complete"], true);
        assert_eq!(result["warnings"].as_array().unwrap().len(), 1);
        let requests = requests.lock().unwrap();
        let paths: Vec<_> = requests.iter().map(|request| request.path.as_str()).collect();
        assert_eq!(paths, ["/v1/images/generations", "/v1/models?limit=100", "/v1/models?after=cursor-1&limit=100"]);
        assert!(requests.iter().all(|request| request.headers.starts_with("GET ") && request.body.is_empty()));
    }).await;
}

#[tokio::test]
async fn gemini_page_tokens_do_not_mutate_configuration_or_leak_into_fresh_refresh() {
    let workspace = Workspace::new();
    serve(
        |request| {
            if request.path.contains("pageToken=") {
                json!({"models":[{"name":"second"}]})
            } else {
                json!({"models":[{"name":"first"}],"nextPageToken":"token-2"})
            }
        },
        |origin, requests| async move {
            let mut raw = workspace.config(&format!("{origin}/v1beta"), "gemini");
            raw["connections"]["test"]["query"] = json!({"scope":"images"});
            let config = config(&workspace, raw);
            let runtime = RuntimePaths::new(&workspace.0).unwrap();
            let mut discovery = Discovery::default();
            for refresh in [false, true] {
                let result = discovery
                    .list(
                        &config,
                        &runtime,
                        &HashMap::new(),
                        None,
                        refresh,
                        &CancellationToken::new(),
                    )
                    .await
                    .unwrap();
                assert_eq!(ids(&result), ["first", "second"]);
            }
            assert_eq!(
                config.value["connections"]["test"]["query"],
                json!({"scope":"images"})
            );
            let requests = requests.lock().unwrap();
            let paths: Vec<_> = requests
                .iter()
                .map(|request| request.path.as_str())
                .collect();
            assert_eq!(
                paths,
                [
                    "/v1beta/models?scope=images",
                    "/v1beta/models?scope=images&pageToken=token-2",
                    "/v1beta/models?scope=images",
                    "/v1beta/models?scope=images&pageToken=token-2"
                ]
            );
        },
    )
    .await;
}

#[tokio::test]
async fn refreshed_catalog_retains_configured_names_native_ids_and_cached_metadata() {
    let workspace = Workspace::new();
    serve(|_| json!({"data":[{"id":"custom/another","type":"image"},{"id":"upstream/new","type":"image"}]}),
        |origin, requests| async move {
        let raw = json!({"api_endpoint":format!("{origin}/v1"),"api_key":"synthetic-local-key-not-real","models":[{"id":"antigravity/opaque-image","name":"Image 日本語"},{"id":"custom/another","name":"Second"}]});
        let config = config(&workspace, raw);
        let runtime = RuntimePaths::new(&workspace.0).unwrap();
        let mut discovery = Discovery::default();
        let refreshed = discovery.list(&config, &runtime, &HashMap::new(), None, true, &CancellationToken::new()).await.unwrap();
        assert_eq!(ids(&refreshed), ["antigravity/opaque-image", "custom/another", "upstream/new"]);
        assert_eq!(refreshed["models"][1]["name"], "Second");
        assert_eq!(refreshed["models"][1]["metadata"]["advertised"]["id"], "custom/another");
        let model = Discovery::model(&config, &runtime, &HashMap::new(), None, &"custom/another".into(), &CancellationToken::new()).await.unwrap();
        assert_eq!(model["model_id"], "custom/another");
        assert_eq!(model["display_name"], "Second");
        assert_eq!(model["account_availability"], "unknown");
        assert_eq!(discovery.list(&config, &runtime, &HashMap::new(), None, false, &CancellationToken::new()).await.unwrap()["cached"], true);
        assert_eq!(requests.lock().unwrap().len(), 1, "cached metadata cannot trigger extra discovery");
    }).await;
}

fn comfy_config(origin: &str) -> Value {
    json!({"adapter":"comfyui","baseUrl":origin,"baseUrlMode":"origin","requestTimeoutMs":3000,
        "workflow":{"graph":{"text":{"class_type":"Text","inputs":{"text":"","image":"","mask":""}}},
        "bindings":{"prompt":{"node":"text","input":"text"},"image":{"node":"text","input":"image"},"mask":{"node":"text","input":"mask"}},
        "outputNodes":["text"],"requireGpu":false},
        "edit":{"mode":"json","encoding":"image","maxReferences":1,"masks":true,"maskPolarity":"transparent-edit"}})
}

#[tokio::test]
async fn malformed_image_and_mask_upload_identity_prevent_prompt_submission_and_unknown_ids_never_retry()
 {
    let image = images::inspect(image_fixture::original(), &json!({}), None).unwrap();
    serve(
        |request| {
            if request.path == "/upload/image" {
                json!({"name":42,"subfolder":""})
            } else {
                json!({})
            }
        },
        |origin, requests| async move {
            let connection = comfy_config(&origin);
            let request = json!({"prompt":"draw","count":1,"provider_options":{}});
            let model = "workflow".into();
            for mask in [false, true] {
                let references = if mask {
                    Vec::new()
                } else {
                    vec![image.clone()]
                };
                let input = Input {
                    connection: &connection,
                    request: &request,
                    model: &model,
                    operation: "generate",
                    references: &references,
                    mask: mask.then_some(&image),
                };
                let error = comfy::submit(
                    &input,
                    workflow::prepare(&input).unwrap(),
                    &HashMap::new(),
                    &CancellationToken::new(),
                )
                .await
                .err()
                .unwrap();
                assert_eq!(error.code, "invalid_response");
                assert_eq!(error.stage, "upload");
                assert!(
                    requests
                        .lock()
                        .unwrap()
                        .iter()
                        .all(|request| request.path == "/upload/image")
                );
            }
            let input = Input {
                connection: &connection,
                request: &request,
                model: &model,
                operation: "generate",
                references: &[],
                mask: None,
            };
            let error = comfy::submit(
                &input,
                workflow::prepare(&input).unwrap(),
                &HashMap::new(),
                &CancellationToken::new(),
            )
            .await
            .err()
            .unwrap();
            assert_eq!(error.code, "outcome_unknown");
            assert_eq!(error.stage, "submission");
            let requests = requests.lock().unwrap();
            assert_eq!(
                requests
                    .iter()
                    .filter(|request| request.path == "/upload/image")
                    .count(),
                2
            );
            assert_eq!(
                requests
                    .iter()
                    .filter(|request| request.path == "/prompt")
                    .count(),
                1
            );
            for request in requests
                .iter()
                .filter(|request| request.path == "/upload/image")
            {
                assert!(
                    request
                        .headers
                        .to_ascii_lowercase()
                        .contains("multipart/form-data")
                );
                assert!(
                    request
                        .body
                        .windows(image.bytes.len())
                        .any(|window| window == image.bytes)
                );
            }
        },
    )
    .await;
}

#[tokio::test]
async fn accepted_partial_workflow_warns_only_rejected_node_ids_and_preserves_mask_evidence() {
    serve(|_| Value::parse(r#"{"prompt_id":"accepted","node_errors":{"bad":{"errors":["private detail"]},"\ud800":{"errors":["private detail"]}}}"#).unwrap(),
        |origin, requests| async move {
        let connection = comfy_config(&origin);
        let request = json!({"prompt":"draw","count":1,"provider_options":{}});
        let model = "workflow".into();
        let input = Input { connection:&connection, request:&request, model:&model, operation:"generate", references:&[], mask:None };
        let (job, warnings) = comfy::submit(&input, workflow::prepare(&input).unwrap(), &HashMap::new(), &CancellationToken::new()).await.unwrap();
        assert_eq!(job.id, "accepted");
        assert_eq!(warnings.len(), 1);
        let mut expected: Vec<u16> = "ComfyUI accepted only valid workflow branches; rejected node IDs: bad, "
            .encode_utf16().collect();
        expected.push(0xd800);
        assert_eq!(warnings[0].as_utf16(), expected);
        assert!(!warnings.iter().any(|warning| warning.to_string().contains("private detail")));
        assert_eq!(capabilities::capabilities(&connection, &model)["gateway_forwarding"]["masks"], "supported");
        let effective = capabilities::effective_request(&connection, &json!({"prompt":"edit","count":1,"provider_options":{},"reference_images":[{"type":"path","path":"/input.png"}],"mask":{"type":"path","path":"/mask.png"}}), &model).unwrap();
        assert!(effective.get("mask").is_some());
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let body = Value::parse(std::str::from_utf8(&requests[0].body).unwrap()).unwrap();
        assert_eq!(body["prompt"]["text"]["inputs"]["text"], "draw");
    }).await;
}
