//! Upstream bottle fetcher (reqwest).
//!
//! Performs a plain HTTPS `GET <upstream_domain>/<canonical_path>` and
//! returns the body bytes. Enforces the configured bottle-size cap.

use bytes::Bytes;
use url::Url;

use crate::error::BrewAdapterError;

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
            .user_agent(concat!(
                "corelink-adapter-brew/",
                env!("CARGO_PKG_VERSION")
            ))
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
        let url = self
            .upstream_domain
            .join(canonical_path)
            .map_err(|err| BrewAdapterError::Upstream(format!("url join: {err}")))?;

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
            let new_total_u64 =
                u64::try_from(new_total).unwrap_or(u64::MAX);
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
