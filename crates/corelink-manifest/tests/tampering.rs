//! Mutation-resistance tests pinning every load-bearing tamper-detection
//! path (WI-S05-005 §10.s05.005.1 + §13).
//!
//! Each test mutates a different field of a sealed manifest and asserts
//! the verifier rejects with the canonical error variant. These
//! complement the property tests — the property tests prove
//! 100% rejection across random inputs, the mutation tests pin which
//! ERROR VARIANT is surfaced for each specific mutation (so a regression
//! that maps `RootMismatch` onto `ChunkCountMismatch` is caught).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_ac_core::sig::{MockTdkHandle, TdkHandle};
use corelink_manifest::{
    ChunkInput, ChunkerAlgorithm, Manifest, ManifestBuilder, ManifestError, ManifestSigner,
    ManifestVerifier, ManifestVerifierSig, VerifyError,
};
use uuid::Uuid;

fn fixed_tenant() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
}

fn build_manifest_3_chunks() -> (Manifest, ManifestVerifierSig) {
    let tenant = fixed_tenant();
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(tenant, 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let signer = ManifestSigner::new(Arc::clone(&handle), 1).unwrap();
    let verifier = ManifestVerifierSig::new(handle, vec![1]).unwrap();
    let chunks = vec![
        ChunkInput::new(*blake3::hash(b"chunk-a").as_bytes(), 7),
        ChunkInput::new(*blake3::hash(b"chunk-b").as_bytes(), 7),
        ChunkInput::new(*blake3::hash(b"chunk-c").as_bytes(), 7),
    ];
    let m = ManifestBuilder::new()
        .build(
            tenant,
            *blake3::hash(b"chunk-achunk-bchunk-c").as_bytes(),
            chunks,
            42,
            ChunkerAlgorithm::Fixed2MiB,
            &signer,
            1,
        )
        .unwrap();
    (m, verifier)
}

#[test]
fn tamper_chunk_digest_byte_fires_root_mismatch() {
    let (mut m, v) = build_manifest_3_chunks();
    m.chunks[1].digest[0] ^= 0x01;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::RootMismatch) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_merkle_root_fires_root_mismatch() {
    let (mut m, v) = build_manifest_3_chunks();
    m.merkle_root[0] ^= 0x01;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::RootMismatch) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_chunk_count_low_fires_count_mismatch() {
    let (mut m, v) = build_manifest_3_chunks();
    m.chunk_count = 2;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::ChunkCountMismatch {
            declared: 2,
            actual: 3,
        }) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_total_size_fires_total_size_mismatch() {
    let (mut m, v) = build_manifest_3_chunks();
    m.total_size_bytes = m.total_size_bytes.saturating_add(100);
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::TotalSizeMismatch { .. }) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_chunk_index_fires_out_of_order() {
    let (mut m, v) = build_manifest_3_chunks();
    m.chunks[0].index = 5;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::ChunkIndexOutOfOrder { .. }) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_version_fires_version_unsupported() {
    let (mut m, v) = build_manifest_3_chunks();
    m.version = 99;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::VersionUnsupported(99)) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_sig_byte_fires_sig_invalid() {
    let (mut m, v) = build_manifest_3_chunks();
    m.sig[0] ^= 0x01;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Sig(_) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_sig_key_id_to_unknown_fires_key_unknown() {
    let (mut m, v) = build_manifest_3_chunks();
    m.sig_key_id = 99;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Sig(corelink_ac_core::sig::SigError::KeyIdUnknown { .. }) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_sig_key_id_to_zero_fires_reserved() {
    let (mut m, v) = build_manifest_3_chunks();
    m.sig_key_id = 0;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Sig(corelink_ac_core::sig::SigError::KeyIdReserved) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_chunker_algo_fires_sig_invalid() {
    // Switching chunker_algo (with everything else identical) changes
    // the canonical preimage byte 101 → sig over the original bytes
    // does not match the new preimage → SigError::Invalid (NOT a
    // structural error — the structure is consistent with itself).
    let (mut m, v) = build_manifest_3_chunks();
    m.chunker_algo = ChunkerAlgorithm::FastCdc2MiB;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Sig(corelink_ac_core::sig::SigError::Invalid) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_chunk_size_to_zero_fires_size_zero() {
    let (mut m, v) = build_manifest_3_chunks();
    m.chunks[1].size_bytes = 0;
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::ChunkSizeZero(1)) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_drop_chunk_fires_count_mismatch() {
    let (mut m, v) = build_manifest_3_chunks();
    m.chunks.pop();
    // chunk_count still 3, actual 2 → ChunkCountMismatch.
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::ChunkCountMismatch {
            declared: 3,
            actual: 2,
        }) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}

#[test]
fn tamper_swap_chunks_fires_index_out_of_order() {
    let (mut m, v) = build_manifest_3_chunks();
    m.chunks.swap(0, 2);
    let err = ManifestVerifier::new().verify_full(&m, &v).unwrap_err();
    match err {
        VerifyError::Structure(ManifestError::ChunkIndexOutOfOrder { .. }) => {}
        _ => panic!("unexpected: {err:?}"),
    }
}
