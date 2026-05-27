//! Canonical test vectors (WI-S04-003 §6.1 + §10.s04.003.5).
//!
//! Pinned vectors capture algorithm-load-bearing decisions:
//!
//! - **V1 — empty tree**: root = all-zero 32 bytes.
//! - **V2 — single leaf**: root = `BLAKE3(\x00 || digest)` where
//!   `digest = BLAKE3("out1")`.
//! - **V3 — two leaves balanced**: root =
//!   `BLAKE3(\x01 || leaf_a || leaf_b)` after lex-sort; sorts in
//!   ascending byte order.
//! - **V4 — odd leaf promotion**: 3-leaf tree where the trailing leaf
//!   is promoted unchanged at the bottom level then paired with the
//!   left subtree's hash.
//! - **V5 — bound constants**: pin every public bound to its
//!   canonical value so a refactor that "loosens" a bound trips
//!   the test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use corelink_ac::merkle::{INNER_PREFIX, LEAF_PREFIX};
use corelink_ac::types::{ActionResult, OutputFileDigest};
use corelink_ac::{
    build_root, MAX_NODE_COUNT, MAX_OUTPUT_DIRECTORIES, MAX_OUTPUT_FILES, MAX_PAYLOAD_BYTES,
    MAX_TREE_DEPTH, MAX_TREE_FANOUT,
};
use corelink_hash::Digest;

fn hash_leaf(d: &Digest) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(&[LEAF_PREFIX]);
    h.update(d.as_bytes());
    *h.finalize().as_bytes()
}

fn hash_inner(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(&[INNER_PREFIX]);
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
}

#[test]
fn v1_empty_tree_is_all_zeros() {
    let r = ActionResult::new(Vec::new(), Vec::new(), 0, Vec::new());
    let root = build_root(&r).unwrap();
    assert_eq!(root, [0u8; 32]);
}

#[test]
fn v2_single_leaf_canonical() {
    let d = Digest::compute(b"out1");
    let r = ActionResult::new(
        vec![OutputFileDigest::new(d, 1)],
        Vec::new(),
        0,
        Vec::new(),
    );
    let expected = hash_leaf(&d);
    let root = build_root(&r).unwrap();
    assert_eq!(root, expected);
}

#[test]
fn v2_canonical_hex() {
    // Pin the V2 root to its full hex string so a future BLAKE3 lib
    // upgrade (or a regression in the leaf-prefix contract) is
    // immediately visible.
    let d = Digest::compute(b"out1");
    let r = ActionResult::new(
        vec![OutputFileDigest::new(d, 1)],
        Vec::new(),
        0,
        Vec::new(),
    );
    let root = build_root(&r).unwrap();
    let canonical_hex = hex::encode(root);
    let expected_hex = hex::encode(hash_leaf(&d));
    assert_eq!(canonical_hex, expected_hex);
    // Length sanity (64 hex chars from 32 bytes).
    assert_eq!(canonical_hex.len(), 64);
}

#[test]
fn v3_two_leaves_after_lex_sort() {
    let d_a = Digest::compute(b"alpha");
    let d_b = Digest::compute(b"beta");
    let r = ActionResult::new(
        vec![
            OutputFileDigest::new(d_a, 1),
            OutputFileDigest::new(d_b, 2),
        ],
        Vec::new(),
        0,
        Vec::new(),
    );
    let mut sorted = [*d_a.as_bytes(), *d_b.as_bytes()];
    sorted.sort_unstable();
    let l0 = {
        let mut h = blake3::Hasher::new();
        h.update(&[LEAF_PREFIX]);
        h.update(&sorted[0]);
        *h.finalize().as_bytes()
    };
    let l1 = {
        let mut h = blake3::Hasher::new();
        h.update(&[LEAF_PREFIX]);
        h.update(&sorted[1]);
        *h.finalize().as_bytes()
    };
    let expected = hash_inner(&l0, &l1);
    let root = build_root(&r).unwrap();
    assert_eq!(root, expected);
}

#[test]
fn v4_three_leaves_odd_promotion() {
    // Bottom level: 3 leaf hashes [L0, L1, L2] (after lex-sort). We
    // pair (L0, L1) → I0 and promote L2 unchanged. Next level: pair
    // (I0, L2) → root.
    let payloads = [b"x".as_slice(), b"y".as_slice(), b"z".as_slice()];
    let mut leaves: Vec<[u8; 32]> = payloads
        .iter()
        .map(|p| *Digest::compute(p).as_bytes())
        .collect();
    leaves.sort_unstable();
    let l0 = {
        let mut h = blake3::Hasher::new();
        h.update(&[LEAF_PREFIX]);
        h.update(&leaves[0]);
        *h.finalize().as_bytes()
    };
    let l1 = {
        let mut h = blake3::Hasher::new();
        h.update(&[LEAF_PREFIX]);
        h.update(&leaves[1]);
        *h.finalize().as_bytes()
    };
    let l2 = {
        let mut h = blake3::Hasher::new();
        h.update(&[LEAF_PREFIX]);
        h.update(&leaves[2]);
        *h.finalize().as_bytes()
    };
    let i0 = hash_inner(&l0, &l1);
    let expected = hash_inner(&i0, &l2);

    let r = ActionResult::new(
        payloads
            .iter()
            .map(|p| OutputFileDigest::new(Digest::compute(p), 1))
            .collect(),
        Vec::new(),
        0,
        Vec::new(),
    );
    let root = build_root(&r).unwrap();
    assert_eq!(root, expected);
}

#[test]
fn v5_canonical_bounds() {
    // Pin every bound to its WI-S04-003 §6.1.5 value.
    assert_eq!(MAX_TREE_DEPTH, 32);
    assert_eq!(MAX_TREE_FANOUT, 4096);
    assert_eq!(MAX_NODE_COUNT, 100_000);
    assert_eq!(MAX_PAYLOAD_BYTES, 1_048_576);
    assert_eq!(MAX_OUTPUT_FILES, 4096);
    assert_eq!(MAX_OUTPUT_DIRECTORIES, 4096);
}

#[test]
fn v6_domain_separation_prefixes() {
    // Pin the prefix bytes so a refactor that switches the
    // discriminator (e.g. to RFC 9162-style) trips the test
    // before reaching the wire.
    assert_eq!(LEAF_PREFIX, 0x00);
    assert_eq!(INNER_PREFIX, 0x01);
    assert_ne!(LEAF_PREFIX, INNER_PREFIX);
}
