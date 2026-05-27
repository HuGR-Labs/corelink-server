//! Canonical error taxonomy for `corelink-ac` (WI-S04-003 §1).
//!
//! Every variant maps to a stable `audit_code` short identifier so the
//! AC handler can split SRE dashboards by failure mode without
//! parsing the human-readable [`thiserror`] strings. The handler
//! returns 422 + `COR_AC_MERKLE_INVALID` for every [`MerkleError`]
//! variant per WI §1.

use thiserror::Error;

/// Errors surfaced by the Merkle builder + verifier.
///
/// Variants are `#[non_exhaustive]` for additive forward
/// compatibility — a future spec evolution adding a new bound (e.g.
/// envelope-version mismatch) ships as a new variant without breaking
/// downstream pattern matchers.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum MerkleError {
    /// Tree depth exceeded the canonical bound.
    #[error("merkle tree depth {depth} exceeds bound {bound}")]
    DepthExceeded {
        /// Observed depth (after potential clamp at [`crate::ac_core::bounds::MAX_TREE_DEPTH`]).
        depth: usize,
        /// Bound — always [`crate::ac_core::bounds::MAX_TREE_DEPTH`].
        bound: usize,
    },
    /// Tree fanout exceeded the canonical bound.
    #[error("merkle tree fanout {fanout} exceeds bound {bound}")]
    FanoutExceeded {
        /// Observed fanout.
        fanout: usize,
        /// Bound — always [`crate::ac_core::bounds::MAX_TREE_FANOUT`].
        bound: usize,
    },
    /// Total node count exceeded the canonical bound.
    #[error("merkle tree node count {count} exceeds bound {bound}")]
    NodeCountExceeded {
        /// Observed node count.
        count: usize,
        /// Bound — always [`crate::ac_core::bounds::MAX_NODE_COUNT`].
        bound: usize,
    },
    /// Cumulative envelope payload bytes exceeded the canonical bound.
    #[error("envelope payload {found_bytes} bytes exceeds bound {bound}")]
    PayloadExceeded {
        /// Observed payload size.
        found_bytes: usize,
        /// Bound — always [`crate::ac_core::bounds::MAX_PAYLOAD_BYTES`].
        bound: usize,
    },
    /// `output_files` slice exceeds the canonical bound.
    #[error("output_files length {len} exceeds bound {bound}")]
    TooManyOutputFiles {
        /// Observed length.
        len: usize,
        /// Bound — always [`crate::ac_core::bounds::MAX_OUTPUT_FILES`].
        bound: usize,
    },
    /// `output_directories` slice exceeds the canonical bound.
    #[error("output_directories length {len} exceeds bound {bound}")]
    TooManyOutputDirectories {
        /// Observed length.
        len: usize,
        /// Bound — always [`crate::ac_core::bounds::MAX_OUTPUT_DIRECTORIES`].
        bound: usize,
    },
    /// The Merkle root computed from the result does not match the
    /// claimed root (envelope tampering signal).
    #[error("merkle root mismatch")]
    RootMismatch,
    /// Cycle detected while traversing nested directory references.
    #[error("merkle cycle detected at digest {digest}")]
    CycleDetected {
        /// 64-char lowercase hex of the digest closing the cycle.
        digest: String,
    },
    /// Tree shape malformed (e.g. all-zero sentinel digest, invalid
    /// hex / size encoding, decoder bound exceeded mid-parse).
    #[error("merkle tree malformed: {reason}")]
    Malformed {
        /// Short canonical reason free-form string.
        reason: String,
    },
    /// `AcEnvelope` declares a `version` byte the codec does not
    /// know how to deserialize.
    #[error("envelope version {0} unsupported (only v1 is canonical)")]
    VersionUnsupported(u8),
    /// JSON / canonical-bytes decode error.
    #[error("envelope decode error: {0}")]
    DecodeError(String),
}

impl MerkleError {
    /// Stable canonical short identifier used in audit envelope
    /// `reason` field. Mirrors
    /// `corelink-worker::reapi::ac::merkle::MerkleError::audit_code`
    /// so downstream `ac.update.merkle_invalid` records use the same
    /// short string regardless of which crate's verifier produced
    /// the failure.
    #[must_use]
    pub const fn audit_code(&self) -> &'static str {
        match self {
            Self::DepthExceeded { .. } => "depth_exceeded",
            Self::FanoutExceeded { .. } => "fanout_exceeded",
            Self::NodeCountExceeded { .. } => "node_count_exceeded",
            Self::PayloadExceeded { .. } => "payload_exceeded",
            Self::TooManyOutputFiles { .. } => "too_many_output_files",
            Self::TooManyOutputDirectories { .. } => "too_many_output_directories",
            Self::RootMismatch => "root_mismatch",
            Self::CycleDetected { .. } => "cycle_detected",
            Self::Malformed { .. } => "malformed_tree",
            Self::VersionUnsupported(_) => "version_unsupported",
            Self::DecodeError(_) => "decode_error",
        }
    }
}

/// Errors surfaced by the envelope-build path. The build path
/// composes the bounds + Merkle pipeline and returns a single
/// canonical error type so callers can branch on logical category
/// rather than enumerate individual upstream variants.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildError {
    /// Bounds / structure / determinism failure during build.
    #[error(transparent)]
    Merkle(#[from] MerkleError),
}

/// Errors surfaced by the dual-side `verify_full` path. Wraps a
/// `MerkleError` (structure check) plus an opaque signature error
/// surfaced from the `SignatureVerifier` delegate (WI-S04-004 supplies
/// the real impl).
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerifyError {
    /// Structure (Merkle / bounds) verification failed.
    #[error(transparent)]
    Structure(#[from] MerkleError),
    /// Signature verification failed (delegate decision).
    #[error("envelope signature invalid: {0}")]
    Sig(String),
}

/// Errors surfaced by the [`crate::OutputsValidator`] path.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutputsCheckError {
    /// One or more referenced output blobs are tombstoned or never
    /// existed in `blob_meta` for the tenant.
    #[error("output blob {digest} missing or tombstoned")]
    BlobMissing {
        /// 64-char lowercase hex of the missing digest.
        digest: String,
    },
    /// Backend (D1) round-trip failed.
    #[error("blob_meta backend error: {0}")]
    BackendError(String),
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
            MerkleError::DepthExceeded {
                depth: 99,
                bound: crate::ac_core::bounds::MAX_TREE_DEPTH,
            }
            .audit_code(),
            "depth_exceeded"
        );
        assert_eq!(MerkleError::RootMismatch.audit_code(), "root_mismatch");
        assert_eq!(
            MerkleError::CycleDetected {
                digest: "00".repeat(32),
            }
            .audit_code(),
            "cycle_detected"
        );
        assert_eq!(MerkleError::VersionUnsupported(2).audit_code(), "version_unsupported");
    }
}
