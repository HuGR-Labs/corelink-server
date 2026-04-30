//! Adapter configuration + builder.
//!
//! [`ClerkConfig`] enforces the canonical bounds documented in
//! WI-S03-001 §9.2 (JWKS TTL 24h), §9.3 (clock skew 60s), §9.4
//! (issuer allowlist, exact match), and the HTTPS-only fetch
//! constraint in §6.1.2. Construction is via the builder so future
//! fields (multi-issuer-per-tenant, S-14) extend without breaking
//! callers.

use std::time::Duration;

use thiserror::Error;

/// Canonical JWKS cache TTL — 24 hours (86_400 s) per WI §9.2.
pub const JWKS_TTL_SECS: u64 = 24 * 60 * 60;

/// Canonical clock-skew leeway — 60 seconds per WI §9.3 / RFC 7519
/// §4.1.4 industry standard.
pub const LEEWAY_SECS: u64 = 60;

/// Hard upper bound on leeway (WI anti-scope §7: "Clock-skew > 120s").
const MAX_LEEWAY_SECS: u64 = 120;

/// Hard upper bound on JWKS cache TTL (defensive — prevents
/// configuration drift to a stale-key window > 48h per WI §28 R-008
/// SEV-2 boundary).
const MAX_JWKS_TTL_SECS: u64 = 48 * 60 * 60;

/// Adapter configuration; immutable post-construction.
#[derive(Clone, Debug)]
pub struct ClerkConfig {
    pub(crate) jwks_url: String,
    pub(crate) issuer_allowlist: Vec<String>,
    pub(crate) audience: String,
    pub(crate) leeway: Duration,
    pub(crate) jwks_ttl: Duration,
    pub(crate) instance_hash: String,
}

/// Errors surfaced during config construction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClerkConfigError {
    /// `jwks_url` was empty.
    #[error("jwks_url is required")]
    JwksUrlMissing,
    /// `jwks_url` did not start with `https://` (WI §6.1.2 + §32 anti).
    #[error("jwks_url must be HTTPS (got {got})")]
    JwksUrlNotHttps {
        /// Scheme-prefixed URL the caller supplied (truncated for logs).
        got: String,
    },
    /// `audience` was empty.
    #[error("audience is required")]
    AudienceMissing,
    /// `issuer_allowlist` was empty (WI §9.4 mandates at least one).
    #[error("issuer_allowlist must contain at least one entry")]
    IssuerAllowlistEmpty,
    /// `issuer_allowlist` contained an empty / non-HTTPS entry.
    #[error("invalid issuer entry: {0}")]
    IssuerInvalid(&'static str),
    /// `leeway` exceeded the canonical bound (WI anti-scope §7).
    #[error("leeway {got_secs}s > maximum {MAX_LEEWAY_SECS}s")]
    LeewayTooLarge {
        /// Caller-supplied leeway in seconds.
        got_secs: u64,
    },
    /// `jwks_ttl` exceeded the canonical bound (defensive).
    #[error("jwks_ttl {got_secs}s > maximum {MAX_JWKS_TTL_SECS}s")]
    JwksTtlTooLarge {
        /// Caller-supplied TTL in seconds.
        got_secs: u64,
    },
}

impl ClerkConfig {
    /// Start a new builder with canonical defaults
    /// (`leeway = 60s`, `jwks_ttl = 24h`).
    #[must_use]
    pub fn builder() -> ClerkConfigBuilder {
        ClerkConfigBuilder::default()
    }

    /// Canonical JWKS endpoint URL (HTTPS-validated at build).
    #[must_use]
    pub fn jwks_url(&self) -> &str {
        &self.jwks_url
    }

    /// Canonical issuer allowlist (exact match per WI §9.4).
    #[must_use]
    pub fn issuer_allowlist(&self) -> &[String] {
        &self.issuer_allowlist
    }

    /// Canonical audience claim (exact match per WI §6.1.3).
    #[must_use]
    pub fn audience(&self) -> &str {
        &self.audience
    }

    /// Canonical leeway (WI §9.3, ≤ 60s).
    #[must_use]
    pub fn leeway(&self) -> Duration {
        self.leeway
    }

    /// Canonical JWKS cache TTL (WI §9.2, 24h).
    #[must_use]
    pub fn jwks_ttl(&self) -> Duration {
        self.jwks_ttl
    }

    /// Canonical KV cache key suffix per WI §1: `clerk:jwks:<instance_hash>`.
    /// Derived deterministically from `(jwks_url ‖ audience)` so distinct
    /// Clerk instances (staging vs prod) get distinct cache slots.
    #[must_use]
    pub fn instance_hash(&self) -> &str {
        &self.instance_hash
    }
}

/// Builder for [`ClerkConfig`].
#[derive(Debug, Default)]
pub struct ClerkConfigBuilder {
    jwks_url: Option<String>,
    issuer_allowlist: Option<Vec<String>>,
    audience: Option<String>,
    leeway: Option<Duration>,
    jwks_ttl: Option<Duration>,
}

impl ClerkConfigBuilder {
    /// Set the JWKS endpoint URL (HTTPS only).
    #[must_use]
    pub fn jwks_url(mut self, url: impl Into<String>) -> Self {
        self.jwks_url = Some(url.into());
        self
    }

    /// Set the issuer allowlist (at least one entry; HTTPS only;
    /// trailing slashes are stripped for canonical comparison).
    #[must_use]
    pub fn issuer_allowlist<I, S>(mut self, issuers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.issuer_allowlist = Some(
            issuers
                .into_iter()
                .map(|s| s.into().trim_end_matches('/').to_owned())
                .collect(),
        );
        self
    }

    /// Set the canonical audience claim (exact match).
    #[must_use]
    pub fn audience(mut self, aud: impl Into<String>) -> Self {
        self.audience = Some(aud.into());
        self
    }

    /// Override leeway (defaults to 60s; bounded ≤ 120s).
    #[must_use]
    pub fn leeway(mut self, leeway: Duration) -> Self {
        self.leeway = Some(leeway);
        self
    }

    /// Override JWKS TTL (defaults to 24h; bounded ≤ 48h).
    #[must_use]
    pub fn jwks_ttl(mut self, ttl: Duration) -> Self {
        self.jwks_ttl = Some(ttl);
        self
    }

    /// Validate inputs and finalise the config.
    pub fn build(self) -> Result<ClerkConfig, ClerkConfigError> {
        let jwks_url = self.jwks_url.ok_or(ClerkConfigError::JwksUrlMissing)?;
        if jwks_url.is_empty() {
            return Err(ClerkConfigError::JwksUrlMissing);
        }
        if !jwks_url.starts_with("https://") {
            return Err(ClerkConfigError::JwksUrlNotHttps {
                got: truncate(&jwks_url, 64),
            });
        }
        let audience = self.audience.ok_or(ClerkConfigError::AudienceMissing)?;
        if audience.is_empty() {
            return Err(ClerkConfigError::AudienceMissing);
        }
        let issuer_allowlist = self
            .issuer_allowlist
            .ok_or(ClerkConfigError::IssuerAllowlistEmpty)?;
        if issuer_allowlist.is_empty() {
            return Err(ClerkConfigError::IssuerAllowlistEmpty);
        }
        for iss in &issuer_allowlist {
            if iss.is_empty() {
                return Err(ClerkConfigError::IssuerInvalid("empty entry"));
            }
            if !iss.starts_with("https://") {
                return Err(ClerkConfigError::IssuerInvalid("issuer must be HTTPS"));
            }
        }
        let leeway = self.leeway.unwrap_or(Duration::from_secs(LEEWAY_SECS));
        if leeway.as_secs() > MAX_LEEWAY_SECS {
            return Err(ClerkConfigError::LeewayTooLarge {
                got_secs: leeway.as_secs(),
            });
        }
        let jwks_ttl = self.jwks_ttl.unwrap_or(Duration::from_secs(JWKS_TTL_SECS));
        if jwks_ttl.as_secs() > MAX_JWKS_TTL_SECS {
            return Err(ClerkConfigError::JwksTtlTooLarge {
                got_secs: jwks_ttl.as_secs(),
            });
        }
        let instance_hash = compute_instance_hash(&jwks_url, &audience);
        Ok(ClerkConfig {
            jwks_url,
            issuer_allowlist,
            audience,
            leeway,
            jwks_ttl,
            instance_hash,
        })
    }
}

/// Deterministic instance hash for KV cache key derivation. Distinct
/// `(jwks_url, audience)` pairs map to distinct cache slots; collisions
/// would require a SHA-256 preimage.
fn compute_instance_hash(jwks_url: &str, audience: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"corelink/v1/clerk-instance\x00");
    hasher.update(jwks_url.as_bytes());
    hasher.update(b"\x00");
    hasher.update(audience.as_bytes());
    let digest = hasher.finalize();
    let first8: [u8; 8] = digest.get(..8).and_then(|s| s.try_into().ok()).unwrap_or_default();
    hex::encode(first8)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        let mut out = String::with_capacity(max + 3);
        // Walk by char_indices so we never split a UTF-8 boundary.
        let mut last_end = 0;
        for (idx, _ch) in s.char_indices() {
            if idx > max {
                break;
            }
            last_end = idx;
        }
        out.push_str(s.get(..last_end).unwrap_or(""));
        out.push_str("...");
        out
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test-only"
)]
mod tests {
    use super::*;

    #[test]
    fn build_valid() {
        let cfg = ClerkConfig::builder()
            .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["https://clerk.example.dev"])
            .audience("corelink-api")
            .build()
            .unwrap();
        assert_eq!(cfg.audience(), "corelink-api");
        assert_eq!(cfg.leeway(), Duration::from_secs(60));
        assert_eq!(cfg.jwks_ttl(), Duration::from_secs(86_400));
        assert_eq!(cfg.instance_hash().len(), 16);
    }

    #[test]
    fn rejects_http_jwks_url() {
        let err = ClerkConfig::builder()
            .jwks_url("http://insecure.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["https://clerk.example.dev"])
            .audience("corelink-api")
            .build()
            .unwrap_err();
        assert!(matches!(err, ClerkConfigError::JwksUrlNotHttps { .. }));
    }

    #[test]
    fn rejects_empty_audience() {
        let err = ClerkConfig::builder()
            .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["https://clerk.example.dev"])
            .audience("")
            .build()
            .unwrap_err();
        assert_eq!(err, ClerkConfigError::AudienceMissing);
    }

    #[test]
    fn rejects_empty_issuer_allowlist() {
        let err = ClerkConfig::builder()
            .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
            .issuer_allowlist::<_, String>([])
            .audience("corelink-api")
            .build()
            .unwrap_err();
        assert_eq!(err, ClerkConfigError::IssuerAllowlistEmpty);
    }

    #[test]
    fn rejects_http_issuer() {
        let err = ClerkConfig::builder()
            .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["http://clerk.example.dev"])
            .audience("corelink-api")
            .build()
            .unwrap_err();
        assert_eq!(err, ClerkConfigError::IssuerInvalid("issuer must be HTTPS"));
    }

    #[test]
    fn rejects_oversized_leeway() {
        let err = ClerkConfig::builder()
            .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["https://clerk.example.dev"])
            .audience("corelink-api")
            .leeway(Duration::from_secs(121))
            .build()
            .unwrap_err();
        assert!(matches!(err, ClerkConfigError::LeewayTooLarge { .. }));
    }

    #[test]
    fn issuer_trailing_slash_normalised() {
        let cfg = ClerkConfig::builder()
            .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["https://clerk.example.dev/"])
            .audience("corelink-api")
            .build()
            .unwrap();
        assert_eq!(cfg.issuer_allowlist(), &["https://clerk.example.dev"]);
    }

    #[test]
    fn instance_hash_is_deterministic_and_distinct_per_instance() {
        let a = ClerkConfig::builder()
            .jwks_url("https://clerk.staging.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["https://clerk.staging.example.dev"])
            .audience("corelink-api")
            .build()
            .unwrap();
        let b = ClerkConfig::builder()
            .jwks_url("https://clerk.prod.example.dev/.well-known/jwks.json")
            .issuer_allowlist(["https://clerk.prod.example.dev"])
            .audience("corelink-api")
            .build()
            .unwrap();
        assert_ne!(a.instance_hash(), b.instance_hash());
    }
}
