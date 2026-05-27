//! Canonical [`SigError`] taxonomy (WI-S04-004 §1 + ADR-0021).
//!
//! Every variant maps to a stable `audit_code` short identifier so the
//! AC handler can split SRE dashboards by failure mode without parsing
//! the human-readable [`thiserror`] strings. The handler returns 422 +
//! `COR_AC_SIG_INVALID` for [`SigError::Invalid`] /
//! [`SigError::LengthMismatch`] / [`SigError::KeyIdReserved`] /
//! [`SigError::KeyIdUnknown`]; 503 + `COR_AC_BACKEND_UNAVAILABLE` for
//! [`SigError::BackendError`] / [`SigError::TdkDerivationFailed`].

use thiserror::Error;

/// Errors surfaced by [`crate::ac_core::sig::SignatureSigner::sign`] /
/// [`crate::ac_core::sig::SignatureVerifier::verify`].
///
/// `#[non_exhaustive]` — additive forward-compatibility for future
/// rotation-policy variants (Lote 10.4-tris P0-R5-001 added
/// `KeyIdReserved`).
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SigError {
    /// Signature input is the wrong length (length is public — fast
    /// fail before the constant-time compare).
    ///
    /// `expected` is always [`crate::ac_core::sig::AC_ENVELOPE_SIG_LEN`] (32).
    #[error("ac envelope signature length invalid: expected {expected}, got {got}")]
    LengthMismatch {
        /// Expected byte length (always 32).
        expected: usize,
        /// Observed byte length on the failed input.
        got: usize,
    },
    /// Cripto MAC compare failed (constant-time path; tampering or
    /// key-mismatch signal). Maps to 422 + `COR_AC_SIG_INVALID` +
    /// audit `ac.get.sig_invalid` (CRITICAL).
    #[error("ac envelope signature mismatch (constant-time compare failed)")]
    Invalid,
    /// Programmer error — `sig_key_id == 0` is the reserved sentinel
    /// per ADR-0021 §Sentinel + Lote 10.4-tris P0-R5-001. Rotation
    /// starts at `1`; the schema CHECK constraint in
    /// `migrations/d1/0002_ac_meta.sql` enforces this column-side too.
    #[error("ac envelope sig_key_id is reserved (must be >= 1)")]
    KeyIdReserved,
    /// Verifier was asked to verify a signature whose `sig_key_id` is
    /// outside the configured `accepted_key_ids` whitelist (rotation
    /// grace expired OR client used a too-old key). Surfaces the
    /// oldest currently-accepted key so SRE dashboards can correlate
    /// rotation events.
    #[error("ac envelope sig_key_id {sig_key_id} unknown or rotated out (oldest active: {oldest_active})")]
    KeyIdUnknown {
        /// `sig_key_id` observed on the rejected signature.
        sig_key_id: u32,
        /// Smallest `sig_key_id` currently in the verifier's
        /// `accepted_key_ids` whitelist.
        oldest_active: u32,
    },
    /// [`crate::ac_core::sig::TdkHandle::fetch`] returned an error (KMS / CF
    /// Secrets backend fault). Maps to 503 + `COR_AC_BACKEND_UNAVAILABLE`.
    #[error("ac sig backend error: {0}")]
    BackendError(String),
    /// HKDF-Expand reported a length error (mathematically
    /// unreachable for a 32-byte output; treated as a structural
    /// failure surface).
    #[error("ac sig tdk derivation failed: {0}")]
    TdkDerivationFailed(String),
}

impl SigError {
    /// Stable canonical short identifier used in audit envelope
    /// `reason` field. SRE dashboards key on this, never on the
    /// human-readable `thiserror` string.
    #[must_use]
    pub const fn audit_code(&self) -> &'static str {
        match self {
            Self::LengthMismatch { .. } => "length_mismatch",
            Self::Invalid => "sig_invalid",
            Self::KeyIdReserved => "key_id_reserved",
            Self::KeyIdUnknown { .. } => "key_id_unknown",
            Self::BackendError(_) => "backend_error",
            Self::TdkDerivationFailed(_) => "tdk_derivation_failed",
        }
    }

    /// Whether this variant maps to the reserved-key sentinel.
    /// Programmer-error short-circuit used by the worker handler to
    /// avoid emitting a security audit for a structural mistake.
    #[must_use]
    pub const fn is_key_id_reserved(&self) -> bool {
        matches!(self, Self::KeyIdReserved)
    }

    /// Whether this variant maps to the cripto mismatch path —
    /// success-or-mismatch is the constant-time-relevant arm.
    #[must_use]
    pub const fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn audit_codes_are_stable() {
        // Pin every variant's canonical short identifier so a refactor
        // that renames an error variant trips the test before reaching
        // production dashboards.
        assert_eq!(
            SigError::LengthMismatch { expected: 32, got: 64 }.audit_code(),
            "length_mismatch"
        );
        assert_eq!(SigError::Invalid.audit_code(), "sig_invalid");
        assert_eq!(SigError::KeyIdReserved.audit_code(), "key_id_reserved");
        assert_eq!(
            SigError::KeyIdUnknown {
                sig_key_id: 99,
                oldest_active: 2
            }
            .audit_code(),
            "key_id_unknown"
        );
        assert_eq!(SigError::BackendError("kms".into()).audit_code(), "backend_error");
        assert_eq!(
            SigError::TdkDerivationFailed("len".into()).audit_code(),
            "tdk_derivation_failed"
        );
    }

    #[test]
    fn predicates_classify_canonical_arms() {
        assert!(SigError::KeyIdReserved.is_key_id_reserved());
        assert!(!SigError::Invalid.is_key_id_reserved());
        assert!(SigError::Invalid.is_invalid());
        assert!(!SigError::KeyIdReserved.is_invalid());
    }
}
