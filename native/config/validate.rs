use super::{invalid, js_trim};
use crate::error::Result;
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path, sync::OnceLock};
use url::Url;

static ADVANCED: OnceLock<
    std::result::Result<(Value, jsonschema::Validator), crate::error::NumeraError>,
> = OnceLock::new();
static SIMPLE: OnceLock<
    std::result::Result<(Value, jsonschema::Validator), crate::error::NumeraError>,
> = OnceLock::new();

pub(crate) fn default_and_validate(value: &mut Value, simple: bool) -> Result<()> {
    let cached = if simple { &SIMPLE } else { &ADVANCED };
    let (schema, validator) = cached
        .get_or_init(|| {
            let text = if simple {
                include_str!("../../schemas/simple-config.schema.json")
            } else {
                include_str!("../../schemas/config.schema.json")
            };
            let schema: Value = serde_json::from_str(text)
                .map_err(|_| invalid("Invalid bundled config schema."))?;
            let mut compiled = schema.clone();
            // Zod counts UTF-16 code units; JSON Schema counts Unicode scalars. Check source lengths
            // separately so astral characters neither bypass maxima nor fail valid minima.
            remove_lengths(&mut compiled);
            let validator = jsonschema::options()
                .should_validate_formats(true)
                .with_format("uri", |text| Url::parse(text).is_ok())
                .build(&compiled)
                .map_err(|_| invalid("Cannot initialize bundled config validation."))?;
            Ok((schema, validator))
        })
        .as_ref()
        .map_err(Clone::clone)?;
    if !simple && value.is_object() && value.get("files").is_none() {
        value["files"] = json!({});
    }
    normalize_numbers(value);
    defaults(value, schema);
    if !validator.is_valid(value) || !source_lengths(value, schema) {
        return Err(invalid(if simple {
            "Simple configuration requires api_endpoint, api_key and models with id/name."
        } else {
            "Configuration does not match schemaVersion 1. Check the generated JSON Schema."
        }));
    }
    Ok(())
}

fn normalize_numbers(value: &mut Value) {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Number(number) => {
                if let Some(n) = number.as_f64() {
                    *value = if n.fract() == 0.0 && (0.0..18446744073709551616.0).contains(&n) {
                        json!(n as u64)
                    } else if n.fract() == 0.0 && (-9223372036854775808.0..0.0).contains(&n) {
                        json!(n as i64)
                    } else {
                        serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
                    };
                }
            }
            Value::Object(object) => pending.extend(object.values_mut()),
            Value::Array(array) => pending.extend(array.iter_mut()),
            _ => {}
        }
    }
}

fn defaults(value: &mut Value, schema: &Value) {
    if let Some(object) = value.as_object_mut() {
        if let Some(properties) = schema["properties"].as_object() {
            for (key, child) in properties {
                if !object.contains_key(key) {
                    if let Some(default) = child.get("default") {
                        object.insert(key.clone(), default.clone());
                    }
                }
                if let Some(value) = object.get_mut(key) {
                    defaults(value, child);
                }
            }
        }
        if let Some(child) = schema
            .get("additionalProperties")
            .filter(|child| child.is_object())
        {
            for (key, value) in object {
                if schema["properties"].get(key).is_none() {
                    defaults(value, child);
                }
            }
        }
    } else if let Some(array) = value.as_array_mut() {
        if let Some(child) = schema.get("items") {
            for value in array {
                defaults(value, child);
            }
        }
    }
}

fn remove_lengths(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            object.remove("minLength");
            object.remove("maxLength");
            if object.get("pattern").and_then(Value::as_str) == Some("\\S") {
                object.remove("pattern");
            }
            for value in object.values_mut() {
                remove_lengths(value);
            }
        }
        Value::Array(array) => {
            for value in array {
                remove_lengths(value);
            }
        }
        _ => {}
    }
}

fn source_lengths(value: &Value, schema: &Value) -> bool {
    if let Some(string) = value.as_str() {
        let n = string.encode_utf16().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|min| n < min)
            || schema["maxLength"].as_u64().is_some_and(|max| n > max)
            || (schema["pattern"] == "\\S" && js_trim(string).is_empty())
        {
            return false;
        }
    }
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            let child = schema["properties"]
                .get(key)
                .or_else(|| schema.get("additionalProperties"));
            if child.is_some_and(|s| !source_lengths(value, s)) {
                return false;
            }
        }
    }
    if let (Some(array), Some(child)) = (value.as_array(), schema.get("items")) {
        if array.iter().any(|value| !source_lengths(value, child)) {
            return false;
        }
    }
    if let Some(branches) = schema["anyOf"].as_array() {
        let matching = |branch: &&Value| match branch["type"].as_str() {
            Some("null") => value.is_null(),
            Some("string") => value.is_string(),
            Some("object") => value.is_object(),
            Some("array") => value.is_array(),
            Some("boolean") => value.is_boolean(),
            Some("integer" | "number") => value.is_number(),
            _ => true,
        };
        if !branches
            .iter()
            .filter(matching)
            .any(|branch| source_lengths(value, branch))
        {
            return false;
        }
    }
    true
}

pub(crate) fn cross_validate(config: &mut Value, timeout: Option<&str>) -> Result<()> {
    let connections = config["connections"]
        .as_object_mut()
        .ok_or_else(|| invalid("Invalid connections."))?;
    for connection in connections.values_mut() {
        let auth = &connection["auth"];
        if auth["apiKey"]
            .as_str()
            .is_some_and(|key| js_trim(key).is_empty() || key.contains(['\r', '\n', '\0']))
        {
            return Err(invalid(
                "Inline credential must be nonempty and cannot contain control characters.",
            ));
        }
        let mut ids = HashSet::new();
        if connection["configuredModels"]
            .as_array()
            .is_some_and(|models| {
                models
                    .iter()
                    .any(|m| !ids.insert(m["id"].as_str().unwrap_or("").to_owned()))
            })
        {
            return Err(invalid("Configured model IDs must be unique."));
        }
        if let Some(proxy) = connection.get("proxy") {
            let url = Url::parse(proxy["url"].as_str().unwrap_or(""))
                .map_err(|_| invalid("Invalid proxy URL."))?;
            if !["http", "https"].contains(&url.scheme())
                || !url.username().is_empty()
                || url.password().is_some_and(|p| !p.is_empty())
                || url.query().is_some_and(|q| !q.is_empty())
                || url.fragment().is_some_and(|f| !f.is_empty())
            {
                return Err(invalid(
                    "Proxy URL must not contain credentials, query or fragments.",
                ));
            }
        }
        validate_base(connection)?;
        if let Some(origin) = auth["origin"].as_str().filter(|v| !v.is_empty()) {
            if url_origin(origin)? != url_origin(connection["baseUrl"].as_str().unwrap_or(""))? {
                return Err(invalid("Credential destination origin mismatch."));
            }
        }
        if auth["type"] != "none"
            && ["apiKey", "secretEnv", "secretFile", "secretDpapiFile"]
                .iter()
                .filter(|field| auth[**field].as_str().is_some_and(|v| !v.is_empty()))
                .count()
                != 1
        {
            return Err(invalid(
                "Select exactly one secret source per authenticated connection.",
            ));
        }
        if auth["type"] == "header" {
            let header = auth["header"].as_str().unwrap_or("");
            if header.is_empty()
                || !header
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || ["host", "content-length", "cookie"]
                    .contains(&header.to_ascii_lowercase().as_str())
            {
                return Err(invalid("Invalid authentication header."));
            }
        }
        if let Some(edit) = connection.get("edit") {
            let multipart = edit["mode"] == "multipart";
            let encoding = edit["encoding"].as_str().unwrap_or("");
            if (multipart && !["image", "image[]"].contains(&encoding))
                || (!multipart && encoding == "image[]")
                || (!multipart
                    && encoding == "image"
                    && edit["maxReferences"].as_u64().unwrap_or(0) > 1)
            {
                return Err(invalid(
                    "Edit mode, encoding and reference capacity are incompatible.",
                ));
            }
        }
        if let Some(timeout) = timeout {
            let n = js_number(timeout).ok_or_else(|| invalid("Invalid request timeout."))?;
            if n.fract() != 0.0 || !(1.0..=3600000.0).contains(&n) {
                return Err(invalid("Invalid request timeout."));
            }
            connection["requestTimeoutMs"] = json!(n as u64);
        }
    }
    let mut paths = vec![
        &config["outputDir"],
        &config["stateDir"],
        &config["logging"]["directory"],
    ];
    if let Some(roots) = config["files"]["allowedInputRoots"].as_array() {
        paths.extend(roots);
    }
    if paths
        .iter()
        .any(|v| !absolute(Path::new(v.as_str().unwrap_or(""))))
    {
        return Err(invalid("Storage and input roots must be absolute paths."));
    }
    if let Some(name) = config["defaultConnection"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        if config["connections"].get(name).is_none() {
            return Err(invalid("Default connection does not exist."));
        }
    }
    Ok(())
}

fn validate_base(connection: &Value) -> Result<()> {
    let base = connection["baseUrl"].as_str().unwrap_or("");
    let url = Url::parse(base).map_err(|_| invalid("Invalid API base URL."))?;
    let lower = base.to_ascii_lowercase();
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some_and(|p| !p.is_empty())
        || url.query().is_some_and(|q| !q.is_empty())
        || url.fragment().is_some_and(|f| !f.is_empty())
        || ["%2e", "%2f", "%5c"].iter().any(|s| lower.contains(s))
        || base.split('/').any(|part| part == "..")
    {
        return Err(invalid(
            "API base must not contain credentials, query, fragments or traversal.",
        ));
    }
    let path = url.path().strip_suffix('/').unwrap_or(url.path());
    if [
        "generations",
        "edits",
        "generateContent",
        "responses",
        "interactions",
        "images",
        "chat/completions",
    ]
    .iter()
    .any(|endpoint| path.ends_with(&format!("/{endpoint}")))
    {
        return Err(invalid("Supply an API prefix, not a full endpoint URL."));
    }
    if connection["baseUrlMode"] == "origin" && url.path() != "/" {
        return Err(invalid("Origin mode requires an origin without a path."));
    }
    if let Some(query) = connection["query"].as_object() {
        for key in query.keys() {
            let lower = key.to_ascii_lowercase();
            if key != "pageToken"
                && ["key", "token", "secret", "auth", "signature"]
                    .iter()
                    .any(|s| lower.contains(s))
            {
                return Err(invalid("Credentials must not be query parameters."));
            }
        }
    }
    Ok(())
}

pub(crate) fn url_origin(value: &str) -> Result<String> {
    let url = Url::parse(value).map_err(|_| invalid("Invalid destination URL."))?;
    Ok(url.origin().ascii_serialization())
}

pub(crate) fn absolute(path: &Path) -> bool {
    #[cfg(windows)]
    {
        let s = path.as_os_str().to_string_lossy();
        s.starts_with(['/', '\\'])
            || (s.as_bytes().get(1) == Some(&b':')
                && s.as_bytes()
                    .get(2)
                    .is_some_and(|b| [b'/', b'\\'].contains(b)))
    }
    #[cfg(not(windows))]
    {
        path.is_absolute()
    }
}

pub(crate) fn js_number(text: &str) -> Option<f64> {
    let text = js_trim(text);
    if text.is_empty() {
        return Some(0.0);
    }
    let radix = if text.starts_with("0x") || text.starts_with("0X") {
        Some(16)
    } else if text.starts_with("0o") || text.starts_with("0O") {
        Some(8)
    } else if text.starts_with("0b") || text.starts_with("0B") {
        Some(2)
    } else {
        None
    };
    if let Some(radix) = radix {
        let digits = &text[2..];
        if digits.is_empty() {
            return None;
        }
        let mut value = 0.0;
        for c in digits.chars() {
            value = value * f64::from(radix) + f64::from(c.to_digit(radix)?);
        }
        return value.is_finite().then_some(value);
    }
    if !text
        .bytes()
        .all(|b| b.is_ascii_digit() || b"+-eE.".contains(&b))
    {
        return None;
    }
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}
