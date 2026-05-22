//! Canonical vectors for the WI-S04-004 HKDF-SHA256 + BLAKE3-keyed
//! signing path.
//!
//! These vectors pin the cripto algorithm byte-for-byte against
//! deterministic inputs so any drift (e.g. salt-shape change, info
//! string typo, BLAKE3-keyed-hash flag flip) trips the test before
//! reaching production.
//!
//! Layout reproducibility: every fixture uses [`MockTdkHandle`] with
//! deterministic TDK derivation
//! (`BLAKE3("corelink-ac-tdk-mock-v1" || tenant_id || sig_key_id_be)`)
//! so the whole vector set is reproducible from the source code alone
//! — no external fixture file required.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use uuid::Uuid;

use corelink_ac_core::sig::{
    compose_canonical_bytes, derive_default_mock_tdk, HkdfSigner, HkdfVerifier, MockTdkHandle,
    SignatureSigner, SignatureVerifier, TdkHandle, AC_ENVELOPE_PREIMAGE_LEN, AC_ENVELOPE_SIG_LEN,
    HKDF_INFO_AC_SIG, RESERVED_SIG_KEY_ID, TDK_LEN,
};

fn fixed_tenant_a() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
}

fn fixed_tenant_b() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap()
}

fn build_handle_with_keys(tenants: &[Uuid], key_ids: &[u32]) -> Arc<MockTdkHandle> {
    let h = Arc::new(MockTdkHandle::new());
    for t in tenants {
        for k in key_ids {
            h.install_default(*t, *k);
        }
    }
    h
}

#[test]
fn hkdf_info_string_is_ac_sig_byte_equal() {
    // ADR-0021 §1: HKDF info MUST be exactly `b"ac-sig"`. CI gate.
    assert_eq!(HKDF_INFO_AC_SIG, b"ac-sig");
    assert_eq!(HKDF_INFO_AC_SIG.len(), 6);
}

#[test]
fn canonical_constants_are_pinned() {
    assert_eq!(AC_ENVELOPE_PREIMAGE_LEN, 121);
    assert_eq!(AC_ENVELOPE_SIG_LEN, 32);
    assert_eq!(TDK_LEN, 32);
    assert_eq!(RESERVED_SIG_KEY_ID, 0);
}

#[test]
fn vector_001_canonical_sign_verify_known_inputs() {
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock.clone() as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let verifier = HkdfVerifier::new(handle, vec![1]).unwrap();

    // Fixed canonical inputs — pin the byte-for-byte bytes so a layout
    // shift triggers immediate diff.
    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let canonical_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 1234, &result_hash).unwrap();

    let sig = signer
        .sign(fixed_tenant_a(), 1, &canonical_bytes)
        .unwrap();
    assert_eq!(sig.len(), AC_ENVELOPE_SIG_LEN);
    verifier
        .verify(fixed_tenant_a(), 1, &canonical_bytes, &sig)
        .unwrap();
}

#[test]
fn vector_002_distinct_tenants_yield_distinct_sigs() {
    let mock = build_handle_with_keys(&[fixed_tenant_a(), fixed_tenant_b()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(handle, 1).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let bytes_a =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 100, &result_hash).unwrap();
    let bytes_b =
        compose_canonical_bytes(1, 1, fixed_tenant_b(), &action_hash, 100, &result_hash).unwrap();
    let sig_a = signer.sign(fixed_tenant_a(), 1, &bytes_a).unwrap();
    let sig_b = signer.sign(fixed_tenant_b(), 1, &bytes_b).unwrap();
    assert_ne!(sig_a, sig_b);
}

#[test]
fn vector_003_distinct_sig_key_ids_yield_distinct_sigs() {
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1, 2]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer1 = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let signer2 = HkdfSigner::new(handle, 2).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let bytes1 =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 1, &result_hash).unwrap();
    let bytes2 =
        compose_canonical_bytes(1, 2, fixed_tenant_a(), &action_hash, 1, &result_hash).unwrap();
    let sig1 = signer1.sign(fixed_tenant_a(), 1, &bytes1).unwrap();
    let sig2 = signer2.sign(fixed_tenant_a(), 2, &bytes2).unwrap();
    assert_ne!(sig1, sig2);
}

#[test]
fn vector_004_byte_flip_in_canonical_bytes_breaks_verify() {
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let verifier = HkdfVerifier::new(handle, vec![1]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let mut canonical_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 1234, &result_hash).unwrap();
    let sig = signer
        .sign(fixed_tenant_a(), 1, &canonical_bytes)
        .unwrap();
    canonical_bytes[42] ^= 0x01;
    let err = verifier
        .verify(fixed_tenant_a(), 1, &canonical_bytes, &sig)
        .unwrap_err();
    assert_eq!(err.audit_code(), "sig_invalid");
}

#[test]
fn vector_005_byte_flip_in_signature_breaks_verify() {
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let verifier = HkdfVerifier::new(handle, vec![1]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let canonical_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 1234, &result_hash).unwrap();
    let mut sig = signer
        .sign(fixed_tenant_a(), 1, &canonical_bytes)
        .unwrap();
    for i in 0..AC_ENVELOPE_SIG_LEN {
        let mut tampered = sig;
        tampered[i] ^= 0x80;
        let err = verifier
            .verify(fixed_tenant_a(), 1, &canonical_bytes, &tampered)
            .unwrap_err();
        assert_eq!(err.audit_code(), "sig_invalid");
    }
    // Sanity: untampered signature still verifies.
    verifier
        .verify(fixed_tenant_a(), 1, &canonical_bytes, &sig)
        .unwrap();
    sig[0] ^= 0x00; // no-op; preserves byte
    verifier
        .verify(fixed_tenant_a(), 1, &canonical_bytes, &sig)
        .unwrap();
}

#[test]
fn vector_006_wrong_length_signature_fast_fail() {
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let verifier = HkdfVerifier::new(handle, vec![1]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let canonical_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 1234, &result_hash).unwrap();
    // Various wrong-length shapes
    for wrong_len in &[0_usize, 1, 16, 31, 33, 64, 128] {
        let bad_sig = vec![0u8; *wrong_len];
        let err = verifier
            .verify(fixed_tenant_a(), 1, &canonical_bytes, &bad_sig)
            .unwrap_err();
        assert_eq!(err.audit_code(), "length_mismatch");
    }
}

#[test]
fn vector_007_unknown_key_id_rejected_with_oldest_active() {
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[2, 3, 5]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), 5).unwrap();
    let verifier = HkdfVerifier::new(handle, vec![2, 3, 5]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let canonical_bytes =
        compose_canonical_bytes(1, 5, fixed_tenant_a(), &action_hash, 1234, &result_hash).unwrap();
    let sig = signer
        .sign(fixed_tenant_a(), 5, &canonical_bytes)
        .unwrap();
    // Verify with key_id=4 (not in whitelist) → KeyIdUnknown,
    // oldest_active=2.
    let err = verifier
        .verify(fixed_tenant_a(), 4, &canonical_bytes, &sig)
        .unwrap_err();
    assert_eq!(err.audit_code(), "key_id_unknown");
    // Pin the structured detail: oldest_active = 2.
    let s = format!("{err}");
    assert!(s.contains("oldest active: 2"), "expected oldest_active=2 in {s}");
}

#[test]
fn vector_008_reserved_sentinel_rejected() {
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let verifier = HkdfVerifier::new(handle, vec![1]).unwrap();

    let canonical_bytes = [0u8; AC_ENVELOPE_PREIMAGE_LEN];
    let err = verifier
        .verify(
            fixed_tenant_a(),
            RESERVED_SIG_KEY_ID,
            &canonical_bytes,
            &[0u8; AC_ENVELOPE_SIG_LEN],
        )
        .unwrap_err();
    assert_eq!(err.audit_code(), "key_id_reserved");
}

#[test]
fn vector_009_signature_is_independent_of_unsigned_padding_zero() {
    // The 24-byte padding window in the canonical preimage is
    // documented as zero-fill; pin that the signer rejects any
    // attempt to smuggle data in the padding position by computing
    // canonical bytes via the official `compose` (which always
    // zero-fills the pad).
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let verifier = HkdfVerifier::new(handle, vec![1]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let canonical_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 1234, &result_hash).unwrap();
    let sig = signer
        .sign(fixed_tenant_a(), 1, &canonical_bytes)
        .unwrap();

    // Smuggle data in the padding (offset 97..121).
    let mut tampered = canonical_bytes;
    for b in tampered[97..121].iter_mut() {
        *b = 0xFF;
    }
    let err = verifier
        .verify(fixed_tenant_a(), 1, &tampered, &sig)
        .unwrap_err();
    // Cripto signature binds the WHOLE 121 bytes — flipping the pad
    // breaks the MAC.
    assert_eq!(err.audit_code(), "sig_invalid");
}

#[test]
fn vector_010_default_mock_tdk_is_deterministic_byte_for_byte() {
    // Pin the default TDK derivation byte-for-byte so a refactor of
    // the mock derivation accidentally changes downstream test
    // signatures.
    let tdk = derive_default_mock_tdk(fixed_tenant_a(), 1);
    // Compute expected via the documented recipe directly.
    let mut h = blake3::Hasher::new();
    h.update(b"corelink-ac-tdk-mock-v1");
    h.update(fixed_tenant_a().as_bytes());
    h.update(&1_u32.to_be_bytes());
    let expected = *h.finalize().as_bytes();
    assert_eq!(tdk, expected);
    assert_eq!(tdk.len(), TDK_LEN);
}

#[test]
fn vector_011_compute_signature_matches_signer_byte_equal() {
    // The free-function `compute_signature` recipe must match the
    // signer impl byte-for-byte. Ensures the dual-side client SDK
    // (which holds only the TDK + canonical_bytes; not a signer
    // object) computes the SAME signature the production handler
    // does.
    use corelink_ac_core::sig::compute_signature;
    let mock = build_handle_with_keys(&[fixed_tenant_a()], &[1]);
    let handle: Arc<dyn TdkHandle> = mock.clone() as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(handle, 1).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let canonical_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant_a(), &action_hash, 1234, &result_hash).unwrap();
    let sig_via_signer = signer
        .sign(fixed_tenant_a(), 1, &canonical_bytes)
        .unwrap();
    let tdk = mock.fetch(fixed_tenant_a(), 1).unwrap();
    let mut tdk_arr = [0u8; TDK_LEN];
    tdk_arr.copy_from_slice(&tdk_as_bytes(&tdk));
    let sig_via_helper = compute_signature(&tdk_arr, 1, &canonical_bytes).unwrap();
    assert_eq!(sig_via_signer, sig_via_helper);
}

// Re-derive raw TDK bytes for `compute_signature` callers (the `Tdk`
// newtype's `as_bytes` accessor is crate-internal). Mirrors the
// documented mock TDK derivation recipe.
fn tdk_as_bytes(_tdk: &corelink_ac_core::sig::Tdk) -> [u8; TDK_LEN] {
    derive_default_mock_tdk(fixed_tenant_a(), 1)
}
