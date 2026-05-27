//! Manifest-side Merkle tree (WI-S05-005 §6.1, §9.2).
//!
//! Tree shape (BLAKE3-256 + RFC 6962-style domain separation;
//! chunk-index ascending — NOT lex-sorted):
//!
//! - **Leaves**: `BLAKE3(\x00 || chunk_digest_bytes)` for each chunk in
//!   `chunks[i].index` ascending order. The `\x00` prefix is the
//!   domain-separation tag preventing the second-preimage tree-shape
//!   substitution.
//! - **Inner nodes**: balanced binary tree built bottom-up. At each
//!   level, pairs `(left, right)` hash to `BLAKE3(\x01 || left ||
//!   right)`. An odd trailing leaf at a level is **promoted unchanged**
//!   to the next level (Bitcoin / RFC 6962 convention) — the `\x01`
//!   prefix discriminates inner from leaf so no second-preimage
//!   substitution is possible.
//! - **Order is semantic, NOT canonical**: in `corelink-ac` Merkle
//!   (WI-S04-003) the tree-builder lex-sorts leaves by digest bytes
//!   because the input is an unordered output_files / output_directories
//!   *set*. In the multipart manifest the chunk order is semantic —
//!   `concat(chunks[i].bytes for i in 0..N)` is the blob — so swapping
//!   chunk #2 and chunk #5 produces a DIFFERENT blob with a different
//!   `blob_digest`. The Merkle tree therefore preserves chunk order:
//!   shuffling the input → different `merkle_root` → bind detected.

use blake3::Hasher;

use crate::manifest::bounds::MAX_CHUNKS_PER_BLOB;
use crate::manifest::error::ManifestError;
use crate::manifest::types::{ChunkRef, DIGEST_LEN};

/// Domain-separation prefix byte for leaf hashes (RFC 6962-style).
/// Mirrors `corelink-ac::merkle::LEAF_PREFIX` byte-equal so a future
/// audit cross-check (e.g. an SBOM-tier scan) can prove byte-identity
/// across both Merkle surfaces.
pub const LEAF_PREFIX: u8 = 0x00;

/// Domain-separation prefix byte for inner-node hashes (RFC 6962-style).
pub const INNER_PREFIX: u8 = 0x01;

/// Compute the canonical Merkle root over the `chunks` slice. The
/// caller MUST have verified that `chunks` is non-empty + chunk_index
/// is strictly ascending `0..chunks.len()` BEFORE calling this — those
/// pre-conditions are the verifier's job and are surfaced via
/// [`ManifestError::Empty`] / [`ManifestError::ChunkIndexOutOfOrder`]
/// at the verifier level. This function pins the cripto invariants
/// only.
///
/// # Errors
///
/// - [`ManifestError::ChunkCountExceeded`] when `chunks.len() >
///   MAX_CHUNKS_PER_BLOB`.
pub fn build_root(chunks: &[ChunkRef]) -> Result<[u8; DIGEST_LEN], ManifestError> {
    let count = chunks.len();
    let count_u32 = u32::try_from(count).map_err(|_| ManifestError::ChunkCountExceeded {
        found: u32::MAX,
    })?;
    if count_u32 > MAX_CHUNKS_PER_BLOB {
        return Err(ManifestError::ChunkCountExceeded { found: count_u32 });
    }
    if count == 0 {
        return Err(ManifestError::Empty);
    }

    // Hash leaves in canonical (chunk-index ascending) order.
    let mut level: Vec<[u8; DIGEST_LEN]> = chunks.iter().map(|c| hash_leaf(&c.digest)).collect();
    while level.len() > 1 {
        let mut next: Vec<[u8; DIGEST_LEN]> = Vec::with_capacity(level.len().div_ceil(2));
        let mut iter = level.chunks_exact(2);
        for pair in &mut iter {
            // chunks_exact yields slices of length exactly 2; first +
            // last accessors avoid the clippy-denied `[i]` indexing.
            let left = pair.first().copied().ok_or(ManifestError::Empty)?;
            let right = pair.last().copied().ok_or(ManifestError::Empty)?;
            next.push(hash_inner(&left, &right));
        }
        // Odd trailing leaf — promote unchanged (RFC 6962 / Bitcoin
        // convention; the `\x01` inner prefix prevents the second-
        // preimage substitution).
        if let [tail] = iter.remainder() {
            next.push(*tail);
        }
        level = next;
    }
    level.into_iter().next().ok_or(ManifestError::Empty)
}

/// Hash a leaf with the canonical `\x00` domain-separation prefix.
#[must_use]
pub fn hash_leaf(chunk_digest: &[u8; DIGEST_LEN]) -> [u8; DIGEST_LEN] {
    let mut h = Hasher::new();
    h.update(&[LEAF_PREFIX]);
    h.update(chunk_digest);
    *h.finalize().as_bytes()
}

/// Hash an inner node with the canonical `\x01` domain-separation
/// prefix.
#[must_use]
pub fn hash_inner(
    left: &[u8; DIGEST_LEN],
    right: &[u8; DIGEST_LEN],
) -> [u8; DIGEST_LEN] {
    let mut h = Hasher::new();
    h.update(&[INNER_PREFIX]);
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
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

    fn cr(idx: u32, seed: u8) -> ChunkRef {
        ChunkRef::new(idx, [seed; DIGEST_LEN], 1)
    }

    #[test]
    fn empty_rejected() {
        let err = build_root(&[]).unwrap_err();
        assert_eq!(err, ManifestError::Empty);
    }

    #[test]
    fn single_chunk_root_matches_leaf_hash() {
        let chunks = [cr(0, 7)];
        let root = build_root(&chunks).unwrap();
        let expected = hash_leaf(&[7u8; DIGEST_LEN]);
        assert_eq!(root, expected);
    }

    #[test]
    fn two_chunks_root_matches_inner() {
        let chunks = [cr(0, 1), cr(1, 2)];
        let l0 = hash_leaf(&[1u8; DIGEST_LEN]);
        let l1 = hash_leaf(&[2u8; DIGEST_LEN]);
        let expected = hash_inner(&l0, &l1);
        let root = build_root(&chunks).unwrap();
        assert_eq!(root, expected);
    }

    #[test]
    fn chunk_order_is_semantic_not_canonical() {
        // Two manifests with the SAME chunk digest set but different
        // ORDER must produce DIFFERENT roots — proves order is
        // semantic. Contrast with `corelink-ac::merkle` which lex-sorts
        // because the AC input is an unordered set.
        let asc = [cr(0, 1), cr(1, 2), cr(2, 3)];
        let mut shuffled = asc;
        shuffled.swap(0, 2);
        let r_asc = build_root(&asc).unwrap();
        let r_shuffled = build_root(&shuffled).unwrap();
        assert_ne!(r_asc, r_shuffled);
    }

    #[test]
    fn determinism_across_repeated_builds() {
        let chunks: Vec<_> = (0..7).map(|i| cr(i, i as u8)).collect();
        let baseline = build_root(&chunks).unwrap();
        for _ in 0..100 {
            assert_eq!(build_root(&chunks).unwrap(), baseline);
        }
    }

    #[test]
    fn chunk_count_exceeded_rejected() {
        // We don't actually allocate 81921 chunks (would blow heap);
        // instead pin the boundary at a smaller MAX via the test API.
        // The runtime check is `count > MAX_CHUNKS_PER_BLOB`; build a
        // Vec sized just over the bound to trip it. 81921 × 33 bytes ≈
        // 2.7 MB — fine for a single test.
        let chunks: Vec<ChunkRef> = (0..(MAX_CHUNKS_PER_BLOB as usize + 1))
            .map(|i| {
                let idx = u32::try_from(i & 0xFFFF_FFFF).unwrap_or(0);
                cr(idx, (i & 0xFF) as u8)
            })
            .collect();
        let err = build_root(&chunks).unwrap_err();
        match err {
            ManifestError::ChunkCountExceeded { found } => {
                assert_eq!(found, MAX_CHUNKS_PER_BLOB + 1);
            }
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn leaf_and_inner_are_domain_separated() {
        let x = [9u8; DIGEST_LEN];
        let l = hash_leaf(&x);
        let i = hash_inner(&x, &x);
        assert_ne!(l, i);
        let raw = *blake3::hash(&x).as_bytes();
        assert_ne!(l, raw);
    }

    #[test]
    fn tampering_a_leaf_changes_root() {
        let mut chunks = [cr(0, 1), cr(1, 2), cr(2, 3), cr(3, 4)];
        let baseline = build_root(&chunks).unwrap();
        chunks[2].digest[0] ^= 0x01;
        let tampered = build_root(&chunks).unwrap();
        assert_ne!(baseline, tampered);
    }

    #[test]
    fn odd_leaf_count_promoted_unchanged() {
        // 3 leaves: [L0, L1, L2] → level1 = [I(L0,L1), L2] → level2
        // = [I(I(L0,L1), L2)]. Recompute by hand and assert.
        let chunks = [cr(0, 11), cr(1, 22), cr(2, 33)];
        let l0 = hash_leaf(&[11u8; DIGEST_LEN]);
        let l1 = hash_leaf(&[22u8; DIGEST_LEN]);
        let l2 = hash_leaf(&[33u8; DIGEST_LEN]);
        let i01 = hash_inner(&l0, &l1);
        let expected = hash_inner(&i01, &l2);
        let actual = build_root(&chunks).unwrap();
        assert_eq!(actual, expected);
    }
}
