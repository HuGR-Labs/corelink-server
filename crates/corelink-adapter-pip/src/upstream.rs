//! Upstream `pypi.org` client.
//!
//! This module is a thin `reqwest`-based wrapper that translates
//! `(operation, args)` into HTTP calls + `PipAdapterError`-shaped
//! results. The adapter intentionally does NOT depend on `pip` /
//! `uv` semantic libraries: the wire surface (PEP 691 JSON + raw
//! bytes for wheels) is small enough to talk to directly.

use std::fmt;

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::Client;
use url::Url;

use crate::error::PipAdapterError;

/// Header value the adapter sends in `Accept` to negotiate PEP 691
/// JSON when `prefer_json_index` is `true`.
pub const PEP691_ACCEPT: &str = "application/vnd.pypi.simple.v1+json";

/// Header value the adapter sends in `Accept` when the client itself
/// asked for the PEP 503 HTML form (fallback path).
pub const PEP503_ACCEPT: &str = "text/html";

/// User-Agent the adapter sends upstream. PyPI's CDN logs and rate-
/// limit policies key off this string; advertising ourselves
/// explicitly is friendlier than an unbranded `reqwest/<ver>`.
pub const ADAPTER_USER_AGENT: &str = "corelink-adapter-pip/0.1 (+https://humangr.com)";

/// Production `pypi.org` HTTP client.
///
/// `#[non_exhaustive]` so future fields (per-tenant rate-limit token,
/// outbound proxy, mTLS for private mirrors) can be added without
/// breaking call sites.
#[non_exhaustive]
pub struct UpstreamClient {
    client: Client,
    base: Url,
}

impl fmt::Debug for UpstreamClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpstreamClient")
            .field("base", &self.base.as_str())
            .finish()
    }
}

impl UpstreamClient {
    /// Construct a new upstream client. The `Client` is built with a
    /// 30-second timeout (covers the slowest wheel CDN edges
    /// observed in practice).
    ///
    /// # Errors
    ///
    /// Returns [`PipAdapterError::Upstream`] if the `reqwest::Client`
    /// fails to build (e.g. TLS backend init failure).
    pub fn new(base: Url) -> Result<Self, PipAdapterError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(ADAPTER_USER_AGENT)
            .build()
            .map_err(|e| PipAdapterError::Upstream(format!("client build: {e}")))?;
        Ok(Self { client, base })
    }

    /// Fetch the PEP 691 JSON index for `project` from upstream.
    ///
    /// Returns the raw JSON bytes (parsing happens in
    /// [`crate::index`]).
    ///
    /// # Errors
    ///
    /// Returns [`PipAdapterError::Upstream`] on any transport failure
    /// or non-2xx status.
    pub async fn fetch_json_index(&self, project: &str) -> Result<Vec<u8>, PipAdapterError> {
        let path = format!("simple/{project}/");
        let url = self
            .base
            .join(&path)
            .map_err(|e| PipAdapterError::Upstream(format!("project URL join: {e}")))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(PEP691_ACCEPT),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(ADAPTER_USER_AGENT));

        let resp = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| PipAdapterError::Upstream(format!("send: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(PipAdapterError::Upstream(format!(
                "non-success status {status} from upstream index"
            )));
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| PipAdapterError::Upstream(format!("read body: {e}")))?;
        Ok(body.to_vec())
    }

    /// Fetch a wheel or sdist by absolute URL. Returns the raw bytes
    /// (integrity check is the caller's responsibility — see
    /// [`crate::wheel`]).
    ///
    /// # Errors
    ///
    /// Returns [`PipAdapterError::Upstream`] on any transport failure
    /// or non-2xx status; [`PipAdapterError::WheelOversized`] if the
    /// payload exceeds `max_bytes`.
    pub async fn fetch_wheel(
        &self,
        wheel_url: &Url,
        max_bytes: u64,
    ) -> Result<Bytes, PipAdapterError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(ADAPTER_USER_AGENT));
        let resp = self
            .client
            .get(wheel_url.clone())
            .headers(headers)
            .send()
            .await
            .map_err(|e| PipAdapterError::Upstream(format!("send: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(PipAdapterError::Upstream(format!(
                "non-success status {status} from upstream wheel"
            )));
        }
        if let Some(len) = resp.content_length() {
            if len > max_bytes {
                return Err(PipAdapterError::WheelOversized(len));
            }
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| PipAdapterError::Upstream(format!("read body: {e}")))?;
        let body_len = u64::try_from(body.len()).unwrap_or(u64::MAX);
        if body_len > max_bytes {
            return Err(PipAdapterError::WheelOversized(body_len));
        }
        Ok(body)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_pep_specs() {
        assert!(PEP691_ACCEPT.contains("application/vnd.pypi.simple"));
        assert!(PEP691_ACCEPT.ends_with("+json"));
        assert_eq!(PEP503_ACCEPT, "text/html");
        assert!(ADAPTER_USER_AGENT.starts_with("corelink-adapter-pip/"));
    }

    #[test]
    fn client_builds_with_default_base() {
        let base = Url::parse("https://pypi.org").expect("parse");
        let client = UpstreamClient::new(base);
        assert!(client.is_ok());
    }
}
