use crate::{files::images::Image, http::api::Body};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

pub struct Input<'a> {
    pub connection: &'a Value,
    pub request: &'a Value,
    pub model: &'a str,
    pub operation: &'a str,
    pub references: &'a [Image],
    pub mask: Option<&'a Image>,
}

pub struct Prepared {
    pub operation: String,
    pub prefix: &'static str,
    pub body: Body,
}
impl Prepared {
    pub fn json(operation: &str, prefix: &'static str, value: Value) -> Self {
        Self { operation: operation.into(), prefix, body: Body::Json(value) }
    }
}

#[derive(Default)]
pub struct ResultItem {
    pub base64: Option<String>,
    pub url: Option<String>,
    pub bytes: Option<Vec<u8>>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Job { pub id: String, pub kind: String }

#[derive(Default)]
pub struct Normalized {
    pub images: Vec<ResultItem>,
    pub terminal: Option<String>,
    pub upstream_model: Option<String>,
    pub upstream_id: Option<String>,
    pub usage: Option<Value>,
    pub job: Option<Job>,
    pub continuation: Option<Value>,
    pub warnings: Vec<String>,
}

pub fn mime_data(image: &Image) -> String {
    format!("data:{};base64,{}", image.mime, STANDARD.encode(&image.bytes))
}
