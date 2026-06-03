//! Adversarial 1 — MFA challenge fails on a destructive arm.
//!
//! Customer submits Erasure WITHOUT an MFA token → API returns
//! `MfaRequired` decision (NOT a server error). No DSR ticket inserted;
//! audit chain captures `mfa_step_up_required` BEFORE the early return
//! (fail-CLOSED envelope per ADR-S11-002).
//!
//! A second submission with an INVALID (empty) token routes through
//! the verifier and surfaces as `DsrError::Mfa` — still no ticket.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr::{
    DsrDecision, DsrEndpoint, DsrError, DsrJurisdiction, DsrMfaError, DsrRequestKind,
    MfaStepUpToken,
};
use e2e_dsr::{
    canonical_dsr_for, make_test_tenant, setup_test_env, verify_audit_chain, ExpectedDsrAuditEvent,
};

#[test]
fn mfa_failure_no_ticket_no_receipt() {
    let env = setup_test_env();
    let tenant = make_test_tenant("mfa-fail-tenant");

    // ---- Case 1: missing MFA token on destructive arm ----
    let request_no_mfa = canonical_dsr_for(
        &tenant,
        DsrRequestKind::Erasure,
        DsrJurisdiction::Lgpd,
        env.now_ms,
    );
    let decision = match env.dsr.submit(&request_no_mfa) {
        Ok(d) => d,
        Err(e) => panic!("submit should return MfaRequired decision, got error {e:?}"),
    };
    match decision {
        DsrDecision::MfaRequired { kind } => {
            assert_eq!(kind, DsrRequestKind::Erasure);
        }
        other => panic!("expected MfaRequired, got {other:?}"),
    }

    // No ticket inserted; audit chain has request_received + mfa_step_up_required.
    assert_eq!(env.dsr_store.len(), 0, "no ticket on MFA gate");
    if let Err(msg) = verify_audit_chain(
        &env,
        &[
            ExpectedDsrAuditEvent::RequestReceived,
            ExpectedDsrAuditEvent::MfaStepUpRequired,
        ],
    ) {
        panic!("audit chain: {msg}");
    }

    // ---- Case 2: invalid (empty) MFA token routes through verifier ----
    let request_bad_mfa = canonical_dsr_for(
        &tenant,
        DsrRequestKind::Erasure,
        DsrJurisdiction::Lgpd,
        env.now_ms.saturating_add(1_000),
    )
    .with_mfa(MfaStepUpToken::synthetic_for_test(""));
    let err = match env.dsr.submit(&request_bad_mfa) {
        Ok(d) => panic!("expected MFA error, got {d:?}"),
        Err(e) => e,
    };
    assert!(
        matches!(err, DsrError::Mfa(DsrMfaError::Invalid(_))),
        "expected DsrError::Mfa::Invalid, got {err:?}"
    );
    // Still no ticket.
    assert_eq!(env.dsr_store.len(), 0);
    assert_eq!(env.dsr_mfa.rejected_count(), 1);
}
