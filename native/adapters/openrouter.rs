use super::types::{Input, Prepared, mime_data};
use crate::compat::value::json;
use crate::{
    capabilities::COMMON_FIELDS,
    error::{NumeraError, Result},
};

pub fn request(input: &Input<'_>) -> Result<Prepared> {
    let Input {
        request: r,
        model,
        references,
        mask,
        ..
    } = input;
    if mask.is_some() {
        return Err(NumeraError::new(
            "unsupported_operation",
            "Dedicated OpenRouter image API does not define a universal mask contract.",
        ));
    }
    let mut fields = json!({"model":model,"prompt":r["prompt"],"n":r["count"],
        "input_references":references.iter().map(|image|json!({"type":"image_url","image_url":{"url":mime_data(image)}})).collect::<Vec<_>>(),
        "provider":{"allow_fallbacks":false}});
    for key in COMMON_FIELDS {
        if let Some(value) = r.get(*key) {
            if *key == "negative_prompt" {
                return Err(NumeraError::new(
                    "unsupported_parameter",
                    "Use explicitly documented provider passthrough for negative prompt.",
                ));
            }
            fields[if *key == "image_size" {
                "resolution"
            } else {
                key
            }] = value.clone();
        }
    }
    if let Some(options) = r["provider_options"].as_object() {
        if let Some(provider_opt) = options.get("provider") {
            let Some(provider_obj) = provider_opt.as_object() else {
                return Err(NumeraError::new(
                    "unsupported_parameter",
                    "provider_options.provider must be an object.",
                ));
            };
            if provider_obj
                .get("allow_fallbacks")
                .is_some_and(|v| v != &json!(false))
            {
                return Err(NumeraError::new(
                    "unsupported_parameter",
                    "provider_options.provider.allow_fallbacks cannot be enabled.",
                ));
            }
        }
        if let Some(object) = fields.as_object_mut() {
            super::types::merge_provider_options(
                object,
                Some(options),
                &["model", "prompt", "input_references"],
            )?;
        }
    }
    if let Some(provider) = fields["provider"].as_object_mut() {
        provider.insert("allow_fallbacks".into(), json!(false));
    }
    Ok(Prepared::json("images", "/api/v1", fields))
}
