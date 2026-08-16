//! Upstream bottle fetcher (reqwest).
//!
//! Performs a plain HTTPS `GET <upstream_domain>/<canonical_path>` and
//! returns the body bytes. Enforces the configured bottle-size cap.

use crate::upstream_ssrf::ssrf_safe_redirect_policy;

use bytes::Bytes;
use url::Url;

use crate::brew::error::BrewAdapterError;

/// Total per-request timeout for the brew upstream client. Matches the
/// npm/pip adapters so a slow/adversarial upstream (ghcr.io or its blob
/// CDN streaming at a trickle) cannot pin a Tokio task indefinitely.
const BREW_UPSTREAM_TIMEOUT_SECS: u64 = 30;

/// TCP connect-phase timeout, bounded separately from the total request
/// budget so a black-holed upstream fails fast at connect time.
const BREW_UPSTREAM_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Maximum number of HTTP redirects to follow before giving up. ghcr.io's
/// OCI blob endpoint 307-redirects to its public download CDN, so redirects
/// must be followed — but only to non-internal hosts (see
/// [`ssrf_safe_redirect_policy`]).
const BREW_UPSTREAM_MAX_REDIRECTS: usize = 5;

/// `Accept` header for upstream fetches. ghcr.io content-negotiates MANIFEST
/// requests on this header and returns **404** (not 401) for a manifest GET that
/// omits it — even WITH a valid anonymous token (verified live against ghcr.io
/// 2026-06-21: token-only → 404; token + these media types → 200). The OCI +
/// Docker manifest/index media types make manifest fetches succeed; the trailing
/// `*/*` keeps blob/tarball fetches (which ignore `Accept`) working unchanged.
const OCI_ACCEPT: &str = "application/vnd.oci.image.index.v1+json,\
application/vnd.oci.image.manifest.v1+json,\
application/vnd.docker.distribution.manifest.list.v2+json,\
application/vnd.docker.distribution.manifest.v2+json,*/*";

/// Reqwest-backed upstream bottle fetcher.
#[non_exhaustive]
#[derive(Debug)]
pub struct UpstreamFetcher {
    client: reqwest::Client,
    upstream_domain: Url,
}

impl UpstreamFetcher {
    /// Construct a new fetcher targeting `upstream_domain`. The client
    /// uses reqwest's default rustls-backed TLS stack.
    ///
    /// # Errors
    ///
    /// Returns [`BrewAdapterError::Upstream`] if the reqwest client
    /// could not be constructed (e.g. system TLS init failure).
    pub fn new(upstream_domain: Url) -> Result<Self, BrewAdapterError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("corelink-adapter-brew/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(BREW_UPSTREAM_TIMEOUT_SECS))
            .connect_timeout(std::time::Duration::from_secs(
                BREW_UPSTREAM_CONNECT_TIMEOUT_SECS,
            ))
            .redirect(ssrf_safe_redirect_policy(BREW_UPSTREAM_MAX_REDIRECTS))
            .build()
            .map_err(|err| BrewAdapterError::Upstream(format!("client build: {err}")))?;
        Ok(Self {
            client,
            upstream_domain,
        })
    }

    /// Fetch `canonical_path` from upstream. Returns the body bytes.
    ///
    /// Enforces:
    ///
    /// - 2xx response status (4xx/5xx surface as
    ///   [`BrewAdapterError::Upstream`]);
    /// - body size ≤ `bottle_size_limit_bytes` — checked first against
    ///   the `Content-Length` header (fail-fast for known-oversize) and
    ///   then against the streamed total (fail-LATE for missing /
    ///   misleading `Content-Length`).
    pub async fn fetch_bottle(
        &self,
        canonical_path: &str,
        bottle_size_limit_bytes: u64,
    ) -> Result<Vec<u8>, BrewAdapterError> {
        // SSRF-guarded join: the resolved URL must stay on the configured
        // upstream origin (scheme + host + port).
        let url = join_within_upstream(&self.upstream_domain, canonical_path)?;

        let mut response = self
            .client
            .get(url.clone())
            .header(reqwest::header::ACCEPT, OCI_ACCEPT)
            .send()
            .await
            .map_err(|err| BrewAdapterError::Upstream(format!("send: {err}")))?;

        // OCI registry anonymous-token dance. ghcr.io (the default bottle host)
        // 401s an UNAUTHENTICATED pull — even of PUBLIC homebrew/core bottles —
        // with a `WWW-Authenticate: Bearer realm=…,service=…,scope=…` challenge.
        // We must fetch the anonymous token from that realm and retry once with
        // it, or EVERY bottle fetch fails closed: the pre-fix plain GET returned
        // 401 → `Upstream` → HTTP 502 (Homebrew on CoreLink was non-functional).
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(token) = self.anon_token_for(&response).await? {
                response = self
                    .client
                    .get(url.clone())
                    .bearer_auth(token)
                    .header(reqwest::header::ACCEPT, OCI_ACCEPT)
                    .send()
                    .await
                    .map_err(|err| BrewAdapterError::Upstream(format!("send (authed): {err}")))?;
            }
        }

        let status = response.status();
        if !status.is_success() {
            return Err(BrewAdapterError::Upstream(format!(
                "upstream status: {status}"
            )));
        }

        // Fail-fast against Content-Length.
        if let Some(declared) = response.content_length() {
            if declared > bottle_size_limit_bytes {
                return Err(BrewAdapterError::BottleOversized(declared));
            }
        }

        // Capacity hint sized to declared length when available.
        let capacity_hint: usize = response
            .content_length()
            .and_then(|l| usize::try_from(l).ok())
            .unwrap_or(0);
        let mut buf: Vec<u8> = Vec::with_capacity(capacity_hint);

        let mut stream = response;
        while let Some(chunk) = stream
            .chunk()
            .await
            .map_err(|err| BrewAdapterError::Upstream(format!("read chunk: {err}")))?
        {
            let new_total = buf
                .len()
                .checked_add(chunk.len())
                .ok_or(BrewAdapterError::BottleOversized(u64::MAX))?;
            let new_total_u64 = u64::try_from(new_total).unwrap_or(u64::MAX);
            if new_total_u64 > bottle_size_limit_bytes {
                return Err(BrewAdapterError::BottleOversized(new_total_u64));
            }
            extend_from_bytes(&mut buf, &chunk);
        }

        Ok(buf)
    }

    /// Resolve an anonymous OCI bearer token from a 401's
    /// `WWW-Authenticate: Bearer realm=…,service=…,scope=…` challenge, then the
    /// caller retries the fetch with it. Returns `Ok(None)` when the response
    /// carries no parseable Bearer challenge (so the caller surfaces the 401).
    ///
    /// SSRF guard: the token `realm` MUST be `https` and stay on the SAME host
    /// as the configured upstream — a challenge is never followed to an
    /// arbitrary host.
    async fn anon_token_for(
        &self,
        challenged: &reqwest::Response,
    ) -> Result<Option<String>, BrewAdapterError> {
        let header = match challenged.headers().get(reqwest::header::WWW_AUTHENTICATE) {
            Some(h) => h
                .to_str()
                .map_err(|_| BrewAdapterError::Upstream("non-ascii WWW-Authenticate".to_owned()))?,
            None => return Ok(None),
        };
        let Some((realm, service, scope)) = parse_bearer_challenge(header) else {
            return Ok(None);
        };
        let realm_url = Url::parse(&realm)
            .map_err(|err| BrewAdapterError::Upstream(format!("token realm parse: {err}")))?;
        if realm_url.scheme() != "https" || realm_url.host_str() != self.upstream_domain.host_str()
        {
            return Err(BrewAdapterError::Upstream(
                "SSRF guard: token realm escapes the configured upstream host".to_owned(),
            ));
        }
        let mut token_url = realm_url;
        {
            let mut qp = token_url.query_pairs_mut();
            if let Some(s) = service.as_deref() {
                qp.append_pair("service", s);
            }
            if let Some(s) = scope.as_deref() {
                qp.append_pair("scope", s);
            }
        }
        let resp = self
            .client
            .get(token_url)
            .send()
            .await
            .map_err(|err| BrewAdapterError::Upstream(format!("token fetch: {err}")))?;
        if !resp.status().is_success() {
            return Err(BrewAdapterError::Upstream(format!(
                "token endpoint status: {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|err| BrewAdapterError::Upstream(format!("token json: {err}")))?;
        // ghcr.io returns `{"token":"…"}`; the spec also permits `access_token`.
        let token = body
            .get("token")
            .or_else(|| body.get("access_token"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        Ok(token)
    }
}

/// Parse a `WWW-Authenticate: Bearer realm="…",service="…",scope="…"` challenge
/// into `(realm, service, scope)`. Returns `None` if the header is not a Bearer
/// challenge or carries no `realm`. ghcr.io's challenge values contain no commas,
/// so a comma split of the param list is sufficient for the bottle upstream.
fn parse_bearer_challenge(header: &str) -> Option<(String, Option<String>, Option<String>)> {
    let rest = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))?;
    let (mut realm, mut service, mut scope) = (None, None, None);
    for part in rest.split(',') {
        if let Some((k, v)) = part.trim().split_once('=') {
            let v = v.trim().trim_matches('"').to_owned();
            match k.trim() {
                "realm" => realm = Some(v),
                "service" => service = Some(v),
                "scope" => scope = Some(v),
                _ => {}
            }
        }
    }
    realm.map(|r| (r, service, scope))
}

/// Copy `chunk` into `buf`. Wrapped in a tiny helper to keep the
/// hot-path call site readable.
fn extend_from_bytes(buf: &mut Vec<u8>, chunk: &Bytes) {
    buf.extend_from_slice(chunk);
}

/// Join `path` onto `upstream` and verify the result stays on the SAME origin
/// (scheme + host + port). `Url::join` host-swaps when `path` carries a scheme
/// (`https://evil/…`) or a protocol-relative authority (`//evil/…`) — this is
/// the SSRF guard for the read-through upstream fetch. Returns
/// [`BrewAdapterError::Upstream`] when the resolved URL escapes the configured
/// upstream origin.
fn join_within_upstream(upstream: &url::Url, path: &str) -> Result<url::Url, BrewAdapterError> {
    let joined = upstream
        .join(path)
        .map_err(|err| BrewAdapterError::Upstream(format!("url join: {err}")))?;
    let same_origin = joined.scheme() == upstream.scheme()
        && joined.host_str() == upstream.host_str()
        && joined.port_or_known_default() == upstream.port_or_known_default();
    if !same_origin {
        return Err(BrewAdapterError::Upstream(
            "SSRF guard: resolved upstream URL escapes the configured host".to_owned(),
        ));
    }
    Ok(joined)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;

    #[test]
    fn join_relative_path_stays_on_host() {
        let up = Url::parse("https://ghcr.io").unwrap();
        let u = join_within_upstream(&up, "v2/homebrew/core/curl").unwrap();
        assert_eq!(u.as_str(), "https://ghcr.io/v2/homebrew/core/curl");
    }

    #[test]
    fn join_absolute_path_stays_on_host() {
        let up = Url::parse("https://ghcr.io").unwrap();
        let u = join_within_upstream(&up, "/v2/foo").unwrap();
        assert_eq!(u.host_str(), Some("ghcr.io"));
    }

    #[test]
    fn join_absolute_url_is_rejected_ssrf() {
        // A scheme-bearing path would host-swap via `Url::join`.
        let up = Url::parse("https://ghcr.io").unwrap();
        assert!(
            join_within_upstream(&up, "https://evil.example/x").is_err(),
            "scheme-bearing path must be rejected by the SSRF guard"
        );
    }

    #[test]
    fn join_protocol_relative_authority_is_rejected_ssrf() {
        // `//authority` is protocol-relative and host-swaps via `Url::join`.
        let up = Url::parse("https://ghcr.io").unwrap();
        assert!(
            join_within_upstream(&up, "//evil.example/x").is_err(),
            "protocol-relative authority must be rejected by the SSRF guard"
        );
    }

    #[test]
    fn parse_ghcr_bearer_challenge() {
        // The exact shape ghcr.io returns for an anonymous homebrew bottle pull.
        let h = r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:homebrew/core/jq:pull""#;
        let (realm, service, scope) = parse_bearer_challenge(h).expect("parses");
        assert_eq!(realm, "https://ghcr.io/token");
        assert_eq!(service.as_deref(), Some("ghcr.io"));
        assert_eq!(scope.as_deref(), Some("repository:homebrew/core/jq:pull"));
    }

    #[test]
    fn parse_non_bearer_challenge_is_none() {
        assert!(parse_bearer_challenge(r#"Basic realm="x""#).is_none());
        assert!(parse_bearer_challenge("Bearer service=\"ghcr.io\"").is_none());
        // no realm
    }

    #[tokio::test]
    async fn fetcher_constructible_with_https_url() {
        let url = Url::parse("https://ghcr.io").unwrap();
        let fetcher = UpstreamFetcher::new(url);
        assert!(fetcher.is_ok());
    }

    #[tokio::test]
    async fn fetcher_constructible_with_localhost() {
        let url = Url::parse("http://127.0.0.1:8080").unwrap();
        let fetcher = UpstreamFetcher::new(url);
        assert!(fetcher.is_ok());
    }
}
