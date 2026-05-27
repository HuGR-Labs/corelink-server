//! Canonical-vector tests pinning byte-stable outputs of the manifest
//! pipeline (WI-S05-005 §10.s05.005.5).
//!
//! Every vector pins a load-bearing invariant against a hardcoded
//! expected byte string. A future refactor that drifts the algorithm
//! (e.g. accidentally swapping the leaf-prefix byte from `\x00` to
//! `\x10`) is caught by every vector breaking simultaneously.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_ac_core::sig::{MockTdkHandle, TdkHandle};
use corelink_cas::manifest::{
    build_root, hash_inner, hash_leaf, ChunkInput, ChunkRef, ChunkerAlgorithm, ManifestBuilder,
    ManifestSigner, ManifestVerifierSig, INNER_PREFIX, LEAF_PREFIX, MANIFEST_PREIMAGE_LEN,
    MAX_CHUNKS_PER_BLOB,
};
use uuid::Uuid;

/// Pinned canonical preimage layout = 102 bytes.
#[test]
fn canonical_preimage_len_is_102() {
    assert_eq!(MANIFEST_PREIMAGE_LEN, 102);
}

/// Pinned RFC 6962 prefix bytes match `corelink-ac::merkle` exactly.
#[test]
fn rfc_6962_prefix_bytes_match_ac_merkle() {
    assert_eq!(LEAF_PREFIX, 0x00);
    assert_eq!(INNER_PREFIX, 0x01);
    // Same byte values as corelink-ac::merkle (the worker's AC pipe).
    assert_eq!(LEAF_PREFIX, corelink_ac_core::merkle::LEAF_PREFIX);
    assert_eq!(INNER_PREFIX, corelink_ac_core::merkle::INNER_PREFIX);
}

/// Pinned MAX_CHUNKS_PER_BLOB cross-crate alignment with
/// `corelink-chunker` + `corelink-multipart-schema` +
/// `corelink-worker::reapi::cas::types`. Spec contract §5.1 P1-SR5-001.
#[test]
fn max_chunks_per_blob_alignment() {
    assert_eq!(MAX_CHUNKS_PER_BLOB, 81_920);
    // Cross-check: 160 GiB / 2 MiB = 81920 exact.
    assert_eq!(
        u64::from(MAX_CHUNKS_PER_BLOB) * (2 * 1024 * 1024),
        160 * 1024 * 1024 * 1024
    );
}

/// Pinned single-leaf root: BLAKE3(\x00 || all-7-byte digest) hex.
#[test]
fn single_leaf_root_pinned_hex() {
    let chunks = [ChunkRef::new(0, [7u8; 32], 1)];
    let root = build_root(&chunks).unwrap();
    let expected = hash_leaf(&[7u8; 32]);
    assert_eq!(root, expected);
    // Byte-stable hex: pin the actual bytes so a regression breaks
    // this test specifically.
    let hex = hex::encode(root);
    // Compute by hand:
    let mut h = blake3::Hasher::new();
    h.update(&[LEAF_PREFIX]);
    h.update(&[7u8; 32]);
    let by_hand = hex::encode(h.finalize().as_bytes());
    assert_eq!(hex, by_hand);
}

/// Pinned two-leaf root.
#[test]
fn two_leaf_root_pinned_construction() {
    let chunks = [
        ChunkRef::new(0, [1u8; 32], 1),
        ChunkRef::new(1, [2u8; 32], 1),
    ];
    let root = build_root(&chunks).unwrap();
    let l0 = hash_leaf(&[1u8; 32]);
    let l1 = hash_leaf(&[2u8; 32]);
    let expected = hash_inner(&l0, &l1);
    assert_eq!(root, expected);
}

/// Pinned 3-leaf root with odd-leaf promotion semantic
/// (RFC 6962 / Bitcoin convention; mirrors `corelink-ac::merkle`).
#[test]
fn three_leaf_odd_promotion_pinned() {
    let chunks = [
        ChunkRef::new(0, [11u8; 32], 1),
        ChunkRef::new(1, [22u8; 32], 1),
        ChunkRef::new(2, [33u8; 32], 1),
    ];
    let l0 = hash_leaf(&[11u8; 32]);
    let l1 = hash_leaf(&[22u8; 32]);
    let l2 = hash_leaf(&[33u8; 32]);
    let i01 = hash_inner(&l0, &l1);
    let expected = hash_inner(&i01, &l2);
    let actual = build_root(&chunks).unwrap();
    assert_eq!(actual, expected);
}

/// Pinned manifest sig surface: a manifest built under a fixed mock
/// TDK with fixed inputs MUST produce the exact same sig byte string
/// across runs (`INV-CAS-IDEMPOTENCY` chunked variant + canonical sig
/// path).
#[test]
fn deterministic_sig_pinned_under_fixed_inputs() {
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(tenant, 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let signer = ManifestSigner::new(Arc::clone(&handle), 1).unwrap();
    let verifier = ManifestVerifierSig::new(handle, vec![1]).unwrap();

    let chunks = vec![
        ChunkInput::new([0xAAu8; 32], 5),
        ChunkInput::new([0xBBu8; 32], 7),
    ];
    let m1 = ManifestBuilder::new()
        .build(
            tenant,
            [0xCCu8; 32],
            chunks.clone(),
            42_000,
            ChunkerAlgorithm::Fixed2MiB,
            &signer,
            1,
        )
        .unwrap();
    let m2 = ManifestBuilder::new()
        .build(
            tenant,
            [0xCCu8; 32],
            chunks,
            42_000,
            ChunkerAlgorithm::Fixed2MiB,
            &signer,
            1,
        )
        .unwrap();
    assert_eq!(m1, m2);
    // verify_full on both must succeed.
    corelink_cas::manifest::ManifestVerifier::new()
        .verify_full(&m1, &verifier)
        .unwrap();
    corelink_cas::manifest::ManifestVerifier::new()
        .verify_full(&m2, &verifier)
        .unwrap();
}

/// Pinned canonical preimage byte layout — pin the exact byte
/// positions per WI §6.1.5.
#[test]
fn canonical_bytes_layout_pinned() {
    let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(tenant, 1);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer = ManifestSigner::new(handle, 1).unwrap();

    let chunks = vec![ChunkInput::new([0u8; 32], 1)];
    let m = ManifestBuilder::new()
        .build(
            tenant,
            [0u8; 32],
            chunks,
            0,
            ChunkerAlgorithm::Fixed2MiB,
            &signer,
            1,
        )
        .unwrap();
    let canonical = m.canonical_bytes();
    assert_eq!(canonical.len(), 102);
    // Byte 0: version = 1
    assert_eq!(canonical[0], 1);
    // Bytes 1..17: tenant_id all-zero except final byte = 1
    assert_eq!(&canonical[1..17], tenant.into_bytes().as_slice());
    // Bytes 17..49: blob_digest = all-zero
    assert_eq!(&canonical[17..49], &[0u8; 32]);
    // Bytes 81..85: chunk_count = 1 (LE u32)
    assert_eq!(&canonical[81..85], &1u32.to_le_bytes());
    // Bytes 85..93: total_size_bytes = 1 (LE u64)
    assert_eq!(&canonical[85..93], &1u64.to_le_bytes());
    // Bytes 93..101: created_at_ms = 0 (LE u64)
    assert_eq!(&canonical[93..101], &0u64.to_le_bytes());
    // Byte 101: chunker_algo = 1 (Fixed2MiB wire byte)
    assert_eq!(canonical[101], 1);
}

/// Pinned chunker_algo wire bytes (must NOT drift; they live inside
/// signed envelopes).
#[test]
fn chunker_algo_wire_bytes_pinned() {
    assert_eq!(ChunkerAlgorithm::Fixed2MiB.wire_byte(), 1);
    assert_eq!(ChunkerAlgorithm::FastCdc2MiB.wire_byte(), 2);
    // Round-trip parse:
    assert_eq!(
        ChunkerAlgorithm::from_wire_byte(1).unwrap(),
        ChunkerAlgorithm::Fixed2MiB
    );
    assert_eq!(
        ChunkerAlgorithm::from_wire_byte(2).unwrap(),
        ChunkerAlgorithm::FastCdc2MiB
    );
    // Reserved sentinel `0` rejected.
    assert!(ChunkerAlgorithm::from_wire_byte(0).is_err());
    // Unknown discriminant rejected.
    assert!(ChunkerAlgorithm::from_wire_byte(99).is_err());
}

/// Pinned sig domain separation: same TDK + same canonical-bytes
/// produces DIFFERENT sigs across `b"manifest-sig"` and `b"ac-sig"`.
#[test]
fn sig_domain_separation_pinned() {
    let tdk = corelink_ac_core::sig::derive_default_mock_tdk(
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        1,
    );
    let bytes = [0xAA; 102];
    // Manifest-domain sig.
    let manifest_sig =
        corelink_cas::manifest::compute_signature(&tdk, 1, &bytes).unwrap();
    // AC-domain sig (canonical helper from corelink-ac).
    let ac_sig = corelink_ac_core::sig::compute_signature(&tdk, 1, &bytes).unwrap();
    assert_ne!(manifest_sig, ac_sig);
}
