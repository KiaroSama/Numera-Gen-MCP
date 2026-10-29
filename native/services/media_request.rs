use super::{finalization::locked, generation::Generation};
use crate::{compat::value::Value, error::{NumeraError, Result}, files::{output, paths}, http::assets, media::{self, Kind, fal}};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub kind: Kind,
    pub role: String,
    pub output_id: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
    pub data_url: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_id: Option<String>,
    pub connection: Option<String>,
    pub model: Option<String>,
    pub mode: String,
    pub prompt: Option<String>,
    #[serde(default = "object")]
    pub parameters: Json,
    #[serde(default)]
    pub references: Vec<Reference>,
    #[serde(default = "yes")]
    pub wait: bool,
    pub output_subdirectory: Option<String>,
    pub filename_prefix: Option<String>,
}
fn object() -> Json { serde_json::json!({}) }
fn yes() -> bool { true }
pub fn invalid(message: &str) -> NumeraError { NumeraError::new("invalid_input", message) }
impl Request {
    pub fn parse(raw: Value) -> Result<Self> {
        let schema: Json = serde_json::from_str(include_str!("../../schemas/media-request.schema.json")).map_err(|_|NumeraError::runtime())?;
        let mut authoritative=raw.to_js();
        crate::compat::js_schema::Schema::compile(&schema)?.default_and_validate(&mut authoritative)
            .map_err(|_|invalid("Media request does not match its public schema."))?;
        let request: Self = serde_json::from_value(Value::from_js(&authoritative).to_serde()?).map_err(|_| invalid("Media request does not match its public schema."))?;
        if !crate::config::media::MODES.contains(&request.mode.as_str())
            || request.request_id.as_ref().is_some_and(|id| id.is_empty() || id.len()>100 || !id.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b)))
            || request.model.as_ref().is_some_and(|s|s.is_empty()||s.len()>300)
            || request.connection.as_ref().is_some_and(|s|s.is_empty()||s.len()>100)
            || request.prompt.as_ref().is_some_and(|s|s.encode_utf16().count()>32768)
            || request.references.len()>16 || request.parameters.as_object().is_none_or(|v|v.len()>32) {
            return Err(invalid("Invalid or oversized media request fields."));
        }
        for reference in &request.references {
            let count=[&reference.output_id,&reference.path,&reference.url,&reference.data_url].iter().filter(|v|v.is_some()).count();
            if count!=1 || reference.role.is_empty() || reference.role.len()>64
                || !reference.role.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b"_-".contains(&b))
                || reference.output_id.as_ref().is_some_and(|s|uuid::Uuid::parse_str(s).is_err())
                || reference.path.as_ref().is_some_and(|s|s.is_empty()||s.len()>32768)
                || reference.url.as_ref().is_some_and(|s|s.is_empty()||s.len()>16384)
                || reference.data_url.as_ref().is_some_and(|s|s.len()>30000000) {
                return Err(invalid("Media references require one bounded typed source and an explicit role."));
            }
        }
        for path in [&request.output_subdirectory,&request.filename_prefix].into_iter().flatten() {
            if path.len()>1000 { return Err(invalid("Output path exceeds its bound.")); }
            paths::safe_relative(path)?;
        }
        if request.filename_prefix.as_ref().is_some_and(|s|s.len()>200||s.contains(['/', '\\'])) {
            return Err(invalid("Filename prefix cannot contain directories."));
        }
        Ok(request)
    }
}
pub fn decode_data(raw: &str, kind: Kind, limit: usize) -> Result<Vec<u8>> {
    let (head, encoded)=raw.split_once(',').ok_or_else(||invalid("Malformed media data URI."))?;
    let mime=head.strip_prefix("data:").and_then(|s|s.strip_suffix(";base64")).ok_or_else(||invalid("Media data URI must use explicit base64 encoding."))?;
    let allowed=match kind { Kind::Image=>["image/png","image/jpeg","image/webp"].contains(&mime),Kind::Audio=>mime=="audio/wav",Kind::Video=>mime=="video/mp4",Kind::Model3d=>mime=="model/gltf-binary" };
    if !allowed || encoded.len()>limit.div_ceil(3).saturating_mul(4).saturating_add(4) {return Err(invalid("Media data URI kind, format or byte limit differs."));}
    let bytes=STANDARD.decode(encoded).map_err(|_|invalid("Invalid media base64 payload."))?;
    if bytes.is_empty()||bytes.len()>limit {return Err(invalid("Media exceeds its byte limit."));}Ok(bytes)
}
pub fn effective_parameters(connection: &Value, model: &str, request: &Request) -> Result<Json> {
    let mut parameters = serde_json::Map::new();
    for defaults in [&connection["defaults"], &connection["modelOverrides"][model]["defaults"]] {
        if defaults.is_null() { continue; }
        if defaults.as_object().is_none_or(|values|values.len()>32) {return Err(invalid("Configured media defaults exceed their bounded object contract."));}
        let defaults = defaults.to_serde()?;
        let defaults = defaults.as_object().ok_or_else(|| invalid("Configured media defaults must be an object."))?;
        parameters.extend(defaults.clone());
    }
    parameters.extend(request.parameters.as_object().ok_or_else(|| invalid("Native media parameters must be an object."))?.clone());
    Ok(Json::Object(parameters))
}

pub fn preflight(model: &str, request: &Request, parameters: &Json, config: &Value) -> Result<()> {
    if request.references.len() as u64 > config["files"]["maxReferences"].as_u64().unwrap_or(14) {
        return Err(invalid("Media reference count exceeds configuration."));
    }
    let references: Vec<_> = request.references.iter().map(|reference| fal::Reference {
        kind: reference.kind, role: reference.role.clone(), url: "validated-before-transfer".into(),
    }).collect();
    fal::prepare(model, &request.mode, request.prompt.as_deref(), &references, parameters)?;
    Ok(())
}

pub async fn references(generation: &Generation, request: &Request, cancel: &CancellationToken) -> Result<(Vec<fal::Reference>, Json)> {
    let config=&generation.config.value;
    if request.references.len() as u64>config["files"]["maxReferences"].as_u64().unwrap_or(14) {return Err(invalid("Media reference count exceeds configuration."));}
    let limit=config["files"]["maxInputBytes"].as_u64().unwrap_or(20*1024*1024) as usize;
    let aggregate=config["files"]["maxAggregateBytes"].as_u64().unwrap_or(100*1024*1024) as usize;
    let pixels=config["files"]["maxPixels"].as_u64().unwrap_or(64000000);
    let roots=crate::error::array(&config["files"]["allowedInputRoots"]).iter().map(|v|v.as_str().map(PathBuf::from).ok_or_else(NumeraError::runtime)).collect::<Result<Vec<_>>>()?;
    let mut total=0usize;let mut wire=Vec::new();let mut hashes=Vec::new();
    for reference in &request.references {
        if cancel.is_cancelled() {return Err(super::generation::cancelled());}
        let bytes=if let Some(id)=&reference.output_id {
            let (owned,bytes)=output::owned_media(config,&*locked(&generation.store)?,id)?;
            if owned["kind"].as_str().unwrap_or("image")!=reference.kind.as_str() {return Err(invalid("Owned reference kind differs."));}bytes
        }else if let Some(path)=&reference.path {paths::input(path,&roots,limit)?}
        else if let Some(raw)=&reference.data_url {decode_data(raw,reference.kind,limit)?}
        else {assets::fetch(&reference.url.as_ref().ok_or_else(NumeraError::runtime)?.as_str().into(),config,limit,cancel).await?};
        total=total.checked_add(bytes.len()).filter(|n|*n<=aggregate).ok_or_else(||invalid("Aggregate media reference limit exceeded."))?;
        let info=media::inspect(reference.kind,&bytes,limit,pixels)?;
        if let Some(raw)=&reference.data_url {
            let mime=raw.split_once(',').and_then(|(head,_)|head.strip_prefix("data:")).and_then(|head|head.strip_suffix(";base64"));
            if mime!=Some(info.mime.as_str()){return Err(invalid("Reference MIME differs from its validated original bytes."));}
        }
        let data=format!("data:{};base64,{}",info.mime,STANDARD.encode(&bytes));
        if data.len()>30000000 {return Err(invalid("Validated reference exceeds the native data URI transfer limit; no upload was attempted."));}
        hashes.push(serde_json::json!({"kind":reference.kind.as_str(),"role":reference.role,"sha256":crate::compat::hash::sha256(&bytes)}));
        wire.push(fal::Reference{kind:reference.kind,role:reference.role.clone(),url:data});
    }
    Ok((wire,Json::Array(hashes)))
}
