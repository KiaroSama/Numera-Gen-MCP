use super::{EnvironmentSource, invalid, js_trim, validate};
use crate::compat::value::Value;
use crate::{
    error::{NumeraError, Result},
    runtime_paths::RuntimePaths,
};
use std::{collections::HashMap, fs::File, io::Read, path::Path};

/// Operation-only object: no Serialize or Debug implementation can expose pinned headers.
pub struct PinnedConnection {
    value: Value,
    headers: HashMap<String, String>,
    proxy_auth: Option<String>,
}

fn zeroize_string(mut value: String) {
    // SAFETY: zeroing raw UTF-8 bytes before immediate drop leaves no readable bytes.
    unsafe {
        for byte in value.as_bytes_mut() {
            std::ptr::write_volatile(byte, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    drop(value);
}

impl Clone for PinnedConnection {
    fn clone(&self) -> Self {
        Self {
            value: super::clone_json(&self.value),
            headers: self.headers.clone(),
            proxy_auth: self.proxy_auth.clone(),
        }
    }
}
impl Drop for PinnedConnection {
    fn drop(&mut self) {
        super::drop_json(std::mem::take(&mut self.value));
        for (key, value) in self.headers.drain() {
            zeroize_string(key);
            zeroize_string(value);
        }
        if let Some(auth) = self.proxy_auth.take() {
            zeroize_string(auth);
        }
    }
}

impl PinnedConnection {
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn headers(&self) -> HashMap<String, String> {
        self.headers.clone()
    }
    pub fn proxy_auth(&self) -> Option<&str> {
        self.proxy_auth.as_deref()
    }
}

pub async fn snapshot_credentials(
    connection: &Value,
    env: &impl EnvironmentSource,
    runtime: &RuntimePaths,
) -> Result<PinnedConnection> {
    let headers = credentials(connection, env, runtime).await?;
    let proxy_auth = resolve_proxy_auth(connection, env)?;
    Ok(PinnedConnection {
        value: super::clone_json(connection),
        headers,
        proxy_auth,
    })
}

fn resolve_proxy_auth(connection: &Value, env: &impl EnvironmentSource) -> Result<Option<String>> {
    let Some(proxy) = connection.get("proxy") else {
        return Ok(None);
    };
    let Some(secret_env) = proxy.get("secretEnv") else {
        return Ok(None);
    };
    let Some(name) = secret_env.as_str().filter(|s| !s.is_empty()) else {
        return Err(invalid("Invalid proxy secretEnv configuration."));
    };
    let value = env
        .value(name)?
        .filter(|v| !v.is_empty() && !v.contains(['\r', '\n', '\0']))
        .ok_or_else(|| {
            missing("The configured proxy credential environment variable is missing or empty.")
        })?;
    reqwest::header::HeaderValue::from_str(&value)
        .map_err(|_| invalid("Invalid proxy credential."))?;
    Ok(Some(value))
}

pub async fn credentials(
    connection: &Value,
    env: &impl EnvironmentSource,
    _runtime: &RuntimePaths,
) -> Result<HashMap<String, String>> {
    let auth = &connection["auth"];
    if auth["type"] == "none" {
        return Ok(HashMap::new());
    }
    let key = if let Some(key) = auth["apiKey"].as_js_string() {
        Some(
            key.checked_str()
                .map_err(|_| {
                    missing("The selected connection secret cannot be used in an HTTP header.")
                })?
                .to_owned(),
        )
    } else if let Some(name) = auth["secretEnv"].as_str().filter(|s| !s.is_empty()) {
        env.value(name)?
    } else if let Some(path) = auth["secretFile"].as_str().filter(|s| !s.is_empty()) {
        let path = Path::new(path);
        if !validate::absolute(path) {
            return Err(invalid("Secret file path must be absolute."));
        }
        Some(
            read_private(path)
                .map_err(|_| missing("Cannot read the declared private secret file."))?,
        )
    } else if let Some(path) = auth["secretDpapiFile"].as_str().filter(|s| !s.is_empty()) {
        #[cfg(not(windows))]
        {
            let _ = path;
            return Err(invalid("DPAPI requires an absolute Windows secret file."));
        }
        #[cfg(windows)]
        {
            let path = Path::new(path);
            if !validate::absolute(path) {
                return Err(invalid("DPAPI requires an absolute Windows secret file."));
            }
            Some(super::dpapi::decrypt(path).map_err(|_| {
                missing("Cannot decrypt the declared DPAPI secret for this Windows user.")
            })?)
        }
    } else {
        None
    };
    let key = key.filter(|key| !key.is_empty() && !key.contains(['\r', '\n', '\0']))
        .ok_or_else(|| missing("The selected connection secret is missing or invalid. Set its declared secret source."))?;
    let (header, value) = if auth["type"] == "bearer" {
        ("Authorization", format!("Bearer {key}"))
    } else if auth["type"] == "key" {
        ("Authorization", format!("Key {key}"))
    } else {
        (
            auth["header"]
                .as_str()
                .ok_or_else(|| invalid("Invalid authentication header."))?,
            key,
        )
    };
    Ok(HashMap::from([(header.to_owned(), value)]))
}

fn missing(message: &str) -> NumeraError {
    NumeraError::new("missing_credentials", message)
}

fn read_private(path: &Path) -> std::io::Result<String> {
    let file = File::open(path)?;
    let info = file.metadata()?;
    if !info.is_file() || info.len() > 65536 {
        return Err(std::io::Error::other("Private file must be bounded"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if info.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::other("Private file permissions required"));
        }
    }
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(std::io::Error::other("Private file exceeds bound"));
    }
    // Node readFile('utf8') replaces malformed sequences; validation still rejects CR/LF/NUL.
    Ok(js_trim(&String::from_utf8_lossy(&bytes)).to_owned())
}
