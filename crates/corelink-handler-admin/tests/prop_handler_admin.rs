//! Property tests for `corelink-handler-admin`.
//!
//! Pins the load-bearing dual-approval invariant AFTER the H5 fix: a `mutate`
//! is authorized ONLY by a persisted, independently-recorded approval whose
//! ledger-recorded approver differs from the initiator. Two properties:
//!
//! 1. A `mutate` whose `approval_id` has NO ledger record is ALWAYS rejected
//!    (`DualApprovalUnknown`) with no state change — no free-text `approver`
//!    string can conjure authorization.
//! 2. Given a genuinely recorded approval, the mutate commits iff the
//!    ledger-recorded approver differs from the initiator; a recorded
//!    self-approval (`approver == initiator`) rejects + audits BEFORE
//!    returning, with no state change.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "proptests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_handler_admin::observer::Sli;
use corelink_handler_admin::{
    AdminHandlerError, AdminMutateHandler, AdminMutateRequest, AuditEventKind, DualApprovalToken,
    InMemoryAdminHandler, InMemoryApprovalLedger, InMemoryAuditSink, InMemorySliObserver, MutateOp,
};

use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 256
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

fn fixture() -> (
    Arc<InMemoryAuditSink>,
    Arc<InMemorySliObserver>,
    Arc<InMemoryApprovalLedger>,
    InMemoryAdminHandler,
) {
    let a = Arc::new(InMemoryAuditSink::new());
    let s = Arc::new(InMemorySliObserver::new());
    let ledger = Arc::new(InMemoryApprovalLedger::new());
    let h = InMemoryAdminHandler::new_with_ledger(a.clone(), s.clone(), ledger.clone());
    (a, s, ledger, h)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// A recorded approval authorizes the mutation iff its recorded approver
    /// differs from the initiator; a recorded self-approval rejects.
    #[test]
    fn prop_dual_approval_distinct_required(
        initiator in "[a-z]{1,8}",
        recorded_approver in "[a-z]{1,8}",
        body_approver in "[a-z]{1,8}",
        tier in "[a-zA-Z]{1,12}",
    ) {
        let (audit, sli, ledger, h) = fixture();
        // Record a genuine approval bound to tenant:t1.
        ledger.record("a1", recorded_approver.clone(), "tenant:t1").expect("record");
        let req = AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", tier),
            initiator.clone(),
            true,
            // The body's approver is advisory and must not affect the decision.
            Some(DualApprovalToken::new("a1", body_approver)),
            1,
        );
        let result = h.mutate(req);
        if initiator == recorded_approver {
            let is_self = matches!(
                result,
                Err(AdminHandlerError::DualApprovalSelfApproval { .. })
            );
            prop_assert!(is_self);
            prop_assert!(h.applied_snapshot().expect("snap").is_empty());
            let rows = audit.snapshot().expect("a");
            prop_assert!(rows.iter().any(|r| r.kind == AuditEventKind::MutateDualApprovalRejected));
        } else {
            prop_assert!(result.is_ok());
            prop_assert!(!h.applied_snapshot().expect("snap").is_empty());
        }
        // Always: at least one AvailControlPlane observation.
        prop_assert!(sli.count(Sli::AvailControlPlane).expect("c") >= 1);
    }

    /// No ledger record ⇒ ALWAYS rejected, regardless of the (free-text)
    /// approver string — the H5 bypass is dead.
    #[test]
    fn prop_unrecorded_approval_always_rejected(
        initiator in "[a-z]{1,8}",
        approver in "[a-z]{1,8}",
        approval_id in "[a-z0-9]{1,10}",
        tier in "[a-zA-Z]{1,12}",
    ) {
        let (_audit, _sli, _ledger, h) = fixture();
        // Nothing recorded in the ledger.
        let req = AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", tier),
            initiator,
            true,
            Some(DualApprovalToken::new(approval_id, approver)),
            1,
        );
        let err = h.mutate(req).expect_err("unrecorded approval must reject");
        let is_unknown = matches!(err, AdminHandlerError::DualApprovalUnknown { .. });
        prop_assert!(is_unknown);
        prop_assert!(h.applied_snapshot().expect("snap").is_empty());
    }

    #[test]
    fn prop_audit_fail_closed_no_state_change(
        initiator in "[a-z]{1,8}",
        approver in "[a-z]{1,8}",
    ) {
        prop_assume!(initiator != approver);
        let (audit, _sli, ledger, h) = fixture();
        ledger.record("a1", approver.clone(), "admin_token:tk").expect("record");
        audit.inject_failure("d1 down").expect("inject");
        let err = h.mutate(AdminMutateRequest::new(
            MutateOp::rotate_admin_token("tk"),
            initiator,
            true,
            Some(DualApprovalToken::new("a1", approver)),
            1,
        )).expect_err("audit closed");
        match &err {
            AdminHandlerError::AuditFailed(msg) => {
                prop_assert!(msg.contains("d1 down"), "expected injected message; got {msg:?}");
            }
            other => prop_assert!(false, "expected AuditFailed, got {other:?}"),
        }
        prop_assert!(h.applied_snapshot().expect("snap").is_empty());
    }
}
