mod credentials;
#[cfg(windows)]
mod dpapi;
mod env;
mod json;
mod load;
mod reload;
mod validate;

use crate::error::{NumeraError, Result};
pub use credentials::{PinnedConnection, credentials, snapshot_credentials};
pub use env::{MAX_CONFIG_BYTES, parse_env_config, read_config_text};
pub use json::{clone_json, drop_json};
pub use load::{config_file, load_config, user_root};
pub use reload::{ConfigReload, ReloadEvent, ReloadResult};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub struct Config {
    pub value: Value,
    pub file: PathBuf,
    pub root: PathBuf,
}

impl Clone for Config {
    fn clone(&self) -> Self {
        Self {
            value: clone_json(&self.value),
            file: self.file.clone(),
            root: self.root.clone(),
        }
    }
}
impl Drop for Config {
    fn drop(&mut self) {
        drop_json(std::mem::take(&mut self.value));
    }
}

impl Config {
    pub fn into_value(mut self) -> Value {
        std::mem::take(&mut self.value)
    }

    pub fn parse(
        text: &str,
        file: &Path,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let flags = load::arguments(args)?;
        let selected = flags
            .get("config")
            .or_else(|| env.get("NUMERA_CONFIG"))
            .filter(|s| !s.is_empty());
        let file = selected.map_or_else(|| file.to_path_buf(), PathBuf::from);
        if !flags.contains_key("openai-compat") && !validate::absolute(&file) {
            return Err(invalid("NUMERA_CONFIG must be absolute."));
        }
        let raw = if flags.contains_key("openai-compat") {
            load::compat_config(env)?
        } else {
            if text.len() > MAX_CONFIG_BYTES {
                return Err(invalid("Configuration exceeds 1 MiB."));
            }
            let filename = file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if filename.ends_with(".env")
                || filename
                    .split_once(".env.")
                    .is_some_and(|(_, tail)| !tail.is_empty())
            {
                parse_env_config(text)?
            } else {
                json::parse(text)?.take()
            }
        };
        let mut raw = json::OwnedJson(raw);
        if !raw.is_object() {
            return Err(invalid("Configuration must be a JSON object."));
        }
        if raw.get("api_endpoint").is_some() || raw.get("api_key").is_some() {
            let expanded = expand_simple(&mut raw, &file)?;
            drop_json(std::mem::replace(&mut raw.0, expanded));
        }
        load::overrides(&mut raw, &flags, env)?;
        validate::default_and_validate(&mut raw, false)?;
        validate::cross_validate(
            &mut raw,
            load::get(
                &flags,
                env,
                "request-timeout-ms",
                "NUMERA_REQUEST_TIMEOUT_MS",
            ),
        )?;
        let root = file.parent().unwrap_or(Path::new("")).to_path_buf();
        Ok(Self {
            value: raw.take(),
            file,
            root,
        })
    }

    pub fn select(&self, name: Option<&str>) -> Result<(String, Value)> {
        let id = name.or_else(|| self.value["defaultConnection"].as_str());
        if let Some(id) = id.filter(|id| !id.is_empty()) {
            if let Some(connection) = self.value["connections"].get(id) {
                if connection["enabled"] == true {
                    return Ok((id.to_owned(), clone_json(connection)));
                }
            }
        }
        Err(NumeraError::new(
            "invalid_input",
            "Select an enabled configured connection.",
        ))
    }
}

pub(crate) fn invalid(message: &str) -> NumeraError {
    NumeraError::new("invalid_configuration", message)
}

pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| matches!(c, '\u{0009}'..='\u{000D}' | '\u{0020}' | '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'))
}

fn expand_simple(raw: &mut Value, file: &Path) -> Result<Value> {
    validate::default_and_validate(raw, true)?;
    let key = raw["api_key"]
        .as_str()
        .ok_or_else(|| invalid("Invalid key."))?;
    let models = raw["models"]
        .as_array()
        .ok_or_else(|| invalid("Invalid models."))?;
    let mut ids = std::collections::HashSet::new();
    if js_trim(key).is_empty()
        || key.contains(['\r', '\n', '\0'])
        || models.iter().any(|m| {
            js_trim(m["id"].as_str().unwrap_or("")).is_empty()
                || js_trim(m["name"].as_str().unwrap_or("")).is_empty()
                || !ids.insert(m["id"].as_str().unwrap_or(""))
        })
    {
        return Err(invalid(
            "Key and model IDs/names must be nonempty; model IDs must be unique.",
        ));
    }
    let profile = raw["profile"].as_str().unwrap_or("openai-images");
    if (profile == "openai-responses") != raw.get("orchestration_model").is_some() {
        return Err(invalid(
            "orchestration_model is required only for the openai-responses profile.",
        ));
    }
    let enabled: Vec<&Value> = models.iter().filter(|m| m["enabled"] == true).collect();
    if enabled.is_empty() {
        return Err(invalid("Enable at least one configured model."));
    }
    let root = file.parent().unwrap_or(Path::new(""));
    let origin = validate::url_origin(raw["api_endpoint"].as_str().unwrap_or(""))?;
    let mut auth = json!({"type": if profile.starts_with("gemini") { "header" } else { "bearer" }, "apiKey":key, "origin":origin});
    if profile.starts_with("gemini") {
        auth["header"] = json!("x-goog-api-key");
    }
    let mut overrides = serde_json::Map::new();
    for model in &enabled {
        let mut defaults = serde_json::Map::new();
        for field in ["output_format", "size", "quality"] {
            if let Some(value) = model.get(field).filter(|v| !v.is_null()) {
                defaults.insert(field.into(), value.clone());
            }
        }
        overrides.insert(
            model["id"].as_str().unwrap_or("").into(),
            json!({"defaults":defaults}),
        );
    }
    let mut connection = json!({
        "adapter": if ["omniroute", "9router"].contains(&profile) { "openai-images" } else { profile },
        "baseUrl":raw["api_endpoint"], "auth":auth, "defaultModel":enabled[0]["id"],
        "deniedModels":models.iter().filter(|m| m["enabled"] == false).map(|m| m["id"].clone()).collect::<Vec<_>>(),
        "configuredModels":enabled.iter().map(|m| json!({"id":m["id"],"name":m["name"]})).collect::<Vec<_>>(),
        "modelOverrides":overrides
    });
    if ["omniroute", "9router"].contains(&profile) {
        connection["gateway"] = json!(profile);
    }
    for (source, target) in [
        ("edit", "edit"),
        ("orchestration_model", "orchestrationModel"),
    ] {
        if let Some(value) = raw.get(source) {
            connection[target] = value.clone();
        }
    }
    Ok(
        json!({"schemaVersion":1,"defaultConnection":"default", "outputDir":root.join("outputs"),
        "stateDir":root.join("state"), "logging":{"directory":root.join("logs")},
        "files":{"allowedInputRoots":[root.join("inputs")]}, "connections":{"default":connection}}),
    )
}
