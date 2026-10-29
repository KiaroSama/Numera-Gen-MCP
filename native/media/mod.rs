pub mod fal;
mod fal_contract;
mod fal_input;
mod glb;
mod inspect;
mod mp4;
mod wav;

use crate::error::NumeraError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Kind {
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "audio")]
    Audio,
    #[serde(rename = "3d")]
    Model3d,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Model3d => "3d",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Inspected {
    pub kind: Kind,
    pub mime: String,
    pub format: String,
    pub metadata: Value,
}

pub use inspect::inspect;

fn invalid(message: &str) -> NumeraError {
    NumeraError::new("invalid_media", message)
}
