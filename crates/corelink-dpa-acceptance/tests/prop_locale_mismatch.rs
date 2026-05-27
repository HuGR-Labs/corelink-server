//! Property test: locale-mismatch rejection is total — every
//! `(server, payload)` pair where `server != payload` must produce
//! `LocaleMismatch` and emit `dpa.locale_mismatch`. CTRL-PRIV-CONSENT-005
//! enforcement (Lote 10.16 canonical).

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
    notice_text_hash, ConsentProofPayload, DpaAcceptanceError, DpaAcceptanceRequest,
    DpaAuditEvent, LocaleBcp47,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 64
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64)
}

fn locale_strategy() -> impl Strategy<Value = LocaleBcp47> {
    prop_oneof![
        Just(LocaleBcp47::EnUs),
        Just(LocaleBcp47::PtBr),
        Just(LocaleBcp47::Es419),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn locale_mismatch_is_total(
        server in locale_strategy(),
        payload in locale_strategy(),
    ) {
        let (svc, _pub) = build_service(1_700_000_000_000);
        let c = ctx("sig-locale", server);
        let registry = registry_three_locales();
        let proof = ConsentProofPayload {
            notice_text_hash: registry.hash_for(payload).expect("locale registered"),
            notice_version: "1.0.0".into(),
            dpa_version: "1.0.0".into(),
            locale: payload,
            wording_id: "00000000-0000-7000-8000-000000000000".into(),
            ui_capture_ts: 1_699_999_999_000,
            submission_ts: 0,
        };
        let req = DpaAcceptanceRequest { proof };
        let result = svc.accept(&c, req);

        if server == payload {
            prop_assert!(result.is_ok(), "matching locale should succeed");
        } else {
            match result {
                Err(DpaAcceptanceError::LocaleMismatch { .. }) => {}
                other => prop_assert!(false, "expected LocaleMismatch, got {:?}", other),
            }
            prop_assert!(
                svc.audit_sink().contains(|e| matches!(e, DpaAuditEvent::LocaleMismatch { .. })),
                "dpa.locale_mismatch audit must fire"
            );
        }
    }
}

#[test]
fn locale_mismatch_audit_carries_both_locales() {
    let (svc, _pub) = build_service(1_700_000_000_000);
    let c = ctx("sig-1", LocaleBcp47::PtBr);
    let registry = registry_three_locales();
    let proof = ConsentProofPayload {
        notice_text_hash: registry.hash_for(LocaleBcp47::EnUs).unwrap(),
        notice_version: "1.0.0".into(),
        dpa_version: "1.0.0".into(),
        locale: LocaleBcp47::EnUs,
        wording_id: "00000000-0000-7000-8000-000000000000".into(),
        ui_capture_ts: 0,
        submission_ts: 0,
    };
    let req = DpaAcceptanceRequest { proof };
    let err = svc.accept(&c, req).unwrap_err();
    assert!(matches!(err, DpaAcceptanceError::LocaleMismatch { .. }));
    let events = svc.audit_sink().snapshot();
    assert_eq!(events.len(), 1);
    match &events[0] {
        DpaAuditEvent::LocaleMismatch { server, payload, .. } => {
            assert_eq!(server, "pt-BR");
            assert_eq!(payload, "en-US");
        }
        other => panic!("unexpected event: {:?}", other),
    }
}

#[test]
fn notice_hash_helper_matches_registry() {
    let r = registry_three_locales();
    let h = r.hash_for(LocaleBcp47::PtBr).unwrap();
    assert_eq!(h, notice_text_hash("DPA v1.0.0 (pt-BR) — texto canônico."));
}
