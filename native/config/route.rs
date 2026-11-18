//! Connection selection by requested model when a tool call omits `connection`.
//!
//! An explicit connection always wins. With one enabled connection behaviour is unchanged. With
//! several, a model id configured on exactly one connection selects it; ambiguous or unknown ids
//! fail before any persistence, credential snapshot or network I/O so no paid request can reach the
//! wrong provider.
use super::Config;
use crate::compat::value::Value;
use crate::error::{NumeraError, Result};

impl Config {
    pub fn select_for_model(
        &self,
        name: Option<&str>,
        model: Option<&Value>,
    ) -> Result<(String, Value)> {
        let enabled: Vec<(String, &Value)> = self.value["connections"]
            .as_object()
            .map(|all| {
                all.iter()
                    .filter(|(_, c)| c["enabled"] == true)
                    .filter_map(|(id, c)| id.as_str().map(|id| (id.to_owned(), c)))
                    .collect()
            })
            .unwrap_or_default();
        let model = model.filter(|m| m.is_string());
        if name.is_some() || enabled.len() < 2 || model.is_none() {
            return self.select(name);
        }
        let model = model.unwrap_or(&Value::Null);
        let owners: Vec<&str> = enabled
            .iter()
            .filter(|(_, c)| {
                c["configuredModels"]
                    .as_array()
                    .is_some_and(|models| models.iter().any(|m| m["id"] == *model))
            })
            .map(|(id, _)| id.as_str())
            .collect();
        match owners.as_slice() {
            [only] => self.select(Some(only)),
            [] => Err(NumeraError::new(
                "invalid_input",
                "Model is not configured on any connection; pass connection or check the id.",
            )),
            _ => Err(NumeraError::new(
                "invalid_input",
                "Model id is configured on more than one connection; pass connection.",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Config;
    use crate::compat::value::{Value, json};
    use std::path::PathBuf;

    fn config(connections: Value, default: &str) -> Config {
        Config {
            value: json!({"defaultConnection":default, "connections":connections}),
            file: PathBuf::new(),
            root: PathBuf::new(),
        }
    }

    fn connection(models: &[&str]) -> Value {
        json!({"enabled":true, "configuredModels":models.iter().map(|m| json!({"id":m,"name":m})).collect::<Vec<_>>()})
    }

    fn two() -> Config {
        config(
            json!({"default":connection(&["main/a", "shared"]), "fal":connection(&["fal-ai/x", "shared"])}),
            "default",
        )
    }

    fn pick(config: &Config, name: Option<&str>, model: Option<&str>) -> Result<String, String> {
        let model = model.map(|m| json!(m));
        config
            .select_for_model(name, model.as_ref())
            .map(|(id, _)| id)
            .map_err(|e| e.message)
    }

    #[test]
    fn decision_table_rows() {
        let c = two();
        assert_eq!(pick(&c, Some("fal"), Some("main/a")).unwrap(), "fal");
        assert_eq!(pick(&c, None, None).unwrap(), "default");
        assert_eq!(pick(&c, None, Some("fal-ai/x")).unwrap(), "fal");
        assert_eq!(pick(&c, None, Some("main/a")).unwrap(), "default");
        assert!(
            pick(&c, None, Some("shared"))
                .unwrap_err()
                .contains("more than one")
        );
        assert!(
            pick(&c, None, Some("unknown"))
                .unwrap_err()
                .contains("not configured")
        );
    }

    #[test]
    fn single_connection_keeps_previous_behaviour() {
        let c = config(json!({"default":connection(&["main/a"])}), "default");
        assert_eq!(pick(&c, None, Some("unlisted/model")).unwrap(), "default");
        let fal_only = config(json!({"fal":connection(&["fal-ai/x"])}), "fal");
        assert_eq!(pick(&fal_only, None, Some("fal-ai/x")).unwrap(), "fal");
    }

    #[test]
    fn disabled_connection_never_routes() {
        let mut fal = connection(&["fal-ai/x"]);
        fal["enabled"] = json!(false);
        let c = config(
            json!({"default":connection(&["main/a"]), "fal":fal}),
            "default",
        );
        assert_eq!(pick(&c, None, Some("fal-ai/x")).unwrap(), "default");
    }
}
