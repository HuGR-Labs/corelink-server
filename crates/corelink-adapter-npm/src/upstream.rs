//! Upstream `registry.npmjs.org` client.
//!
//! Thin `reqwest`-based wrapper that translates `(operation, args)`
//! into HTTP calls + `NpmAdapterError`-shaped results. The adapter
//! does NOT depend on any npm semantic library — the wire surface
//! (JSON metadata + raw bytes for tarballs) is small enough to talk
//! to directly.

use std::fmt;

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::Client;
use url::Url;

use crate::error::NpmAdapterError;

/// `Accept` header value for npm JSON metadata responses.
pub const NPM_ACCEPT: &str = "application/json";

/// User-Agent the adapter sends upstream.
pub const ADAPTER_USER_AGENT: &str = "corelink-adapter-npm/0.1 (+https://humangr.com)";

/// Production `registry.npmjs.org` HTTP client.
///
/// `#[non_exhaustive]` so future fields (per-tenant rate-limit token,
/// outbound proxy) can be added without breaking call sites.
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
    /// Construct a new upstream client with a 30-second timeout.
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Upstream`] if the `reqwest::Client`
    /// fails to build.
    pub fn new(base: Url) -> Result<Self, NpmAdapterError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(ADAPTER_USER_AGENT)
            .build()
            .map_err(|e| NpmAdapterError::Upstream(format!("client build: {e}")))?;
        Ok(Self { client, base })
    }

    /// Fetch the full package metadata JSON for `pkg` from upstream.
    ///
    /// Returns the raw JSON bytes (parsing happens in
    /// [`crate::metadata`]).
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Upstream`] on any transport failure
    /// or non-2xx status.
    pub async fn fetch_metadata(&self, pkg: &str) -> Result<Vec<u8>, NpmAdapterError> {
        let url = self
            .base
            .join(pkg)
            .map_err(|e| NpmAdapterError::Upstream(format!("package URL join: {e}")))?;
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(NPM_ACCEPT));
        headers.insert(USER_AGENT, HeaderValue::from_static(ADAPTER_USER_AGENT));

        let resp = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| NpmAdapterError::Upstream(format!("send: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(NpmAdapterError::Upstream(format!(
                "non-success status {status} from upstream metadata"
            )));
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| NpmAdapterError::Upstream(format!("read body: {e}")))?;
        Ok(body.to_vec())
    }

    /// Fetch a tarball by absolute URL. Returns the raw bytes
    /// (integrity check is the caller's responsibility — see
    /// [`crate::tarball`]).
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Upstream`] on transport failure or
    /// non-2xx status; [`NpmAdapterError::TarballOversized`] if the
    /// payload exceeds `max_bytes`.
    pub async fn fetch_tarball(
        &self,
        tarball_url: &Url,
        max_bytes: u64,
    ) -> Result<Bytes, NpmAdapterError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(ADAPTER_USER_AGENT));
        let resp = self
            .client
            .get(tarball_url.clone())
            .headers(headers)
            .send()
            .await
            .map_err(|e| NpmAdapterError::Upstream(format!("send: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(NpmAdapterError::Upstream(format!(
                "non-success status {status} from upstream tarball"
            )));
        }
        if let Some(len) = resp.content_length() {
            if len > max_bytes {
                return Err(NpmAdapterError::TarballOversized(len));
            }
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| NpmAdapterError::Upstream(format!("read body: {e}")))?;
        let body_len = u64::try_from(body.len()).unwrap_or(u64::MAX);
        if body_len > max_bytes {
            return Err(NpmAdapterError::TarballOversized(body_len));
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
    fn constants_are_well_formed() {
        assert!(NPM_ACCEPT.contains("application/json"));
        assert!(ADAPTER_USER_AGENT.starts_with("corelink-adapter-npm/"));
    }

    #[test]
    fn client_builds_with_default_base() {
        let base = Url::parse("https://registry.npmjs.org").expect("parse");
        let client = UpstreamClient::new(base);
        assert!(client.is_ok());
    }
}
