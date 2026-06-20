//! Upstream bottle fetcher (reqwest).
//!
//! Performs a plain HTTPS `GET <upstream_domain>/<canonical_path>` and
//! returns the body bytes. Enforces the configured bottle-size cap.

use std::net::IpAddr;

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

        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|err| BrewAdapterError::Upstream(format!("send: {err}")))?;

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
}

/// Copy `chunk` into `buf`. Wrapped in a tiny helper to keep the
/// hot-path call site readable.
fn extend_from_bytes(buf: &mut Vec<u8>, chunk: &Bytes) {
    buf.extend_from_slice(chunk);
}

/// True when `host` is a literal IP address in a range that must never be
/// reachable from an outbound upstream fetch (the classic SSRF targets:
/// loopback, RFC-1918 / RFC-4193 private space, link-local — which includes
/// the 169.254.169.254 cloud-metadata endpoint — and the unspecified
/// address). DNS host names are NOT classified here: they are resolved by
/// the OS at connect time and the danger surface this guard closes is a
/// redirect `Location` pointing straight at an internal IP literal.
fn host_is_internal_ip(host: &str) -> bool {
    // `url` hands IPv6 hosts back WITHOUT the surrounding brackets.
    let stripped = host.strip_prefix('[').and_then(|h| h.strip_suffix(']'));
    let candidate = stripped.unwrap_or(host);
    let Ok(ip) = candidate.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => {
            // Destructure (no indexing — `indexing_slicing` is denied).
            let [a, b, _, _] = v4.octets();
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                // Carrier-grade NAT 100.64.0.0/10 — internal-ish, deny.
                || (a == 100 && (b & 0xc0) == 0x40)
        }
        IpAddr::V6(v6) => {
            let [s0, ..] = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique-local fc00::/7.
                || (s0 & 0xfe00) == 0xfc00
                // Link-local fe80::/10.
                || (s0 & 0xffc0) == 0xfe80
                // IPv4-mapped / -compatible: re-classify the embedded v4.
                || v6.to_ipv4().is_some_and(|m| {
                    m.is_private() || m.is_loopback() || m.is_link_local() || m.is_unspecified()
                })
        }
    }
}

/// Build a redirect policy that follows up to `max` redirects but REFUSES any
/// hop whose `Location` resolves to an internal IP literal (SSRF guard, see
/// [`host_is_internal_ip`]). reqwest's DEFAULT policy follows up to 10 hops to
/// ANY host, which would let a first-hop upstream bounce the request at an
/// RFC-1918 / metadata address; this policy closes that on EVERY hop while
/// still permitting the legitimate ghcr → public-CDN 307.
fn ssrf_safe_redirect_policy(max: usize) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= max {
            return attempt.stop();
        }
        match attempt.url().host_str() {
            Some(host) if host_is_internal_ip(host) => attempt.stop(),
            _ => attempt.follow(),
        }
    })
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
    fn internal_ip_hosts_are_classified_ssrf() {
        // Loopback, RFC-1918, link-local (incl. cloud metadata), unspecified.
        for h in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "[::1]",
            "[fc00::1]",
            "[fe80::1]",
            "[::ffff:127.0.0.1]",
        ] {
            assert!(host_is_internal_ip(h), "{h} must be flagged internal");
        }
    }

    #[test]
    fn public_hosts_and_names_are_not_internal() {
        for h in ["ghcr.io", "1.1.1.1", "8.8.8.8", "[2606:4700::1111]"] {
            assert!(!host_is_internal_ip(h), "{h} must NOT be flagged internal");
        }
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
