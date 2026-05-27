//! Error types for [`ClientVerifier`](crate::ClientVerifier) operations.
//!
//! `VerifyError` is the only fallible result variant returned by sync verify.
//! It is intentionally narrow: today the only failure mode is a digest
//! mismatch (bit rot, cache poisoning, or an opt-out caller asking for a
//! verify that was disabled). Stream-mode adds `StreamVerifyError` which
//! carries the upstream I/O error in addition to mismatch.

use thiserror::Error;

use crate::digest::{COR_CAS_DIGEST_MISMATCH, COR_CAS_VERIFY_DISABLED};

/// Error returned by [`ClientVerifier::verify`](crate::ClientVerifier::verify)
/// and the C-ABI sync verify entry point.
///
/// Mismatch carries hex-rendered `expected` and `computed` digests for
/// log/diagnostic surfaces. Both fields are deterministic projections of
/// inputs the caller already holds (the claimed digest the SDK passed in
/// and the BLAKE3 of the body the SDK already received), so the error
/// surface does not introduce any new side channel beyond what the caller
/// can compute itself.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerifyError {
    /// The body's BLAKE3 digest does not match the claimed digest. The
    /// canonical CTRL-CAS-002 / INV-CAS-INTEGRITY signal: bit rot, cache
    /// poisoning at transit, or a server-side regression returning the
    /// wrong blob for a key.
    #[error(
        "client verify failed: BLAKE3 digest mismatch (expected {expected}, \
         computed {computed}; {code})",
        code = COR_CAS_DIGEST_MISMATCH
    )]
    DigestMismatch {
        /// Hex-rendered claimed digest (what the SDK asked for).
        expected: String,
        /// Hex-rendered actual BLAKE3 of the body the SDK received.
        computed: String,
    },

    /// The verifier was constructed via [`VerifyConfig::disabled`] and the
    /// caller subsequently invoked `verify`. This variant exists so the
    /// SDK can route the explicit opt-out path through the same Result
    /// arm an integration test would assert on; production callers
    /// expecting an opt-out should not call `verify` at all.
    ///
    /// [`VerifyConfig::disabled`]: crate::VerifyConfig::disabled
    #[error(
        "client verify is disabled (opt-out); proceed at own risk \
         ({code})",
        code = COR_CAS_VERIFY_DISABLED
    )]
    VerifyDisabled,
}

impl VerifyError {
    /// Stable error-taxonomy code for this error variant.
    ///
    /// Used by FFI wrappers to map the Rust-typed error onto language
    /// idiomatic exceptions while keeping the wire / log code stable.
    /// Mirrors the canonical mapping in `error_taxonomy.md`.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::DigestMismatch { .. } => COR_CAS_DIGEST_MISMATCH,
            Self::VerifyDisabled => COR_CAS_VERIFY_DISABLED,
        }
    }
}

/// Stream-mode verify error.
///
/// In addition to digest mismatch (detected at end-of-stream when the
/// running BLAKE3 hasher is finalized), stream verify can also fail
/// because the upstream `AsyncRead` itself errored. The two cases are
/// kept distinct so SDK consumers can decide independently whether to
/// retry (transport error) or treat as an integrity violation
/// (mismatch).
#[cfg(feature = "stream")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StreamVerifyError {
    /// Body finished streaming but its computed BLAKE3 does not match
    /// the claimed digest. Detected at end-of-stream (BLAKE3 cannot
    /// rule out a mismatch until it has consumed the final byte;
    /// per-chunk Merkle proofs are deferred to S-05).
    #[error(transparent)]
    Mismatch(#[from] VerifyError),

    /// The underlying `AsyncRead` returned an I/O error before
    /// end-of-stream. Verify is incomplete and the caller MUST NOT
    /// treat the partial body as integrity-checked.
    #[error("client verify stream I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(feature = "stream")]
impl StreamVerifyError {
    /// Stable error-taxonomy code (same lookup table as
    /// [`VerifyError::code`]; transport I/O surfaces as
    /// `COR_CAS_VERIFY_IO`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Mismatch(e) => e.code(),
            Self::Io(_) => crate::digest::COR_CAS_VERIFY_IO,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code; panic on assertion failure is the contract"
)]
mod tests {
    use super::*;

    #[test]
    fn verify_error_code_lookup() {
        let m = VerifyError::DigestMismatch {
            expected: "0".repeat(64),
            computed: "f".repeat(64),
        };
        assert_eq!(m.code(), COR_CAS_DIGEST_MISMATCH);
        assert_eq!(VerifyError::VerifyDisabled.code(), COR_CAS_VERIFY_DISABLED);
    }

    #[test]
    fn verify_error_display_includes_canonical_code() {
        let m = VerifyError::DigestMismatch {
            expected: "0".repeat(64),
            computed: "f".repeat(64),
        };
        let rendered = format!("{m}");
        assert!(rendered.contains(COR_CAS_DIGEST_MISMATCH));
        assert!(rendered.contains(&"0".repeat(64)));
        assert!(rendered.contains(&"f".repeat(64)));

        let d = VerifyError::VerifyDisabled;
        assert!(format!("{d}").contains(COR_CAS_VERIFY_DISABLED));
    }

    #[cfg(feature = "stream")]
    #[test]
    fn stream_verify_error_code_lookup() {
        let m = StreamVerifyError::Mismatch(VerifyError::DigestMismatch {
            expected: "0".repeat(64),
            computed: "f".repeat(64),
        });
        assert_eq!(m.code(), COR_CAS_DIGEST_MISMATCH);

        let d = StreamVerifyError::Mismatch(VerifyError::VerifyDisabled);
        assert_eq!(d.code(), COR_CAS_VERIFY_DISABLED);

        let io = StreamVerifyError::Io(std::io::Error::other("synthetic"));
        assert_eq!(io.code(), crate::digest::COR_CAS_VERIFY_IO);
    }
}
