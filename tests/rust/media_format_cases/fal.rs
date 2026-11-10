use numera_gen_mcp::media::{Kind, fal};
use serde_json::{Value, json};

fn references(endpoint: &str, mode: &str) -> Vec<fal::Reference> {
    fal::contract(endpoint, mode)
        .unwrap()
        .required_roles
        .into_iter()
        .map(|role| {
            let kind = match role.as_str() {
                "image" => Kind::Image,
                "video" => Kind::Video,
                "audio" => Kind::Audio,
                "mesh" => Kind::Model3d,
                _ => panic!("unknown fixture role"),
            };
            fal::Reference {
                kind,
                role,
                url: "https://example.invalid/validated-original".into(),
            }
        })
        .collect()
}

pub fn parameter_boundaries() {
    let rows = [
        (
            "fal-ai/flux/dev",
            "text-to-image",
            Some("fixture"),
            json!({"image_size":"square_hd","guidance_scale":1,"num_inference_steps":1,"num_images":4,"enable_safety_checker":false,"sync_mode":false,"acceleration":"none","seed":null,"output_format":"jpeg"}),
        ),
        (
            "fal-ai/flux/dev/image-to-image",
            "image-to-image",
            Some("fixture"),
            json!({"strength":0.01,"num_inference_steps":10,"guidance_scale":20,"seed":-1,"output_format":"png","acceleration":"high"}),
        ),
        (
            "fal-ai/kling-video/v2.5-turbo/pro/text-to-video",
            "text-to-video",
            Some("fixture"),
            json!({"negative_prompt":"دریا","aspect_ratio":"9:16","cfg_scale":0,"duration":"10","enable_safety_checker":true}),
        ),
        (
            "fal-ai/kling-video/v2.1/standard/image-to-video",
            "image-to-video",
            Some("fixture"),
            json!({"negative_prompt":"","cfg_scale":1,"duration":"5","enable_safety_checker":false}),
        ),
        (
            "fal-ai/cogvideox-5b/video-to-video",
            "video-to-video",
            Some("fixture"),
            json!({"video_size":{"width":16,"height":16},"strength":0.05,"export_fps":4,"guidance_scale":0,"num_inference_steps":50,"seed":1.0,"use_rife":true,"sync_mode":false,"enable_safety_checker":true,"negative_prompt":"x"}),
        ),
        (
            "fal-ai/stable-audio",
            "text-to-audio",
            Some("fixture"),
            json!({"seconds_start":0,"seconds_total":47,"steps":1000,"sync_mode":false,"enable_safety_checker":true}),
        ),
        (
            "fal-ai/stable-audio-25/audio-to-audio",
            "audio-to-audio",
            Some("fixture"),
            json!({"strength":1,"guidance_scale":25,"num_inference_steps":4,"total_seconds":null,"seed":1,"sync_mode":false,"enable_safety_checker":true}),
        ),
        (
            "fal-ai/hunyuan3d-v3/text-to-3d",
            "text-to-3d",
            Some("fixture"),
            json!({"enable_pbr":true,"face_count":40000,"polygon_type":"quadrilateral","generate_type":"LowPoly","enable_safety_checker":true}),
        ),
        (
            "fal-ai/trellis",
            "image-to-3d",
            None,
            json!({"slat_sampling_steps":1,"ss_sampling_steps":50,"mesh_simplify":0.98,"slat_guidance_strength":10,"ss_guidance_strength":0,"texture_size":512,"seed":-1}),
        ),
        (
            "fal-ai/hyper3d/rodin",
            "image-to-3d",
            None,
            json!({"seed":65535,"condition_mode":"fuse","geometry_file_format":"glb","TAPose":true,"quality":"extra-low","tier":"Sketch","material":"Shaded","use_hyper":true}),
        ),
        (
            "fal-ai/trellis-2/retexture",
            "3d-to-3d",
            None,
            json!({"texture_size":4096,"resolution":1024,"tex_slat_guidance_rescale":0.5,"tex_slat_sampling_steps":1,"tex_slat_guidance_strength":10,"tex_slat_rescale_t":6,"tex_slat_guidance_interval_start":0,"tex_slat_guidance_interval_end":1,"seed":null}),
        ),
    ];
    for (endpoint, mode, prompt, parameters) in rows {
        let refs = references(endpoint, mode);
        let actual = fal::prepare(endpoint, mode, prompt, &refs, &parameters).unwrap();
        for (key, value) in parameters.as_object().unwrap() {
            assert_eq!(
                &actual[key], value,
                "native parameter changed: {endpoint}/{key}"
            );
            let mut wrong = parameters.clone();
            wrong[key] = json!([]);
            assert!(
                fal::prepare(endpoint, mode, prompt, &refs, &wrong).is_err(),
                "invalid native type accepted: {endpoint}/{key}"
            );
        }
    }
    for (endpoint, mode, key, value) in [
        (
            "fal-ai/flux/dev",
            "text-to-image",
            "image_size",
            json!({"width":0,"height":1}),
        ),
        (
            "fal-ai/flux/dev",
            "text-to-image",
            "image_size",
            json!({"width":1,"height":1,"extra":1}),
        ),
        (
            "fal-ai/flux/dev",
            "text-to-image",
            "acceleration",
            json!("other"),
        ),
        (
            "fal-ai/flux/dev",
            "text-to-image",
            "guidance_scale",
            json!(0.9),
        ),
        ("fal-ai/stable-audio", "text-to-audio", "steps", json!(1)),
        (
            "fal-ai/stable-audio",
            "text-to-audio",
            "seconds_total",
            json!(48),
        ),
        (
            "fal-ai/stable-audio-25/audio-to-audio",
            "audio-to-audio",
            "num_inference_steps",
            json!(9),
        ),
        (
            "fal-ai/stable-audio-25/audio-to-audio",
            "audio-to-audio",
            "total_seconds",
            json!(0),
        ),
        (
            "fal-ai/trellis",
            "image-to-3d",
            "mesh_simplify",
            json!(0.99),
        ),
        ("fal-ai/trellis", "image-to-3d", "texture_size", json!(4096)),
        ("fal-ai/hyper3d/rodin", "text-to-3d", "seed", json!(65536)),
        (
            "fal-ai/trellis-2/retexture",
            "3d-to-3d",
            "resolution",
            json!(513),
        ),
    ] {
        let refs = references(endpoint, mode);
        let prompt = (!matches!(mode, "image-to-3d" | "3d-to-3d")).then_some("fixture");
        let mut parameters = json!({});
        parameters[key] = value;
        assert!(
            fal::prepare(endpoint, mode, prompt, &refs, &parameters).is_err(),
            "invalid boundary accepted: {endpoint}/{key}"
        );
    }
}

pub fn rejections() {
    let endpoint = "fal-ai/flux/dev";
    for prompt in ["", "  ", "bad\0prompt", &"x".repeat(32769)] {
        assert!(fal::prepare(endpoint, "text-to-image", Some(prompt), &[], &json!({})).is_err());
    }
    let excessive = (0..33)
        .map(|n| (format!("key{n}"), json!(1)))
        .collect::<serde_json::Map<_, _>>();
    for parameters in [json!([]), Value::Object(excessive)] {
        assert!(fal::prepare(endpoint, "text-to-image", Some("x"), &[], &parameters).is_err());
    }
    let endpoint = "fal-ai/hyper3d/rodin";
    assert!(
        fal::prepare(
            endpoint,
            "text-to-3d",
            Some(&"x".repeat(1025)),
            &[],
            &json!({})
        )
        .is_err()
    );
    let image = references(endpoint, "image-to-3d").pop().unwrap();
    let body = fal::prepare(
        endpoint,
        "image-to-3d",
        None,
        &vec![image.clone(); 16],
        &json!({}),
    )
    .unwrap();
    assert_eq!(body["input_image_urls"].as_array().unwrap().len(), 16);
    assert!(fal::prepare(endpoint, "image-to-3d", None, &vec![image; 17], &json!({})).is_err());
    for url in ["", "bad\rurl", "bad\nurl", "bad\0url"] {
        let mut refs = references(endpoint, "image-to-3d");
        refs[0].url = url.into();
        assert!(fal::prepare(endpoint, "image-to-3d", None, &refs, &json!({})).is_err());
    }
    let endpoint = "fal-ai/trellis-2/retexture";
    let mut refs = references(endpoint, "3d-to-3d");
    refs[0].kind = Kind::Audio;
    assert!(fal::prepare(endpoint, "3d-to-3d", None, &refs, &json!({})).is_err());
}
