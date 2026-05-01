//! Property tests for HKDF-SHA256 signing infra (WI-S04-004 §6.1.9 +
//! §10.s04.004.1).
//!
//! Six canonical properties × 10 000 iter (PR cadence; 100k nightly
//! per WI §10):
//!
//! 1. `prop_sign_verify_roundtrip` — random canonical inputs;
//!    sign → verify always OK.
//! 2. `prop_tampered_sig_rejected` — random sigs; flip any bit; verify
//!    rejects with `SigError::Invalid` (constant-time arm).
//! 3. `prop_wrong_key_id_rejected` — random `sig_key_id` not in the
//!    accepted whitelist; verify rejects with `KeyIdUnknown`.
//! 4. `prop_canonical_bytes_byte_stable` — same `(version, sig_key_id,
//!    tenant_id, action_hash, action_size, result_hash)` always yields
//!    identical canonical bytes (deterministic encoding).
//! 5. `prop_cross_tenant_signature_rejected` — sig from tenant A
//!    against tenant B's TDK rejects with `Invalid`.
//! 6. `prop_key_rotation_grace` — verifier accepts both pre- and
//!    post-rotation envelopes during the grace window.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use proptest::prelude::*;
use proptest::test_runner::Config;
use uuid::Uuid;

use corelink_ac::sig::{
    compose_canonical_bytes, HkdfSigner, HkdfVerifier, MockTdkHandle, SigError, SignatureSigner,
    SignatureVerifier, TdkHandle, AC_ENVELOPE_PREIMAGE_LEN, AC_ENVELOPE_SIG_LEN,
};

const PROP_CASES: u32 = 10_000;

/// Build a signer + verifier pair sharing one mock backend with the
/// requested key ids pre-installed for both `tenant_a` and `tenant_b`.
fn build_pair(
    accepted_key_ids: Vec<u32>,
    tenant_a: Uuid,
    tenant_b: Uuid,
) -> (HkdfSigner, HkdfVerifier, Arc<MockTdkHandle>) {
    let mock = Arc::new(MockTdkHandle::new());
    for k in &accepted_key_ids {
        mock.install_default(tenant_a, *k);
        mock.install_default(tenant_b, *k);
    }
    let signer_kid = *accepted_key_ids.last().unwrap();
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), signer_kid).unwrap();
    let verifier = HkdfVerifier::new(handle, accepted_key_ids).unwrap();
    (signer, verifier, mock)
}

fn arb_uuid() -> impl Strategy<Value = Uuid> {
    // 16 random bytes → arbitrary UUID. We don't constrain to v7 here
    // — the signer treats `tenant_id` as 16 raw bytes.
    proptest::array::uniform16(any::<u8>()).prop_map(Uuid::from_bytes)
}

fn arb_canonical_inputs() -> impl Strategy<Value = (Uuid, [u8; 32], i64, [u8; 32])> {
    (
        arb_uuid(),
        proptest::array::uniform32(any::<u8>()),
        any::<i64>(),
        proptest::array::uniform32(any::<u8>()),
    )
}

proptest! {
    #![proptest_config(Config {
        cases: PROP_CASES,
        max_shrink_iters: 64,
        ..Config::default()
    })]

    /// 1. `prop_sign_verify_roundtrip`.
    #[test]
    fn prop_sign_verify_roundtrip(
        (tenant_id, action_hash, action_size, result_hash) in arb_canonical_inputs(),
        sig_key_id in 1u32..=16
    ) {
        let mock = Arc::new(MockTdkHandle::new());
        mock.install_default(tenant_id, sig_key_id);
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let signer = HkdfSigner::new(Arc::clone(&handle), sig_key_id).unwrap();
        let verifier = HkdfVerifier::new(handle, vec![sig_key_id]).unwrap();
        let canonical_bytes =
            compose_canonical_bytes(1, sig_key_id, tenant_id, &action_hash, action_size, &result_hash)
                .unwrap();
        let sig = signer.sign(tenant_id, sig_key_id, &canonical_bytes).unwrap();
        prop_assert_eq!(sig.len(), AC_ENVELOPE_SIG_LEN);
        verifier.verify(tenant_id, sig_key_id, &canonical_bytes, &sig).unwrap();
    }

    /// 2. `prop_tampered_sig_rejected`.
    #[test]
    fn prop_tampered_sig_rejected(
        (tenant_id, action_hash, action_size, result_hash) in arb_canonical_inputs(),
        sig_key_id in 1u32..=8,
        flip_byte_idx in 0usize..AC_ENVELOPE_SIG_LEN,
        flip_bit_idx in 0u8..8,
    ) {
        let mock = Arc::new(MockTdkHandle::new());
        mock.install_default(tenant_id, sig_key_id);
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let signer = HkdfSigner::new(Arc::clone(&handle), sig_key_id).unwrap();
        let verifier = HkdfVerifier::new(handle, vec![sig_key_id]).unwrap();
        let canonical_bytes =
            compose_canonical_bytes(1, sig_key_id, tenant_id, &action_hash, action_size, &result_hash)
                .unwrap();
        let mut sig = signer.sign(tenant_id, sig_key_id, &canonical_bytes).unwrap();
        sig[flip_byte_idx] ^= 1u8 << flip_bit_idx;
        let err = verifier.verify(tenant_id, sig_key_id, &canonical_bytes, &sig).unwrap_err();
        prop_assert_eq!(err, SigError::Invalid);
    }

    /// 3. `prop_wrong_key_id_rejected`.
    #[test]
    fn prop_wrong_key_id_rejected(
        (tenant_id, action_hash, action_size, result_hash) in arb_canonical_inputs(),
        accepted_kid in 5u32..=10,
        wrong_kid in 100u32..=200,
    ) {
        let mock = Arc::new(MockTdkHandle::new());
        mock.install_default(tenant_id, accepted_kid);
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let signer = HkdfSigner::new(Arc::clone(&handle), accepted_kid).unwrap();
        let verifier = HkdfVerifier::new(handle, vec![accepted_kid]).unwrap();
        let canonical_bytes =
            compose_canonical_bytes(1, accepted_kid, tenant_id, &action_hash, action_size, &result_hash)
                .unwrap();
        let sig = signer.sign(tenant_id, accepted_kid, &canonical_bytes).unwrap();
        let err = verifier.verify(tenant_id, wrong_kid, &canonical_bytes, &sig).unwrap_err();
        match err {
            SigError::KeyIdUnknown { sig_key_id, oldest_active } => {
                prop_assert_eq!(sig_key_id, wrong_kid);
                prop_assert_eq!(oldest_active, accepted_kid);
            }
            e => prop_assert!(false, "expected KeyIdUnknown, got {:?}", e),
        }
    }

    /// 4. `prop_canonical_bytes_byte_stable`.
    #[test]
    fn prop_canonical_bytes_byte_stable(
        (tenant_id, action_hash, action_size, result_hash) in arb_canonical_inputs(),
        sig_key_id in 1u32..=u32::MAX,
    ) {
        let a =
            compose_canonical_bytes(1, sig_key_id, tenant_id, &action_hash, action_size, &result_hash)
                .unwrap();
        let b =
            compose_canonical_bytes(1, sig_key_id, tenant_id, &action_hash, action_size, &result_hash)
                .unwrap();
        prop_assert_eq!(a, b);
        prop_assert_eq!(a.len(), AC_ENVELOPE_PREIMAGE_LEN);
    }

    /// 5. `prop_cross_tenant_signature_rejected`.
    #[test]
    fn prop_cross_tenant_signature_rejected(
        tenant_a in arb_uuid(),
        tenant_b in arb_uuid(),
        action_hash in proptest::array::uniform32(any::<u8>()),
        action_size in any::<i64>(),
        result_hash in proptest::array::uniform32(any::<u8>()),
        sig_key_id in 1u32..=8,
    ) {
        prop_assume!(tenant_a != tenant_b);
        let (signer, verifier, _) = build_pair(vec![sig_key_id], tenant_a, tenant_b);
        let bytes_a = compose_canonical_bytes(1, sig_key_id, tenant_a, &action_hash, action_size, &result_hash).unwrap();
        let sig_a = signer.sign(tenant_a, sig_key_id, &bytes_a).unwrap();
        // Swap canonical bytes' tenant_id field to tenant_b and try to verify
        // with tenant_b's TDK. Re-compose with tenant_b for a clean test.
        let bytes_b = compose_canonical_bytes(1, sig_key_id, tenant_b, &action_hash, action_size, &result_hash).unwrap();
        let err = verifier.verify(tenant_b, sig_key_id, &bytes_b, &sig_a).unwrap_err();
        prop_assert_eq!(err, SigError::Invalid);
    }

    /// 6. `prop_key_rotation_grace`.
    #[test]
    fn prop_key_rotation_grace(
        (tenant_id, action_hash, action_size, result_hash) in arb_canonical_inputs(),
        old_kid in 1u32..=10,
        new_kid in 11u32..=20,
    ) {
        let mock = Arc::new(MockTdkHandle::new());
        mock.install_default(tenant_id, old_kid);
        mock.install_default(tenant_id, new_kid);
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let pre_signer = HkdfSigner::new(Arc::clone(&handle), old_kid).unwrap();
        let post_signer = HkdfSigner::new(Arc::clone(&handle), new_kid).unwrap();
        let verifier = HkdfVerifier::new(handle, vec![old_kid, new_kid]).unwrap();

        let pre_bytes = compose_canonical_bytes(1, old_kid, tenant_id, &action_hash, action_size, &result_hash).unwrap();
        let post_bytes = compose_canonical_bytes(1, new_kid, tenant_id, &action_hash, action_size, &result_hash).unwrap();
        let pre_sig = pre_signer.sign(tenant_id, old_kid, &pre_bytes).unwrap();
        let post_sig = post_signer.sign(tenant_id, new_kid, &post_bytes).unwrap();
        verifier.verify(tenant_id, old_kid, &pre_bytes, &pre_sig).unwrap();
        verifier.verify(tenant_id, new_kid, &post_bytes, &post_sig).unwrap();
    }
}
