//! Error types for [`Digest`](crate::Digest) parsing and
//! [`VerifiedBody`](crate::VerifiedBody) construction.

use thiserror::Error;

/// Hex-parse failure modes for [`Digest::from_hex`](crate::Digest::from_hex).
///
/// Surfaces the *category* of the failure (length vs alphabet) without
/// echoing back the offending input — server-side logs should never include
/// untrusted client digest strings verbatim, since correlated mismatches are
/// audit-relevant in their own right (poisoning attempts).
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// Hex string had the wrong length (expected exactly 64 ASCII hex chars
    /// for a 32-byte BLAKE3 digest).
    #[error("digest hex length must be 64 chars (32 bytes); got {0}")]
    InvalidLength(usize),

    /// Hex string contained a non-`[0-9a-fA-F]` byte.
    #[error("digest hex contained a non-hex byte at position {0}")]
    InvalidHexByte(usize),
}

/// Canonical error-taxonomy code surfaced by [`HashMismatch`].
///
/// Mirrors `error_taxonomy.md` entry `COR_CAS_DIGEST_MISMATCH` (HTTP 409,
/// `retryable=never`). Storage adapters and REAPI handlers map this code
/// onto the wire (HTTP status, gRPC code, customer message, next-action
/// hint) — the canonical mapping lives outside this crate to avoid
/// drift between transport-specific surfaces.
pub const COR_CAS_DIGEST_MISMATCH: &str = "COR_CAS_DIGEST_MISMATCH";

/// Returned when a body fails to verify against its claimed digest.
///
/// Carries no data — including the computed/claimed digests in the error
/// type would tempt callers to log them, and a poisoning attempt is a
/// security-audit event, not a debug log entry. The audit layer
/// (`observability_model.md §S-09`) is the single canonical sink for the
/// `corelink.cas.poisoning_attempt` event; the `Err` arm here only signals
/// "reject the request".
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error(
    "CAS digest mismatch — body BLAKE3 hash does not match the claimed digest \
     (COR_CAS_DIGEST_MISMATCH; do not retry with the same payload)"
)]
pub struct HashMismatch;

impl HashMismatch {
    /// Stable error-taxonomy code (`COR_CAS_DIGEST_MISMATCH`). Storage and
    /// REAPI layers use this when wiring HTTP 409 / gRPC ABORTED responses
    /// without re-stringifying the `Display` text.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        COR_CAS_DIGEST_MISMATCH
    }
}
