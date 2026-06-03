//! Property test: idempotent retries with the **same** payload return
//! the original receipt (PAT-RETRY-IDEMPOTENT-001); diverging retries
//! return `IdempotencyConflict` + emit `dpa.idempotency_conflict`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures"
)]

mod common;

use common::{build_service, ctx, registry_three_locales};

use corelink_dpa_acceptance::{
    ConsentProofPayload, DpaAcceptanceError, DpaAcceptanceRequest, DpaAuditEvent, LocaleBcp47,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 32
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(32)
}

fn build_request(ui_capture_ts: i64, wording_id: &str) -> DpaAcceptanceRequest {
    let registry = registry_three_locales();
    DpaAcceptanceRequest {
        proof: ConsentProofPayload {
            notice_text_hash: registry.hash_for(LocaleBcp47::EnUs).unwrap(),
            notice_version: "1.0.0".into(),
            dpa_version: "1.0.0".into(),
            locale: LocaleBcp47::EnUs,
            wording_id: wording_id.into(),
            ui_capture_ts,
            submission_ts: 0,
        },
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn idempotent_retry_returns_same_jti(retries in 1u8..6) {
        let (svc, _pub) = build_service(1_700_000_000_000);
        let c = ctx("sig-idem", LocaleBcp47::EnUs);
        let req = build_request(1_699_999_999_000, "11111111-1111-7111-8111-111111111111");
        let first = svc.accept(&c, req.clone()).unwrap();
        for _ in 0..retries {
            let again = svc.accept(&c, req.clone()).unwrap();
            prop_assert_eq!(&again.jti, &first.jti);
            prop_assert_eq!(again.accepted_at_ms, first.accepted_at_ms);
        }
        prop_assert_eq!(svc.store().len(), 1);
    }
}

#[test]
fn idempotency_conflict_on_diverging_payload() {
    let (svc, _pub) = build_service(1_700_000_000_000);
    let c = ctx("sig-conflict", LocaleBcp47::EnUs);
    let req_a = build_request(1_699_999_999_000, "11111111-1111-7111-8111-111111111111");
    let req_b = build_request(1_699_999_999_000, "22222222-2222-7222-8222-222222222222");
    let _r1 = svc.accept(&c, req_a).unwrap();
    let err = svc.accept(&c, req_b).unwrap_err();
    assert!(matches!(err, DpaAcceptanceError::IdempotencyConflict));
    assert!(svc
        .audit_sink()
        .contains(|e| matches!(e, DpaAuditEvent::IdempotencyConflict { .. })));
}

#[test]
fn distinct_signups_get_distinct_records() {
    let (svc, _pub) = build_service(1_700_000_000_000);
    let req = build_request(1_699_999_999_000, "11111111-1111-7111-8111-111111111111");
    let r1 = svc
        .accept(&ctx("sig-A", LocaleBcp47::EnUs), req.clone())
        .unwrap();
    let r2 = svc.accept(&ctx("sig-B", LocaleBcp47::EnUs), req).unwrap();
    assert_ne!(r1.jti, r2.jti);
    assert_eq!(svc.store().len(), 2);
}
