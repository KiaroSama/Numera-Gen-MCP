use crate::compat::value::{Object, Value, json};
use crate::runtime_paths::private_directory;
use regex::Regex;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

struct Patterns {
    sensitive: Regex,
    urls: Regex,
    bearer: Regex,
    key_scheme: Regex,
    basic_scheme: Regex,
    api_headers: Regex,
    data_urls: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        sensitive: Regex::new("(?i)authorization|proxy-authorization|cookie|set-cookie|token|key|secret|password|signature|prompt|base64|b64_json|inlineData|data_url|path|credential")
            .expect("static sensitive field pattern"),
        urls: Regex::new(r#"https?://[^\s\"<>]+"#).expect("static URL pattern"),
        bearer: Regex::new(r"(?i)\bBearer\s+\S+").expect("static bearer pattern"),
        key_scheme: Regex::new(r"(?i)\bKey\s+\S+").expect("static key auth pattern"),
        basic_scheme: Regex::new(r"(?i)\bBasic\s+[A-Za-z0-9+/=_-]+").expect("static basic auth pattern"),
        api_headers: Regex::new(r#"(?i)\b(x-api-key|x-goog-api-key|proxy-authorization|authorization)\s*[:=]\s*[^\s\"';,]+"#).expect("static header secret pattern"),
        data_urls: Regex::new(r"(?i)data:[a-z0-9.+-]+/[a-z0-9.+-]+;base64,[A-Za-z0-9+/=_-]+").expect("static data URL pattern"),
    })
}

pub fn redact(value: &Value, depth: usize) -> Value {
    if depth > 8 {
        return Value::String("[REDACTED]".into());
    }
    let p = patterns();
    match value {
        Value::String(text) => {
            let Some(text) = text.as_str() else {
                return json!("[REDACTED]");
            };
            let text = p.urls.replace_all(text, |capture: &regex::Captures<'_>| {
                url::Url::parse(&capture[0]).map_or_else(
                    |_| "[REDACTED]".into(),
                    |mut url| {
                        url.set_query(None);
                        let _ = url.set_username("");
                        let _ = url.set_password(None);
                        url.to_string()
                    },
                )
            });
            let text = p.data_urls.replace_all(&text, "data:[REDACTED]");
            let text = p.bearer.replace_all(&text, "Bearer [REDACTED]");
            let text = p.key_scheme.replace_all(&text, "Key [REDACTED]");
            let text = p.basic_scheme.replace_all(&text, "Basic [REDACTED]");
            let text = p.api_headers.replace_all(&text, "$1: [REDACTED]");
            Value::String(text.into_owned().into())
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .take(30)
                .map(|v| redact(v, depth + 1))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        if k.as_str().is_none_or(|key| p.sensitive.is_match(key)) {
                            json!("[REDACTED]")
                        } else {
                            redact(v, depth + 1)
                        },
                    )
                })
                .collect(),
        ),
        value => value.clone(),
    }
}

pub struct Logger {
    file: Mutex<Option<File>>,
    level: usize,
    run_id: String,
}

impl Logger {
    pub fn new(directory: &Path, level: &str) -> Self {
        Self::with_retention(directory, level, 14)
    }

    pub fn with_retention(directory: &Path, level: &str, retention_days: u64) -> Self {
        let run_id = Uuid::new_v4().to_string();
        let file = (|| {
            private_directory(directory).ok()?;
            let name = format!(
                "numera-gen-mcp_{}_UTC-{}-{}.log",
                crate::time_utc::log_file_stamp(),
                std::process::id(),
                run_id
            );
            crate::devtools::private_files::create_private_file(&directory.join(name)).ok()
        })();
        if file.is_none() {
            eprintln!("WARNING: File logging unavailable; using stderr.");
        } else if let Ok(entries) = std::fs::read_dir(directory) {
            let age = std::time::Duration::from_secs(retention_days.saturating_mul(86400));
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if ["numera-gen-mcp_", "numera-image-gen-mcp_"]
                    .iter()
                    .any(|prefix| name.starts_with(*prefix))
                    && name.ends_with(".log")
                    && let Ok(info) = std::fs::symlink_metadata(entry.path())
                    && info.is_file()
                    && info
                        .modified()
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|elapsed| elapsed > age)
                    && std::fs::remove_file(entry.path()).is_err()
                {
                    eprintln!("WARNING: Log retention cleanup failed.");
                }
            }
        }
        Self {
            file: Mutex::new(file),
            level: level_index(level),
            run_id,
        }
    }

    pub fn log(&self, level: &str, component: &str, message: &str, context: Value) {
        let Some(line) = self.line(level, component, message, &context) else {
            return;
        };
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
            && file.write_all(&line).and_then(|_| file.flush()).is_err()
        {
            *guard = None;
            eprintln!("WARNING: File logging failed.");
        }
        let _ = std::io::stderr().write_all(&line);
    }

    /// Writes only to the private file log. Phase records must not reach stderr, where a
    /// bounded supervisor counts them as child output and would reset its idle deadline.
    pub fn file_only(&self, level: &str, component: &str, message: &str, context: Value) {
        let Some(line) = self.line(level, component, message, &context) else {
            return;
        };
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
            && file.write_all(&line).and_then(|_| file.flush()).is_err()
        {
            *guard = None;
        }
    }

    fn line(
        &self,
        level: &str,
        component: &str,
        message: &str,
        context: &Value,
    ) -> Option<Vec<u8>> {
        if level_index(level) < self.level {
            return None;
        }
        let mut data = redact(context, 0)
            .as_object()
            .cloned()
            .unwrap_or_else(Object::new);
        data.insert("run_id".into(), json!(self.run_id));
        data.insert("timestamp".into(), json!(crate::time_utc::rfc3339_millis()));
        data.insert("level".into(), json!(level));
        data.insert("component".into(), json!(component));
        data.insert("message".into(), redact(&json!(message), 0));
        let mut line = serde_json::to_vec(&Value::Object(data)).ok()?;
        line.push(b'\n');
        Some(line)
    }
}

fn level_index(level: &str) -> usize {
    match level {
        "DEBUG" => 0,
        "INFO" => 1,
        "WARNING" => 2,
        _ => 3,
    }
}
