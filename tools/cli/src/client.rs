//! Thin HTTP client wrapper for `corelink-cli` (WI-S15-001).
//!
//! Wraps `reqwest` with the PAT bearer auth header and a base URL resolved
//! from `CORELINK_BASE_URL` env var (default `https://corelink.humangr.com`).
//! Retry with exponential backoff (FM-150) is implemented here.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::{Client, StatusCode};
use tracing::warn;

use crate::error::CliError;

/// Default base URL.
const DEFAULT_BASE_URL: &str = "https://corelink.humangr.com";

/// Maximum retry attempts for transient failures (FM-150).
const MAX_RETRIES: u32 = 3;

/// Initial backoff duration.
const BACKOFF_INIT_MS: u64 = 100;

/// Maximum backoff duration (cap).
const BACKOFF_MAX_MS: u64 = 10_000;

/// Shared HTTP client for CLI commands.
#[derive(Debug, Clone)]
pub struct CorelinkClient {
    inner: Arc<ClientInner>,
}

#[derive(Debug)]
struct ClientInner {
    http: Client,
    base_url: String,
    pat: String,
}

impl CorelinkClient {
    /// Build a new client from a resolved PAT.
    pub fn new(pat: String) -> Result<Self, CliError> {
        let base_url = std::env::var("CORELINK_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        let http = Client::builder()
            .use_rustls_tls()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            inner: Arc::new(ClientInner { http, base_url, pat }),
        })
    }

    /// GET with bearer auth + retry.
    pub async fn get_bytes(&self, path: &str) -> Result<Bytes, CliError> {
        let url = format!("{}{}", self.inner.base_url, path);
        let mut attempt = 0u32;
        loop {
            let resp = self
                .inner
                .http
                .get(&url)
                .bearer_auth(&self.inner.pat)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    return Ok(r.bytes().await?);
                }
                Ok(r) if is_transient(r.status()) => {
                    attempt += 1;
                    if attempt > MAX_RETRIES {
                        return Err(CliError::Other(format!(
                            "HTTP {} after {MAX_RETRIES} retries",
                            r.status()
                        )));
                    }
                    let backoff = backoff_ms(attempt);
                    warn!(attempt, backoff_ms = backoff, "transient error, retrying");
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
                Ok(r) => {
                    return Err(CliError::Other(format!("HTTP error {}", r.status())));
                }
                Err(e) if attempt < MAX_RETRIES => {
                    attempt += 1;
                    let backoff = backoff_ms(attempt);
                    warn!(attempt, backoff_ms = backoff, error = %e, "network error, retrying");
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
                Err(e) => return Err(CliError::Network(e)),
            }
        }
    }

    /// GET JSON response with bearer auth + retry.
    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value, CliError> {
        let url = format!("{}{}", self.inner.base_url, path);
        let mut attempt = 0u32;
        loop {
            let resp = self
                .inner
                .http
                .get(&url)
                .bearer_auth(&self.inner.pat)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    return Ok(r.json().await?);
                }
                Ok(r) if is_transient(r.status()) => {
                    attempt += 1;
                    if attempt > MAX_RETRIES {
                        return Err(CliError::Other(format!("HTTP {} after retries", r.status())));
                    }
                    tokio::time::sleep(Duration::from_millis(backoff_ms(attempt))).await;
                }
                Ok(r) => {
                    return Err(CliError::Other(format!("HTTP error {}", r.status())));
                }
                Err(e) if attempt < MAX_RETRIES => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(backoff_ms(attempt))).await;
                    let _ = e;
                }
                Err(e) => return Err(CliError::Network(e)),
            }
        }
    }

    /// POST bytes with bearer auth.
    pub async fn post_bytes(&self, path: &str, body: Bytes) -> Result<serde_json::Value, CliError> {
        let url = format!("{}{}", self.inner.base_url, path);
        let resp = self
            .inner
            .http
            .post(&url)
            .bearer_auth(&self.inner.pat)
            .header("content-type", "application/octet-stream")
            .body(body)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(CliError::Other(format!("HTTP error {}", resp.status())))
        }
    }

    /// Base URL accessor (for doctor check #1).
    #[must_use]
    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }
}

fn is_transient(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
            | StatusCode::REQUEST_TIMEOUT
    )
}

/// Exponential backoff with jitter (FM-150).
fn backoff_ms(attempt: u32) -> u64 {
    let shift = attempt.saturating_sub(1).min(62);
    let base = BACKOFF_INIT_MS.saturating_mul(1u64 << shift);
    let capped = base.min(BACKOFF_MAX_MS);
    // 50% jitter: range [capped/2, capped].
    let jitter_range = capped / 2;
    // Deterministic pseudo-jitter using attempt number (no rand dep).
    let jitter = (attempt as u64 * 37) % (jitter_range + 1);
    capped - jitter_range + jitter
}

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_exponentially() {
        let b1 = backoff_ms(1);
        let b2 = backoff_ms(2);
        let b3 = backoff_ms(3);
        assert!(b1 <= b2, "backoff should grow: {b1} <= {b2}");
        assert!(b2 <= b3, "backoff should grow: {b2} <= {b3}");
        assert!(b3 <= BACKOFF_MAX_MS);
    }

    #[test]
    fn backoff_within_bounds() {
        for i in 1..=10 {
            let b = backoff_ms(i);
            assert!(b <= BACKOFF_MAX_MS, "backoff {b} exceeds max at attempt {i}");
        }
    }
}
