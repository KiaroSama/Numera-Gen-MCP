use super::{MAX_CONFIG_BYTES, invalid};
use crate::error::Result;
use serde::Deserialize;
use serde_json::Value;
use std::ops::{Deref, DerefMut};

/// Parsing is byte bounded before allowing deep source-compatible structures. serde_stacker
/// grows only the parse stack; retained values are cloned and dropped iteratively below.
pub(super) fn parse(text: &str) -> Result<OwnedJson> {
    if text.len() > MAX_CONFIG_BYTES {
        return Err(invalid("Configuration exceeds 1 MiB."));
    }
    let mut decoder = serde_json::Deserializer::from_str(text);
    decoder.disable_recursion_limit();
    let value = Value::deserialize(serde_stacker::Deserializer::new(&mut decoder))
        .map_err(|_| invalid("Cannot parse JSON configuration."))?;
    let value = OwnedJson(value);
    decoder
        .end()
        .map_err(|_| invalid("Cannot parse JSON configuration."))?;
    Ok(value)
}

pub(super) struct OwnedJson(pub Value);
impl OwnedJson {
    pub fn take(&mut self) -> Value {
        std::mem::take(&mut self.0)
    }
}
impl Deref for OwnedJson {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.0
    }
}
impl DerefMut for OwnedJson {
    fn deref_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}
impl Drop for OwnedJson {
    fn drop(&mut self) {
        drop_json(self.take());
    }
}

pub fn drop_json(value: Value) {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(array) => pending.extend(array),
            Value::Object(object) => pending.extend(object.into_iter().map(|(_, value)| value)),
            _ => {}
        }
    }
}

pub fn clone_json(value: &Value) -> Value {
    enum Task<'a> {
        Value(&'a Value),
        Array(usize),
        Object(Vec<String>),
    }
    let mut tasks = vec![Task::Value(value)];
    let mut complete = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Value(Value::Array(array)) => {
                tasks.push(Task::Array(array.len()));
                tasks.extend(array.iter().rev().map(Task::Value));
            }
            Task::Value(Value::Object(object)) => {
                tasks.push(Task::Object(object.keys().cloned().collect()));
                tasks.extend(object.values().rev().map(Task::Value));
            }
            Task::Value(value) => complete.push(value.clone()),
            Task::Array(length) => {
                let array = complete.split_off(complete.len() - length);
                complete.push(Value::Array(array));
            }
            Task::Object(keys) => {
                let values = complete.split_off(complete.len() - keys.len());
                complete.push(Value::Object(keys.into_iter().zip(values).collect()));
            }
        }
    }
    complete.pop().unwrap_or(Value::Null)
}
