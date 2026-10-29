use super::{Kind, fal::Contract, invalid};
use crate::error::Result;

pub(super) struct Spec {
    pub modes: &'static [&'static str],
    pub output: Kind,
    pub side_images: bool,
    pub role: Option<(&'static str, Kind, &'static str)>,
    pub prompt: bool,
    pub parameters: &'static [&'static str],
}

pub(super) fn spec(endpoint: &str) -> Result<Spec> {
    let image = Kind::Image;
    let video = Kind::Video;
    let audio = Kind::Audio;
    let model = Kind::Model3d;
    let value = match endpoint {
        "fal-ai/flux/dev" => Spec { modes: &["text-to-image"], output: image, side_images: false, role: None, prompt: true,
            parameters: &["image_size", "guidance_scale", "output_format", "num_inference_steps", "num_images", "enable_safety_checker", "sync_mode", "acceleration", "seed"] },
        "fal-ai/flux/dev/image-to-image" => Spec { modes: &["image-to-image"], output: image, side_images: false, role: Some(("image", image, "image_url")), prompt: true,
            parameters: &["guidance_scale", "output_format", "num_inference_steps", "num_images", "enable_safety_checker", "sync_mode", "strength", "acceleration", "seed"] },
        "fal-ai/kling-video/v2.5-turbo/pro/text-to-video" => Spec { modes: &["text-to-video"], output: video, side_images: false, role: None, prompt: true,
            parameters: &["negative_prompt", "aspect_ratio", "cfg_scale", "duration", "enable_safety_checker"] },
        "fal-ai/kling-video/v2.1/standard/image-to-video" => Spec { modes: &["image-to-video"], output: video, side_images: false, role: Some(("image", image, "image_url")), prompt: true,
            parameters: &["negative_prompt", "cfg_scale", "duration", "enable_safety_checker"] },
        "fal-ai/cogvideox-5b/video-to-video" => Spec { modes: &["video-to-video"], output: video, side_images: false, role: Some(("video", video, "video_url")), prompt: true,
            parameters: &["sync_mode", "negative_prompt", "seed", "video_size", "strength", "export_fps", "guidance_scale", "enable_safety_checker", "use_rife", "num_inference_steps"] },
        "fal-ai/stable-audio" => Spec { modes: &["text-to-audio"], output: audio, side_images: false, role: None, prompt: true,
            parameters: &["sync_mode", "seconds_total", "seconds_start", "enable_safety_checker", "steps"] },
        "fal-ai/stable-audio-25/audio-to-audio" => Spec { modes: &["audio-to-audio"], output: audio, side_images: false, role: Some(("audio", audio, "audio_url")), prompt: true,
            parameters: &["seed", "enable_safety_checker", "strength", "guidance_scale", "total_seconds", "num_inference_steps", "sync_mode"] },
        "fal-ai/hunyuan3d-v3/text-to-3d" => Spec { modes: &["text-to-3d"], output: model, side_images: true, role: None, prompt: true,
            parameters: &["enable_pbr", "face_count", "polygon_type", "generate_type", "enable_safety_checker"] },
        "fal-ai/trellis" => Spec { modes: &["image-to-3d"], output: model, side_images: false, role: Some(("image", image, "image_url")), prompt: false,
            parameters: &["slat_sampling_steps", "mesh_simplify", "slat_guidance_strength", "texture_size", "ss_guidance_strength", "ss_sampling_steps", "seed"] },
        "fal-ai/hyper3d/rodin" => Spec { modes: &["text-to-3d", "image-to-3d"], output: model, side_images: true, role: None, prompt: true,
            parameters: &["seed", "condition_mode", "geometry_file_format", "TAPose", "quality", "tier", "material", "use_hyper"] },
        "fal-ai/trellis-2/retexture" => Spec { modes: &["3d-to-3d"], output: model, side_images: false, role: None, prompt: false,
            parameters: &["texture_size", "resolution", "tex_slat_guidance_rescale", "tex_slat_sampling_steps", "tex_slat_guidance_strength", "tex_slat_rescale_t", "tex_slat_guidance_interval_start", "tex_slat_guidance_interval_end", "seed"] },
        _ => return Err(invalid("No evidenced fal contract exists for this exact endpoint.")),
    };
    Ok(value)
}

pub fn contract(endpoint: &str, mode: &str) -> Result<Contract> {
    let s = spec(endpoint)?;
    if !s.modes.contains(&mode) {
        return Err(invalid("The exact fal endpoint does not implement the selected mode."));
    }
    let mut roles: Vec<String> = s.role.map(|(role, _, _)| vec![role.into()]).unwrap_or_default();
    if endpoint == "fal-ai/hyper3d/rodin" && mode == "image-to-3d" {
        roles.push("image".into());
    } else if endpoint == "fal-ai/trellis-2/retexture" {
        roles.extend(["mesh".into(), "image".into()]);
    }
    let mut output_types = vec![s.output];
    if s.side_images { output_types.push(Kind::Image); }
    Ok(Contract {
        endpoint: endpoint.into(), mode: mode.into(), output_types, required_roles: roles,
        evidence: format!("https://fal.ai/api/openapi/queue/openapi.json?endpoint_id={endpoint}"),
    })
}
