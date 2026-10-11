use numera_image_gen_mcp::{adapters::{build_request, normalize::normalize, types::{Input, Normalized}}, error::Result, files::images::{decode_base64, inspect}, http::api::Body};
use serde_json::{Value, json};

fn outcome(result: Result<Value>) -> Value {
    match result { Ok(value) => json!({"value":value}), Err(error) => json!({"error":error.code}) }
}
fn normalized(value: Normalized) -> Value {
    let mut result = json!({"images":value.images.into_iter().map(|image| {
        let mut item = json!({});
        if let Some(value) = image.base64 { item["base64"] = json!(value); }
        if let Some(value) = image.url { item["url"] = json!(value); }
        if let Some(value) = image.error { item["error"] = json!(value); }
        if let Some(value) = image.bytes { item["bytes"] = json!({"type":"Buffer","data":value}); }
        item
    }).collect::<Vec<_>>(), "upstreamModel":value.upstream_model,"warnings":value.warnings});
    if let Some(value) = value.terminal { result["terminal"] = json!(value); }
    if let Some(value) = value.upstream_id { result["upstreamId"] = json!(value); }
    if let Some(value) = value.job { result["job"] = json!({"id":value.id,"kind":value.kind}); }
    if let Some(value) = value.usage { result["usage"] = value; }
    if let Some(value) = value.continuation { result["continuation"] = value; }
    result
}

#[test]
fn source_generated_wire_and_result_contracts_match() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(root.join(".ci-work/parity/adapters.json")).expect("source oracle required");
    let oracle: Value = serde_json::from_slice(&bytes).expect("valid source oracle");
    for group in ["wires", "normalization"] { assert!(!oracle[group].as_array().expect("required group").is_empty()); }
    for case in oracle["wires"].as_array().unwrap() {
        let references = case["references"].as_array().unwrap().iter().map(|i| {
            inspect(decode_base64(i["base64"].as_str().unwrap(), 1048576).unwrap(), &json!({"files":{"maxOutputBytes":1048576,"maxPixels":64000000}}), None).unwrap()
        }).collect::<Vec<_>>();
        let input = Input { connection:&case["connection"], request:&case["request"], model:case["model"].as_str().unwrap(),
            operation:case["operation"].as_str().unwrap(), references:&references, mask:None };
        let actual = build_request(&input).map(|prepared| {
            match prepared.body {
                Body::Json(body) => json!({"operation":prepared.operation,"prefix":prepared.prefix,"body":body}),
                // Multipart boundary/content is tested through an actual loopback HTTP capture.
                Body::Multipart(_) => json!({"operation":prepared.operation,"prefix":prepared.prefix,"multipart":true}),
                Body::Empty => panic!("adapter cannot emit an empty submission"),
            }
        });
        if case["expected"]["value"].get("multipart").is_some() {
            let actual = actual.expect("multipart source case must build");
            assert_eq!(actual["operation"], case["expected"]["value"]["operation"]);
            assert_eq!(actual["prefix"], case["expected"]["value"]["prefix"]);
            assert_eq!(actual["multipart"], true);
        } else { assert_eq!(outcome(actual), case["expected"], "{}", case["name"]); }
    }
    for case in oracle["normalization"].as_array().unwrap() {
        assert_eq!(outcome(normalize(&case["connection"], &case["raw"]).map(normalized)), case["expected"]);
    }
}

#[test]
fn base64_source_acceptance_and_bytes_match() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".ci-work/parity/base64.json");
    let bytes = std::fs::read(file).expect("source-generated Base64 oracle");
    let cases: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let actual = decode_base64(case["input"].as_str().unwrap(), 1024)
            .map(|bytes|json!(bytes.iter().map(|b|format!("{b:02x}")).collect::<String>()));
        assert_eq!(outcome(actual), case["expected"], "{}", case["input"]);
    }
}
