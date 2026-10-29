use crate::context::fail;
use reqwest::{Client, Url, header::{CONTENT_LENGTH, LOCATION}};
use std::{io, time::Duration};
use tokio::time::Instant;

#[derive(Clone, Copy)]
pub struct Limits { pub bytes: usize, pub wall: Duration, pub idle: Duration }
impl Limits {
    fn validate(self) -> io::Result<Self> {
        if self.bytes == 0 || self.bytes > 64 * 1024 * 1024 || self.wall.is_zero()
            || self.wall > Duration::from_secs(90) || self.idle.is_zero() || self.idle > Duration::from_secs(30) {
            return Err(fail("Invalid bounded official download limits."));
        }
        Ok(self)
    }
}
fn allowed(url: &Url, initial: &Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some()
        || url.as_str().len() > 16384 { return false; }
    if initial.scheme() == "http" {
        return initial.host_str() == Some("127.0.0.1") && url.origin() == initial.origin();
    }
    url.scheme() == "https" && url.port_or_known_default() == Some(443)
        && match initial.host_str() {
            Some("static.rust-lang.org") => url.host_str() == Some("static.rust-lang.org"),
            Some("github.com") => matches!(url.host_str(), Some("github.com" | "release-assets.githubusercontent.com" | "objects.githubusercontent.com")),
            _ => false,
        }
}
enum Failure { Timeout, Invalid }
impl From<reqwest::Error> for Failure {
    fn from(error: reqwest::Error) -> Self { if error.is_timeout() { Self::Timeout } else { Self::Invalid } }
}
async fn attempt(initial: &Url, limits: Limits, remaining: Duration) -> Result<Vec<u8>, Failure> {
    let client = Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never()).connect_timeout(limits.idle.min(remaining))
        .read_timeout(limits.idle).timeout(remaining)
        .no_gzip().no_brotli().no_deflate()
        .build().map_err(Failure::from)?;
    let mut url = initial.clone();
    for redirect in 0..=3 {
        if !allowed(&url, initial) { return Err(Failure::Invalid); }
        let mut response = client.get(url.clone()).header("Accept-Encoding", "identity")
            .send().await.map_err(Failure::from)?;
        if response.status().is_redirection() {
            if redirect == 3 { return Err(Failure::Invalid); }
            let location = response.headers().get(LOCATION).and_then(|v|v.to_str().ok()).ok_or(Failure::Invalid)?;
            url = url.join(location).map_err(|_|Failure::Invalid)?;
            continue;
        }
        if response.status() != reqwest::StatusCode::OK { return Err(Failure::Invalid); }
        let length = response.headers().get(CONTENT_LENGTH).map(|raw| {
            let raw = raw.to_str().map_err(|_|Failure::Invalid)?;
            if raw.is_empty() || !raw.bytes().all(|b|b.is_ascii_digit()) { return Err(Failure::Invalid); }
            raw.parse::<usize>().map_err(|_|Failure::Invalid)
        }).transpose()?;
        if length.is_some_and(|n|n > limits.bytes) { return Err(Failure::Invalid); }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(Failure::from)? {
            if bytes.len().checked_add(chunk.len()).is_none_or(|n|n > limits.bytes) { return Err(Failure::Invalid); }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() || length.is_some_and(|n|n != bytes.len()) { return Err(Failure::Invalid); }
        return Ok(bytes);
    }
    Err(Failure::Invalid)
}

/// Bounded anonymous GET only. A returned buffer still requires the caller's pinned checksum.
pub async fn bounded_get(raw: &str, limits: Limits) -> io::Result<Vec<u8>> {
    let limits = limits.validate()?;
    let initial = Url::parse(raw).map_err(|_|fail("Invalid official download URL."))?;
    if initial.query().is_some() || !allowed(&initial, &initial) { return Err(fail("Official download destination is not allowlisted.")); }
    let deadline = Instant::now() + limits.wall;
    for index in 0..3 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { break; }
        match tokio::time::timeout_at(deadline, attempt(&initial, limits, remaining)).await {
            Ok(Ok(bytes)) => return Ok(bytes),
            Ok(Err(Failure::Timeout)) if index < 2 => continue,
            _ => break,
        }
    }
    Err(fail("Official download failed within its byte, attempt and time bounds."))
}
pub async fn github_release(raw: &str, bytes: usize) -> io::Result<Vec<u8>> {
    let url = Url::parse(raw).map_err(|_|fail("Invalid GitHub release URL."))?;
    if url.host_str() != Some("github.com") || !url.path().contains("/releases/download/") {
        return Err(fail("Expected an explicit official GitHub release asset."));
    }
    bounded_get(raw, Limits { bytes, wall: Duration::from_secs(90), idle: Duration::from_secs(30) }).await
}
