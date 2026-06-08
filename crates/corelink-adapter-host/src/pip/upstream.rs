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

use crate::pip::error::PipAdapterError;

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

/// Canonical PyPI file-download host. PyPI serves the simple index from
/// `pypi.org` but the wheel/sdist BYTES from this SECOND host, so the wheel
/// SSRF guard ([`require_wheel_host_allowed`]) allows it in addition to the
/// configured index origin.
pub const PYPI_FILES_HOST: &str = "files.pythonhosted.org";

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
    /// [`crate::pip::index`]).
    ///
    /// # Errors
    ///
    /// Returns [`PipAdapterError::Upstream`] on any transport failure
    /// or non-2xx status.
    pub async fn fetch_json_index(&self, project: &str) -> Result<Vec<u8>, PipAdapterError> {
        let path = format!("simple/{project}/");
        // SSRF-guarded join: the resolved index URL must stay on the configured
        // index origin (scheme + host + port) — `pypi.org` by default.
        let url = join_within_upstream(&self.base, &path)?;
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(PEP691_ACCEPT));
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
    /// [`crate::pip::wheel`]).
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
        // SSRF guard: the wheel URL is an ABSOLUTE url taken from the parsed
        // upstream index. On real PyPI the index host (`pypi.org`) and the
        // wheel/sdist file host (`files.pythonhosted.org`) DIFFER, so the guard
        // allows EITHER the configured index origin OR the canonical PyPI files
        // host (https only). Any other origin (a foreign host injected into the
        // index payload) is rejected before we fetch it.
        require_wheel_host_allowed(&self.base, wheel_url)?;
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

/// Join `path` onto `upstream` and verify the result stays on the SAME origin
/// (scheme + host + port). `Url::join` host-swaps when `path` carries a scheme
/// (`https://evil/…`) or a protocol-relative authority (`//evil/…`) — this is
/// the SSRF guard for the read-through simple-index fetch. Returns
/// [`PipAdapterError::Upstream`] when the resolved URL escapes the configured
/// index origin.
fn join_within_upstream(upstream: &Url, path: &str) -> Result<Url, PipAdapterError> {
    let joined = upstream
        .join(path)
        .map_err(|e| PipAdapterError::Upstream(format!("url join: {e}")))?;
    let same_origin = joined.scheme() == upstream.scheme()
        && joined.host_str() == upstream.host_str()
        && joined.port_or_known_default() == upstream.port_or_known_default();
    if !same_origin {
        return Err(PipAdapterError::Upstream(
            "SSRF guard: resolved upstream URL escapes the configured host".to_owned(),
        ));
    }
    Ok(joined)
}

/// Verify an ABSOLUTE wheel/sdist `candidate` URL is on an ALLOWED file host.
///
/// PyPI's simple index (`upstream`, default `pypi.org`) names file URLs on a
/// DIFFERENT host (`files.pythonhosted.org`), so a single-origin pin (as used
/// for the index join) would reject every legitimate wheel download. This
/// guard therefore allows the wheel host to be EITHER:
///
/// - the configured index origin (scheme + host + port) — covers private
///   mirrors / test servers that co-locate index + files; or
/// - `https://`[`PYPI_FILES_HOST`] — the canonical public PyPI file host.
///
/// Any other origin (a foreign host injected into the index payload — the
/// SSRF vector) is rejected. Returns [`PipAdapterError::Upstream`] on a
/// disallowed host.
fn require_wheel_host_allowed(upstream: &Url, candidate: &Url) -> Result<(), PipAdapterError> {
    let on_index_origin = candidate.scheme() == upstream.scheme()
        && candidate.host_str() == upstream.host_str()
        && candidate.port_or_known_default() == upstream.port_or_known_default();
    let on_files_host =
        candidate.scheme() == "https" && candidate.host_str() == Some(PYPI_FILES_HOST);
    if on_index_origin || on_files_host {
        return Ok(());
    }
    Err(PipAdapterError::Upstream(format!(
        "SSRF guard: wheel host `{}` is not an allowed upstream file host",
        candidate.host_str().unwrap_or("<none>")
    )))
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

    // --- index-join SSRF guard (the 4 required cases) ---

    #[test]
    fn join_relative_path_stays_on_host() {
        let up = Url::parse("https://pypi.org").unwrap();
        let u = join_within_upstream(&up, "simple/requests/").unwrap();
        assert_eq!(u.as_str(), "https://pypi.org/simple/requests/");
    }

    #[test]
    fn join_absolute_path_stays_on_host() {
        let up = Url::parse("https://pypi.org").unwrap();
        let u = join_within_upstream(&up, "/simple/requests/").unwrap();
        assert_eq!(u.host_str(), Some("pypi.org"));
    }

    #[test]
    fn join_absolute_url_is_rejected_ssrf() {
        // A scheme-bearing path host-swaps via `Url::join`.
        let up = Url::parse("https://pypi.org").unwrap();
        assert!(
            join_within_upstream(&up, "https://evil.example/x").is_err(),
            "scheme-bearing path must be rejected by the SSRF guard"
        );
    }

    #[test]
    fn join_protocol_relative_authority_is_rejected_ssrf() {
        // `//authority` is protocol-relative and host-swaps via `Url::join`.
        let up = Url::parse("https://pypi.org").unwrap();
        assert!(
            join_within_upstream(&up, "//evil.example/x").is_err(),
            "protocol-relative authority must be rejected by the SSRF guard"
        );
    }

    // --- wheel-fetch two-host allowlist (don't break legit PyPI downloads) ---

    #[test]
    fn wheel_on_pythonhosted_files_host_is_allowed() {
        let up = Url::parse("https://pypi.org").unwrap();
        let wheel = Url::parse(
            "https://files.pythonhosted.org/packages/xx/requests-2.31.0-py3-none-any.whl",
        )
        .unwrap();
        assert!(
            require_wheel_host_allowed(&up, &wheel).is_ok(),
            "the canonical PyPI files host must be allowed"
        );
    }

    #[test]
    fn wheel_on_configured_index_origin_is_allowed() {
        // A private mirror co-locating index + files on one host.
        let up = Url::parse("https://mirror.internal:8443").unwrap();
        let wheel = Url::parse("https://mirror.internal:8443/packages/x-1.0.whl").unwrap();
        assert!(require_wheel_host_allowed(&up, &wheel).is_ok());
    }

    #[test]
    fn wheel_on_foreign_host_is_rejected_ssrf() {
        let up = Url::parse("https://pypi.org").unwrap();
        let wheel = Url::parse("https://evil.example/packages/x-1.0.whl").unwrap();
        assert!(
            require_wheel_host_allowed(&up, &wheel).is_err(),
            "a foreign file host injected into the index must be rejected"
        );
    }

    #[test]
    fn wheel_on_files_host_over_http_is_rejected() {
        // Downgrade attack: the canonical files host but plaintext http.
        let up = Url::parse("https://pypi.org").unwrap();
        let wheel = Url::parse("http://files.pythonhosted.org/packages/x-1.0.whl").unwrap();
        assert!(
            require_wheel_host_allowed(&up, &wheel).is_err(),
            "the files host must be https-only"
        );
    }
}
