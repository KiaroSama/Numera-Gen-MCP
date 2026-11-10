use numera_gen_mcp::{
    compat::value::Value,
    media::Kind,
    services::media_request::{self, Request},
};
use serde_json::{Value as Json, json};

fn parse(value: Json) -> numera_gen_mcp::error::Result<Request> {
    Request::parse(Value::from_serde(&value).unwrap())
}

pub fn run() {
    let base = json!({"mode":"text-to-image"});
    let request = parse(base.clone()).unwrap();
    assert!(request.wait && request.references.is_empty());
    for (key, value) in [
        ("mode", json!("other")),
        ("request_id", json!("bad/id")),
        ("model", json!("é".repeat(160))),
        ("connection", json!("é".repeat(60))),
        ("prompt", json!("x".repeat(32769))),
        ("parameters", json!([])),
        ("output_subdirectory", json!("../escape")),
        ("output_subdirectory", json!("é".repeat(501))),
        ("filename_prefix", json!("directory/name")),
        ("filename_prefix", json!("é".repeat(101))),
        ("extra", json!(true)),
    ] {
        let mut changed = base.clone();
        changed[key] = value;
        assert!(
            parse(changed).is_err(),
            "invalid request field accepted: {key}"
        );
    }
    let reference = json!({"kind":"image","role":"image","path":"original.png"});
    for changed in [
        json!({"kind":"image","role":"image"}),
        json!({"kind":"image","role":"image","path":"x","url":"https://example.invalid/x"}),
        json!({"kind":"image","role":"IMAGE","path":"x"}),
        json!({"kind":"image","role":"image","output_id":"invalid"}),
        json!({"kind":"image","role":"image","path":"é".repeat(20000)}),
        json!({"kind":"image","role":"image","url":"é".repeat(10000)}),
    ] {
        assert!(parse(json!({"mode":"image-to-image","references":[changed]})).is_err());
    }
    let excessive = (0..33)
        .map(|n| (format!("key{n}"), json!(1)))
        .collect::<serde_json::Map<_, _>>();
    assert!(parse(json!({"mode":"text-to-image","parameters":excessive})).is_err());
    let mut request =
        parse(json!({"mode":"image-to-image","prompt":"fixture","references":[reference]}))
            .unwrap();
    let connection = Value::from_serde(
        &json!({"defaults":{"seed":1},"modelOverrides":{"model":{"defaults":{"seed":2}}}}),
    )
    .unwrap();
    request.parameters = json!({"seed":3});
    assert_eq!(
        media_request::effective_parameters(&connection, "model", &request).unwrap()["seed"],
        3
    );
    for defaults in [
        json!([]),
        json!(7),
        Json::Object((0..33).map(|n| (format!("key{n}"), json!(1))).collect()),
    ] {
        let connection = Value::from_serde(&json!({"defaults":defaults})).unwrap();
        assert!(media_request::effective_parameters(&connection, "model", &request).is_err());
    }
    request.parameters = json!([]);
    assert!(media_request::effective_parameters(&connection, "model", &request).is_err());
    request.parameters = json!({});
    let config = Value::from_serde(&json!({"files":{"maxReferences":1}})).unwrap();
    media_request::preflight(
        "fal-ai/flux/dev/image-to-image",
        &request,
        &json!({}),
        &config,
    )
    .unwrap();
    request.references.push(request.references[0].clone());
    assert!(
        media_request::preflight(
            "fal-ai/flux/dev/image-to-image",
            &request,
            &json!({}),
            &config
        )
        .is_err()
    );
    for (kind, mime) in [
        (Kind::Image, "image/png"),
        (Kind::Audio, "audio/wav"),
        (Kind::Video, "video/mp4"),
        (Kind::Model3d, "model/gltf-binary"),
    ] {
        assert_eq!(
            media_request::decode_data(&format!("data:{mime};base64,AA=="), kind, 1).unwrap(),
            vec![0]
        );
    }
    for raw in [
        "missing-comma",
        "image/png;base64,AA==",
        "data:image/png,AA==",
        "data:audio/wav;base64,AA==",
        "data:image/png;base64,!",
        "data:image/png;base64,",
        "data:image/png;base64,AAAAAAAAAAAAAAAA",
    ] {
        assert!(media_request::decode_data(raw, Kind::Image, 1).is_err());
    }
    assert!(media_request::decode_data("data:image/png;base64,AAA=", Kind::Image, 1).is_err());
}
