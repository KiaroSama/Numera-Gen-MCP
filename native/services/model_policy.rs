use crate::{
    capabilities::COMMON_FIELDS,
    error::{NumeraError, Result, array, object},
    http::api::{self, Body},
};
use reqwest::Method;
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

fn accepts(descriptor: &Value, value: &Value) -> bool {
    if !descriptor.is_object() {
        return false;
    }
    match descriptor["type"].as_str() {
        Some("enum") => array(&descriptor["values"]).contains(value),
        Some("range") => value.as_f64().is_some_and(|n| {
            n.is_finite()
                && n.fract() == 0.0
                && descriptor["min"].as_f64().is_some_and(|min| n >= min)
                && descriptor["max"].as_f64().is_some_and(|max| n <= max)
        }),
        Some("boolean") => true,
        _ => false,
    }
}

pub async fn validate(
    c: &Value,
    headers: &HashMap<String, String>,
    model: &str,
    mut request: Value,
    cancel: &CancellationToken,
) -> Result<(Value, Vec<String>)> {
    if c["adapter"] != "openrouter-images" {
        let warnings = if c["modelOverrides"][model]["evidence"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        {
            vec![]
        } else {
            vec!["Model/account parameter support is unknown unless documented in configured capability evidence.".into()]
        };
        return Ok((request, warnings));
    }
    for key in object(&request["provider_options"])?.keys() {
        if !["provider", "stream", "output_compression"].contains(&key.as_str()) {
            return Err(NumeraError::new(
                "unsupported_parameter",
                "Dedicated Image API requires provider-specific settings under provider.options.",
            ));
        }
    }
    let path = format!(
        "images/models/{}/endpoints",
        model
            .split('/')
            .map(crate::adapters::encode_component)
            .collect::<Vec<_>>()
            .join("/")
    );
    let raw=match api::json(
        c,
        headers,
        api::Operation {
            operation: &path,
            prefix: "/api/v1",
            method: Method::GET,
            body: Body::Empty,
            query: None,
        },
        cancel,
    ).await.and_then(|v|{object(&v)?;Ok(v)}) {
        Ok(raw)=>raw,
        Err(_)=>return Ok((request,vec!["Endpoint capability discovery failed; explicit model availability and parameter forwarding remain unknown. No generation fallback will occur.".into()])),
    };
    let endpoints = array(&raw["endpoints"]);
    for e in endpoints {
        object(e)?;
    }
    if endpoints.is_empty() {
        return Ok((
            request,
            vec!["Endpoint catalog is empty; model support is unknown.".into()],
        ));
    }
    let mut requirements = json!({});
    for key in COMMON_FIELDS {
        if let Some(value) = request.get(*key) {
            requirements[if *key == "image_size" {
                "resolution"
            } else {
                key
            }] = value.clone();
        }
    }
    if request["count"].as_u64().unwrap_or(1) > 1 {
        requirements["n"] = request["count"].clone();
    }
    let refs = array(&request["reference_images"]).len();
    if refs > 0 {
        requirements["input_references"] = json!(refs);
    }
    for key in ["stream", "output_compression"] {
        if let Some(value) = request["provider_options"].get(key) {
            requirements[key] = value.clone();
        }
    }
    let empty = json!({});
    let routing = request["provider_options"]
        .get("provider")
        .filter(|v| !v.is_null() && **v != false)
        .unwrap_or(&empty);
    object(routing)?;
    let selected = array(&routing["only"]);
    let mut compatible = Vec::new();
    for e in endpoints {
        let tag = e["provider_tag"]
            .as_str()
            .or_else(|| e["provider_slug"].as_str())
            .unwrap_or("");
        if !selected.is_empty() && !selected.iter().any(|v| v == tag) {
            continue;
        }
        let params = e
            .get("supported_parameters")
            .filter(|v| !v.is_null())
            .unwrap_or(&empty);
        object(params)?;
        if object(&requirements)?
            .iter()
            .all(|(key, value)| accepts(&params[key], value))
        {
            compatible.push(e);
        }
    }
    let endpoint=compatible.first().ok_or_else(||NumeraError::new("unsupported_parameter","No discovered endpoint supports every explicit image requirement; nothing was submitted."))?;
    let Some(tag) = endpoint
        .get("provider_tag")
        .filter(|v| !v.is_null())
        .or_else(|| endpoint.get("provider_slug").filter(|v| !v.is_null()))
    else {
        if compatible.len() != endpoints.len() {
            return Err(NumeraError::new(
                "capability_unknown",
                "Cannot pin a compatible endpoint from this catalog.",
            ));
        }
        return Ok((request, vec![]));
    };
    let passthrough = routing
        .get("options")
        .filter(|v| !v.is_null())
        .unwrap_or(&empty);
    for (provider, options) in object(passthrough)? {
        if endpoint["provider_slug"] != provider.as_str() {
            return Err(NumeraError::new(
                "unsupported_parameter",
                "Passthrough options name a different provider than selected endpoint.",
            ));
        }
        let allowed = array(&endpoint["allowed_passthrough_parameters"]);
        if object(options)?
            .keys()
            .any(|key| !allowed.iter().any(|v| v == key))
        {
            return Err(NumeraError::new(
                "unsupported_parameter",
                "Endpoint does not allow an explicit passthrough parameter.",
            ));
        }
    }
    let mut routing = routing.clone();
    routing["only"] = json!([tag
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| tag.to_string())]);
    routing["allow_fallbacks"] = json!(false);
    request["provider_options"]["provider"] = routing;
    Ok((request, vec![]))
}
