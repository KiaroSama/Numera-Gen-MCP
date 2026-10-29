use super::host::{Host, Era, Launch};
use crate::common::Workspace;
use base64::Engine;
use numera_gen_mcp::{compat::value::Value as NativeValue, config::{Config, snapshot_credentials}, jobs::{Receipt, Store}, runtime_paths::RuntimePaths, services::identity::connection_identity};
use serde_json::{Value, json};
use std::{collections::HashMap, path::{Path, PathBuf}};

pub fn root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")) }
pub fn workspace() -> Workspace {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"), "actual native SDK acceptance is hosted CI only");
    let work = Workspace::new();
    std::fs::create_dir_all(work.0.join("temp")).unwrap();
    work
}
pub fn directory(work: &Workspace, name: &str) -> PathBuf {
    let path = work.0.join(name); std::fs::create_dir(&path).unwrap(); path
}
pub fn config(directory: &Path, connections: Value) -> Value {
    let first = connections.as_object().unwrap().keys().next().cloned();
    let mut value = json!({
        "schemaVersion":1,"outputDir":directory.join("outputs"),"stateDir":directory.join("state"),
        "logging":{"directory":directory.join("logs")},"files":{"allowedInputRoots":[directory.join("inputs")]},
        "connections":connections
    });
    if let Some(first) = first { value["defaultConnection"] = json!(first); }
    normalized(directory, &value)
}
pub fn normalized(directory: &Path, value: &Value) -> Value {
    let parsed = Config::parse(&value.to_string(), &directory.join("config.json"), &[], &HashMap::new()).unwrap();
    serde_json::from_str(&parsed.value.to_string()).unwrap()
}
pub fn connection(origin: &str, adapter: &str) -> Value {
    let mut value = json!({"adapter":adapter,"baseUrl":origin,"auth":{"type":"none"},"defaultModel":"image-model"});
    if adapter == "comfyui" { value["baseUrlMode"] = json!("origin"); }
    else { value["orchestrationModel"] = json!("text"); }
    value
}
pub fn write_config(directory: &Path, value: &Value) -> PathBuf {
    let file = directory.join("config.json"); write(&file, value); file
}
pub fn write(file: &Path, value: &Value) { std::fs::write(file, serde_json::to_vec(value).unwrap()).unwrap(); }
pub async fn start(directory: &Path, config: &Value, name: &str) -> Host {
    let file = write_config(directory, config);
    let host = Host::connect(directory, Launch::bridge(directory, &file, name), Era::Auto, false).await.unwrap();
    assert_eq!(serde_json::to_value(host.client().peer_info().unwrap().as_ref()).unwrap()["protocolVersion"], "2025-11-25");
    host
}
pub fn image(name: &str) -> (Value, Vec<u8>) {
    let path = root().join(".ci-work/parity/image-acceptance.json");
    assert!(std::fs::metadata(&path).unwrap().len() <= 1048576);
    let rows: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let row = rows.as_array().unwrap().iter().find(|r| r["name"] == name).expect("exact source image oracle required").clone();
    assert_eq!(row["metadata"]["width"], 8);
    assert_eq!(row["metadata"]["height"], 6);
    if name == "png" { assert_eq!(row["metadata"]["mime"], "image/png"); assert_eq!(row["metadata"]["alpha"], true); }
    let bytes = base64::engine::general_purpose::STANDARD.decode(row["base64"].as_str().unwrap()).unwrap();
    assert!(!bytes.is_empty());
    (row, bytes)
}
pub fn first_image() -> (Value, Vec<u8>) {
    let rows: Value = serde_json::from_slice(&std::fs::read(root().join(".ci-work/parity/images.json")).unwrap()).unwrap();
    let row = rows[0].clone();
    let bytes = base64::engine::general_purpose::STANDARD.decode(row["base64"].as_str().unwrap()).unwrap();
    (row, bytes)
}
pub fn receipt(id: &str, connection: &str, adapter: &str) -> Receipt {
    Receipt::new(NativeValue::parse(&json!({
        "request_id":id,"connection":connection,"adapter":adapter,"operation":"generate","status":"prepared",
        "requested_model":"image-model","upstream_model":null,"outputs":[],"warnings":[],"deviations":[],"errors":[],
        "generation_outcome":"not_submitted","storage_outcome":"not_started","another_submission_may_charge":false,
        "created_at":"2026-10-02T00:00:00.000Z","updated_at":"2026-10-02T00:00:00.000Z"
    }).to_string()).unwrap())
}
pub fn store(config: &Value) -> Store { Store::open(Path::new(config["stateDir"].as_str().unwrap())).unwrap() }
pub async fn identity(config: &Value, name: &str, directory: &Path) -> String {
    let parsed = Config::parse(&config.to_string(), &directory.join("config.json"), &[], &HashMap::new()).unwrap();
    let (_, connection) = parsed.select(Some(name)).unwrap();
    let pin = snapshot_credentials(&connection, &HashMap::new(), &RuntimePaths::new(directory).unwrap()).await.unwrap();
    connection_identity(name, &pin).unwrap()
}
pub async fn seed(config: &Value, directory: &Path, id: &str, name: &str, kind: &str, handle: &str, status: &str, hash: &str, wrong: Option<&str>, capacity: (usize, usize)) -> String {
    let identity = identity(config, name, directory).await;
    let adapter = config["connections"][name]["adapter"].as_str().unwrap();
    let mut store = store(config);
    let mut saved = store.prepare(receipt(id, name, adapter), hash, wrong.unwrap_or(&identity)).unwrap().1;
    assert!(store.admit(id, name, capacity.0, capacity.1).unwrap());
    saved.value["status"] = NativeValue::from(status);
    saved.value["generation_outcome"] = NativeValue::from(status);
    saved.value["upstream_job"] = NativeValue::parse(&json!({"id":handle,"kind":kind}).to_string()).unwrap();
    store.update(&mut saved).unwrap();
    store.release(id, Some(&saved)).unwrap();
    identity
}
pub fn as_json(receipt: &Receipt) -> Value { serde_json::from_str(&receipt.value.to_string()).unwrap() }
pub fn admission(store: &mut Store, id: &str, identity: &str, expected: bool) {
    store.prepare(receipt(id, "local", "openai-images"), id, identity).unwrap();
    assert_eq!(store.admit(id, "local", 1, 1).unwrap(), expected);
    if expected {
        let mut done = store.get(id).unwrap(); done.value["status"] = NativeValue::from("failed");
        store.update(&mut done).unwrap(); store.release(id, Some(&done)).unwrap();
    }
}
pub fn saved(result: &Value, bytes: &[u8]) {
    assert_eq!(result["outputs"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(result["outputs"][0]["path"].as_str().unwrap()).unwrap(), bytes);
}
pub fn comfy_workflow() -> Value {
    json!({"graph":{"text":{"class_type":"Text","inputs":{"text":""}},"save":{"class_type":"Save","inputs":{"text":"output"}}},
        "bindings":{"prompt":{"node":"text","input":"text"}},"outputNodes":["save"]})
}
pub fn comfy_objects() -> Value { json!({"Text":{"input":{"required":{"text":["STRING"]}}},"Save":{"output_node":true,"input":{"required":{"text":["STRING"]}}}}) }
pub fn comfy_history(filename: &str, handle: &str) -> Value {
    json!({(handle):{"status":{"completed":true,"status_str":"success"},"outputs":{"save":{"images":[{"filename":filename,"subfolder":"","type":"output"}]}}}})
}
pub fn logs(directory: &Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(directory).expect("actual native file logs required") {
        let path = entry.unwrap().path(); assert!(std::fs::metadata(&path).unwrap().len() <= 1048576);
        text.push_str(&std::fs::read_to_string(path).unwrap()); text.push('\n');
    }
    assert!(!text.is_empty(), "actual native logs required"); text
}
