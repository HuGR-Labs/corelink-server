//! Key rotation forward-compatibility integration tests
//! (WI-S04-004 §10.s04.004.8 + §30.1).
//!
//! Validates the canonical 30-day rotation procedure:
//!
//! 1. Pre-rotation: signer issues envelopes with `current_key_id = N`;
//!    verifier whitelist `[N]`; both verify OK.
//! 2. Rotation event: signer rotates to `N+1`; verifier whitelist
//!    extended to `[N, N+1]`; both pre- and post-rotation envelopes
//!    verify OK during grace.
//! 3. Grace expiry: verifier retires `N`; pre-rotation envelopes now
//!    fail with `KeyIdUnknown`; post-rotation envelopes still verify.
//! 4. Schema invariant: `sig_key_id != 0` enforced at every step.

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
    compose_canonical_bytes, HkdfSigner, HkdfVerifier, MockTdkHandle, SigError, SignatureSigner,
    SignatureVerifier, TdkHandle, RESERVED_SIG_KEY_ID,
};

fn fixed_tenant() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
}

#[test]
fn rotation_lifecycle_admit_then_retire() {
    let mock = Arc::new(MockTdkHandle::new());
    // Pre-rotation: only key 1 exists.
    mock.install_default(fixed_tenant(), 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let mut signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let mut verifier = HkdfVerifier::new(Arc::clone(&handle), vec![1]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let pre_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant(), &action_hash, 100, &result_hash).unwrap();
    let pre_sig = signer.sign(fixed_tenant(), 1, &pre_bytes).unwrap();

    // Step 1: rotation event — install new TDK + admit verifier.
    mock.install_default(fixed_tenant(), 2);
    signer.rotate_to(2).unwrap();
    verifier.admit(2).unwrap();
    assert_eq!(verifier.accepted_key_ids(), &[1, 2]);

    let post_bytes =
        compose_canonical_bytes(1, 2, fixed_tenant(), &action_hash, 100, &result_hash).unwrap();
    let post_sig = signer.sign(fixed_tenant(), 2, &post_bytes).unwrap();

    // Step 2: grace window — both verify OK.
    verifier.verify(fixed_tenant(), 1, &pre_bytes, &pre_sig).unwrap();
    verifier
        .verify(fixed_tenant(), 2, &post_bytes, &post_sig)
        .unwrap();

    // Step 3: grace expiry — retire key 1.
    verifier.retire(1).unwrap();
    assert_eq!(verifier.accepted_key_ids(), &[2]);

    // Pre-rotation envelopes now rejected.
    let err = verifier.verify(fixed_tenant(), 1, &pre_bytes, &pre_sig).unwrap_err();
    match err {
        SigError::KeyIdUnknown { sig_key_id: 1, oldest_active: 2 } => {}
        _ => panic!("expected KeyIdUnknown, got {err:?}"),
    }
    // Post-rotation envelopes still OK.
    verifier.verify(fixed_tenant(), 2, &post_bytes, &post_sig).unwrap();
}

#[test]
fn rotation_attempt_to_zero_rejected() {
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(fixed_tenant(), 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let mut signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let mut verifier = HkdfVerifier::new(handle, vec![1]).unwrap();

    let err = signer.rotate_to(RESERVED_SIG_KEY_ID).unwrap_err();
    assert_eq!(err, SigError::KeyIdReserved);
    let err = verifier.admit(RESERVED_SIG_KEY_ID).unwrap_err();
    assert_eq!(err, SigError::KeyIdReserved);
}

#[test]
fn retire_last_key_rejected() {
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(fixed_tenant(), 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let mut verifier = HkdfVerifier::new(handle, vec![1]).unwrap();
    let err = verifier.retire(1).unwrap_err();
    assert!(matches!(err, SigError::TdkDerivationFailed(_)));
}

#[test]
fn admit_idempotent_no_duplicates() {
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(fixed_tenant(), 1);
    mock.install_default(fixed_tenant(), 2);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let mut verifier = HkdfVerifier::new(handle, vec![1]).unwrap();
    verifier.admit(2).unwrap();
    verifier.admit(2).unwrap();
    verifier.admit(2).unwrap();
    assert_eq!(verifier.accepted_key_ids(), &[1, 2]);
}

#[test]
fn retire_unknown_id_is_no_op() {
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(fixed_tenant(), 1);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let mut verifier = HkdfVerifier::new(handle, vec![1]).unwrap();
    // Retiring an unknown id is a no-op (idempotent grace expiry).
    verifier.retire(99).unwrap();
    assert_eq!(verifier.accepted_key_ids(), &[1]);
}

#[test]
fn signer_initial_zero_rejected() {
    let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
    let err = HkdfSigner::new(mock, RESERVED_SIG_KEY_ID).unwrap_err();
    assert_eq!(err, SigError::KeyIdReserved);
}

#[test]
fn verifier_initial_with_zero_in_whitelist_rejected() {
    let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
    let err = HkdfVerifier::new(mock, vec![0, 1, 2]).unwrap_err();
    assert_eq!(err, SigError::KeyIdReserved);
}

#[test]
fn forward_compat_sig_key_id_can_skip_versions() {
    // Schema allows non-contiguous sig_key_id values (post-incident
    // rotation may skip a compromised key version). Verifier accepts
    // arbitrary u32 whitelist.
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(fixed_tenant(), 5);
    mock.install_default(fixed_tenant(), 42);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), 42).unwrap();
    let verifier = HkdfVerifier::new(handle, vec![5, 42]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let bytes = compose_canonical_bytes(1, 42, fixed_tenant(), &action_hash, 100, &result_hash)
        .unwrap();
    let sig = signer.sign(fixed_tenant(), 42, &bytes).unwrap();
    verifier.verify(fixed_tenant(), 42, &bytes, &sig).unwrap();
    // Oldest-active surfaces 5, not 1.
    assert_eq!(verifier.oldest_active(), 5);
}

#[test]
fn three_way_rotation_grace_overlap() {
    // Simulate sustained rotation: keys 1 → 2 → 3 with overlapping
    // grace windows. Verifier grows then shrinks.
    let mock = Arc::new(MockTdkHandle::new());
    for k in 1..=3 {
        mock.install_default(fixed_tenant(), k);
    }
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;

    let mut signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let mut verifier = HkdfVerifier::new(Arc::clone(&handle), vec![1]).unwrap();

    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];

    let mut sigs = Vec::new();
    for kid in 1..=3 {
        signer.rotate_to(kid).unwrap();
        let bytes =
            compose_canonical_bytes(1, kid, fixed_tenant(), &action_hash, 100, &result_hash)
                .unwrap();
        let sig = signer.sign(fixed_tenant(), kid, &bytes).unwrap();
        sigs.push((kid, bytes, sig));
        if kid > 1 {
            verifier.admit(kid).unwrap();
        }
    }
    // All 3 verify under whitelist [1, 2, 3].
    for (kid, bytes, sig) in &sigs {
        verifier.verify(fixed_tenant(), *kid, bytes, sig).unwrap();
    }

    // Retire key 1 (sliding grace window).
    verifier.retire(1).unwrap();
    let err = verifier
        .verify(fixed_tenant(), sigs[0].0, &sigs[0].1, &sigs[0].2)
        .unwrap_err();
    match err {
        SigError::KeyIdUnknown {
            sig_key_id: 1,
            oldest_active: 2,
        } => {}
        _ => panic!("expected KeyIdUnknown(1, oldest=2), got {err:?}"),
    }
    // Keys 2 and 3 still verify.
    for (kid, bytes, sig) in sigs.iter().skip(1) {
        verifier.verify(fixed_tenant(), *kid, bytes, sig).unwrap();
    }
}
