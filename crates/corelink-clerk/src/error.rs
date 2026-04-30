//! Canonical [`AuthError`] taxonomy (WI-S03-001 §1).
//!
//! Variants map 1:1 to the error model documented in the WI; downstream
//! middleware (WI-S03-003) is responsible for translating these into
//! HTTP status codes per WI §23 (e.g. `SignatureInvalid` → 401
//! `invalid_token`, `JwksFetchFailed` → 503 `auth_unavailable`).

use thiserror::Error;

/// Errors surfaced by [`crate::ClerkAdapter::validate`].
#[derive(Debug, Error)]
pub enum AuthError {
    /// JWT signature did not verify against the resolved JWKS key.
    #[error("JWT signature invalid")]
    SignatureInvalid,

    /// Token is past `exp` (with leeway already applied).
    #[error("JWT expired (now > exp + leeway)")]
    Expired,

    /// Token's `nbf` claim is in the future (with leeway already applied).
    #[error("JWT not yet valid (now < nbf - leeway)")]
    NotYetValid,

    /// Issuer is not in the configured allowlist (exact match).
    #[error("issuer mismatch (got {got}, expected one of {expected:?})")]
    IssuerMismatch {
        /// Issuer claim presented by the token.
        got: String,
        /// Configured allowlist of accepted issuers.
        expected: Vec<String>,
    },

    /// Audience claim does not match configured audience (exact match).
    #[error("audience mismatch")]
    AudienceMismatch,

    /// JWKS fetch failed (network error, TLS error, malformed JSON, …).
    #[error("JWKS fetch failed: {0}")]
    JwksFetchFailed(String),

    /// `kid` from JWT header was not present in the (refreshed) JWKS.
    #[error("KID not in JWKS")]
    KidNotInJwks,

    /// Token did not parse as a JWT at all (malformed header / segments).
    #[error("malformed JWT: {0}")]
    Malformed(String),

    /// Token used an algorithm outside the canonical `{RS256}` allowlist.
    /// Captured separately from [`AuthError::SignatureInvalid`] only at
    /// the decoder layer so test harnesses can assert the exact reason
    /// for an `alg=none` regression; downstream middleware MUST treat
    /// this as 401 `invalid_token` (same severity as
    /// [`AuthError::SignatureInvalid`]).
    #[error("disallowed algorithm")]
    AlgNotAllowed,
}

impl AuthError {
    /// Outcome label for `corelink_auth_clerk_validate_total{outcome=…}`
    /// metric per WI §6.1.6. Allowed values per WI §6.1.6 spec text:
    /// `ok | sig_invalid | expired | aud_mismatch | iss_mismatch |
    /// kid_miss | jwks_fetch_failed`. Variants outside that allowlist
    /// fold to the closest canonical value (Malformed → `sig_invalid`,
    /// AlgNotAllowed → `sig_invalid`, NotYetValid → `expired`).
    #[must_use]
    pub fn metric_outcome(&self) -> &'static str {
        match self {
            AuthError::SignatureInvalid | AuthError::Malformed(_) | AuthError::AlgNotAllowed => {
                "sig_invalid"
            }
            AuthError::Expired | AuthError::NotYetValid => "expired",
            AuthError::IssuerMismatch { .. } => "iss_mismatch",
            AuthError::AudienceMismatch => "aud_mismatch",
            AuthError::JwksFetchFailed(_) => "jwks_fetch_failed",
            AuthError::KidNotInJwks => "kid_miss",
        }
    }
}
