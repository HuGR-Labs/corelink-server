//! Canonical [`AuthMiddlewareError`] taxonomy (WI-S03-003 §23).
//!
//! Each variant maps 1:1 to a `COR_AUTH_*` wire error code per the WI's
//! HTTP error mapping table; the canonical translation lives on
//! [`AuthMiddlewareError::error_code`] and downstream HTTP / gRPC
//! adapters call that to surface the wire-stable label.
//!
//! The taxonomy is intentionally narrow: every cryptographic failure
//! (parse mismatch, signature mismatch, expired-token, KID-not-found,
//! Argon2id mismatch) folds to the same [`AuthMiddlewareError::InvalidToken`]
//! variant so the wire response cannot be used as an existence oracle.
//! The header-shape errors (`HeaderMissing`, `HeaderMalformed`,
//! `HeaderAmbiguous`) survive as distinct variants because the
//! presence-vs-absence of an `Authorization` header is observable to
//! the network in any case.

use thiserror::Error;

/// Canonical error type surfaced by the auth middleware (WI-S03-003).
///
/// Variants map 1:1 to the `COR_AUTH_*` wire error codes per
/// `auth_model.md §6` + WI-S03-003 §23. Downstream HTTP / gRPC error
/// adapters call [`AuthMiddlewareError::error_code`] to obtain the
/// wire-stable label.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum AuthMiddlewareError {
    /// `Authorization` header absent. 401 `COR_AUTH_HEADER_MISSING`.
    #[error("authorization header missing")]
    HeaderMissing,

    /// `Authorization` header is present but does not parse as
    /// `Bearer <token>` (RFC 6750). 401 `COR_AUTH_HEADER_MALFORMED`.
    #[error("authorization header malformed (expected `Bearer <token>`)")]
    HeaderMalformed,

    /// Bearer payload matches neither the canonical PAT prefix
    /// `corelink_<env>_*` nor the JWT base64url shape (3 dot-separated
    /// segments). 401 `COR_AUTH_HEADER_AMBIGUOUS`.
    #[error("authorization payload not recognisably PAT nor JWT")]
    HeaderAmbiguous,

    /// Multi-value `Authorization` header (e.g. comma-separated). RFC
    /// 6750 §2.1 mandates single token; we reject defensively rather
    /// than picking. 401 `COR_AUTH_HEADER_MALFORMED` (folded).
    #[error("multiple authorization values not supported")]
    HeaderMultiValue,

    /// Token is structurally well-formed but failed cryptographic
    /// verification (PAT signature OR JWT signature OR Argon2id
    /// mismatch). The downstream wire response is uniform — same body
    /// as `COR_AUTH_TOKEN_EXPIRED` — so an attacker cannot use the
    /// distinction as an oracle. 401 `COR_AUTH_INVALID_TOKEN`.
    #[error("token verification failed")]
    InvalidToken,

    /// Token validated cryptographically but is past `exp`
    /// (JWT path) — surfaced separately because the canonical wire
    /// taxonomy distinguishes the two. PAT path NEVER produces
    /// `Expired` here: PAT expiry is enforced by the authoritative
    /// DB (Neon `pat.expires_at`), surfaced via the verifier callback
    /// as [`Self::InvalidToken`]. 401 `COR_AUTH_TOKEN_EXPIRED`.
    #[error("token expired")]
    TokenExpired,

    /// Authenticated principal lacks at least one required scope.
    /// 403 `COR_AUTH_SCOPE_INSUFFICIENT`.
    #[error("scope insufficient (required {required:?}, granted {granted:?})")]
    ScopeInsufficient {
        /// Canonical wire-form scope literal that was required.
        required: &'static str,
        /// Snapshot of canonical wire-form scopes that were granted.
        granted: Vec<&'static str>,
    },

    /// Authenticated identity references a tenant that has not been
    /// provisioned in the canonical Neon `tenant` table. Surfaces
    /// during JWT validation when `org_id` claim cannot be resolved.
    /// 412 `COR_AUTH_TENANT_NOT_PROVISIONED`.
    #[error("tenant not provisioned")]
    TenantNotProvisioned,

    /// Cross-tenant header smuggling attempt detected. The middleware
    /// observed an explicit tenant override header
    /// (`X-Corelink-Tenant-Id`, `X-Tenant-Id`, etc.) that disagrees
    /// with the verified principal's tenant binding. 401
    /// `COR_AUTH_INVALID_TOKEN` on the wire (folded so the audit
    /// stream still sees the discriminator while the attacker gets
    /// the uniform 401). The named variant survives so the audit
    /// outbox emits a SEV-1 `corelink.auth.cross_tenant_header_attempt`
    /// CloudEvent instead of a benign `auth.invalid` event.
    #[error("cross-tenant header smuggling rejected")]
    CrossTenantHeaderRejected,

    /// Auth backend (Clerk JWKS endpoint OR PAT verifier subsystem)
    /// is unavailable. 503 `COR_AUTH_BACKEND_UNAVAILABLE` with
    /// `Retry-After: 5s`.
    #[error("auth backend unavailable: {0}")]
    BackendUnavailable(&'static str),
}

impl AuthMiddlewareError {
    /// Canonical wire-stable error code (`COR_AUTH_*`).
    #[must_use]
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::HeaderMissing => "COR_AUTH_HEADER_MISSING",
            Self::HeaderMalformed | Self::HeaderMultiValue => "COR_AUTH_HEADER_MALFORMED",
            Self::HeaderAmbiguous => "COR_AUTH_HEADER_AMBIGUOUS",
            Self::InvalidToken | Self::CrossTenantHeaderRejected => "COR_AUTH_INVALID_TOKEN",
            Self::TokenExpired => "COR_AUTH_TOKEN_EXPIRED",
            Self::ScopeInsufficient { .. } => "COR_AUTH_SCOPE_INSUFFICIENT",
            Self::TenantNotProvisioned => "COR_AUTH_TENANT_NOT_PROVISIONED",
            Self::BackendUnavailable(_) => "COR_AUTH_BACKEND_UNAVAILABLE",
        }
    }

    /// Canonical wire-stable HTTP status code.
    #[must_use]
    pub fn http_status(&self) -> u16 {
        match self {
            Self::HeaderMissing
            | Self::HeaderMalformed
            | Self::HeaderMultiValue
            | Self::HeaderAmbiguous
            | Self::InvalidToken
            | Self::CrossTenantHeaderRejected
            | Self::TokenExpired => 401,
            Self::ScopeInsufficient { .. } => 403,
            Self::TenantNotProvisioned => 412,
            Self::BackendUnavailable(_) => 503,
        }
    }

    /// Canonical metric outcome label
    /// (`corelink.auth.middleware.requests_total{result=…}`).
    #[must_use]
    pub fn metric_outcome(&self) -> &'static str {
        match self {
            Self::HeaderMissing
            | Self::HeaderMalformed
            | Self::HeaderMultiValue
            | Self::HeaderAmbiguous => "header_invalid",
            Self::InvalidToken | Self::CrossTenantHeaderRejected => "invalid_token",
            Self::TokenExpired => "expired",
            Self::ScopeInsufficient { .. } => "scope_insufficient",
            Self::TenantNotProvisioned => "tenant_not_provisioned",
            Self::BackendUnavailable(_) => "backend_unavailable",
        }
    }
}
