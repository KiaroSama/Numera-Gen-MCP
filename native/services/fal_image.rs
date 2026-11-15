//! Maps `generate_image`/`edit_image` arguments onto the typed fal media path so the fal
//! profile works through the image tools too. No new wire behaviour: the exact-endpoint
//! contract in `media::fal` still decides which parameters and roles are accepted.
use crate::compat::value::{Value, json};
use crate::error::{NumeraError, Result};

/// True when the selected connection is the fal queue adapter.
pub fn selected(config: &crate::config::Config, args: &Value) -> bool {
    config
        .select(args.get("connection").and_then(Value::as_str))
        .map(|(_, c)| c["adapter"] == "fal-queue")
        .unwrap_or(false)
}

pub fn to_media_request(args: &Value, operation: &str) -> Result<Value> {
    for key in [
        "size",
        "quality",
        "background",
        "frame_count",
        "frame_duration_ms",
        "aspect_ratio",
        "mask",
        "edit_region",
        "upscale",
        "target_language",
    ] {
        if args.get(key).is_some_and(|v| !v.is_null()) {
            return Err(NumeraError::new(
                "unsupported_parameter",
                &format!(
                    "fal endpoints do not take {key}; pass endpoint fields via provider_options."
                ),
            ));
        }
    }
    let refs = args["reference_images"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if operation == "edit" && refs.is_empty() {
        return Err(NumeraError::new(
            "invalid_input",
            "Editing requires source images.",
        ));
    }
    let mut out = json!({
        "mode": if refs.is_empty() { "text-to-image" } else { "image-to-image" },
        "prompt": args["prompt"],
    });
    let mut parameters = json!({});
    for key in ["image_size", "output_format", "seed", "negative_prompt"] {
        if let Some(v) = args.get(key).filter(|v| !v.is_null()) {
            parameters[key] = v.clone();
        }
    }
    if args["count"].as_u64().is_some_and(|n| n > 1) {
        parameters["num_images"] = args["count"].clone();
    }
    if let Some(options) = args["provider_options"].as_object() {
        for (k, v) in options {
            if let Some(o) = parameters.as_object_mut() {
                o.insert(k.clone(), v.clone());
            }
        }
    }
    out["parameters"] = parameters;
    let mut references = Vec::new();
    for r in &refs {
        let field = r["type"].as_str().unwrap_or("");
        if !["output_id", "path", "url", "data_url"].contains(&field) {
            return Err(NumeraError::new(
                "invalid_input",
                "Invalid reference source.",
            ));
        }
        let mut item = json!({"kind":"image","role":"image"});
        item[field] = r[field].clone();
        references.push(item);
    }
    out["references"] = Value::Array(references);
    for key in [
        "connection",
        "model",
        "request_id",
        "wait",
        "output_subdirectory",
        "filename_prefix",
    ] {
        if let Some(v) = args.get(key).filter(|v| !v.is_null()) {
            out[key] = v.clone();
        }
    }
    Ok(out)
}
