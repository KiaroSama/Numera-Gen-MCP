use numera_gen_mcp::media::{Kind, fal};
use serde_json::json;

const FLUX: &str = "fal-ai/flux/dev";
const AUDIO: &str = "fal-ai/stable-audio-25/audio-to-audio";
const RETEXTURE: &str = "fal-ai/trellis-2/retexture";

#[test]
fn exact_endpoint_mode_controls_capability_instead_of_brand_name() {
    let table = [
        (FLUX, "text-to-image", Kind::Image),
        ("fal-ai/flux/dev/image-to-image", "image-to-image", Kind::Image),
        ("fal-ai/kling-video/v2.5-turbo/pro/text-to-video", "text-to-video", Kind::Video),
        ("fal-ai/kling-video/v2.1/standard/image-to-video", "image-to-video", Kind::Video),
        ("fal-ai/cogvideox-5b/video-to-video", "video-to-video", Kind::Video),
        ("fal-ai/stable-audio", "text-to-audio", Kind::Audio),
        (AUDIO, "audio-to-audio", Kind::Audio),
        ("fal-ai/hunyuan3d-v3/text-to-3d", "text-to-3d", Kind::Model3d),
        ("fal-ai/trellis", "image-to-3d", Kind::Model3d),
        ("fal-ai/hyper3d/rodin", "text-to-3d", Kind::Model3d),
        ("fal-ai/hyper3d/rodin", "image-to-3d", Kind::Model3d),
        (RETEXTURE, "3d-to-3d", Kind::Model3d),
    ];
    for (endpoint, mode, output) in table {
        let c = fal::contract(endpoint, mode).unwrap();
        assert_eq!(c.endpoint, endpoint);
        assert_eq!(c.mode, mode);
        assert!(c.output_types.contains(&output));
        assert!(c.evidence.starts_with("https://fal.ai/api/openapi/queue/openapi.json?endpoint_id="));
    }
    assert!(fal::contract(FLUX, "image-to-video").is_err());
    assert!(fal::contract("fal-ai/flux/../dev", "text-to-image").is_err());
    assert!(fal::contract("fal-ai/unknown", "text-to-image").is_err());
}

#[test]
fn preparation_preserves_unicode_roles_and_native_parameter_types() {
    let reference = fal::Reference { kind: Kind::Audio, role: "audio".into(), url: "data:audio/wav;base64,UklGRg==".into() };
    let body = fal::prepare(AUDIO, "audio-to-audio", Some("آرام — 東京"), &[reference], &json!({"strength":0.5,"total_seconds":2})).unwrap();
    assert_eq!(body["prompt"], "آرام — 東京");
    assert_eq!(body["audio_url"], "data:audio/wav;base64,UklGRg==");
    assert_eq!(body["strength"], 0.5);
    assert_eq!(body["total_seconds"], 2);
    assert!(body.get("input").is_none());
    let reference = fal::Reference { kind: Kind::Image, role: "image".into(), url: "https://example.invalid/image.png".into() };
    let body = fal::prepare("fal-ai/kling-video/v2.1/standard/image-to-video", "image-to-video", Some("move"), &[reference], &json!({"duration":"5"})).unwrap();
    assert_eq!(body["duration"], "5");
}

#[test]
fn preparation_fails_before_submission_when_inputs_cannot_be_forwarded() {
    let image = || fal::Reference { kind: Kind::Image, role: "image".into(), url: "https://example.invalid/input.png".into() };
    assert!(fal::prepare(FLUX, "text-to-image", None, &[], &json!({})).is_err());
    assert!(fal::prepare(FLUX, "text-to-image", Some("x"), &[image()], &json!({})).is_err());
    assert!(fal::prepare(FLUX, "text-to-image", Some("x"), &[], &json!({"prompt":"replace"})).is_err());
    assert!(fal::prepare(FLUX, "text-to-image", Some("x"), &[], &json!({"webhook":"https://example.invalid"})).is_err());
    assert!(fal::prepare(AUDIO, "audio-to-audio", Some("x"), &[image()], &json!({})).is_err());
    assert!(fal::prepare("fal-ai/trellis", "image-to-3d", None, &[image(), image()], &json!({})).is_err());
    assert!(fal::prepare("fal-ai/trellis", "image-to-3d", Some("silently ignored?"), &[image()], &json!({})).is_err());
    assert!(fal::prepare("fal-ai/kling-video/v2.1/standard/image-to-video", "image-to-video", Some("x"), &[image()], &json!({"duration":5})).is_err());
    let mesh = fal::Reference { kind: Kind::Model3d, role: "mesh".into(), url: "https://example.invalid/mesh.glb".into() };
    assert!(fal::prepare(RETEXTURE, "3d-to-3d", None, &[mesh], &json!({})).is_err());
    // A documented provider format is not a supported local original-byte validator.
    for format in ["usdz", "fbx", "obj", "stl"] {
        assert!(fal::prepare("fal-ai/hyper3d/rodin", "text-to-3d", Some("x"), &[],
            &json!({"geometry_file_format":format})).is_err());
    }
    assert_eq!(fal::prepare("fal-ai/hyper3d/rodin", "text-to-3d", Some("x"), &[],
        &json!({"geometry_file_format":"glb"})).unwrap()["geometry_file_format"], "glb");
}

#[test]
fn tracking_uses_owner_app_and_never_replays_missing_acknowledgement() {
    let h = fal::handle("fal-ai/flux/dev/image-to-image", &json!({"request_id":"req-123","status":"IN_QUEUE"})).unwrap();
    assert_eq!(h.id, "req-123");
    assert_eq!(h.status_url, "/fal-ai/flux/requests/req-123/status");
    assert_eq!(h.response_url, "/fal-ai/flux/requests/req-123");
    assert_eq!(h.cancel_url, "/fal-ai/flux/requests/req-123/cancel");
    assert!(fal::handle(FLUX, &json!({"status":"IN_QUEUE"})).is_err());
    assert!(fal::handle(FLUX, &json!({"request_id":"../escape"})).is_err());
    assert!(fal::handle(FLUX, &json!({"request_id":"x","status_url":"https://evil.invalid/steal"})).is_err());
    let h = fal::handle(FLUX, &json!({"request_id":"x","response_url":"https://queue.fal.run/fal-ai/flux/requests/x"})).unwrap();
    assert_eq!(h.response_url, "https://queue.fal.run/fal-ai/flux/requests/x");
}

#[test]
fn completed_failure_and_cancellation_races_remain_distinct() {
    assert_eq!(fal::status(&json!({"status":"IN_QUEUE","request_id":"x"})).unwrap(), fal::Status::InQueue);
    assert_eq!(fal::status(&json!({"status":"IN_PROGRESS","request_id":"x"})).unwrap(), fal::Status::InProgress);
    assert_eq!(fal::status(&json!({"status":"COMPLETED","request_id":"x","error":"private echo","error_type":"runner_disconnected"})).unwrap(), fal::Status::Completed { failed:true, error_type:Some("runner_disconnected".into()) });
    assert!(fal::status(&json!({"status":"SUCCESS"})).is_err());
    assert_eq!(fal::cancel(&json!({"status":"OK"}), 200).unwrap(), fal::Cancel::Removed);
    assert_eq!(fal::cancel(&json!({"status":"CANCELLATION_REQUESTED"}), 202).unwrap(), fal::Cancel::Requested);
    assert_eq!(fal::cancel(&json!({"status":"ALREADY_COMPLETED"}), 400).unwrap(), fal::Cancel::AlreadyCompleted);
    assert_eq!(fal::cancel(&json!({"status":"NOT_FOUND"}), 404).unwrap(), fal::Cancel::NotFound);
    assert!(fal::cancel(&json!({"status":"OK"}), 202).is_err());
    assert_eq!(fal::status(&json!({"status":"COMPLETED","request_id":"x"})).unwrap(), fal::Status::Completed { failed:false, error_type:None });
    for value in [json!(false), json!([]), json!("COMPLETED"), json!({"status":"COMPLETED","request_id":"x","error_type":17})] {
        assert!(fal::status(&value).is_err());
    }
}

#[test]
fn malformed_file_items_and_parameter_objects_cannot_panic() {
    for response in [json!({"images":[false,[],12,"x",null]}), json!({"images":[]})] {
        let artifacts = fal::outputs(FLUX, &response).unwrap();
        assert!(!artifacts.is_empty());
        assert!(artifacts.iter().all(|item| item.error.is_some()));
    }
    assert!(fal::prepare(FLUX, "text-to-image", Some("x"), &[], &json!({"image_size":{"a":1,"b":2}})).is_err());
    assert!(fal::prepare(FLUX, "text-to-image", Some("x"), &[], &json!({"num_images":5})).is_err());
    assert!(fal::outputs(FLUX, &json!({"images":vec![json!({"url":"https://example.invalid/x.png"});65]})).is_err());
}

#[test]
fn outputs_preserve_mesh_texture_roles_and_invalid_items_instead_of_dropping() {
    let outputs = fal::outputs("fal-ai/hyper3d/rodin", &json!({"model_mesh":{"url":"https://example.invalid/object.glb","content_type":"model/gltf-binary"},"textures":[{"url":"https://example.invalid/color.png"},{"url":false}]})).unwrap();
    assert_eq!(outputs.len(), 3);
    assert_eq!((outputs[0].kind, outputs[0].role.as_str()), (Kind::Model3d, "model"));
    assert_eq!((outputs[1].kind, outputs[1].role.as_str()), (Kind::Image, "texture"));
    assert!(outputs[2].error.is_some());
    let missing = fal::outputs("fal-ai/stable-audio", &json!({"audio":{"url":"https://example.invalid/not_audio_file.wav"}})).unwrap();
    assert!(missing.iter().all(|v| v.error.is_some()));
    let outputs = fal::outputs("fal-ai/hunyuan3d-v3/text-to-3d", &json!({"model_glb":{"url":"https://example.invalid/a.glb"},"model_urls":{"glb":{"url":"https://example.invalid/a.glb"},"obj":{"url":"https://example.invalid/a.obj"}},"thumbnail":{"url":"https://example.invalid/thumb.png"}})).unwrap();
    assert_eq!(outputs.len(), 3);
    assert!(outputs.iter().any(|v| v.role == "model_obj"));
    assert!(outputs.iter().any(|v| v.role == "thumbnail"));
}
