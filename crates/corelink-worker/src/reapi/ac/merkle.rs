//! Merkle codec / verifier trait surface (WI-S04-001 §6.1.6).
//!
//! The full Merkle tree builder + verifier (depth ≤ 32, fanout ≤ 4096)
//! lives in WI-S04-003 (`corelink-ac` crate). This module ships the
//! trait surface the AC handler depends on plus a deterministic
//! [`InMemoryMerkleVerifier`] that exercises every documented code
//! path:
//!
//! - **Pass**: `output_files.len() <= MAX_OUTPUT_FILES`,
//!   `output_directories.len() <= MAX_OUTPUT_DIRECTORIES`, every
//!   digest is non-zero (pseudo well-formed). Sufficient for the
//!   property-test boundary; the real tree-walk is WI-S04-003.
//! - **DepthExceeded**: caller-supplied "synthetic depth" hint > 32.
//! - **FanoutExceeded**: caller-supplied "synthetic fanout" hint >
//!   4096.
//! - **MalformedTree**: any output digest is the canonical
//!   all-zero digest (sentinel for malformed).
//!
//! The fake's surface is:
//!
//! ```ignore
//! verify(&result) -> Result<(), MerkleError>
//! ```
//!
//! and the production impl will match the same signature so the
//! handler never imports a different verifier type when WI-S04-003
//! lands.

use corelink_ac::MerkleVerifier as _CorelinkAcMerkleVerifier;
use thiserror::Error;

use super::types::ActionResult;

/// Maximum permitted `output_files.len()` (WI §6.1.6 + WI §1
/// "max output_files 4096"). Exceed → 422 + `COR_AC_MERKLE_INVALID`.
pub const MAX_OUTPUT_FILES: usize = 4096;

/// Maximum permitted `output_directories.len()`. Same canonical bound
/// as [`MAX_OUTPUT_FILES`].
pub const MAX_OUTPUT_DIRECTORIES: usize = 4096;

/// Canonical max Merkle tree depth (WI §1 "max depth 32"). The fake
/// honors a synthetic depth hint per [`InMemoryMerkleVerifier::with_depth_hint`]
/// so property tests can exercise the depth-exceed boundary without
/// crafting a real tree.
pub const MAX_TREE_DEPTH: usize = 32;

/// Canonical max Merkle tree fanout (WI §1 "max fanout 4096"). Same
/// hint pattern as [`MAX_TREE_DEPTH`].
pub const MAX_TREE_FANOUT: usize = 4096;

/// Errors surfaced by [`MerkleVerifier::verify`].
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum MerkleError {
    /// Tree depth exceeded [`MAX_TREE_DEPTH`].
    #[error("merkle tree depth {depth} exceeds bound {bound}")]
    DepthExceeded {
        /// Observed tree depth.
        depth: usize,
        /// Bound — always [`MAX_TREE_DEPTH`].
        bound: usize,
    },
    /// Tree fanout exceeded [`MAX_TREE_FANOUT`].
    #[error("merkle tree fanout {fanout} exceeds bound {bound}")]
    FanoutExceeded {
        /// Observed fanout.
        fanout: usize,
        /// Bound — always [`MAX_TREE_FANOUT`].
        bound: usize,
    },
    /// `output_files` slice exceeds [`MAX_OUTPUT_FILES`].
    #[error("output_files length {len} exceeds bound {bound}")]
    TooManyOutputFiles {
        /// Observed length.
        len: usize,
        /// Bound — always [`MAX_OUTPUT_FILES`].
        bound: usize,
    },
    /// `output_directories` slice exceeds [`MAX_OUTPUT_DIRECTORIES`].
    #[error("output_directories length {len} exceeds bound {bound}")]
    TooManyOutputDirectories {
        /// Observed length.
        len: usize,
        /// Bound — always [`MAX_OUTPUT_DIRECTORIES`].
        bound: usize,
    },
    /// Malformed tree (all-zero digest sentinel; or production impl
    /// failure).
    #[error("merkle tree malformed: {0}")]
    MalformedTree(String),
}

/// Merkle verifier trait. Real impl ships in WI-S04-003.
pub trait MerkleVerifier: Send + Sync {
    /// Verify the [`ActionResult`]'s embedded Merkle tree references.
    /// Returns `Ok(())` on a well-formed tree; otherwise a
    /// [`MerkleError`] mapping 1:1 to a `COR_AC_MERKLE_INVALID` 422.
    ///
    /// # Errors
    ///
    /// Maps to a [`MerkleError`] variant per WI §1.
    fn verify(&self, result: &ActionResult) -> Result<(), MerkleError>;
}

/// Test-only Merkle verifier that exercises every documented failure
/// mode. The "synthetic depth" / "synthetic fanout" hints let tests
/// exercise the depth-exceeded / fanout-exceeded arms without
/// crafting real trees (the real WI-S04-003 verifier walks the tree).
#[derive(Clone, Copy, Debug, Default)]
pub struct InMemoryMerkleVerifier {
    depth_hint: usize,
    fanout_hint: usize,
}

impl InMemoryMerkleVerifier {
    /// Construct a fake verifier with `depth_hint = 1`, `fanout_hint
    /// = 1` — the canonical "always pass" configuration unless the
    /// test boosts a hint above the bound.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            depth_hint: 1,
            fanout_hint: 1,
        }
    }

    /// Override the synthetic depth hint. `depth > MAX_TREE_DEPTH` ⇒
    /// every [`Self::verify`] call returns [`MerkleError::DepthExceeded`].
    #[must_use]
    pub const fn with_depth_hint(mut self, depth: usize) -> Self {
        self.depth_hint = depth;
        self
    }

    /// Override the synthetic fanout hint. `fanout > MAX_TREE_FANOUT`
    /// ⇒ every [`Self::verify`] call returns
    /// [`MerkleError::FanoutExceeded`].
    #[must_use]
    pub const fn with_fanout_hint(mut self, fanout: usize) -> Self {
        self.fanout_hint = fanout;
        self
    }
}

/// All-zero digest sentinel for malformed-tree detection in the fake.
/// The real WI-S04-003 verifier does not need this sentinel — it
/// walks the tree.
fn is_zero_digest(d: &corelink_hash::Digest) -> bool {
    d.as_bytes().iter().all(|b| *b == 0)
}

impl MerkleVerifier for InMemoryMerkleVerifier {
    fn verify(&self, result: &ActionResult) -> Result<(), MerkleError> {
        if self.depth_hint > MAX_TREE_DEPTH {
            return Err(MerkleError::DepthExceeded {
                depth: self.depth_hint,
                bound: MAX_TREE_DEPTH,
            });
        }
        if self.fanout_hint > MAX_TREE_FANOUT {
            return Err(MerkleError::FanoutExceeded {
                fanout: self.fanout_hint,
                bound: MAX_TREE_FANOUT,
            });
        }
        if result.output_files.len() > MAX_OUTPUT_FILES {
            return Err(MerkleError::TooManyOutputFiles {
                len: result.output_files.len(),
                bound: MAX_OUTPUT_FILES,
            });
        }
        if result.output_directories.len() > MAX_OUTPUT_DIRECTORIES {
            return Err(MerkleError::TooManyOutputDirectories {
                len: result.output_directories.len(),
                bound: MAX_OUTPUT_DIRECTORIES,
            });
        }
        for f in &result.output_files {
            if is_zero_digest(&f.digest) {
                return Err(MerkleError::MalformedTree(format!(
                    "output_file digest all-zero (sentinel; size_bytes={})",
                    f.size_bytes
                )));
            }
        }
        for d in &result.output_directories {
            if is_zero_digest(&d.digest) {
                return Err(MerkleError::MalformedTree(format!(
                    "output_directory digest all-zero (sentinel; size_bytes={})",
                    d.size_bytes
                )));
            }
        }
        Ok(())
    }
}

impl MerkleError {
    /// Stable canonical short identifier for the audit envelope's
    /// `reason` field. The handler maps every variant to 422
    /// `COR_AC_MERKLE_INVALID`; this code surfaces in
    /// `ac.update.merkle_invalid` audits so SRE dashboards can split
    /// by failure mode without parsing the human-readable
    /// [`fmt::Display`] string.
    #[must_use]
    pub const fn audit_code(&self) -> &'static str {
        match self {
            Self::DepthExceeded { .. } => "depth_exceeded",
            Self::FanoutExceeded { .. } => "fanout_exceeded",
            Self::TooManyOutputFiles { .. } => "too_many_output_files",
            Self::TooManyOutputDirectories { .. } => "too_many_output_directories",
            Self::MalformedTree(_) => "malformed_tree",
        }
    }
}

/// Production [`MerkleVerifier`] adapter wrapping the canonical
/// `corelink-ac::CanonicalMerkleVerifier` (WI-S04-003).
///
/// Converts the worker's local [`ActionResult`] shape into the
/// `corelink-ac` wire shape, runs the canonical verifier, and maps
/// the canonical [`corelink_ac::MerkleError`] variants back to this
/// crate's [`MerkleError`] enum (preserving the `audit_code`
/// short-id contract).
///
/// This is the verifier the production handler wires; the
/// [`InMemoryMerkleVerifier`] above remains the canonical test
/// fake.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalAcMerkleVerifier {
    inner: corelink_ac::CanonicalMerkleVerifier,
}

impl CanonicalAcMerkleVerifier {
    /// Construct a fresh canonical verifier.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: corelink_ac::CanonicalMerkleVerifier::new(),
        }
    }

    /// Project the worker's [`ActionResult`] into the
    /// `corelink-ac` wire shape. Cheap clone — the `digest` field is
    /// `Copy`, only the `Vec<...>` digest references allocate.
    fn project(result: &ActionResult) -> corelink_ac::ActionResult {
        let files: Vec<corelink_ac::OutputFileDigest> = result
            .output_files
            .iter()
            .map(|f| corelink_ac::OutputFileDigest::new(f.digest, f.size_bytes))
            .collect();
        let dirs: Vec<corelink_ac::OutputDirectoryDigest> = result
            .output_directories
            .iter()
            .map(|d| corelink_ac::OutputDirectoryDigest::new(d.digest, d.size_bytes))
            .collect();
        corelink_ac::ActionResult::new(
            files,
            dirs,
            result.exit_code,
            result.raw_proto_bytes.clone(),
        )
    }
}

impl MerkleVerifier for CanonicalAcMerkleVerifier {
    fn verify(&self, result: &ActionResult) -> Result<(), MerkleError> {
        let projected = Self::project(result);
        self.inner.verify(&projected).map_err(map_canonical_error)
    }
}

/// Map a `corelink-ac` canonical error to the worker's local enum.
///
/// The two enums overlap on every variant the canonical verifier
/// emits today; depth/fanout/file-count/directory-count map 1:1, and
/// every "structural" failure (`Malformed`, `RootMismatch`,
/// `CycleDetected`, `NodeCountExceeded`, `PayloadExceeded`,
/// `VersionUnsupported`, `DecodeError`) collapses onto
/// [`MerkleError::MalformedTree`] with a canonical-reason prefix so
/// the audit-code dashboard split still surfaces the underlying
/// failure mode via the message body.
fn map_canonical_error(err: corelink_ac::MerkleError) -> MerkleError {
    match err {
        corelink_ac::MerkleError::DepthExceeded { depth, bound } => {
            MerkleError::DepthExceeded { depth, bound }
        }
        corelink_ac::MerkleError::FanoutExceeded { fanout, bound } => {
            MerkleError::FanoutExceeded { fanout, bound }
        }
        corelink_ac::MerkleError::TooManyOutputFiles { len, bound } => {
            MerkleError::TooManyOutputFiles { len, bound }
        }
        corelink_ac::MerkleError::TooManyOutputDirectories { len, bound } => {
            MerkleError::TooManyOutputDirectories { len, bound }
        }
        corelink_ac::MerkleError::Malformed { reason } => MerkleError::MalformedTree(reason),
        corelink_ac::MerkleError::RootMismatch => {
            MerkleError::MalformedTree("root_mismatch".to_string())
        }
        corelink_ac::MerkleError::CycleDetected { digest } => {
            MerkleError::MalformedTree(format!("cycle_detected:{digest}"))
        }
        corelink_ac::MerkleError::NodeCountExceeded { count, bound } => MerkleError::MalformedTree(
            format!("node_count_exceeded:found={count},bound={bound}"),
        ),
        corelink_ac::MerkleError::PayloadExceeded { found_bytes, bound } => {
            MerkleError::MalformedTree(format!(
                "payload_exceeded:found={found_bytes},bound={bound}"
            ))
        }
        corelink_ac::MerkleError::VersionUnsupported(v) => {
            MerkleError::MalformedTree(format!("version_unsupported:{v}"))
        }
        corelink_ac::MerkleError::DecodeError(s) => {
            MerkleError::MalformedTree(format!("decode_error:{s}"))
        }
        // Forward-compat for additive variants in `corelink-ac`.
        other => MerkleError::MalformedTree(format!("unmapped:{other}")),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use crate::reapi::ac::types::{OutputFileDigest};
    use corelink_hash::Digest;

    fn fresh_result() -> ActionResult {
        ActionResult::new(
            vec![OutputFileDigest::new(Digest::compute(b"out1"), 1)],
            Vec::new(),
            0,
            b"raw".to_vec(),
        )
    }

    #[test]
    fn well_formed_passes() {
        let v = InMemoryMerkleVerifier::new();
        v.verify(&fresh_result()).unwrap();
    }

    #[test]
    fn depth_hint_exceeded_fails() {
        let v = InMemoryMerkleVerifier::new().with_depth_hint(MAX_TREE_DEPTH + 1);
        let err = v.verify(&fresh_result()).unwrap_err();
        assert!(matches!(err, MerkleError::DepthExceeded { .. }));
    }

    #[test]
    fn fanout_hint_exceeded_fails() {
        let v = InMemoryMerkleVerifier::new().with_fanout_hint(MAX_TREE_FANOUT + 1);
        let err = v.verify(&fresh_result()).unwrap_err();
        assert!(matches!(err, MerkleError::FanoutExceeded { .. }));
    }

    #[test]
    fn all_zero_digest_in_files_rejected() {
        let v = InMemoryMerkleVerifier::new();
        let res = ActionResult::new(
            vec![OutputFileDigest::new(
                Digest::from_hex(&"00".repeat(32)).unwrap(),
                1,
            )],
            Vec::new(),
            0,
            Vec::new(),
        );
        let err = v.verify(&res).unwrap_err();
        assert!(matches!(err, MerkleError::MalformedTree(_)));
    }

    #[test]
    fn canonical_ac_verifier_passes_well_formed() {
        let v = CanonicalAcMerkleVerifier::new();
        v.verify(&fresh_result()).unwrap();
    }

    #[test]
    fn canonical_ac_verifier_rejects_too_many_files() {
        let v = CanonicalAcMerkleVerifier::new();
        let mut files = Vec::with_capacity(MAX_OUTPUT_FILES + 1);
        for i in 0..=MAX_OUTPUT_FILES {
            files.push(OutputFileDigest::new(
                Digest::compute(format!("f{i}").as_bytes()),
                1,
            ));
        }
        let res = ActionResult::new(files, Vec::new(), 0, Vec::new());
        let err = v.verify(&res).unwrap_err();
        assert!(matches!(err, MerkleError::TooManyOutputFiles { .. }));
    }

    #[test]
    fn canonical_ac_verifier_rejects_all_zero_digest_in_files() {
        let v = CanonicalAcMerkleVerifier::new();
        let res = ActionResult::new(
            vec![OutputFileDigest::new(
                Digest::from_hex(&"00".repeat(32)).unwrap(),
                1,
            )],
            Vec::new(),
            0,
            Vec::new(),
        );
        let err = v.verify(&res).unwrap_err();
        assert!(matches!(err, MerkleError::MalformedTree(_)));
    }
}
