//! [`AttestationError`] — unified error taxonomy for erasure attestation.

use crate::region::Region;
use thiserror::Error;

/// Erasure attestation error taxonomy.
///
/// `#[non_exhaustive]` per CoreLink codex: new variants can be added
/// without breaking downstream `match` arms.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AttestationError {
    /// Ed25519 signing failed (key material issue or dalek error).
    #[error("Ed25519 signing error: {0}")]
    Sign(String),

    /// Ed25519 signature verification failed (invalid or forged signature).
    #[error("Ed25519 verify error: {0}")]
    Verify(String),

    /// JCS canonicalization via `serde_jcs` failed.
    #[error("JCS canonicalization error: {0}")]
    Canonicalization(String),

    /// No active attestation signing key found for the requested region and
    /// timestamp. The key may have been retired post-overlap window.
    #[error("attestation key not found for region {region} at ts {ts_ms}")]
    KeyNotFound {
        /// Region where the key was expected.
        region: Region,
        /// Millisecond timestamp of the attestation attempt.
        ts_ms: u64,
    },

    /// Attestation key rotated; the overlap window may have expired.
    #[error("attestation key rotated; verify uses old key during 30d overlap window")]
    KeyRotated,

    /// R2 audit bucket persistence failure.
    #[error("R2 storage error: {0}")]
    Storage(String),

    /// D1 index INSERT failure.
    #[error("D1 storage error: {0}")]
    D1(String),

    /// Audit chain emit failure (fail-CLOSED: attestation is NOT emitted if
    /// the audit chain emit fails).
    #[error("audit chain emit error: {0}")]
    AuditChain(String),

    /// `evidence_hash` computation failed (missing or malformed bundle).
    #[error("evidence hash error: {0}")]
    EvidenceHash(String),

    /// Serialization / deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}
