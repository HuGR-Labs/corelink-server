//! Canonical Merkle tree builder + dual-side verifier (WI-S04-003 §6.1).
//!
//! Tree shape (ADR-0037 + WI §9.1 / §9.2):
//!
//! - **Leaves**: every entry of `result.output_files` followed by every
//!   entry of `result.output_directories`, **lex-sorted by digest
//!   bytes** (stable across runs — no `HashMap`/`HashSet` iteration).
//!   Each leaf is `BLAKE3(\x00 || digest_bytes)` — the `\x00` prefix
//!   is the RFC 6962-style domain separation tag preventing
//!   second-preimage tree-shape attacks.
//! - **Inner nodes**: balanced binary tree built bottom-up. At each
//!   level, pairs `(left, right)` hash to
//!   `BLAKE3(\x01 || left || right)`. An odd trailing leaf at a level
//!   is **promoted unchanged** to the next level (Bitcoin / RFC 6962
//!   convention) — the `\x01` prefix discriminates inner from leaf so
//!   no second-preimage substitution is possible.
//! - **Empty tree**: when both `output_files` and `output_directories`
//!   are empty, the canonical root is the all-zero 32-byte digest
//!   (the same sentinel `corelink-worker::reapi::ac::merkle` rejects
//!   on individual digests; the empty-tree case is permitted only
//!   here, where the *root* being zero unambiguously encodes "no
//!   outputs"; the worker's individual-digest sentinel still rejects
//!   any `output_*[i].digest` of all-zeros).
//!
//! Bounds enforcement (WI §6.1.5 + [`crate::ac_core::bounds`]) is integrated
//! into both the build path and the verify path. The verify path
//! short-circuits at the cheapest bound check (slice length) to keep
//! the bench gate p99 ≤ 10 ms tight.

use blake3::Hasher;
use corelink_hash::Digest;

use crate::ac_core::bounds::{
    MAX_NODE_COUNT, MAX_OUTPUT_DIRECTORIES, MAX_OUTPUT_FILES, MAX_TREE_DEPTH,
};
use crate::ac_core::error::MerkleError;
use crate::ac_core::types::{ActionResult, MERKLE_ROOT_LEN};

/// Domain-separation prefix byte for leaf hashes (RFC 6962-style).
pub const LEAF_PREFIX: u8 = 0x00;

/// Domain-separation prefix byte for inner-node hashes (RFC 6962-style).
pub const INNER_PREFIX: u8 = 0x01;

/// Public Merkle verifier trait — the canonical surface consumed by
/// the AC handler (server-side `verify_structure`) and the client SDK
/// (post-download `verify_full` reuses the same impl by composing
/// with a `SignatureVerifier` delegate from WI-S04-004).
pub trait MerkleVerifier: Send + Sync + core::fmt::Debug {
    /// Verify the Merkle structure of an [`ActionResult`]. Returns
    /// `Ok(())` on a well-formed tree (bounds clean + all output
    /// digests well-formed); otherwise a [`MerkleError`] mapping 1:1
    /// to a `COR_AC_MERKLE_INVALID` 422 response per WI §1.
    ///
    /// # Errors
    ///
    /// Maps to a [`MerkleError`] variant per WI §6.1.5 / §1.
    fn verify(&self, result: &ActionResult) -> Result<(), MerkleError>;
}

/// Canonical real Merkle verifier (WI-S04-003 §6.1.4).
///
/// Stateless; cheap to clone. The same instance is safe to share
/// across spawned async tasks (`Send + Sync`).
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalMerkleVerifier;

impl CanonicalMerkleVerifier {
    /// Construct a fresh verifier. `const fn` so callers can pin one
    /// in a `static` for zero-allocation reuse.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl MerkleVerifier for CanonicalMerkleVerifier {
    fn verify(&self, result: &ActionResult) -> Result<(), MerkleError> {
        // Build the tree and let the build-time bounds + structural
        // checks short-circuit. The verifier discards the root —
        // when called against a claimed root a separate
        // `verify_against` API is used (see [`verify_root`]).
        let _root = build_root(result)?;
        Ok(())
    }
}

/// Compute the canonical 32-byte Merkle root of `result`'s output
/// digest set. Equivalent to `build_envelope(...).merkle_root` but
/// without the surrounding envelope construction overhead — the AC
/// handler calls this directly in the UPDATE path's "compute root +
/// store" step.
///
/// # Errors
///
/// Surfaces [`MerkleError::TooManyOutputFiles`] /
/// [`MerkleError::TooManyOutputDirectories`] /
/// [`MerkleError::Malformed`] / [`MerkleError::DepthExceeded`] /
/// [`MerkleError::NodeCountExceeded`] per WI §6.1.5.
pub fn build_root(result: &ActionResult) -> Result<[u8; MERKLE_ROOT_LEN], MerkleError> {
    enforce_bounds(result)?;

    // Empty-tree canonical root = all-zeros (see module rustdoc).
    if result.output_files.is_empty() && result.output_directories.is_empty() {
        return Ok([0u8; MERKLE_ROOT_LEN]);
    }

    // Collect leaves in canonical order: files first, directories
    // after, BUT each group lex-sorted internally by digest bytes
    // for determinism (the wire surface admits any insertion order;
    // the tree binding is order-independent at the API surface).
    let mut leaf_inputs: Vec<[u8; MERKLE_ROOT_LEN]> = Vec::with_capacity(result.output_count());
    for o in &result.output_files {
        if is_zero_digest(&o.digest) {
            return Err(MerkleError::Malformed {
                reason: "output_file digest all-zero (sentinel)".to_string(),
            });
        }
        leaf_inputs.push(*o.digest.as_bytes());
    }
    for o in &result.output_directories {
        if is_zero_digest(&o.digest) {
            return Err(MerkleError::Malformed {
                reason: "output_directory digest all-zero (sentinel)".to_string(),
            });
        }
        leaf_inputs.push(*o.digest.as_bytes());
    }
    // Stable canonical sort over the collected raw digest bytes.
    // Lex-byte ordering is deterministic, total, and locale-free.
    leaf_inputs.sort_unstable();

    // Hash leaves (domain-separated `\x00 || digest_bytes`).
    let mut level: Vec<[u8; MERKLE_ROOT_LEN]> = leaf_inputs.iter().map(hash_leaf).collect();

    let mut node_count = level.len();
    let mut depth = 0usize;
    while level.len() > 1 {
        depth = depth.saturating_add(1);
        if depth > MAX_TREE_DEPTH {
            return Err(MerkleError::DepthExceeded {
                depth,
                bound: MAX_TREE_DEPTH,
            });
        }
        let mut next: Vec<[u8; MERKLE_ROOT_LEN]> = Vec::with_capacity(level.len().div_ceil(2));
        let mut iter = level.chunks_exact(2);
        for pair in &mut iter {
            // chunks_exact yields slices of length exactly 2; the
            // [0]/[1] indices are sound here. Rather than rely on the
            // clippy-denied `[i]` indexing pattern we destructure via
            // first/last accessors so the code remains lint-clean.
            let left = pair
                .first()
                .copied()
                .ok_or_else(|| MerkleError::Malformed {
                    reason: "internal: chunks_exact yielded short slice (len 0)".to_string(),
                })?;
            let right = pair.last().copied().ok_or_else(|| MerkleError::Malformed {
                reason: "internal: chunks_exact yielded short slice (len 1)".to_string(),
            })?;
            next.push(hash_inner(&left, &right));
        }
        // Odd trailing leaf — promote unchanged (RFC 6962 / Bitcoin
        // convention; with the `\x01` inner prefix this cannot collide
        // with a synthetic inner-node second-preimage).
        if let [tail] = iter.remainder() {
            next.push(*tail);
        }
        node_count = node_count.saturating_add(next.len());
        if node_count > MAX_NODE_COUNT {
            return Err(MerkleError::NodeCountExceeded {
                count: node_count,
                bound: MAX_NODE_COUNT,
            });
        }
        level = next;
    }

    // `level.len() == 1` invariant holds because we exited the loop;
    // the empty case was returned eagerly above.
    level
        .into_iter()
        .next()
        .ok_or_else(|| MerkleError::Malformed {
            reason: "internal: empty level vector after build loop".to_string(),
        })
}

/// Verify a claimed `merkle_root` against the canonical recomputation
/// over `result`. Used by the post-download client verifier — given
/// the envelope's `merkle_root` field plus its `result` payload,
/// recomputes the tree and rejects on mismatch.
///
/// # Errors
///
/// Returns [`MerkleError::RootMismatch`] when the claimed root
/// disagrees with the canonical recomputation; otherwise propagates
/// any [`MerkleError`] surfaced by [`build_root`].
pub fn verify_root(
    result: &ActionResult,
    claimed_root: &[u8; MERKLE_ROOT_LEN],
) -> Result<(), MerkleError> {
    let actual = build_root(result)?;
    // Constant-time-ish equality. `subtle::ConstantTimeEq` is the
    // canonical primitive; we avoid the dep here because the root
    // mismatch is not a side-channel-relevant decision (the tree
    // recomputation dominates the timing). Plain slice equality
    // is sufficient.
    if actual == *claimed_root {
        Ok(())
    } else {
        Err(MerkleError::RootMismatch)
    }
}

fn enforce_bounds(result: &ActionResult) -> Result<(), MerkleError> {
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
    Ok(())
}

fn is_zero_digest(d: &Digest) -> bool {
    d.as_bytes().iter().all(|b| *b == 0)
}

fn hash_leaf(digest_bytes: &[u8; MERKLE_ROOT_LEN]) -> [u8; MERKLE_ROOT_LEN] {
    let mut h = Hasher::new();
    h.update(&[LEAF_PREFIX]);
    h.update(digest_bytes);
    *h.finalize().as_bytes()
}

fn hash_inner(
    left: &[u8; MERKLE_ROOT_LEN],
    right: &[u8; MERKLE_ROOT_LEN],
) -> [u8; MERKLE_ROOT_LEN] {
    let mut h = Hasher::new();
    h.update(&[INNER_PREFIX]);
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
}

/// Compute the canonical `result_hash` index-column value
/// (`BLAKE3(merkle_root)`) per ADR-0037 §Decision.
#[must_use]
pub fn compute_result_hash(merkle_root: &[u8; MERKLE_ROOT_LEN]) -> [u8; MERKLE_ROOT_LEN] {
    *blake3::hash(merkle_root).as_bytes()
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
    use crate::ac_core::types::{OutputDirectoryDigest, OutputFileDigest};
    use corelink_hash::Digest;

    fn fresh_result(n_files: usize) -> ActionResult {
        let files = (0..n_files)
            .map(|i| {
                OutputFileDigest::new(
                    Digest::compute(format!("file-{i}").as_bytes()),
                    i64::try_from(i + 1).unwrap_or(1),
                )
            })
            .collect();
        ActionResult::new(files, Vec::new(), 0, b"raw".to_vec())
    }

    #[test]
    fn empty_tree_is_all_zeros() {
        let r = ActionResult::new(Vec::new(), Vec::new(), 0, Vec::new());
        let root = build_root(&r).unwrap();
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn single_leaf_root_matches_canonical_leaf_hash() {
        let r = fresh_result(1);
        let leaf_bytes = *r.output_files[0].digest.as_bytes();
        let expected_leaf = hash_leaf(&leaf_bytes);
        let root = build_root(&r).unwrap();
        assert_eq!(root, expected_leaf);
    }

    #[test]
    fn two_leaves_balance_canonically() {
        let r = fresh_result(2);
        let mut leaf_inputs: Vec<[u8; 32]> = r
            .output_files
            .iter()
            .map(|f| *f.digest.as_bytes())
            .collect();
        leaf_inputs.sort_unstable();
        let l0 = hash_leaf(&leaf_inputs[0]);
        let l1 = hash_leaf(&leaf_inputs[1]);
        let expected = hash_inner(&l0, &l1);
        let root = build_root(&r).unwrap();
        assert_eq!(root, expected);
    }

    #[test]
    fn determinism_across_repeated_builds() {
        let r = fresh_result(7);
        let root1 = build_root(&r).unwrap();
        for _ in 0..100 {
            let root_n = build_root(&r).unwrap();
            assert_eq!(root1, root_n);
        }
    }

    #[test]
    fn order_independence_via_lex_sort() {
        // Same digest set in two different insertion orders must
        // produce identical Merkle root — the canonical builder
        // lex-sorts internally.
        let mut a = fresh_result(5);
        let b = a.clone();
        a.output_files.reverse();
        let root_a = build_root(&a).unwrap();
        let root_b = build_root(&b).unwrap();
        assert_eq!(root_a, root_b);
    }

    #[test]
    fn tampering_in_any_leaf_changes_root() {
        let mut r = fresh_result(8);
        let baseline = build_root(&r).unwrap();
        // Flip a single byte in leaf #3's digest by replacing it
        // with a different content hash.
        let new_d = Digest::compute(b"tampered");
        r.output_files[3] = OutputFileDigest::new(new_d, r.output_files[3].size_bytes);
        let tampered = build_root(&r).unwrap();
        assert_ne!(baseline, tampered);
    }

    #[test]
    fn file_vs_directory_with_same_digest_share_a_leaf() {
        // Documented intentional behavior: the Merkle binding is
        // over the *digest set* without distinguishing files vs
        // directories — that classification is preserved in the
        // surrounding `raw_proto_bytes` echo, not in the Merkle
        // tree. Two outputs with the same digest collapse onto a
        // single leaf after lex-sort dedup is **not** done; both
        // contribute. Pinning current behavior so a future
        // refactor doesn't silently change the binding.
        let h = Digest::compute(b"shared");
        let r1 = ActionResult::new(vec![OutputFileDigest::new(h, 1)], Vec::new(), 0, Vec::new());
        let r2 = ActionResult::new(
            Vec::new(),
            vec![OutputDirectoryDigest::new(h, 2)],
            0,
            Vec::new(),
        );
        // Both produce the same single-leaf root (lex-sort produces
        // the same ordering of one element).
        assert_eq!(build_root(&r1).unwrap(), build_root(&r2).unwrap());
    }

    #[test]
    fn all_zero_digest_in_files_rejected() {
        let r = ActionResult::new(
            vec![OutputFileDigest::new(
                Digest::from_hex(&"00".repeat(32)).unwrap(),
                1,
            )],
            Vec::new(),
            0,
            Vec::new(),
        );
        let err = build_root(&r).unwrap_err();
        assert!(matches!(err, MerkleError::Malformed { .. }));
    }

    #[test]
    fn too_many_output_files_rejected() {
        // Construct synthetically; we don't actually allocate
        // 4097 distinct digests — we only need to trip the slice
        // length check before any hashing happens.
        let mut files = Vec::with_capacity(MAX_OUTPUT_FILES + 1);
        for i in 0..=MAX_OUTPUT_FILES {
            files.push(OutputFileDigest::new(
                Digest::compute(format!("f{i}").as_bytes()),
                1,
            ));
        }
        let r = ActionResult::new(files, Vec::new(), 0, Vec::new());
        let err = build_root(&r).unwrap_err();
        assert!(matches!(err, MerkleError::TooManyOutputFiles { .. }));
    }

    #[test]
    fn verify_root_matches_canonical() {
        let r = fresh_result(4);
        let root = build_root(&r).unwrap();
        verify_root(&r, &root).unwrap();
    }

    #[test]
    fn verify_root_rejects_mismatch() {
        let r = fresh_result(4);
        let mut root = build_root(&r).unwrap();
        // Flip a single bit in the claimed root.
        root[0] ^= 0x01;
        let err = verify_root(&r, &root).unwrap_err();
        assert_eq!(err, MerkleError::RootMismatch);
    }

    #[test]
    fn canonical_verifier_passes_well_formed() {
        let v = CanonicalMerkleVerifier::new();
        v.verify(&fresh_result(3)).unwrap();
    }

    #[test]
    fn compute_result_hash_is_blake3_of_root() {
        let r = fresh_result(2);
        let root = build_root(&r).unwrap();
        let rh = compute_result_hash(&root);
        let expected = *blake3::hash(&root).as_bytes();
        assert_eq!(rh, expected);
    }

    #[test]
    fn leaf_and_inner_are_domain_separated() {
        // A 32-byte input X must hash differently as a leaf vs as
        // half of an inner-node concat (with the other half being
        // the same X) — proves the prefix bytes prevent
        // second-preimage substitution.
        let x = [7u8; 32];
        let l = hash_leaf(&x);
        let i = hash_inner(&x, &x);
        assert_ne!(l, i);
        // And the leaf hash differs from the raw blake3 of just X
        // (without the prefix) — proves the leaf prefix is applied.
        let raw = *blake3::hash(&x).as_bytes();
        assert_ne!(l, raw);
    }
}
