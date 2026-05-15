//! Happy-path 4 — Rectification (LGPD Art. 18 III / GDPR Art. 16
//! correction of inaccurate data).
//!
//! Destructive arm: requires MFA step-up. Customer submits with a
//! valid token → API returns `RequestAccepted` + JWT receipt. Audit
//! chain MUST contain `mfa_verified` BEFORE `request_accepted`.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr::{DsrDecision, DsrEndpoint, DsrJurisdiction, DsrRequestKind};
use e2e_dsr::{
    canonical_dsr_for, canonical_mfa_token, make_test_tenant, setup_test_env, verify_audit_chain,
    ExpectedDsrAuditEvent,
};

#[test]
fn rectification_with_mfa_accepts_and_audits() {
    let env = setup_test_env();
    let tenant = make_test_tenant("rectification-tenant");

    let request = canonical_dsr_for(
        &tenant,
        DsrRequestKind::Rectification,
        DsrJurisdiction::Gdpr,
        env.now_ms,
    )
    .with_mfa(canonical_mfa_token());

    let decision = match env.dsr.submit(&request) {
        Ok(d) => d,
        Err(e) => panic!("dsr submit failed: {e:?}"),
    };
    let (receipt, sla_deadline_ms) = match decision {
        DsrDecision::RequestAccepted {
            receipt,
            sla_deadline_ms,
        } => (receipt, sla_deadline_ms),
        other => panic!("expected RequestAccepted, got {other:?}"),
    };
    assert!(receipt.is_non_empty());
    let expected_sla = env.now_ms.saturating_add(30 * 86_400_000); // GDPR 30d
    assert_eq!(sla_deadline_ms, expected_sla);
    assert_eq!(env.dsr_mfa.verified_count(), 1);

    // mfa_verified precedes request_accepted (canonical fail-CLOSED
    // ordering per ADR-S11-002).
    if let Err(msg) = verify_audit_chain(
        &env,
        &[
            ExpectedDsrAuditEvent::RequestReceived,
            ExpectedDsrAuditEvent::MfaVerified,
            ExpectedDsrAuditEvent::RequestAccepted,
            ExpectedDsrAuditEvent::ReceiptIssued,
        ],
    ) {
        panic!("audit chain: {msg}");
    }
}
