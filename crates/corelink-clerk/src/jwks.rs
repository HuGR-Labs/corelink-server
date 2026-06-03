//! JWKS document model + fetcher trait.
//!
//! The crate intentionally does NOT depend on Cloudflare's `worker`
//! crate — the production HTTPS fetch wraps `worker::Fetch` inside
//! `corelink-worker` (S-03 wiring) and consumes a [`JwksFetcher`]
//! impl from this crate. Tests use [`crate::fakes::StaticJwksFetcher`]
//! and [`crate::fakes::ScriptedJwksFetcher`].
//!
//! Only RS256 keys are surfaced — anything else is filtered at parse
//! time so the decoder cannot accidentally see an HS / EC key
//! (defense in depth on top of the explicit `Algorithm::RS256`
//! validation already enforced at the `jsonwebtoken` boundary).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Deserialize;
use thiserror::Error;

/// Canonical JWKS document — vector of RS256 keys keyed by `kid`.
#[derive(Clone, Debug, Default)]
pub struct Jwks {
    keys: Vec<JwksKey>,
}

/// Single RS256 signing key extracted from JWKS.
#[derive(Clone, Debug)]
pub struct JwksKey {
    /// Key id (`kid` JOSE header parameter).
    pub kid: String,
    /// Modulus, base64url-no-pad.
    pub n_b64url: String,
    /// Exponent, base64url-no-pad (typically `AQAB`).
    pub e_b64url: String,
}

/// Errors surfaced by [`Jwks::parse`] / [`JwksFetcher`].
#[derive(Debug, Error)]
pub enum JwksFetchError {
    /// Underlying network / TLS / HTTP error.
    #[error("transport error: {0}")]
    Transport(String),
    /// HTTP response was non-2xx.
    #[error("HTTP {status} from JWKS endpoint")]
    HttpStatus {
        /// HTTP status code surfaced.
        status: u16,
    },
    /// Response was not valid JSON / not a JWKS-shape doc.
    #[error("malformed JWKS: {0}")]
    Malformed(String),
}

impl Jwks {
    /// Parse a JWKS JSON document. Filters to RS256 / use=sig keys
    /// only; any other algorithm or use surface is silently dropped.
    pub fn parse(raw: &[u8]) -> Result<Self, JwksFetchError> {
        let doc: JwksDoc = serde_json::from_slice(raw)
            .map_err(|e| JwksFetchError::Malformed(format!("json parse: {e}")))?;
        let mut out = Vec::with_capacity(doc.keys.len());
        for jwk in doc.keys {
            // Algorithm: RS256 only. `kty` for RSA must be "RSA". `use`
            // optional — when present, must be "sig".
            if jwk.alg.as_deref() != Some("RS256") {
                continue;
            }
            if jwk.kty.as_deref() != Some("RSA") {
                continue;
            }
            if let Some(usage) = jwk.use_.as_deref() {
                if usage != "sig" {
                    continue;
                }
            }
            let (kid, n, e) = match (jwk.kid, jwk.n, jwk.e) {
                (Some(kid), Some(n), Some(e))
                    if !kid.is_empty() && !n.is_empty() && !e.is_empty() =>
                {
                    (kid, n, e)
                }
                _ => continue,
            };
            out.push(JwksKey {
                kid,
                n_b64url: n,
                e_b64url: e,
            });
        }
        Ok(Self { keys: out })
    }

    /// All known RS256 keys.
    #[must_use]
    pub fn keys(&self) -> &[JwksKey] {
        &self.keys
    }

    /// Lookup a key by `kid`. Returns `None` if absent — caller is
    /// responsible for the lazy-refresh + retry semantics.
    #[must_use]
    pub fn find(&self, kid: &str) -> Option<&JwksKey> {
        self.keys.iter().find(|k| k.kid == kid)
    }

    /// Build a Jwks from explicit keys (test / fake helper).
    #[must_use]
    pub fn from_keys(keys: Vec<JwksKey>) -> Self {
        Self { keys }
    }

    /// Serialise to canonical JWKS JSON form
    /// (`{"keys":[{kid,kty,alg,use,n,e},…]}`). Used by fakes when
    /// emitting a "fetch result" body.
    #[must_use]
    pub fn to_json(&self) -> String {
        let serialised: Vec<JwkOut> = self
            .keys
            .iter()
            .map(|k| JwkOut {
                kid: &k.kid,
                kty: "RSA",
                alg: "RS256",
                use_: "sig",
                n: &k.n_b64url,
                e: &k.e_b64url,
            })
            .collect();
        let doc = JwksOut { keys: serialised };
        serde_json::to_string(&doc).unwrap_or_else(|_| "{\"keys\":[]}".to_owned())
    }
}

#[derive(Deserialize)]
struct JwksDoc {
    keys: Vec<JwkRaw>,
}

#[derive(Deserialize)]
struct JwkRaw {
    kid: Option<String>,
    kty: Option<String>,
    alg: Option<String>,
    #[serde(rename = "use")]
    use_: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

#[derive(serde::Serialize)]
struct JwksOut<'a> {
    keys: Vec<JwkOut<'a>>,
}

#[derive(serde::Serialize)]
struct JwkOut<'a> {
    kid: &'a str,
    kty: &'a str,
    alg: &'a str,
    #[serde(rename = "use")]
    use_: &'a str,
    n: &'a str,
    e: &'a str,
}

/// Type alias for a boxed JWKS-fetch future. Hand-rolled to avoid a
/// proc-macro `async-trait` dependency on the crate's MSRV (1.80).
pub type JwksFetchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Jwks, JwksFetchError>> + Send + 'a>>;

/// Trait abstracting the HTTPS JWKS fetch.
///
/// The production impl wraps `worker::Fetch::send` (Workers runtime)
/// inside `corelink-worker` shim. Tests use the in-memory fakes in
/// [`crate::fakes`].
pub trait JwksFetcher: Send + Sync + 'static {
    /// Fetch a JWKS document from `url`.
    fn fetch<'a>(&'a self, url: &'a str) -> JwksFetchFuture<'a>;
}

/// Convenience: shared fetcher behind an `Arc`.
pub type SharedJwksFetcher = Arc<dyn JwksFetcher>;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test-only"
)]
mod tests {
    use super::*;

    fn jwk(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "kid": kid,
            "kty": "RSA",
            "alg": "RS256",
            "use": "sig",
            "n": "abcdef",
            "e": "AQAB"
        })
    }

    #[test]
    fn parses_canonical_jwks() {
        let body = serde_json::json!({"keys": [jwk("k1"), jwk("k2")]}).to_string();
        let jwks = Jwks::parse(body.as_bytes()).unwrap();
        assert_eq!(jwks.keys().len(), 2);
        assert!(jwks.find("k1").is_some());
        assert!(jwks.find("missing").is_none());
    }

    #[test]
    fn filters_non_rs256() {
        let body = serde_json::json!({"keys": [
            {"kid": "k1", "kty": "RSA", "alg": "HS256", "use": "sig", "n": "abc", "e": "AQAB"},
            {"kid": "k2", "kty": "EC", "alg": "ES256", "use": "sig", "n": "abc", "e": "AQAB"},
            jwk("k3"),
        ]})
        .to_string();
        let jwks = Jwks::parse(body.as_bytes()).unwrap();
        assert_eq!(jwks.keys().len(), 1);
        assert!(jwks.find("k3").is_some());
    }

    #[test]
    fn rejects_malformed_json() {
        let err = Jwks::parse(b"not-a-jwks").unwrap_err();
        assert!(matches!(err, JwksFetchError::Malformed(_)));
    }

    #[test]
    fn round_trips_to_json() {
        let body = serde_json::json!({"keys": [jwk("k1")]}).to_string();
        let jwks = Jwks::parse(body.as_bytes()).unwrap();
        let serialised = jwks.to_json();
        let reparsed = Jwks::parse(serialised.as_bytes()).unwrap();
        assert_eq!(reparsed.keys().len(), 1);
        assert_eq!(reparsed.find("k1").unwrap().n_b64url, "abcdef");
    }
}
