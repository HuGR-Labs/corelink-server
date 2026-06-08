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

use crate::npm::error::NpmAdapterError;

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
    /// [`crate::npm::metadata`]).
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Upstream`] on any transport failure
    /// or non-2xx status.
    pub async fn fetch_metadata(&self, pkg: &str) -> Result<Vec<u8>, NpmAdapterError> {
        // SSRF-guarded join: the resolved metadata URL must stay on the
        // configured registry origin (scheme + host + port).
        let url = join_within_upstream(&self.base, pkg)?;
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
    /// [`crate::npm::tarball`]).
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
        // SSRF guard: npm tarballs are served from the SAME registry origin as
        // metadata (`{registry}/{pkg}/-/{file}`). The URL is constructed from
        // path-derived `pkg`/`file` segments, so pin the resolved origin to the
        // configured registry (scheme + host + port) before fetching — a
        // host-swapped URL is rejected rather than fetched.
        require_same_origin(&self.base, tarball_url)?;
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

/// Join `path` onto `upstream` and verify the result stays on the SAME origin
/// (scheme + host + port). `Url::join` host-swaps when `path` carries a scheme
/// (`https://evil/…`) or a protocol-relative authority (`//evil/…`) — this is
/// the SSRF guard for the read-through metadata fetch. Returns
/// [`NpmAdapterError::Upstream`] when the resolved URL escapes the configured
/// registry origin.
fn join_within_upstream(upstream: &Url, path: &str) -> Result<Url, NpmAdapterError> {
    let joined = upstream
        .join(path)
        .map_err(|e| NpmAdapterError::Upstream(format!("url join: {e}")))?;
    require_same_origin(upstream, &joined)?;
    Ok(joined)
}

/// Verify `candidate` is on the SAME origin (scheme + host + port) as
/// `upstream`. Used to pin an already-absolute URL (e.g. a constructed tarball
/// URL) to the configured registry origin. Returns
/// [`NpmAdapterError::Upstream`] on mismatch.
fn require_same_origin(upstream: &Url, candidate: &Url) -> Result<(), NpmAdapterError> {
    let same_origin = candidate.scheme() == upstream.scheme()
        && candidate.host_str() == upstream.host_str()
        && candidate.port_or_known_default() == upstream.port_or_known_default();
    if !same_origin {
        return Err(NpmAdapterError::Upstream(
            "SSRF guard: resolved upstream URL escapes the configured host".to_owned(),
        ));
    }
    Ok(())
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

    #[test]
    fn join_relative_path_stays_on_host() {
        let up = Url::parse("https://registry.npmjs.org").unwrap();
        let u = join_within_upstream(&up, "lodash").unwrap();
        assert_eq!(u.as_str(), "https://registry.npmjs.org/lodash");
    }

    #[test]
    fn join_absolute_path_stays_on_host() {
        let up = Url::parse("https://registry.npmjs.org").unwrap();
        let u = join_within_upstream(&up, "/lodash/-/lodash-4.17.21.tgz").unwrap();
        assert_eq!(u.host_str(), Some("registry.npmjs.org"));
    }

    #[test]
    fn join_absolute_url_is_rejected_ssrf() {
        // A scheme-bearing path host-swaps via `Url::join`.
        let up = Url::parse("https://registry.npmjs.org").unwrap();
        assert!(
            join_within_upstream(&up, "https://evil.example/x").is_err(),
            "scheme-bearing path must be rejected by the SSRF guard"
        );
    }

    #[test]
    fn join_protocol_relative_authority_is_rejected_ssrf() {
        // `//authority` is protocol-relative and host-swaps via `Url::join`.
        let up = Url::parse("https://registry.npmjs.org").unwrap();
        assert!(
            join_within_upstream(&up, "//evil.example/x").is_err(),
            "protocol-relative authority must be rejected by the SSRF guard"
        );
    }
}
