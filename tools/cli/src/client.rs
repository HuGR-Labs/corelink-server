//! Thin HTTP client wrapper for `corelink-cli` (WI-S15-001).
//!
//! Wraps `reqwest` with the PAT bearer auth header and a base URL resolved
//! from `CORELINK_BASE_URL` env var (default `https://corelink-api.humangr.com`).
//! Retry with exponential backoff (FM-150) is implemented here.
//!
//! Stream-1 additions: `whoami`, `cas_put`, `cas_get`, `ac_put`, `ac_get`
//! wrapping the production API at the canonical tenant-scoped paths.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tracing::warn;

use crate::config::DEFAULT_ENDPOINT;
use crate::error::CliError;

/// Default base URL — overridden by `CORELINK_BASE_URL` env var or config.
const DEFAULT_BASE_URL: &str = DEFAULT_ENDPOINT;

/// Response from `GET /v1/users/me`.
#[derive(Debug, Deserialize)]
pub struct WhoamiResp {
    /// Tenant identifier (used in API paths: `/v1/cas/<tenant_id>/<blake3>`).
    pub tenant_id: String,
    /// PAT prefix shown in audit logs (e.g. `corelink_pat_ABCDEF***`).
    pub token_prefix: String,
    /// Route kind: `pat`, `ci`, or `ro`.
    pub route_kind: String,
}

/// Response for PUT CAS/AC operations (201 fresh or 200 idempotent).
#[derive(Debug, Deserialize)]
pub struct PutResp {
    /// The BLAKE3 hex digest echoed by the server.
    pub hash: Option<String>,
}

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
    /// Cached tenant_id; populated from config or whoami response.
    tenant_id: Option<String>,
}

impl CorelinkClient {
    /// Build a new client from a resolved PAT.
    /// Reads base_url from env `CORELINK_BASE_URL`, then config, then default.
    /// Reads tenant_id from config if available.
    pub fn new(pat: String) -> Result<Self, CliError> {
        let base_url =
            std::env::var("CORELINK_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        // Load tenant_id from config (best-effort; None if not set).
        let tenant_id = crate::config::load()
            .ok()
            .and_then(|c| c.defaults.tenant_id);
        let http = Client::builder()
            .use_rustls_tls()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            inner: Arc::new(ClientInner {
                http,
                base_url,
                pat,
                tenant_id,
            }),
        })
    }

    /// Call `GET /v1/users/me` and return the parsed response.
    pub async fn whoami(&self) -> Result<WhoamiResp, CliError> {
        let url = format!("{}/v1/users/me", self.inner.base_url);
        let resp = self
            .inner
            .http
            .get(&url)
            .bearer_auth(&self.inner.pat)
            .send()
            .await?;
        if resp.status().is_success() {
            let body: WhoamiResp = resp.json().await?;
            Ok(body)
        } else {
            Err(CliError::Other(format!(
                "whoami: HTTP {} from /v1/users/me",
                resp.status()
            )))
        }
    }

    /// `PUT /v1/cas/<tenant>/<blake3>` — upload raw bytes.
    ///
    /// Returns `CliError::Other("tenant_id not set")` if tenant is unknown.
    pub async fn cas_put(&self, blake3_hex: &str, body: Bytes) -> Result<PutResp, CliError> {
        let tenant = self.require_tenant()?;
        let url = format!("{}/v1/cas/{tenant}/{blake3_hex}", self.inner.base_url);
        let mut attempt = 0u32;
        loop {
            let resp = self
                .inner
                .http
                .put(&url)
                .bearer_auth(&self.inner.pat)
                .header("content-type", "application/octet-stream")
                .body(body.clone())
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    // Drain body; ignore parse error (server may return empty 201).
                    let put_resp: PutResp = r.json().await.unwrap_or(PutResp {
                        hash: Some(blake3_hex.to_owned()),
                    });
                    return Ok(put_resp);
                }
                Ok(r) if is_transient(r.status()) => {
                    attempt += 1;
                    if attempt > MAX_RETRIES {
                        return Err(CliError::Other(format!(
                            "cas_put: HTTP {} after {MAX_RETRIES} retries",
                            r.status()
                        )));
                    }
                    let backoff = backoff_ms(attempt);
                    warn!(attempt, backoff_ms = backoff, "transient error, retrying");
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
                Ok(r) => {
                    return Err(CliError::Other(format!(
                        "cas_put: HTTP {} for /v1/cas/{tenant}/{blake3_hex}",
                        r.status()
                    )));
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

    /// `GET /v1/cas/<tenant>/<blake3>` — download bytes.
    pub async fn cas_get(&self, blake3_hex: &str) -> Result<Bytes, CliError> {
        let tenant = self.require_tenant()?;
        let path = format!("/v1/cas/{tenant}/{blake3_hex}");
        self.get_bytes(&path).await
    }

    /// `PUT /v1/ac/<tenant>/<digest>` — store action cache result.
    pub async fn ac_put(&self, digest: &str, payload: Bytes) -> Result<PutResp, CliError> {
        let tenant = self.require_tenant()?;
        let url = format!("{}/v1/ac/{tenant}/{digest}", self.inner.base_url);
        let resp = self
            .inner
            .http
            .put(&url)
            .bearer_auth(&self.inner.pat)
            .header("content-type", "application/octet-stream")
            .body(payload)
            .send()
            .await?;
        if resp.status().is_success() {
            let put_resp = resp.json().await.unwrap_or(PutResp {
                hash: Some(digest.to_owned()),
            });
            Ok(put_resp)
        } else {
            Err(CliError::Other(format!(
                "ac_put: HTTP {} for /v1/ac/{tenant}/{digest}",
                resp.status()
            )))
        }
    }

    /// `GET /v1/ac/<tenant>/<digest>` — retrieve action cache result.
    pub async fn ac_get(&self, digest: &str) -> Result<Bytes, CliError> {
        let tenant = self.require_tenant()?;
        let path = format!("/v1/ac/{tenant}/{digest}");
        self.get_bytes(&path).await
    }

    /// Cached tenant id, if resolved from config. Public accessor for
    /// commands (`audit`, `tenant`, `cas`) that build tenant-scoped
    /// paths themselves.
    #[must_use]
    pub fn tenant_id(&self) -> Option<&str> {
        self.inner.tenant_id.as_deref()
    }

    /// Require tenant_id or return an informative error.
    fn require_tenant(&self) -> Result<&str, CliError> {
        self.inner.tenant_id.as_deref().ok_or_else(|| {
            CliError::Other(
                "tenant_id not set — run `corelink login --token=<PAT>` or \
                 `corelink whoami` to cache your tenant ID."
                    .to_owned(),
            )
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

    /// GET raw bytes plus one named response header value, with bearer
    /// auth + retry. Used by `audit export` / `audit tail` /
    /// `tenant export` to capture the `X-CoreLink-Audit-Export-Chain-Head-Anchor`
    /// header alongside the NDJSON body.
    pub async fn get_bytes_with_header(
        &self,
        path: &str,
        header_name: &str,
    ) -> Result<(Bytes, Option<String>), CliError> {
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
                    let header_val = r
                        .headers()
                        .get(header_name)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.trim().to_owned());
                    let bytes = r.bytes().await?;
                    return Ok((bytes, header_val));
                }
                Ok(r) if is_transient(r.status()) => {
                    attempt += 1;
                    if attempt > MAX_RETRIES {
                        return Err(CliError::Other(format!(
                            "HTTP {} after {MAX_RETRIES} retries",
                            r.status()
                        )));
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
                        return Err(CliError::Other(format!(
                            "HTTP {} after retries",
                            r.status()
                        )));
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
            assert!(
                b <= BACKOFF_MAX_MS,
                "backoff {b} exceeds max at attempt {i}"
            );
        }
    }
}
