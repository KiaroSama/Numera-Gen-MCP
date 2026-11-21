use super::{Kind, fal::Contract, invalid};
use crate::error::Result;

#[derive(Clone, Copy)]
pub(super) struct PrimaryOutput {
    pub field: &'static str,
    pub kind: Kind,
    pub role: &'static str,
    pub array: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Spec {
    pub endpoint: &'static str,
    pub modes: &'static [&'static str],
    pub output: Kind,
    pub primary: PrimaryOutput,
    pub side_images: bool,
    pub role: Option<(&'static str, Kind, &'static str)>,
    pub prompt: bool,
    pub parameters: &'static [&'static str],
}

pub(super) const SPECS: &[Spec] = &[
    Spec {
        endpoint: "fal-ai/flux/dev",
        modes: &["text-to-image"],
        output: Kind::Image,
        primary: PrimaryOutput {
            field: "images",
            kind: Kind::Image,
            role: "image",
            array: true,
        },
        side_images: false,
        role: None,
        prompt: true,
        parameters: &[
            "image_size",
            "guidance_scale",
            "output_format",
            "num_inference_steps",
            "num_images",
            "enable_safety_checker",
            "sync_mode",
            "acceleration",
            "seed",
        ],
    },
    Spec {
        endpoint: "fal-ai/flux/dev/image-to-image",
        modes: &["image-to-image"],
        output: Kind::Image,
        primary: PrimaryOutput {
            field: "images",
            kind: Kind::Image,
            role: "image",
            array: true,
        },
        side_images: false,
        role: Some(("image", Kind::Image, "image_url")),
        prompt: true,
        parameters: &[
            "guidance_scale",
            "output_format",
            "num_inference_steps",
            "num_images",
            "enable_safety_checker",
            "sync_mode",
            "strength",
            "acceleration",
            "seed",
        ],
    },
    Spec {
        endpoint: "fal-ai/kling-video/v2.5-turbo/pro/text-to-video",
        modes: &["text-to-video"],
        output: Kind::Video,
        primary: PrimaryOutput {
            field: "video",
            kind: Kind::Video,
            role: "video",
            array: false,
        },
        side_images: false,
        role: None,
        prompt: true,
        parameters: &[
            "negative_prompt",
            "aspect_ratio",
            "cfg_scale",
            "duration",
            "enable_safety_checker",
        ],
    },
    Spec {
        endpoint: "fal-ai/kling-video/v2.1/standard/image-to-video",
        modes: &["image-to-video"],
        output: Kind::Video,
        primary: PrimaryOutput {
            field: "video",
            kind: Kind::Video,
            role: "video",
            array: false,
        },
        side_images: false,
        role: Some(("image", Kind::Image, "image_url")),
        prompt: true,
        parameters: &[
            "negative_prompt",
            "cfg_scale",
            "duration",
            "enable_safety_checker",
        ],
    },
    Spec {
        endpoint: "fal-ai/cogvideox-5b/video-to-video",
        modes: &["video-to-video"],
        output: Kind::Video,
        primary: PrimaryOutput {
            field: "video",
            kind: Kind::Video,
            role: "video",
            array: false,
        },
        side_images: false,
        role: Some(("video", Kind::Video, "video_url")),
        prompt: true,
        parameters: &[
            "sync_mode",
            "negative_prompt",
            "seed",
            "video_size",
            "strength",
            "export_fps",
            "guidance_scale",
            "enable_safety_checker",
            "use_rife",
            "num_inference_steps",
        ],
    },
    Spec {
        endpoint: "fal-ai/mmaudio-v2",
        modes: &["video-to-audio", "video-to-video"],
        output: Kind::Video,
        primary: PrimaryOutput {
            field: "video",
            kind: Kind::Video,
            role: "video",
            array: false,
        },
        side_images: false,
        role: Some(("video", Kind::Video, "video_url")),
        prompt: true,
        parameters: &[
            "negative_prompt",
            "seed",
            "num_steps",
            "duration",
            "cfg_strength",
            "mask_away_clip",
        ],
    },
    Spec {
        endpoint: "fal-ai/stable-audio",
        modes: &["text-to-audio"],
        output: Kind::Audio,
        primary: PrimaryOutput {
            field: "audio_file",
            kind: Kind::Audio,
            role: "audio",
            array: false,
        },
        side_images: false,
        role: None,
        prompt: true,
        parameters: &[
            "sync_mode",
            "seconds_total",
            "seconds_start",
            "enable_safety_checker",
            "steps",
        ],
    },
    Spec {
        endpoint: "fal-ai/stable-audio-25/audio-to-audio",
        modes: &["audio-to-audio"],
        output: Kind::Audio,
        primary: PrimaryOutput {
            field: "audio",
            kind: Kind::Audio,
            role: "audio",
            array: false,
        },
        side_images: false,
        role: Some(("audio", Kind::Audio, "audio_url")),
        prompt: true,
        parameters: &[
            "seed",
            "enable_safety_checker",
            "strength",
            "guidance_scale",
            "total_seconds",
            "num_inference_steps",
            "sync_mode",
        ],
    },
    Spec {
        endpoint: "fal-ai/hunyuan3d-v3/text-to-3d",
        modes: &["text-to-3d"],
        output: Kind::Model3d,
        primary: PrimaryOutput {
            field: "model_glb",
            kind: Kind::Model3d,
            role: "model",
            array: false,
        },
        side_images: true,
        role: None,
        prompt: true,
        parameters: &[
            "enable_pbr",
            "face_count",
            "polygon_type",
            "generate_type",
            "enable_safety_checker",
        ],
    },
    Spec {
        endpoint: "fal-ai/trellis",
        modes: &["image-to-3d"],
        output: Kind::Model3d,
        primary: PrimaryOutput {
            field: "model_mesh",
            kind: Kind::Model3d,
            role: "model",
            array: false,
        },
        side_images: false,
        role: Some(("image", Kind::Image, "image_url")),
        prompt: false,
        parameters: &[
            "slat_sampling_steps",
            "mesh_simplify",
            "slat_guidance_strength",
            "texture_size",
            "ss_guidance_strength",
            "ss_sampling_steps",
            "seed",
        ],
    },
    Spec {
        endpoint: "fal-ai/hyper3d/rodin",
        modes: &["text-to-3d", "image-to-3d"],
        output: Kind::Model3d,
        primary: PrimaryOutput {
            field: "model_mesh",
            kind: Kind::Model3d,
            role: "model",
            array: false,
        },
        side_images: true,
        role: None,
        prompt: true,
        parameters: &[
            "seed",
            "condition_mode",
            "geometry_file_format",
            "TAPose",
            "quality",
            "tier",
            "material",
            "use_hyper",
        ],
    },
    Spec {
        endpoint: "fal-ai/trellis-2/retexture",
        modes: &["3d-to-3d"],
        output: Kind::Model3d,
        primary: PrimaryOutput {
            field: "model_glb",
            kind: Kind::Model3d,
            role: "model",
            array: false,
        },
        side_images: false,
        role: None,
        prompt: false,
        parameters: &[
            "texture_size",
            "resolution",
            "tex_slat_guidance_rescale",
            "tex_slat_sampling_steps",
            "tex_slat_guidance_strength",
            "tex_slat_rescale_t",
            "tex_slat_guidance_interval_start",
            "tex_slat_guidance_interval_end",
            "seed",
        ],
    },
];

pub(super) fn spec(endpoint: &str) -> Result<Spec> {
    SPECS
        .iter()
        .copied()
        .find(|s| s.endpoint == endpoint)
        .ok_or_else(|| invalid("No evidenced fal contract exists for this exact endpoint."))
}

pub fn contract(endpoint: &str, mode: &str) -> Result<Contract> {
    let s = spec(endpoint)?;
    if !s.modes.contains(&mode) {
        return Err(invalid(
            "The exact fal endpoint does not implement the selected mode.",
        ));
    }
    let mut roles: Vec<String> = s
        .role
        .map(|(role, _, _)| vec![role.into()])
        .unwrap_or_default();
    if endpoint == "fal-ai/hyper3d/rodin" && mode == "image-to-3d" {
        roles.push("image".into());
    } else if endpoint == "fal-ai/trellis-2/retexture" {
        roles.extend(["mesh".into(), "image".into()]);
    }
    let mut output_types = vec![s.output];
    if s.side_images {
        output_types.push(Kind::Image);
    }
    if endpoint == "fal-ai/mmaudio-v2" && mode == "video-to-audio" {
        output_types.push(Kind::Audio);
    }
    Ok(Contract {
        endpoint: endpoint.into(),
        mode: mode.into(),
        output_types,
        required_roles: roles,
        evidence: format!("https://fal.ai/api/openapi/queue/openapi.json?endpoint_id={endpoint}"),
    })
}
