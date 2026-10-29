use super::{fixtures as f, host, support as s};
use serde_json::json;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

pub async fn run() {
    let cases: &[(&str, &str, &str, bool, &[(&str, &str)], &[&str])] = &[
        (s::IMAGE, "text-to-image", "image", true, &[], &["image"]),
        (s::IMAGE_EDIT, "image-to-image", "image", true, &[("image", "image")], &["image"]),
        (s::VIDEO, "text-to-video", "video", true, &[], &["video"]),
        (s::IMAGE_VIDEO, "image-to-video", "video", true, &[("image", "image")], &["video"]),
        (s::VIDEO_VIDEO, "video-to-video", "video", true, &[("video", "video")], &["video"]),
        (s::AUDIO, "text-to-audio", "audio", true, &[], &["audio"]),
        (s::AUDIO_AUDIO, "audio-to-audio", "audio", true, &[("audio", "audio")], &["audio"]),
        (s::MESH, "text-to-3d", "3d", true, &[], &["3d", "image"]),
        (s::IMAGE_MESH, "image-to-3d", "3d", false, &[("image", "image")], &["3d"]),
        (s::RETEXTURE, "3d-to-3d", "3d", false, &[("3d", "mesh"), ("image", "image")], &["3d"]),
    ];
    for (index, &(endpoint, mode, kind, prompt, roles, kinds)) in cases.iter().enumerate() {
        host::scenario(25000, async {
            let work = f::workspace();
            let directory = f::directory(&work, &format!("media-mode-{index}"));
            let (url, mime, bytes) = s::data(kind);
            let polls = Arc::new(AtomicUsize::new(0));
            let wire = s::normal_provider(endpoint, s::output(endpoint, &url, mime), polls.clone()).await;
            let config = s::config(&directory, &wire.origin, &[(endpoint, mode, kinds)]);
            let mut host = s::start(&directory, &config, "media-mode-host").await;
            if index == 0 {
                let tools = host::bounded(5000, host.client().list_tools(None)).await.unwrap();
                assert_eq!(tools.tools.len(), 12);
                for name in ["health_check", "list_connections", "list_models", "get_model_capabilities", "generate_image", "edit_image", "get_job", "cancel_job", "list_outputs", "get_output_info", "generate_media", "get_media_output_info"] {
                    assert!(tools.tools.iter().any(|tool| tool.name == name), "missing tool {name}");
                }
            }
            let id = format!("media-mode-{index}");
            let mut request = s::request(&id, endpoint, mode, prompt, roles);
            // The same approved data is exercised as a path through the actual input-root policy.
            if index == 1 {
                request["references"][0] = json!({"kind":"image","role":"image","path":s::input_file(&directory,"image")});
            }
            let result = host.call_timeout("generate_media", request.clone(), 10000).await;
            assert_ne!(result["isError"], true, "{result}");
            let generated = host::structured(&result);
            let output_id = s::verified(&host, &generated, kind, mime, &bytes).await;
            assert!(result["content"].as_array().unwrap().iter().any(|item|
                item["type"] == "resource_link" && item["uri"] == format!("numera-gen://outputs/{output_id}")));
            for scheme in ["numera-gen", "numera-image"] {
                let uri = format!("{scheme}://outputs/{output_id}");
                let resource = host::bounded(5000, host.client().read_resource(rmcp::model::ReadResourceRequestParams::new(uri.clone()))).await.unwrap();
                let resource = serde_json::to_value(resource).unwrap();
                assert_eq!(resource["contents"][0]["uri"], uri);
                assert_eq!(resource["contents"][0]["mimeType"], mime);
                assert_eq!(resource["contents"][0]["blob"], base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes));
            }
            if index == 0 {
                let templates = host::bounded(5000, host.client().list_resource_templates(None)).await.unwrap();
                assert_eq!(templates.resource_templates.len(), 2);
                assert_eq!(templates.resource_templates[0].uri_template, "numera-image://outputs/{id}");
                assert_eq!(templates.resource_templates[1].uri_template, "numera-gen://outputs/{id}");
            }
            let existing = host.job(&id, false).await;
            assert_eq!(existing["outputs"][0]["output_id"], output_id);
            let again = s::generate(&host, request).await;
            assert_eq!(again["outputs"], existing["outputs"]);
            assert_eq!(wire.posts(), 1);
            assert!(polls.load(Ordering::SeqCst) >= 2, "200 and 202 status paths must both execute");
            let calls = wire.captured();
            let body = calls.iter().find(|request| request.method == "POST").unwrap().json();
            if prompt { assert_eq!(body["prompt"], "آرام — 東京 123"); }
            for (_, role) in roles {
                let key = match *role { "image" => "image_url", "video" => "video_url", "audio" => "audio_url", "mesh" => "mesh_url", _ => panic!("unsupported fixture role") };
                assert!(body[key].as_str().is_some_and(|value| value.starts_with("data:")), "required {key} was not forwarded");
            }
            let listed = host.call("list_outputs", json!({"limit":100})).await;
            assert_ne!(listed["isError"], true);
            let listed = host::structured(&listed);
            assert!(listed["outputs"].as_array().unwrap().iter().any(|output| output["output_id"] == output_id));
            if endpoint == s::AUDIO_AUDIO {
                let mut owned = s::request("owned-audio-input", endpoint, mode, true, &[]);
                owned["references"] = json!([{"kind":"audio","role":"audio","output_id":output_id}]);
                let owned_result = s::generate(&host, owned).await;
                s::verified(&host, &owned_result, kind, mime, &bytes).await;
                assert_eq!(wire.posts(), 2);
            }
            host.close().await.unwrap();
            wire.close().await;
        }).await;
    }
}
