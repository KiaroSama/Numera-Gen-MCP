//! Optional fal provider section of a merged `.env` file.
//!
//! `FAL_API_ENDPOINT`, `FAL_API_KEY` and `FAL_MODEL_<N>_<FIELD>` describe a second connection named
//! `fal` beside the main `API_*`/`MODEL_*` section. The section is carried inside the simple
//! configuration as a nested `fal` object and expanded with the same validator and connection
//! builder as the main section, always with the fixed fal profile.
use super::env::{env_error, models};
use super::invalid;
use crate::compat::value::{Object, Value, json};
use crate::error::Result;
use std::collections::HashMap;

pub(super) const CONNECTION: &str = "fal";
const PREFIX: &str = "FAL_";

/// Removes every `FAL_*` key from `values` and returns them; the main grammar never sees them.
pub(super) fn split(values: &mut HashMap<String, String>) -> HashMap<String, String> {
    let keys: Vec<String> = values
        .keys()
        .filter(|key| key.starts_with(PREFIX))
        .cloned()
        .collect();
    keys.into_iter()
        .filter_map(|key| values.remove_entry(&key))
        .collect()
}

fn configured(values: &HashMap<String, String>, keys: [&str; 2]) -> bool {
    keys.iter()
        .any(|key| values.get(*key).is_some_and(|value| !value.is_empty()))
}

/// Validates the fal keys and attaches them to the parsed simple configuration.
pub(super) fn attach(fal: HashMap<String, String>, raw: &mut Object) -> Result<()> {
    let main_values: HashMap<String, String> =
        [("API_ENDPOINT", "api_endpoint"), ("API_KEY", "api_key")]
            .into_iter()
            .filter_map(|(key, field)| {
                raw.get(field)
                    .and_then(Value::as_str)
                    .map(|value| (key.to_owned(), value.to_owned()))
            })
            .collect();
    let main = configured(&main_values, ["API_ENDPOINT", "API_KEY"]);
    let has_fal_keys = !fal.is_empty();
    let fal_section = configured(&fal, ["FAL_API_ENDPOINT", "FAL_API_KEY"]);
    if !has_fal_keys {
        // Pre-0.5.0 files: unchanged behaviour and unchanged error messages.
        return Ok(());
    }
    if raw.get("profile").and_then(Value::as_str) == Some("fal") {
        return Err(invalid(
            "PROFILE=\"fal\" cannot be combined with FAL_* keys; use the FAL_* section only.",
        ));
    }
    let models = models(
        fal.iter()
            .filter(|(key, _)| !matches!(key.as_str(), "FAL_API_ENDPOINT" | "FAL_API_KEY")),
        "FAL_MODEL_",
    )
    .map_err(|_| env_error())?;
    if !fal_section {
        // Empty FAL_API_ENDPOINT/FAL_API_KEY disable the section even when model slots are
        // filled: the shipped template keeps its fal model list ready for the owner's key.
        if !main {
            return Err(sections_missing());
        }
        return Ok(());
    }
    if ["FAL_API_ENDPOINT", "FAL_API_KEY"]
        .iter()
        .any(|key| fal.get(*key).is_none_or(String::is_empty))
    {
        return Err(invalid(
            "The fal section needs both FAL_API_ENDPOINT and FAL_API_KEY.",
        ));
    }
    let mut section = Object::new();
    for (key, field) in [
        ("FAL_API_ENDPOINT", "api_endpoint"),
        ("FAL_API_KEY", "api_key"),
    ] {
        if let Some(value) = fal.get(key) {
            section.insert(field.into(), json!(value));
        }
    }
    section.insert("models".into(), models);
    section.insert("profile".into(), json!("fal"));
    if !main {
        // Main keys absent or empty: EDIT_* and ORCHESTRATION_MODEL have no connection to apply to.
        if raw.get("edit").is_some() || raw.get("orchestration_model").is_some() {
            return Err(invalid(
                "EDIT_* and ORCHESTRATION_MODEL apply only to the API_ENDPOINT/API_KEY section.",
            ));
        }
        let main_models = raw.get("models").and_then(Value::as_array);
        if main_models.is_some_and(|m| !m.is_empty()) {
            return Err(invalid(
                "MODEL_* entries need API_ENDPOINT and API_KEY; fal models use FAL_MODEL_*.",
            ));
        }
        raw.remove("api_endpoint");
        raw.remove("api_key");
        raw.remove("models");
        raw.remove("profile");
    }
    raw.insert(CONNECTION.into(), Value::Object(section));
    Ok(())
}

fn sections_missing() -> crate::error::NumeraError {
    invalid("Configure API_ENDPOINT/API_KEY, FAL_API_ENDPOINT/FAL_API_KEY, or both.")
}

/// Removes the nested fal section from a simple configuration before schema validation.
pub(super) fn take(raw: &mut Value) -> Option<Value> {
    raw.as_object_mut()?.remove(CONNECTION)
}

/// True when a simple configuration still carries a main section after `take`.
pub(super) fn has_main(raw: &Value) -> bool {
    raw.get("api_endpoint").is_some() || raw.get("api_key").is_some()
}

#[cfg(test)]
mod tests {
    use super::super::parse_env_config;

    const MAIN: &str = "API_ENDPOINT=https://main.example/v1\nAPI_KEY=main-secret\nMODEL_1_ID=m/one\nMODEL_1_NAME=One\nMODEL_1_ENABLED=true\n";
    const FAL: &str = "FAL_API_ENDPOINT=https://queue.fal.run\nFAL_API_KEY=fal-secret\nFAL_MODEL_4_ID=fal-ai/flux/dev\nFAL_MODEL_4_NAME=FLUX\nFAL_MODEL_4_ENABLED=true\nFAL_MODEL_4_OUTPUT_TYPES=image\nFAL_MODEL_4_MODES=text-to-image\n";

    fn error(text: &str) -> String {
        let error = parse_env_config(text).expect_err("must reject");
        let rendered = format!("{error:?}");
        assert!(!rendered.contains("main-secret") && !rendered.contains("fal-secret"));
        rendered
    }

    #[test]
    fn main_only_file_is_unchanged() {
        let raw = parse_env_config(MAIN).unwrap();
        assert!(raw.get("fal").is_none());
        assert_eq!(raw["api_key"], "main-secret");
    }

    #[test]
    fn both_sections_keep_separate_keys_and_models() {
        let raw = parse_env_config(&format!("{MAIN}{FAL}")).unwrap();
        assert_eq!(raw["api_endpoint"], "https://main.example/v1");
        assert_eq!(raw["models"][0]["id"], "m/one");
        assert_eq!(raw["fal"]["api_endpoint"], "https://queue.fal.run");
        assert_eq!(raw["fal"]["api_key"], "fal-secret");
        assert_eq!(raw["fal"]["profile"], "fal");
        assert_eq!(raw["fal"]["models"][0]["id"], "fal-ai/flux/dev");
        assert_eq!(raw["fal"]["models"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn fal_only_file_drops_empty_main_placeholders() {
        let text = format!("API_ENDPOINT=\nAPI_KEY=\nMODEL_1_ID=\nMODEL_1_NAME=\n{FAL}");
        let raw = parse_env_config(&text).unwrap();
        assert!(raw.get("api_endpoint").is_none() && raw.get("models").is_none());
        assert_eq!(raw["fal"]["models"][0]["id"], "fal-ai/flux/dev");
    }

    #[test]
    fn empty_fal_template_is_ignored_beside_main() {
        let text = format!("{MAIN}FAL_API_ENDPOINT=\nFAL_API_KEY=\nFAL_MODEL_1_ID=\n");
        assert!(parse_env_config(&text).unwrap().get("fal").is_none());
        // Filled slots without endpoint/key (the shipped template) stay disabled, not rejected.
        let template = format!("{MAIN}FAL_API_ENDPOINT=\nFAL_API_KEY=\nFAL_MODEL_2_ID=x\n");
        assert!(parse_env_config(&template).unwrap().get("fal").is_none());
    }

    #[test]
    fn invalid_fal_layouts_reject_without_disclosing_keys() {
        assert!(error(&format!("{MAIN}{FAL}PROFILE=fal\n")).contains("PROFILE"));
        error(&format!("{MAIN}FAL_PROFILE=fal\n"));
        error(&format!("{MAIN}FAL_EDIT_MODE=json\n"));
        error(&format!("{MAIN}{FAL}FAL_MODEL_0_ID=x\n"));
        error(&format!("{MAIN}{FAL}FAL_MODEL_01_ID=x\n"));
        error(&format!("{MAIN}{FAL}FAL_MODEL_101_ID=x\n"));
        error(&format!("{MAIN}FAL_API_ENDPOINT=https://queue.fal.run\n"));
        assert!(
            error("FAL_API_ENDPOINT=\nFAL_API_KEY=\n").contains("FAL_API_ENDPOINT/FAL_API_KEY")
        );
        let only_fal_with_edit = format!("{FAL}EDIT_MODE=json\nEDIT_ENCODING=images\n");
        assert!(error(&only_fal_with_edit).contains("EDIT_*"));
    }
}
