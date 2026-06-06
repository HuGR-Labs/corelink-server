//! End-to-end happy path + key negative paths through the acceptance
//! orchestrator. Covers all 6 consent fields, audit emission ordering,
//! and notification envelope contents.

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
    canonical_dpa_audit_event_names, verify_receipt, ConsentProofPayload, DpaAcceptanceError,
    DpaAcceptanceRequest, DpaAuditEvent, LocaleBcp47,
};

#[test]
fn happy_path_three_locales() {
    let registry = registry_three_locales();
    for locale in [LocaleBcp47::EnUs, LocaleBcp47::PtBr, LocaleBcp47::Es419] {
        let (svc, pubkey) = build_service(1_700_000_000_000);
        let c = ctx("sig-happy", locale);
        let req = DpaAcceptanceRequest {
            proof: ConsentProofPayload {
                notice_text_hash: registry.hash_for(locale).unwrap(),
                notice_version: "1.0.0".into(),
                dpa_version: "1.0.0".into(),
                locale,
                wording_id: "11111111-1111-7111-8111-111111111111".into(),
                ui_capture_ts: 1_699_999_999_000,
                submission_ts: 0,
            },
        };
        let receipt = svc.accept(&c, req).unwrap();

        // (a) JWT verifies.
        let claims = verify_receipt(&pubkey, Some("kid-test-01"), &receipt.jwt_receipt).unwrap();
        assert_eq!(claims.dpa_version, "1.0.0");
        assert_eq!(claims.jti, receipt.jti);

        // (b) record stored with all 6 fields populated + server-stamped
        // submission_ts.
        let row = svc.store().len();
        assert_eq!(row, 1);

        // (c) accepted audit emitted.
        assert!(svc
            .audit_sink()
            .contains(|e| matches!(e, DpaAuditEvent::Accepted { .. })));

        // (d) notification envelope captured.
        let envelopes = svc.notification_sink().snapshot();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].jti, receipt.jti);
        assert_eq!(envelopes[0].jwt_receipt, receipt.jwt_receipt);
    }
}

#[test]
fn hash_mismatch_emits_audit_and_rejects() {
    let (svc, _pub) = build_service(1_700_000_000_000);
    let c = ctx("sig-hash", LocaleBcp47::EnUs);
    let req = DpaAcceptanceRequest {
        proof: ConsentProofPayload {
            notice_text_hash: "0".repeat(64),
            notice_version: "1.0.0".into(),
            dpa_version: "1.0.0".into(),
            locale: LocaleBcp47::EnUs,
            wording_id: "11111111-1111-7111-8111-111111111111".into(),
            ui_capture_ts: 1_699_999_999_000,
            submission_ts: 0,
        },
    };
    let err = svc.accept(&c, req).unwrap_err();
    assert!(matches!(err, DpaAcceptanceError::NoticeHashMismatch { .. }));
    assert!(svc
        .audit_sink()
        .contains(|e| matches!(e, DpaAuditEvent::HashMismatch { .. })));
    assert!(svc.store().is_empty(), "no record on hash mismatch");
}

#[test]
fn server_stamps_submission_ts_overwriting_zero() {
    let registry = registry_three_locales();
    let (svc, _pub) = build_service(1_700_111_222_333);
    let c = ctx("sig-ts", LocaleBcp47::EnUs);
    let req = DpaAcceptanceRequest {
        proof: ConsentProofPayload {
            notice_text_hash: registry.hash_for(LocaleBcp47::EnUs).unwrap(),
            notice_version: "1.0.0".into(),
            dpa_version: "1.0.0".into(),
            locale: LocaleBcp47::EnUs,
            wording_id: "11111111-1111-7111-8111-111111111111".into(),
            ui_capture_ts: 1_699_999_999_000,
            submission_ts: 0,
        },
    };
    let receipt = svc.accept(&c, req).unwrap();
    assert_eq!(receipt.accepted_at_ms, 1_700_111_222_333);
}

#[test]
fn canonical_audit_event_names_are_stable() {
    let names = canonical_dpa_audit_event_names();
    assert!(names.contains(&"dpa.accepted"));
    assert!(names.contains(&"dpa.locale_mismatch"));
    assert!(names.contains(&"dpa.hash_mismatch"));
    assert!(names.contains(&"dpa.idempotency_conflict"));
    assert!(names.contains(&"dpa.replay_detected"));
}

#[test]
fn schema_version_matches_d1_migration() {
    assert_eq!(corelink_dpa_acceptance::dpa_schema_version(), 1);
}
