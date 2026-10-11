use super::types::{Normalized, ResultItem, Job};
use crate::{error::{NumeraError, Result}, http::sse};
use serde_json::{Value, json};

fn error(code: &str, message: &str) -> NumeraError { NumeraError::new(code, message).stage("response") }
fn malformed(images: &mut Vec<ResultItem>, value: &Value) -> bool {
    if value.is_object() { true } else { images.push(ResultItem { error: Some("Malformed image result item.".into()), ..Default::default() }); false }
}
fn array(value: &Value) -> &[Value] { value.as_array().map_or(&[], Vec::as_slice) }
fn string(value: &Value) -> Option<String> { value.as_str().map(str::to_owned) }
fn truthy(value: &Value) -> bool {
    match value { Value::Null => false, Value::Bool(v) => *v, Value::String(v) => !v.is_empty(), Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0), _ => true }
}
fn js_string(value: &Value) -> String {
    match value { Value::String(s) => s.clone(), Value::Null => "null".into(), Value::Object(_) => "[object Object]".into(),
        Value::Array(a) => a.iter().map(|v| if v.is_null(){String::new()}else{js_string(v)}).collect::<Vec<_>>().join(","), _ => value.to_string() }
}
fn push(images: &mut Vec<ResultItem>, value: &Value) {
    if !malformed(images, value) { return; }
    if let Some(encoded) = value["b64_json"].as_str().filter(|s| !s.is_empty()) {
        images.push(ResultItem { base64: Some(encoded.into()), ..Default::default() });
    } else if let Some(url) = value["url"].as_str().filter(|s| !s.is_empty()) {
        images.push(ResultItem { url: Some(url.into()), ..Default::default() });
    } else { images.push(ResultItem { error: Some("Image item contains no final bytes or URL.".into()), ..Default::default() }); }
}

pub fn normalize(connection: &Value, value: &Value) -> Result<Normalized> {
    if !value.is_object() { return Err(error("invalid_response", "Expected an object.")); }
    let mut result = Normalized::default();
    let mut signatures = Vec::new();
    let adapter = connection["adapter"].as_str().unwrap_or("");
    if adapter == "gemini" {
        if truthy(&value["promptFeedback"]) {
            if !value["promptFeedback"].is_object() { return Err(error("invalid_response", "Expected an object.")); }
            if truthy(&value["promptFeedback"]["blockReason"]) { return Err(error("provider_rejection", "Provider rejected image generation.")); }
        }
        for candidate in array(&value["candidates"]) {
            if !malformed(&mut result.images, candidate) { continue; }
            if truthy(&candidate["finishReason"]) && candidate["finishReason"] != "STOP" { result.warnings.push(format!("Candidate ended with {}.", js_string(&candidate["finishReason"]))); }
            let content = &candidate["content"];
            if truthy(content) && !malformed(&mut result.images, content) { continue; }
            for (i, part) in array(&content["parts"]).iter().enumerate() {
                if !malformed(&mut result.images, part) { continue; }
                if let Some(signature) = part["thoughtSignature"].as_str() { signatures.push(json!({"part_index":i,"signature":signature})); }
                if part["thought"] == true { continue; }
                if truthy(&part["inlineData"]) && malformed(&mut result.images, &part["inlineData"]) { push(&mut result.images, &json!({"b64_json":part["inlineData"]["data"]})); }
            }
        }
    } else if ["gemini-interactions", "openai-responses"].contains(&adapter) {
        let status = value["status"].as_str().unwrap_or("");
        if ["in_progress", "queued"].contains(&status) {
            let id = value["id"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| error("outcome_unknown", "Async response has no recoverable job ID."))?;
            result.job = Some(Job { id: id.into(), kind: if adapter=="gemini-interactions"{"interactions"}else{"responses"}.into() });
            if adapter == "gemini-interactions" { result.upstream_model = string(&value["model"]); }
            return Ok(result);
        }
        if adapter == "gemini-interactions" && status == "budget_exceeded" { result.terminal = Some("failed".into()); }
        else if ["failed", "cancelled", "incomplete"].contains(&status) { result.terminal = Some(status.into()); }
        else if truthy(&value["status"]) && status != "completed" {
            return Err(error("provider_rejection", &format!("{} ended with {}.", if adapter=="gemini-interactions"{"Interaction"}else{"Response"}, js_string(&value["status"]))));
        }
        if adapter == "gemini-interactions" {
            for step in array(&value["steps"]) {
                if !malformed(&mut result.images, step) { continue; }
                if step["type"] == "thought" {
                    if let Some(signature) = step["signature"].as_str() { signatures.push(json!({"step_type":"thought","signature":signature})); }
                }
                if step["type"] != "model_output" { continue; }
                for part in array(&step["content"]) {
                    if malformed(&mut result.images, part) && part["type"] == "image" { push(&mut result.images, &json!({"b64_json":part["data"],"url":part["uri"]})); }
                }
            }
            if !result.images.iter().any(|i| i.base64.is_some() || i.url.is_some()) && truthy(&value["output_image"]) && malformed(&mut result.images, &value["output_image"]) {
                push(&mut result.images, &json!({"b64_json":value["output_image"]["data"],"url":value["output_image"]["uri"]}));
            }
        } else {
            for item in array(&value["output"]) {
                if malformed(&mut result.images, item) && item["type"] == "image_generation_call" && item["status"] == "completed" { push(&mut result.images, &json!({"b64_json":item["result"]})); }
            }
        }
    } else if adapter == "chat-images" {
        for choice in array(&value["choices"]) {
            if !malformed(&mut result.images, choice) { continue; }
            let message = &choice["message"];
            if !malformed(&mut result.images, message) { continue; }
            if truthy(&message["refusal"]) { return Err(error("provider_rejection", "Provider refused image output.")); }
            for image in array(&message["images"]) {
                if malformed(&mut result.images, image) && malformed(&mut result.images, &image["image_url"]) { push(&mut result.images, &json!({"url":image["image_url"]["url"]})); }
            }
        }
    } else { for item in array(&value["data"]) { push(&mut result.images, item); } }
    if result.terminal.is_none() && !result.images.iter().any(|i| i.base64.is_some() || i.url.is_some() || i.bytes.is_some()) {
        return Err(error("no_image_returned", "No final image was returned; text, empty bodies and partial previews are not final images."));
    }
    result.upstream_model = string(&value["model"]);
    result.upstream_id = string(&value["id"]);
    result.usage = value.get("usage").filter(|v|!v.is_null()).or_else(||value.get("usageMetadata")).cloned();
    if !signatures.is_empty() {
        let mut continuation=json!({"signatures":signatures});
        if let Some(id)=value.get("id"){continuation["upstream_id"]=id.clone();}
        result.continuation=Some(continuation);
    }
    Ok(result)
}

pub fn response(connection: &Value, bytes: Vec<u8>, mime: &str) -> Result<Normalized> {
    let image_mime = mime.split(';').next().unwrap_or("");
    if ["image/png", "image/jpeg", "image/webp"].contains(&image_mime) {
        return Ok(Normalized { images: vec![ResultItem { bytes: Some(bytes), ..Default::default() }], ..Default::default() });
    }
    if !mime.contains("text/event-stream") {
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| error("invalid_response", "Image response is not valid JSON."))?;
        return normalize(connection, &value);
    }
    let events = sse::events(&bytes, 16 * 1024 * 1024)?;
    if connection["adapter"] == "openai-responses" {
        if let Some(event) = events.iter().find(|e| ["response.failed", "response.incomplete"].contains(&e.kind.as_str())) {
            let result = &event.data["response"];
            if truthy(result) {
                if !result.is_object() { return Err(error("invalid_response", "Expected an object.")); }
                if result["id"].as_str().is_some_and(|s| !s.trim().is_empty()) && ["failed", "incomplete"].contains(&result["status"].as_str().unwrap_or("")) { return normalize(connection, result); }
            }
        }
    }
    if events.iter().any(|e| ["error", "response.failed", "response.incomplete"].contains(&e.kind.as_str())) {
        return Err(error("outcome_unknown", "Image stream ended with an error; no resubmission made."));
    }
    if connection["gateway"] == "9router" {
        let done = events.iter().find(|e| e.kind == "done" && truthy(&e.data["data"]))
            .ok_or_else(||error("outcome_unknown", "9router stream has no terminal done result."))?;
        return normalize(connection, &done.data);
    }
    match connection["adapter"].as_str().unwrap_or("") {
        "openrouter-images" => {
            let finals: Vec<_> = events.iter().filter(|e| e.kind == "image_generation.completed").collect();
            if !events.iter().any(|e| e.kind == "done") || finals.is_empty() { return Err(error("outcome_unknown", "Image stream has no final completion.")); }
            normalize(connection, &json!({"data":finals.iter().map(|e|&e.data).collect::<Vec<_>>(),"usage":finals.last().map(|e|&e.data["usage"])}))
        }
        "openai-responses" => {
            let done = events.iter().find(|e| e.kind == "response.completed" && truthy(&e.data["response"]))
                .ok_or_else(||error("outcome_unknown", "Responses stream has no completed response."))?;
            normalize(connection, &done.data["response"])
        }
        "gemini" => normalize(connection, &json!({"candidates":events.iter().filter(|e|truthy(&e.data["candidates"])).flat_map(|e|array(&e.data["candidates"]).iter()).collect::<Vec<_>>()})),
        _ => Err(error("unsupported_operation", "Streaming contract is not implemented for this adapter.")),
    }
}
