//! Property tests for `corelink-handler-admin`.
//!
//! Pins the load-bearing dual-approval invariant: any `mutate` call
//! whose initiator equals the second approver MUST reject + audit
//! BEFORE returning, with no underlying state change.

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
    AdminHandlerError, AdminMutateHandler, AdminMutateRequest, AuditEventKind,
    DualApprovalToken, InMemoryAdminHandler, InMemoryAuditSink, InMemorySliObserver,
    MutateOp,
};

use proptest::prelude::*;

fn fixture() -> (
    Arc<InMemoryAuditSink>,
    Arc<InMemorySliObserver>,
    InMemoryAdminHandler,
) {
    let a = Arc::new(InMemoryAuditSink::new());
    let s = Arc::new(InMemorySliObserver::new());
    let h = InMemoryAdminHandler::new(a.clone(), s.clone());
    (a, s, h)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn prop_dual_approval_distinct_required(
        initiator in "[a-z]{1,8}",
        approver in "[a-z]{1,8}",
        tier in "[a-zA-Z]{1,12}",
    ) {
        let (audit, sli, h) = fixture();
        let req = AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", tier),
            initiator.clone(),
            true,
            Some(DualApprovalToken::new("a1", approver.clone())),
            1,
        );
        let result = h.mutate(req);
        if initiator == approver {
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

    #[test]
    fn prop_audit_fail_closed_no_state_change(
        initiator in "[a-z]{1,8}",
        approver in "[a-z]{1,8}",
    ) {
        prop_assume!(initiator != approver);
        let (audit, _sli, h) = fixture();
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
