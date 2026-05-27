//! Canonical bounded-parser limits enforced by every public surface in
//! this crate (WI-S04-003 §6.1.5).
//!
//! These constants are duplicated verbatim in
//! `corelink-worker::reapi::ac::merkle` so the worker handler can
//! short-circuit on bounds before invoking the verifier; the
//! [`MAX_TREE_DEPTH`] / [`MAX_TREE_FANOUT`] / [`MAX_OUTPUT_FILES`] /
//! [`MAX_OUTPUT_DIRECTORIES`] values are equal across both crates by
//! contract — `corelink-ac::tests::canonical_vectors` includes a
//! parity test that asserts equality at compile time when the worker
//! crate is in scope.

/// Maximum permitted Merkle tree depth (WI §6.1.5). A balanced
/// binary tree at depth 32 admits up to `2^32` leaves which dwarfs the
/// `MAX_OUTPUT_FILES + MAX_OUTPUT_DIRECTORIES = 8192` ceiling — the
/// depth bound is in place to short-circuit pathological tree-shape
/// adversarial inputs. Exceed → [`crate::MerkleError::DepthExceeded`].
pub const MAX_TREE_DEPTH: usize = 32;

/// Maximum permitted per-node fanout (WI §6.1.5). The canonical
/// Merkle builder ships a balanced **binary** tree (fanout 2 by
/// design); the bound exists so a future schema-evolution proposing a
/// k-ary tree variant cannot silently bypass the parser
/// discipline. Exceed → [`crate::MerkleError::FanoutExceeded`].
pub const MAX_TREE_FANOUT: usize = 4096;

/// Maximum total node count (WI §6.1.5). The balanced binary tree
/// allocates `2 * leaves - 1` nodes; this bound (100 000) covers any
/// tree fed by the bounded `MAX_OUTPUT_FILES + MAX_OUTPUT_DIRECTORIES`
/// leaf set with substantial headroom. Exceed →
/// [`crate::MerkleError::NodeCountExceeded`].
pub const MAX_NODE_COUNT: usize = 100_000;

/// Maximum cumulative envelope payload bytes (WI §6.1.5). Caps the
/// whole `AcEnvelope` JSON encoding plus the embedded `ActionResult`
/// canonical bytes — the Cloudflare Worker memory budget is 128 MiB
/// per isolate so a 1 MiB cap leaves three orders of magnitude of
/// safety. Exceed → [`crate::MerkleError::PayloadExceeded`].
pub const MAX_PAYLOAD_BYTES: usize = 1_048_576;

/// Maximum permitted `output_files.len()` (WI §6.1.5). Mirrors the
/// REAPI v2 conformance test suite's reasonable upper bound; Bazel
/// builds emit ≤ a few dozen output files in practice. Exceed →
/// [`crate::MerkleError::TooManyOutputFiles`].
pub const MAX_OUTPUT_FILES: usize = 4096;

/// Maximum permitted `output_directories.len()` (WI §6.1.5). Same
/// canonical bound as [`MAX_OUTPUT_FILES`]. Exceed →
/// [`crate::MerkleError::TooManyOutputDirectories`].
pub const MAX_OUTPUT_DIRECTORIES: usize = 4096;

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
    fn bounds_are_consistent() {
        // The MAX_NODE_COUNT must comfortably exceed the
        // (files + directories) leaf cap doubled (balanced binary
        // tree node count = 2 * leaves - 1).
        let max_leaves = MAX_OUTPUT_FILES + MAX_OUTPUT_DIRECTORIES;
        assert!(MAX_NODE_COUNT > 2 * max_leaves);
        // Depth bound covers any leaf count up to 2^MAX_TREE_DEPTH.
        assert!((1u128 << MAX_TREE_DEPTH) > max_leaves as u128);
    }

    #[test]
    fn payload_bound_is_one_mib() {
        assert_eq!(MAX_PAYLOAD_BYTES, 1024 * 1024);
    }
}
