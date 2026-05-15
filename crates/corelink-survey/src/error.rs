//! Error taxonomy for the survey crate.
//!
//! Every variant carries a stable `code()` string so downstream metric +
//! audit pipelines can pivot on it without leaking the human-readable
//! `Display` to a SIEM dashboard.

use thiserror::Error;

/// Top-level error returned by every public entry point in this crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SurveyError {
    /// Token decode failed: the URL-safe base64 segment was malformed, or
    /// the inner JSON payload could not be parsed. Returned **before**
    /// any signature comparison so the recorder doesn't leak structural
    /// information about the token to an unauthenticated caller.
    #[error("survey token malformed")]
    MalformedToken,

    /// HMAC signature did not match. Returned via constant-time compare.
    #[error("survey token signature invalid")]
    InvalidSignature,

    /// Token expired (`now_ms >= token.expires_at_ms`). The HMAC was
    /// otherwise valid; the recorder rejects the response cleanly.
    #[error("survey token expired")]
    TokenExpired,

    /// Token was already used. The first `record` call succeeded; this
    /// second call is the replay attempt. Caller MUST treat this as a
    /// suspicious-input signal and feed it into the abuse pipeline.
    #[error("survey token replay rejected")]
    ReplayRejected,

    /// The response payload's variant does not match the survey kind
    /// encoded in the token (e.g. recipient sent NPS but the survey was
    /// CSAT). Possible attack: tampering the URL after token decode.
    #[error("survey response kind mismatch")]
    KindMismatch,

    /// NPS / CSAT score outside the valid range, or free-text exceeds
    /// the per-survey length cap, or multi-choice option set exceeds the
    /// bounded cardinality / per-option byte cap.
    #[error("survey response value invalid: {0}")]
    InvalidResponse(&'static str),

    /// Audit emit failed. The recorder DID NOT insert the response row
    /// (fail-CLOSED). Caller MUST translate to 503 so a downstream
    /// monitor catches the gap.
    #[error("survey audit emit failed: {0}")]
    AuditEmitFailed(String),

    /// Internal storage backend failed. The recorder's invariant is that
    /// audit emit completed BEFORE storage write was attempted; if this
    /// error fires, the audit shows an attempted-record event without a
    /// corresponding success — operators can reconcile manually.
    #[error("survey storage backend error: {0}")]
    Storage(String),

    /// The signing key has illegal length (must be exactly 32 bytes).
    #[error("survey signing key length invalid (need 32 bytes)")]
    SigningKeyLength,

    /// Recipient hash is malformed (not 32 bytes of hex-encoded SHA-256).
    #[error("survey recipient hash malformed")]
    MalformedRecipientHash,
}

impl SurveyError {
    /// Stable machine-readable code suitable for metric labels + SIEM
    /// pivots. Never includes free-form payload data.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MalformedToken => "malformed_token",
            Self::InvalidSignature => "invalid_signature",
            Self::TokenExpired => "token_expired",
            Self::ReplayRejected => "replay_rejected",
            Self::KindMismatch => "kind_mismatch",
            Self::InvalidResponse(_) => "invalid_response",
            Self::AuditEmitFailed(_) => "audit_emit_failed",
            Self::Storage(_) => "storage",
            Self::SigningKeyLength => "signing_key_length",
            Self::MalformedRecipientHash => "malformed_recipient_hash",
        }
    }
}
