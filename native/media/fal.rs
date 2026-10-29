use super::{Kind, fal_contract::spec, invalid};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use super::{fal_contract::contract, fal_input::prepare};

#[derive(Clone, Debug)]
pub struct Contract {
    pub endpoint: String,
    pub mode: String,
    pub output_types: Vec<Kind>,
    pub required_roles: Vec<String>,
    pub evidence: String,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub kind: Kind,
    pub role: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Handle {
    pub id: String,
    pub endpoint: String,
    pub status_url: String,
    pub response_url: String,
    pub cancel_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    InQueue,
    InProgress,
    Completed { failed: bool, error_type: Option<String> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cancel {
    Removed,
    Requested,
    AlreadyCompleted,
    NotFound,
}

#[derive(Clone, Debug)]
pub struct Artifact {
    pub kind: Kind,
    pub role: String,
    pub url: String,
    pub reported_mime: Option<String>,
    pub error: Option<String>,
}

fn identifier(value: &Value) -> Result<&str> {
    value.as_str().filter(|s| !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
        .ok_or_else(|| invalid("Invalid or missing fal request identifier; do not resubmit."))
}

// The network owner must additionally bind every absolute URL to its pinned queue origin.
pub fn handle(endpoint: &str, response: &Value) -> Result<Handle> {
    spec(endpoint)?;
    let id = identifier(&response["request_id"])?;
    let mut parts = endpoint.split('/');
    let owner = parts.next().ok_or_else(|| invalid("Invalid fal owner."))?;
    let app = parts.next().ok_or_else(|| invalid("Invalid fal app."))?;
    let root = format!("/{owner}/{app}/requests/{id}");
    let tracking = |key: &str, suffix: &str| -> Result<String> {
        let expected = format!("{root}{suffix}");
        match response.get(key) {
            None | Some(Value::Null) => Ok(expected),
            Some(Value::String(value)) if value == &expected => Ok(value.clone()),
            Some(Value::String(value)) if value.len() <= 4096 => {
                let url = url::Url::parse(value).map_err(|_| invalid("Invalid fal tracking URL."))?;
                let host = url.host_str().unwrap_or("");
                let allowed = (url.scheme() == "https" && host == "queue.fal.run" && url.port().is_none())
                    || (url.scheme() == "http" && matches!(host, "127.0.0.1" | "localhost" | "[::1]"));
                if !allowed || !url.username().is_empty() || url.password().is_some() || url.path() != expected || url.query().is_some() || url.fragment().is_some() {
                    return Err(invalid("Fal tracking URL does not match its original endpoint/request."));
                }
                Ok(value.clone())
            }
            _ => Err(invalid("Invalid fal tracking URL field.")),
        }
    };
    Ok(Handle { id: id.into(), endpoint: endpoint.into(), status_url: tracking("status_url", "/status")?, response_url: tracking("response_url", "")?, cancel_url: tracking("cancel_url", "/cancel")? })
}

pub fn status(value: &Value) -> Result<Status> {
    identifier(&value["request_id"])?;
    let error_type = match value.get("error_type") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.len() <= 128 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') => Some(s.clone()),
        _ => return Err(invalid("Invalid fal queue error type.")),
    };
    let failed = value.get("error").is_some_and(|e| !e.is_null()) || error_type.is_some();
    match value["status"].as_str() {
        Some("IN_QUEUE") if !failed => Ok(Status::InQueue),
        Some("IN_PROGRESS") if !failed => Ok(Status::InProgress),
        Some("COMPLETED") => Ok(Status::Completed { failed, error_type }),
        _ => Err(invalid("Unknown or inconsistent fal queue status.")),
    }
}

pub fn cancel(value: &Value, http_status: u16) -> Result<Cancel> {
    match (http_status, value["status"].as_str()) {
        (200, Some("OK")) => Ok(Cancel::Removed),
        (202, Some("CANCELLATION_REQUESTED")) => Ok(Cancel::Requested),
        (400, Some("ALREADY_COMPLETED")) => Ok(Cancel::AlreadyCompleted),
        (404, Some("NOT_FOUND")) => Ok(Cancel::NotFound),
        _ => Err(invalid("Fal cancellation was not confirmed by a matching status/body.")),
    }
}

pub fn outputs(endpoint: &str, response: &Value) -> Result<Vec<Artifact>> {
    spec(endpoint)?;
    if !response.is_object() { return Err(invalid("Fal result must be an object.")); }
    let mut result = Vec::new();
    let primary = match endpoint {
        "fal-ai/flux/dev" | "fal-ai/flux/dev/image-to-image" => ("images", Kind::Image, "image", true),
        "fal-ai/stable-audio" => ("audio_file", Kind::Audio, "audio", false),
        "fal-ai/stable-audio-25/audio-to-audio" => ("audio", Kind::Audio, "audio", false),
        "fal-ai/trellis" | "fal-ai/hyper3d/rodin" => ("model_mesh", Kind::Model3d, "model", false),
        "fal-ai/hunyuan3d-v3/text-to-3d" | "fal-ai/trellis-2/retexture" => ("model_glb", Kind::Model3d, "model", false),
        _ => ("video", Kind::Video, "video", false),
    };
    collect(&mut result, &response[primary.0], primary.1, primary.2, primary.3, true)?;
    if endpoint == "fal-ai/hyper3d/rodin" {
        collect(&mut result, &response["textures"], Kind::Image, "texture", true, false)?;
    }
    if endpoint == "fal-ai/hunyuan3d-v3/text-to-3d" {
        if let Some(variants) = response.get("model_urls") {
            let variants = variants.as_object().ok_or_else(|| invalid("Fal model variants must be an object."))?;
            if variants.len() > 16 { return Err(invalid("Too many fal model variants.")); }
            for (format, value) in variants {
                if value.is_null() { continue; }
                if !["glb", "obj", "fbx"].contains(&format.as_str()) {
                    return Err(invalid("Fal returned an unrecognized model variant role."));
                }
                if format == "glb" && value["url"].as_str().is_some_and(|url| result.iter().any(|a| a.role == "model" && a.error.is_none() && a.url == url)) {
                    continue;
                }
                collect(&mut result, value, Kind::Model3d, &format!("model_{format}"), false, true)?;
            }
        }
        collect(&mut result, &response["thumbnail"], Kind::Image, "thumbnail", false, false)?;
    }
    Ok(result)
}

fn collect(result: &mut Vec<Artifact>, value: &Value, kind: Kind, role: &str, array: bool, required: bool) -> Result<()> {
    if value.is_null() && !required { return Ok(()); }
    let missing = Value::Null;
    let values: Vec<&Value> = if array {
        match value.as_array() {
            Some(items) if items.len().saturating_add(result.len()) > 64 => return Err(invalid("Fal returned too many media artifacts.")),
            Some(items) if !items.is_empty() => items.iter().collect(),
            Some(_) if !required => return Ok(()),
            _ => vec![&missing],
        }
    } else { vec![value] };
    if values.len().saturating_add(result.len()) > 64 {
        return Err(invalid("Fal returned too many media artifacts."));
    }
    for item in values {
        let url = item["url"].as_str().filter(|s| !s.is_empty() && s.len() <= 32 * 1024 * 1024 && !s.contains(['\0', '\r', '\n']));
        let mime = item.get("content_type").and_then(Value::as_str).filter(|s| s.len() <= 256).map(str::to_owned);
        result.push(Artifact { kind, role: role.into(), url: url.unwrap_or("").into(), reported_mime: mime,
            error: url.is_none().then(|| "Fal returned an artifact without a usable URL.".into()) });
    }
    Ok(())
}
