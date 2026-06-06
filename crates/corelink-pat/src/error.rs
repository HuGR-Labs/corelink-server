//! Error taxonomy for the `corelink-pat` crate.
//!
//! Every fallible surface in this crate maps to a single concrete
//! [`PatError`] variant. The taxonomy is intentionally narrow so that
//! the downstream Tower middleware (WI-S03-003) can map errors to HTTP
//! / gRPC status codes without ambiguity:
//!
//! | Variant | Wire mapping (downstream) |
//! |---|---|
//! | [`PatError::Malformed`] | HTTP 401 `invalid_token_format` |
//! | [`PatError::InvalidPat`] | HTTP 401 `invalid_token` (constant-time response; same body as not-found) |
//! | [`PatError::HashError`] | HTTP 500 `internal_error` (DB corruption — PHC string unparseable) |
//! | [`PatError::EntropyUnavailable`] | HTTP 503 `service_unavailable` (CSPRNG miss; cold-start Worker issue) |
//! | [`PatError::SigningKeyTooShort`] | HTTP 500 `internal_error` (deploy-time misconfig) |
//!
//! Each variant carries no PII and no plaintext token material; every
//! `Display` impl is safe to log directly.

use thiserror::Error;

/// Canonical error type surfaced by the `corelink-pat` crate.
#[derive(Debug, Error)]
pub enum PatError {
    /// PAT plaintext does not match the canonical
    /// `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` shape, the
    /// `<env>` segment is not one of `pat | ci | ro`, or one of the
    /// base64url segments has a non-canonical length / illegal byte.
    /// Every shape mismatch folds to this single variant so callers
    /// cannot use the error to discriminate "wrong env" vs "wrong
    /// length" via timing or branching.
    #[error(
        "PAT format invalid (expected `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`)"
    )]
    Malformed,

    /// HMAC signature mismatch OR Argon2id verify mismatch. Returned
    /// without distinction so the wire response cannot be used as an
    /// existence oracle (the middleware additionally executes
    /// [`crate::dummy_verify_for_constant_time`] on the cold path so
    /// even "token_id absent in DB" surfaces with the same latency
    /// envelope as a real Argon2id mismatch).
    #[error("PAT verify failed")]
    InvalidPat,

    /// The stored `PatHash` PHC string is unparseable — indicates DB
    /// corruption or accidental column truncation. NOT used for
    /// "wrong password"; `InvalidPat` covers that path. The contained
    /// string is a category label, never the offending PHC payload.
    #[error("PAT hash error: {0}")]
    HashError(&'static str),

    /// `getrandom` failed to draw entropy from the OS / Web Crypto.
    /// CSPRNG missing is a fail-safe path for crypto code: caller
    /// returns 503, customer retries, deploy team investigates.
    #[error("entropy source unavailable")]
    EntropyUnavailable,

    /// HMAC signing key has fewer than 32 bytes — rejected at
    /// construction time so a downgraded key never reaches the
    /// HMAC-SHA256 boundary.
    #[error("HMAC signing key must be ≥ 32 bytes")]
    SigningKeyTooShort,
}
