//! Property tests: idempotency replay-safety (AC-008; PAT-RETRY-IDEMPOTENT-001).
//!
//! Coverage:
//! - `prop_replay_same_proof_returns_same_id` — 100k: submitting the same
//!   proof twice returns the same consent_id and replay=true on the second
//!   call; consent_ledger NOT duplicated.
//! - `prop_different_proof_inserts_new_row` — 100k: submitting a proof with
//!   a different ui_capture_ts creates a new row.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures"
)]

use std::sync::Arc;

use corelink_privacy_consent_ledger::{
    audit_emit::InMemoryConsentAuditSink,
    cascade::InMemoryCascadeSink,
    hmac_sign::InMemoryConsentHmacSigner,
    ledger::{ConsentLedger, InMemoryConsentLedger},
    schema::{ConsentProofPayload, ConsentPurpose, LocaleBcp47},
    store::InMemoryConsentStore,
};
use proptest::prelude::*;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn make_ledger() -> InMemoryConsentLedger {
    let store = Arc::new(InMemoryConsentStore::new());
    let audit = Arc::new(InMemoryConsentAuditSink::new());
    let signer = Arc::new(InMemoryConsentHmacSigner::new_test());
    let cascade = Arc::new(InMemoryCascadeSink::new());
    InMemoryConsentLedger::new(store, audit, signer, cascade, "1.0.0")
}

fn proof_with(hash: &str, ts: &str, locale: LocaleBcp47) -> ConsentProofPayload {
    ConsentProofPayload {
        notice_text_hash: hash.to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale,
        wording_id: "wording-v1".to_owned(),
        ui_capture_ts: ts.to_owned(),
        submission_ts: "2026-05-13T10:00:02Z".to_owned(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_replay_same_proof_returns_same_id(
        tenant_id in "[a-z]{4,8}",
        subject_id in "[a-z]{4,8}",
        hash in "[a-f0-9]{8,16}",
        ts in "[0-9]{10}",
    ) {
        let ledger = make_ledger();
        let p = proof_with(&hash, &ts, LocaleBcp47::EnUs);
        let r1 = ledger.grant_consent(
            &tenant_id, &subject_id, "en-US",
            ConsentPurpose::MarketingEmail, p.clone(),
        ).unwrap();
        let r2 = ledger.grant_consent(
            &tenant_id, &subject_id, "en-US",
            ConsentPurpose::MarketingEmail, p,
        ).unwrap();
        let ids_match = r1.consent_id == r2.consent_id;
        let second_is_replay = r2.replay;
        prop_assert!(ids_match, "replay must return same consent_id");
        prop_assert!(second_is_replay, "second identical submit must set replay=true");
    }

    #[test]
    fn prop_different_ts_inserts_new_row(
        tenant_id in "[a-z]{4,8}",
        subject_id in "[a-z]{4,8}",
        hash in "[a-f0-9]{8,16}",
        ts1 in "[0-9]{10}",
        ts2 in "[0-9]{10}",
    ) {
        prop_assume!(ts1 != ts2);
        let ledger = make_ledger();
        let p1 = proof_with(&hash, &ts1, LocaleBcp47::EnUs);
        let p2 = proof_with(&hash, &ts2, LocaleBcp47::EnUs);
        let r1 = ledger.grant_consent(
            &tenant_id, &subject_id, "en-US",
            ConsentPurpose::MarketingEmail, p1,
        ).unwrap();
        let r2 = ledger.grant_consent(
            &tenant_id, &subject_id, "en-US",
            ConsentPurpose::MarketingEmail, p2,
        ).unwrap();
        let new_row = r1.consent_id != r2.consent_id;
        prop_assert!(new_row, "different ui_capture_ts must insert new row");
    }
}
