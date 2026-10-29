use super::{Kind, fal::Reference, fal_contract::{contract, spec}, invalid};
use crate::error::Result;
use serde_json::Value;
use std::collections::HashSet;

// References arrive only after the service has validated their source bytes and transfer policy.
pub fn prepare(endpoint: &str, mode: &str, prompt: Option<&str>, references: &[Reference], parameters: &Value) -> Result<Value> {
    let c = contract(endpoint, mode)?;
    let s = spec(endpoint)?;
    if references.len() > 16 {
        return Err(invalid("Too many fal media references."));
    }
    let mut body = parameters.as_object().filter(|p| p.len() <= 32).cloned().ok_or_else(|| invalid("Fal parameters must be a bounded object."))?;
    for (key, value) in &body {
        if !s.parameters.contains(&key.as_str()) || !valid_parameter(key, value, endpoint) {
            return Err(invalid("Unsupported fal parameter or native parameter type/value."));
        }
    }
    let rodin = endpoint == "fal-ai/hyper3d/rodin";
    let prompt_required = s.prompt && !(rodin && mode == "image-to-3d");
    if prompt_required && prompt.is_none_or(|p| p.trim().is_empty()) {
        return Err(invalid("The fal mode requires an explicit prompt."));
    }
    if let Some(prompt) = prompt {
        let limit = if endpoint.contains("hunyuan3d") || rodin { 1024 } else { 32768 };
        if !s.prompt || prompt.chars().count() > limit || prompt.contains('\0') {
            return Err(invalid("This fal endpoint cannot carry the supplied prompt."));
        }
        body.insert("prompt".into(), Value::String(prompt.into()));
    }
    let mut seen = HashSet::new();
    let mut images = Vec::new();
    for reference in references {
        if reference.url.is_empty() || reference.url.len() > 32 * 1024 * 1024 || reference.url.contains(['\0', '\r', '\n']) {
            return Err(invalid("Invalid or excessive fal reference URL/data URI."));
        }
        let field = if rodin && mode == "image-to-3d" && reference.role == "image" && reference.kind == Kind::Image {
            images.push(Value::String(reference.url.clone()));
            seen.insert("image");
            continue;
        } else if endpoint == "fal-ai/trellis-2/retexture" {
            match (reference.role.as_str(), reference.kind) {
                ("mesh", Kind::Model3d) => "mesh_url",
                ("image", Kind::Image) => "image_url",
                _ => return Err(invalid("Fal retexture requires explicit mesh and image roles.")),
            }
        } else if let Some((role, kind, field)) = s.role {
            if reference.role != role || reference.kind != kind {
                return Err(invalid("Fal reference kind/role does not match the exact endpoint."));
            }
            field
        } else {
            return Err(invalid("This fal mode does not accept the supplied media reference."));
        };
        if !seen.insert(reference.role.as_str()) {
            return Err(invalid("Fal reference role appears more than once."));
        }
        body.insert(field.into(), Value::String(reference.url.clone()));
    }
    if !images.is_empty() { body.insert("input_image_urls".into(), Value::Array(images)); }
    if c.required_roles.iter().any(|role| !seen.contains(role.as_str())) {
        return Err(invalid("A required fal media reference is missing."));
    }
    Ok(Value::Object(body))
}

fn valid_parameter(key: &str, value: &Value, endpoint: &str) -> bool {
    let number = |min: f64, max: f64| value.as_f64().is_some_and(|n| n.is_finite() && n >= min && n <= max);
    let integer = |min: u64, max: u64| value.as_u64().is_some_and(|n| n >= min && n <= max);
    let choice = |values: &[&str]| value.as_str().is_some_and(|v| values.contains(&v));
    match key {
        "enable_safety_checker" | "sync_mode" | "use_rife" | "enable_pbr" | "TAPose" | "use_hyper" => value.is_boolean(),
        "seed" if endpoint == "fal-ai/hyper3d/rodin" => value.is_null() || integer(0, 65535),
        "seed" => value.is_null() || value.is_i64() || value.is_u64(),
        "duration" => choice(&["5", "10"]),
        "aspect_ratio" => choice(&["16:9", "9:16", "1:1"]),
        "output_format" => choice(&["jpeg", "png"]),
        "acceleration" => choice(&["none", "regular", "high"]),
        "negative_prompt" => value.as_str().is_some_and(|v| v.len() <= 32768 && !v.contains('\0')),
        "image_size" | "video_size" => size(value),
        "num_images" => integer(1, 4),
        "cfg_scale" | "tex_slat_guidance_rescale" | "tex_slat_guidance_interval_start" | "tex_slat_guidance_interval_end" => number(0.0, 1.0),
        "strength" => number(if endpoint.contains("cogvideox") { 0.05 } else { 0.01 }, 1.0),
        "mesh_simplify" => number(0.9, 0.98),
        "guidance_scale" => number(if endpoint.contains("cogvideox") { 0.0 } else { 1.0 }, if endpoint.contains("stable-audio-25") { 25.0 } else { 20.0 }),
        "slat_guidance_strength" | "ss_guidance_strength" | "tex_slat_guidance_strength" => number(0.0, 10.0),
        "tex_slat_rescale_t" => number(1.0, 6.0),
        "num_inference_steps" => integer(if endpoint.contains("stable-audio-25") { 4 } else if endpoint.ends_with("image-to-image") { 10 } else { 1 }, if endpoint.contains("stable-audio-25") { 8 } else { 50 }),
        "steps" => integer(2, 1000),
        "slat_sampling_steps" | "ss_sampling_steps" | "tex_slat_sampling_steps" => integer(1, 50),
        "seconds_start" | "seconds_total" => integer(0, 47),
        "total_seconds" => value.is_null() || integer(1, 190),
        "export_fps" => integer(4, 32),
        "texture_size" => value.as_u64().is_some_and(|n| if endpoint.ends_with("retexture") { [1024, 2048, 4096].contains(&n) } else { [512, 1024, 2048].contains(&n) }),
        "resolution" => value.as_u64().is_some_and(|n| [512, 1024].contains(&n)),
        "face_count" => integer(40000, 1500000),
        "polygon_type" => choice(&["triangle", "quadrilateral"]),
        "generate_type" => choice(&["Normal", "LowPoly", "Geometry"]),
        "condition_mode" => choice(&["concat", "fuse"]),
        "geometry_file_format" => choice(&["glb"]),
        "quality" => choice(&["high", "medium", "low", "extra-low"]),
        "tier" => choice(&["Regular", "Sketch"]),
        "material" => choice(&["PBR", "Shaded"]),
        _ => false,
    }
}

fn size(value: &Value) -> bool {
    if let Some(s) = value.as_str() {
        return ["square_hd", "square", "portrait_4_3", "portrait_16_9", "landscape_4_3", "landscape_16_9"].contains(&s);
    }
    value.as_object().is_some_and(|o| o.len() == 2 && ["width", "height"].iter().all(|k| o.get(*k).and_then(Value::as_u64).is_some_and(|v| v > 0 && v <= 14142)))
}
