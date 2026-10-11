use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct NumeraError {
    pub code: String,
    pub message: String,
    pub stage: String,
    pub http_status: Option<u16>,
    pub next_action: String,
}

pub type Result<T> = std::result::Result<T, NumeraError>;

impl NumeraError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
            stage: "validation".into(),
            http_status: None,
            next_action: "Correct the request or configuration before submitting again.".into(),
        }
    }

    pub fn stage(mut self, stage: &str) -> Self {
        self.stage = stage.to_owned();
        self
    }

    pub fn runtime() -> Self {
        Self::new("upstream_error", "The operation failed. Inspect the redacted local log.")
            .stage("runtime")
    }

    pub fn storage() -> Self {
        Self::new("storage_error", "The storage operation failed. Preserve existing data and inspect the redacted local log.")
            .stage("storage")
    }
}

pub fn object(value: &serde_json::Value) -> Result<&serde_json::Map<String, serde_json::Value>> {
    value.as_object().ok_or_else(|| {
        NumeraError::new("invalid_response", "Expected an object.").stage("response")
    })
}

pub fn array(value: &serde_json::Value) -> &[serde_json::Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}
