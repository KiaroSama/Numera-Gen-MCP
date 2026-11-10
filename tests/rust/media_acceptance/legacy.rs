use super::{fixtures as f, host, provider::{Provider, Reply}, support as s};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;

pub async fn run() {
    host::scenario(35000, async {
        let work = f::workspace();
        let directory = f::directory(&work, "media-legacy");
        let (data, _, original) = s::data("image");
        let response = STANDARD.encode(&original);
        let asset = original.clone();
        let wire = Provider::new(move |request, _| {
            let response = response.clone();
            let asset = asset.clone();
            async move {
                assert!(request.header("authorization").is_none());
                if request.method == "GET" {
                    assert_eq!(request.path, "/assets/original.png");
                    return Ok(Reply::bytes(200, "image/png", asset));
                }
                assert_eq!(request.method, "POST");
                assert!(["/images/generations", "/images/edits"].contains(&request.path.as_str()));
                assert_eq!(request.json()["model"], "image-model");
                Ok(Reply::json(json!({"data":[{"b64_json":response}]})))
            }
        }).await;
        let mut connection = f::connection(&wire.origin, "openai-images");
        connection["edit"] = json!({"mode":"json","encoding":"images","maxReferences":4});
        connection["configuredModels"] = json!([{"id":"image-model","name":"fixture"}]);
        connection["modelOverrides"] = json!({"image-model":{"outputTypes":["image"],"modes":["text-to-image","image-to-image"],"maxReferences":4}});
        let mut config = f::config(&directory, json!({"local":connection}));
        config["files"]["assetAllowances"] = json!([{"origin":wire.origin,"pathPrefix":"/assets"}]);
        let config = f::normalized(&directory, &config);
        let mut host = s::start(&directory, &config, "media-legacy-host").await;
        let mut request = s::request("legacy-original", "image-model", "text-to-image", true, &[]);
        request["output_subdirectory"] = json!("legacy/originals");
        request["filename_prefix"] = json!("original");
        let receipt = s::generate(&host, request).await;
        assert_eq!(receipt["status"], "completed", "{receipt}");
        f::saved(&receipt, &original);
        let id = receipt["outputs"][0]["output_id"].as_str().unwrap();
        let info = host.call("get_media_output_info", json!({"output_id":id})).await;
        assert_ne!(info["isError"], true, "{info}");
        assert_eq!(host::structured(&info)["sha256"], numera_gen_mcp::compat::hash::sha256(&original));
        for (index, source) in [
            json!({"data_url":data}),
            json!({"path":s::input_file(&directory,"image")}),
            json!({"output_id":id}),
            json!({"url":format!("{}/assets/original.png",wire.origin)}),
        ].into_iter().enumerate() {
            let mut reference = source;
            reference["kind"] = json!("image");
            reference["role"] = json!("image");
            let mut request = s::request(&format!("legacy-edit-{index}"), "image-model", "image-to-image", true, &[]);
            request["references"] = json!([reference]);
            let receipt = s::generate(&host, request).await;
            assert_eq!(receipt["status"], "completed", "{receipt}");
            f::saved(&receipt, &original);
        }
        for key in ["prompt","request_id","connection","model","reference_images","wait","output_subdirectory","filename_prefix"] {
            let mut request = s::request("legacy-conflict", "image-model", "text-to-image", true, &[]);
            request["parameters"][key] = json!(true);
            let result = host.call("generate_media", request).await;
            assert_eq!(result["isError"], true, "conflict {key}: {result}");
        }
        let base = s::request("legacy-invalid", "image-model", "image-to-image", true, &[]);
        for (key,value) in [
            ("prompt",json!(null)),
            ("model",json!("undeclared")),
            ("mode",json!("text-to-video")),
            ("references",json!([{"kind":"audio","role":"audio","data_url":s::data("audio").0}])),
            ("references",json!([{"kind":"image","role":"mask","data_url":s::data("image").0}])),
        ] {
            let mut request = base.clone();request[key]=value;
            let result = host.call("generate_media",request).await;
            assert_eq!(result["isError"],true,"invalid {key}: {result}");
        }
        let request = s::request("legacy-wrong-mode", "image-model", "text-to-image", true, &[("image","image")]);
        assert_eq!(host.call("generate_media",request).await["isError"],true);
        assert_eq!(wire.posts(),5);
        host.close().await.unwrap();
        wire.close().await;
    }).await;
}
