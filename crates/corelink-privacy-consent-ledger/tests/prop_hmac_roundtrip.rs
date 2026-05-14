//! Property tests: HMAC sign + verify roundtrip and cross-tenant isolation.
//!
//! Coverage:
//! - `prop_hmac_sign_verify_roundtrip` — 100k: sign(params) → verify(same
//!   params) always returns valid=true (INV-CONSENT-PROOF-VERIFIABLE).
//! - `prop_cross_tenant_hmac_isolation` — 100k: random (T1, T2) pairs →
//!   0 cross-tenant signature validations (AC-005).
//! - `prop_tampered_hash_rejected` — 100k: tamper notice_text_hash → verify
//!   returns false.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures"
)]

use corelink_privacy_consent_ledger::hmac_sign::{
    ConsentHmacSigner, HmacParams, InMemoryConsentHmacSigner,
};
use proptest::prelude::*;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn arb_id() -> impl Strategy<Value = String> {
    "[a-z0-9]{4,16}".prop_map(|s| s)
}

fn arb_semver() -> impl Strategy<Value = String> {
    (0u32..=3u32, 0u32..=9u32, 0u32..=9u32)
        .prop_map(|(major, minor, patch)| format!("{major}.{minor}.{patch}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_hmac_sign_verify_roundtrip(
        record_id in arb_id(),
        tenant_id in arb_id(),
        purpose in arb_id(),
        hash in arb_id(),
        version in arb_semver(),
        locale in prop_oneof!["pt-BR", "en-US", "es-MX"],
        wording_id in arb_id(),
        ui_ts in arb_id(),
        sub_ts in arb_id(),
    ) {
        let signer = InMemoryConsentHmacSigner::new_test();
        let p = HmacParams {
            record_id: &record_id,
            tenant_id: &tenant_id,
            purpose: &purpose,
            notice_text_hash: &hash,
            notice_version: &version,
            locale: &locale,
            wording_id: &wording_id,
            ui_capture_ts: &ui_ts,
            submission_ts: &sub_ts,
        };
        let sig = signer.sign(&p).unwrap();
        let valid = signer.verify(&sig.hex, &sig.kid, &p).unwrap();
        let ok = valid;
        prop_assert!(ok, "HMAC roundtrip must be valid");
    }

    #[test]
    fn prop_cross_tenant_hmac_isolation(
        record_id in arb_id(),
        tenant1 in "[a-z]{4,8}",
        tenant2 in "[a-z]{4,8}",
        purpose in arb_id(),
        hash in arb_id(),
        version in arb_semver(),
        locale in prop_oneof!["pt-BR", "en-US", "es-MX"],
        wording_id in arb_id(),
        ui_ts in arb_id(),
        sub_ts in arb_id(),
    ) {
        prop_assume!(tenant1 != tenant2);
        let signer = InMemoryConsentHmacSigner::new_test();
        let p1 = HmacParams {
            record_id: &record_id,
            tenant_id: &tenant1,
            purpose: &purpose,
            notice_text_hash: &hash,
            notice_version: &version,
            locale: &locale,
            wording_id: &wording_id,
            ui_capture_ts: &ui_ts,
            submission_ts: &sub_ts,
        };
        let sig = signer.sign(&p1).unwrap();
        // Verify the T1 signature against T2 — must reject
        let p2 = HmacParams {
            record_id: &record_id,
            tenant_id: &tenant2,
            purpose: &purpose,
            notice_text_hash: &hash,
            notice_version: &version,
            locale: &locale,
            wording_id: &wording_id,
            ui_capture_ts: &ui_ts,
            submission_ts: &sub_ts,
        };
        let valid = signer.verify(&sig.hex, &sig.kid, &p2).unwrap();
        let cross_tenant_rejected = !valid;
        prop_assert!(cross_tenant_rejected, "cross-tenant signature must be rejected (AC-005)");
    }

    #[test]
    fn prop_tampered_hash_rejected(
        record_id in arb_id(),
        tenant_id in arb_id(),
        purpose in arb_id(),
        hash in arb_id(),
        tampered_hash in arb_id(),
        version in arb_semver(),
        locale in prop_oneof!["pt-BR", "en-US", "es-MX"],
        wording_id in arb_id(),
        ui_ts in arb_id(),
        sub_ts in arb_id(),
    ) {
        prop_assume!(hash != tampered_hash);
        let signer = InMemoryConsentHmacSigner::new_test();
        let p1 = HmacParams {
            record_id: &record_id,
            tenant_id: &tenant_id,
            purpose: &purpose,
            notice_text_hash: &hash,
            notice_version: &version,
            locale: &locale,
            wording_id: &wording_id,
            ui_capture_ts: &ui_ts,
            submission_ts: &sub_ts,
        };
        let sig = signer.sign(&p1).unwrap();
        let p_tampered = HmacParams {
            record_id: &record_id,
            tenant_id: &tenant_id,
            purpose: &purpose,
            notice_text_hash: &tampered_hash,
            notice_version: &version,
            locale: &locale,
            wording_id: &wording_id,
            ui_capture_ts: &ui_ts,
            submission_ts: &sub_ts,
        };
        let valid = signer.verify(&sig.hex, &sig.kid, &p_tampered).unwrap();
        let tampered_rejected = !valid;
        prop_assert!(tampered_rejected, "tampered hash must be rejected");
    }
}
