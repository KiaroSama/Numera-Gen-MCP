//! Strictly validated opt-in user-declared fal endpoint contracts (`modelOverrides.<id>.falContract`).
use super::{
    Kind,
    fal::{Artifact, Contract, Reference},
    fal_contract::spec,
    invalid,
};
use crate::compat::value::Value as CompatValue;
use crate::error::Result;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct ParamRule {
    pub kind: ParamKind,
}

#[derive(Clone, Debug)]
pub enum ParamKind {
    Boolean,
    Integer { min: i64, max: i64 },
    Number { min: f64, max: f64 },
    Enum(Vec<String>),
    String { max_length: usize },
}

#[derive(Clone, Debug)]
pub struct ReferenceSpec {
    pub role: String,
    pub kind: Kind,
    pub wire_field: String,
}

#[derive(Clone, Debug)]
pub struct DynamicContract {
    pub output_field: String,
    pub output_array: bool,
    pub output_kind: Kind,
    pub prompt: bool,
    pub reference: Option<ReferenceSpec>,
    pub allowed_parameters: HashMap<String, ParamRule>,
}

pub fn valid_endpoint_id(endpoint: &str) -> bool {
    let segments: Vec<&str> = endpoint.split('/').collect();
    if !(2..=6).contains(&segments.len()) {
        return false;
    }
    segments.iter().all(|seg| {
        !seg.is_empty()
            && seg.len() <= 64
            && *seg != "."
            && *seg != ".."
            && seg
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && seg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    })
}

fn valid_field(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn parse_kind(value: Option<&str>) -> Result<Kind> {
    match value {
        Some("image") => Ok(Kind::Image),
        Some("video") => Ok(Kind::Video),
        Some("audio") => Ok(Kind::Audio),
        Some("3d") => Ok(Kind::Model3d),
        _ => Err(invalid("Invalid falContract kind.")),
    }
}

pub fn parse(endpoint: &str, raw: &CompatValue) -> Result<DynamicContract> {
    if !valid_endpoint_id(endpoint) {
        return Err(invalid("Invalid fal endpoint identifier for falContract."));
    }
    if spec(endpoint).is_ok() {
        return Err(invalid(
            "Built-in evidenced fal endpoints cannot be overridden with falContract.",
        ));
    }
    let obj = raw
        .as_object()
        .ok_or_else(|| invalid("falContract must be an object."))?;
    let output_field = obj
        .get("outputField")
        .and_then(CompatValue::as_str)
        .filter(|s| valid_field(s))
        .ok_or_else(|| invalid("falContract.outputField must be a valid field identifier."))?
        .to_owned();
    let output_array = match obj.get("outputArray") {
        None => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| invalid("falContract.outputArray must be a boolean."))?,
    };
    let output_kind = parse_kind(obj.get("outputKind").and_then(CompatValue::as_str))?;
    let prompt = match obj.get("prompt") {
        None => true,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| invalid("falContract.prompt must be a boolean."))?,
    };
    let reference = if let Some(r) = obj.get("reference") {
        let r = r
            .as_object()
            .ok_or_else(|| invalid("falContract.reference must be an object."))?;
        let role = r
            .get("role")
            .and_then(CompatValue::as_str)
            .filter(|s| valid_field(s))
            .ok_or_else(|| invalid("falContract.reference.role is invalid."))?
            .to_owned();
        let kind = parse_kind(r.get("kind").and_then(CompatValue::as_str))?;
        let wire_field = r
            .get("wireField")
            .and_then(CompatValue::as_str)
            .filter(|s| valid_field(s))
            .ok_or_else(|| invalid("falContract.reference.wireField is invalid."))?
            .to_owned();
        Some(ReferenceSpec {
            role,
            kind,
            wire_field,
        })
    } else {
        None
    };
    let mut allowed_parameters = HashMap::new();
    if let Some(params) = obj.get("allowedParameters") {
        let params = params
            .as_object()
            .filter(|p| p.len() <= 32)
            .ok_or_else(|| invalid("falContract.allowedParameters must be a bounded object."))?;
        for (k, v) in params {
            let name = k
                .as_str()
                .filter(|s| valid_field(s))
                .ok_or_else(|| invalid("Invalid falContract parameter name."))?;
            let spec_obj = v
                .as_object()
                .ok_or_else(|| invalid("Invalid falContract parameter rule."))?;
            let kind = match spec_obj.get("type").and_then(CompatValue::as_str) {
                Some("boolean") => ParamKind::Boolean,
                Some("integer") => {
                    let min = spec_obj
                        .get("min")
                        .and_then(CompatValue::as_i64)
                        .ok_or_else(|| invalid("Integer parameter requires min."))?;
                    let max = spec_obj
                        .get("max")
                        .and_then(CompatValue::as_i64)
                        .filter(|&m| m >= min)
                        .ok_or_else(|| invalid("Integer parameter requires max >= min."))?;
                    ParamKind::Integer { min, max }
                }
                Some("number") => {
                    let min = spec_obj
                        .get("min")
                        .and_then(CompatValue::as_f64)
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| invalid("Number parameter requires finite min."))?;
                    let max = spec_obj
                        .get("max")
                        .and_then(CompatValue::as_f64)
                        .filter(|n| n.is_finite() && *n >= min)
                        .ok_or_else(|| invalid("Number parameter requires finite max >= min."))?;
                    ParamKind::Number { min, max }
                }
                Some("enum") => {
                    let values = spec_obj
                        .get("values")
                        .and_then(CompatValue::as_array)
                        .filter(|a| !a.is_empty() && a.len() <= 64)
                        .ok_or_else(|| invalid("Enum parameter requires non-empty values."))?
                        .iter()
                        .map(|item| {
                            item.as_str()
                                .filter(|s| s.len() <= 128 && !s.contains('\0'))
                                .map(str::to_owned)
                                .ok_or_else(|| invalid("Invalid enum value in falContract."))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    ParamKind::Enum(values)
                }
                Some("string") => {
                    let max_length = spec_obj
                        .get("maxLength")
                        .and_then(CompatValue::as_u64)
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|&n| (1..=32768).contains(&n))
                        .ok_or_else(|| {
                            invalid("String parameter requires maxLength in 1..=32768.")
                        })?;
                    ParamKind::String { max_length }
                }
                _ => return Err(invalid("Unsupported falContract parameter type.")),
            };
            allowed_parameters.insert(name.to_owned(), ParamRule { kind });
        }
    }
    Ok(DynamicContract {
        output_field,
        output_array,
        output_kind,
        prompt,
        reference,
        allowed_parameters,
    })
}

pub fn contract(endpoint: &str, mode: &str, raw: &CompatValue) -> Result<Contract> {
    let dynamic = parse(endpoint, raw)?;
    let roles = dynamic
        .reference
        .as_ref()
        .map(|r| vec![r.role.clone()])
        .unwrap_or_default();
    Ok(Contract {
        endpoint: endpoint.into(),
        mode: mode.into(),
        output_types: vec![dynamic.output_kind],
        required_roles: roles,
        evidence: format!("user_declared:{endpoint}"),
    })
}

pub fn prepare(
    endpoint: &str,
    mode: &str,
    raw: &CompatValue,
    prompt: Option<&str>,
    references: &[Reference],
    parameters: &Value,
) -> Result<Value> {
    let dynamic = parse(endpoint, raw)?;
    let c = contract(endpoint, mode, raw)?;
    if references.len() > 16 {
        return Err(invalid("Too many fal media references."));
    }
    let mut body: Map<String, Value> = parameters
        .as_object()
        .filter(|p| p.len() <= 32)
        .cloned()
        .ok_or_else(|| invalid("Fal parameters must be a bounded object."))?;
    for (key, value) in &body {
        let Some(rule) = dynamic.allowed_parameters.get(key) else {
            return Err(invalid(
                "Parameter is not declared in falContract.allowedParameters.",
            ));
        };
        let valid = match &rule.kind {
            ParamKind::Boolean => value.is_boolean(),
            ParamKind::Integer { min, max } => {
                value.as_i64().is_some_and(|n| n >= *min && n <= *max)
            }
            ParamKind::Number { min, max } => value
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= *min && n <= *max),
            ParamKind::Enum(values) => value
                .as_str()
                .is_some_and(|s| values.iter().any(|v| v == s)),
            ParamKind::String { max_length } => value
                .as_str()
                .is_some_and(|s| s.encode_utf16().count() <= *max_length && !s.contains('\0')),
        };
        if !valid {
            return Err(invalid(
                "Parameter value does not satisfy falContract.allowedParameters.",
            ));
        }
    }
    if dynamic.prompt && prompt.is_none_or(|p| p.trim().is_empty()) {
        return Err(invalid("The fal mode requires an explicit prompt."));
    }
    if let Some(prompt) = prompt {
        if !dynamic.prompt || prompt.encode_utf16().count() > 32768 || prompt.contains('\0') {
            return Err(invalid(
                "This fal endpoint cannot carry the supplied prompt.",
            ));
        }
        body.insert("prompt".into(), Value::String(prompt.into()));
    }
    let mut seen = HashSet::new();
    for reference in references {
        if reference.url.is_empty()
            || reference.url.len() > 32 * 1024 * 1024
            || reference.url.contains(['\0', '\r', '\n'])
        {
            return Err(invalid("Invalid or excessive fal reference URL/data URI."));
        }
        let Some(spec_ref) = &dynamic.reference else {
            return Err(invalid("This falContract does not accept references."));
        };
        if reference.role != spec_ref.role || reference.kind != spec_ref.kind {
            return Err(invalid(
                "Reference role or kind does not match falContract.",
            ));
        }
        if !seen.insert(spec_ref.role.as_str()) {
            return Err(invalid("Duplicate fal media reference role."));
        }
        body.insert(
            spec_ref.wire_field.clone(),
            Value::String(reference.url.clone()),
        );
    }
    if c.required_roles
        .iter()
        .any(|role| !seen.contains(role.as_str()))
    {
        return Err(invalid("A required fal media reference is missing."));
    }
    Ok(Value::Object(body))
}

pub fn outputs(endpoint: &str, raw: &CompatValue, response: &Value) -> Result<Vec<Artifact>> {
    let dynamic = parse(endpoint, raw)?;
    if !response.is_object() {
        return Err(invalid("Fal result must be an object."));
    }
    let mut result = Vec::new();
    let role = match dynamic.output_kind {
        Kind::Image => "image",
        Kind::Video => "video",
        Kind::Audio => "audio",
        Kind::Model3d => "model",
    };
    super::fal::collect_artifacts(
        &mut result,
        &response[&dynamic.output_field],
        dynamic.output_kind,
        role,
        dynamic.output_array,
        true,
    )?;
    if result.is_empty() {
        return Err(invalid("Fal result did not contain the declared output."));
    }
    Ok(result)
}
