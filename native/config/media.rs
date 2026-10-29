use super::invalid;
use crate::{
    compat::value::{JsString, Value, json},
    error::Result,
};
use std::collections::HashSet;

pub const OUTPUT_TYPES: &[&str] = &["image", "video", "audio", "3d"];
pub const MODES: &[&str] = &[
    "text-to-image",
    "image-to-image",
    "text-to-video",
    "image-to-video",
    "video-to-video",
    "text-to-audio",
    "audio-to-audio",
    "video-to-audio",
    "text-to-3d",
    "image-to-3d",
    "3d-to-3d",
];

fn list<'a>(value: &'a Value, allowed: &[&str]) -> Result<Vec<&'a str>> {
    let values = value
        .as_array()
        .filter(|values| !values.is_empty() && values.len() <= allowed.len())
        .ok_or_else(|| invalid("Media declarations require bounded nonempty lists."))?;
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| allowed.contains(value) && seen.insert(*value))
                .ok_or_else(|| invalid("Media declaration values must be recognized and unique."))
        })
        .collect()
}

pub(crate) fn csv(value: &str, modes: bool) -> Result<Value> {
    let allowed = if modes { MODES } else { OUTPUT_TYPES };
    let values = Value::Array(
        value
            .split(',')
            .take(allowed.len() + 1)
            .map(|item| json!(item))
            .collect(),
    );
    list(&values, allowed)?;
    Ok(values)
}

pub(crate) fn validate_declaration(
    value: &Value,
    output_field: &str,
) -> Result<bool> {
    match (value.get(output_field), value.get("modes")) {
        (None, None) => Ok(false),
        (Some(outputs), Some(modes)) => {
            let outputs = list(outputs, OUTPUT_TYPES)?;
            for mode in list(modes, MODES)? {
                let target = mode
                    .rsplit_once("-to-")
                    .map(|(_, target)| target)
                    .ok_or_else(|| invalid("Invalid media mode."))?;
                if !outputs.contains(&target) {
                    return Err(invalid("Every media mode target must be a declared output type."));
                }
            }
            Ok(true)
        }
        _ => Err(invalid("Model output types and modes must be declared together.")),
    }
}

/// Read declarations without inserting defaults into legacy configuration or request identities.
pub fn declaration<'a>(connection: &'a Value, model: &JsString) -> Option<&'a Value> {
    connection["modelOverrides"]
        .get(model)
        .filter(|value| value.get("outputTypes").is_some() && value.get("modes").is_some())
}

pub fn has_image_output(connection: &Value, model: &JsString) -> bool {
    declaration(connection, model).map_or(connection["adapter"] != "fal-queue", |value| {
        value["outputTypes"]
            .as_array()
            .is_some_and(|types| types.iter().any(|kind| kind == "image"))
    })
}

pub(crate) fn validate_connection(connection: &Value) -> Result<()> {
    let fal = connection["adapter"] == "fal-queue";
    if fal {
        if connection["modelOverrides"]
            .as_object()
            .is_some_and(|models| models.len() > 100)
        {
            return Err(invalid("A fal connection supports at most 100 declared models."));
        }
        let base = connection["baseUrl"]
            .as_str()
            .and_then(|value| url::Url::parse(value).ok())
            .ok_or_else(|| invalid("Invalid fal queue origin."))?;
        let loopback = base.scheme() == "http" && matches!(base.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if (!loopback && (base.scheme() != "https" || base.host_str() != Some("queue.fal.run") || base.port().is_some()))
            || base.path() != "/"
            || connection["auth"]["type"] != "key"
            || connection.get("gateway").is_some()
            || connection.get("edit").is_some()
            || connection.get("orchestrationModel").is_some()
            || connection["paths"].as_object().is_some_and(|v| !v.is_empty())
            || connection["query"].as_object().is_some_and(|v| !v.is_empty())
        {
            return Err(invalid("fal-queue requires a queue origin, Key authentication and native endpoint paths."));
        }
    } else if connection["auth"]["type"] == "key" {
        return Err(invalid("Key authentication is reserved for fal-queue connections."));
    }
    if let Some(overrides) = connection["modelOverrides"].as_object() {
        for (model, value) in overrides {
            if !validate_declaration(value, "outputTypes")? {
                continue;
            }
            let outputs = list(&value["outputTypes"], OUTPUT_TYPES)?;
            let mut implemented_outputs = HashSet::new();
            for mode in list(&value["modes"], MODES)? {
                if fal {
                    let endpoint = model
                        .checked_str()
                        .map_err(|_| invalid("Fal model IDs must be valid Unicode endpoint identifiers."))?;
                    let contract = crate::media::fal::contract(endpoint, mode)
                        .map_err(|_| invalid("The declared fal endpoint and mode have no implemented contract."))?;
                    for kind in contract.output_types {
                        let kind = kind.as_str();
                        if !outputs.contains(&kind) {
                            return Err(invalid("Declared output types omit an artifact kind returned by the fal contract."));
                        }
                        implemented_outputs.insert(kind.to_owned());
                    }
                } else if !["text-to-image", "image-to-image"].contains(&mode)
                    || outputs != ["image"]
                {
                    return Err(invalid("This adapter implements only image output modes."));
                }
            }
            if fal && outputs.iter().any(|kind| !implemented_outputs.contains(*kind)) {
                return Err(invalid("Declared output types exceed the implemented fal artifact kinds."));
            }
        }
    }
    if fal {
        if connection["modelOverrides"]
            .as_object()
            .is_none_or(|models| !models.values().any(|model| model.get("outputTypes").is_some()))
        {
            return Err(invalid("A fal connection requires at least one explicitly declared model."));
        }
        if let Some(model) = connection["defaultModel"].as_js_string()
            && declaration(connection, model).is_none()
        {
            return Err(invalid("A fal default model requires explicit output types and modes."));
        }
        if let Some(models) = connection["configuredModels"].as_array() {
            for model in models {
                if model["id"]
                    .as_js_string()
                    .is_none_or(|id| declaration(connection, id).is_none())
                {
                    return Err(invalid("Every configured fal model requires explicit output types and modes."));
                }
            }
        }
    }
    Ok(())
}
