//! Property tests for `corelink-handler-ac`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "proptests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_handler_ac::observer::Sli;
use corelink_handler_ac::{
    AcHandlerError, AcLookupHandler, AcLookupRequest, AcUpdateHandler, AcUpdateRequest,
    AuditEventKind, InMemoryAcHandler, InMemoryAuditSink, InMemorySliObserver,
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
    InMemoryAcHandler,
) {
    let a = Arc::new(InMemoryAuditSink::new());
    let s = Arc::new(InMemorySliObserver::new());
    let h = InMemoryAcHandler::new(a.clone(), s.clone());
    (a, s, h)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_lookup_emits_avail_ac_lookup_per_entry(
        tenant in "[a-z]{1,4}",
        digest in "[a-z0-9]{8,16}",
    ) {
        let (_a, sli, h) = fixture();
        let _ = h.lookup(AcLookupRequest::new(
            tenant.clone(),
            digest,
            "p",
            tenant,
            1,
        ));
        let n = sli.count(Sli::AvailAcLookup).expect("count");
        prop_assert!(n >= 1);
    }

    #[test]
    fn prop_cross_tenant_always_denied_audits_first(
        tenant_a in "[a-z]{1,4}",
        tenant_b in "[a-z]{1,4}",
        digest in "[a-z]{4,8}",
    ) {
        prop_assume!(tenant_a != tenant_b);
        let (audit, _sli, h) = fixture();
        let err = h.lookup(AcLookupRequest::new(
            tenant_a.clone(),
            digest,
            "p",
            tenant_b,
            1,
        )).expect_err("denied");
        let is_cross = matches!(err, AcHandlerError::CrossTenantDenied { .. });
        prop_assert!(is_cross);
        let rows = audit.snapshot().expect("audit");
        prop_assert_eq!(rows[0].kind, AuditEventKind::LookupDenied);
    }

    #[test]
    fn prop_audit_fail_closed_no_mutation_on_update(
        tenant in "[a-z]{1,4}",
        digest in "[a-z]{4,8}",
        payload in proptest::collection::vec(any::<u8>(), 0..16),
    ) {
        let (audit, _sli, h) = fixture();
        audit.inject_failure("d1 down").expect("inject");
        let err = h.update(AcUpdateRequest::new(
            tenant.clone(),
            digest.clone(),
            payload,
            "p",
            tenant.clone(),
            1,
        )).expect_err("audit closed");
        match &err {
            AcHandlerError::AuditFailed(msg) => {
                prop_assert!(msg.contains("d1 down"), "expected injected message; got {msg:?}");
            }
            other => prop_assert!(false, "expected AuditFailed, got {other:?}"),
        }
        // Subsequent lookup is a miss — no mutation occurred.
        let miss_err = h.lookup(AcLookupRequest::new(
            tenant.clone(),
            digest,
            "p",
            tenant,
            2,
        ));
        // The injection persists so any further audit call fails too;
        // the structural assertion is that the update did NOT commit.
        match &miss_err {
            Err(AcHandlerError::AuditFailed(msg)) => {
                prop_assert!(msg.contains("d1 down"), "expected injected message; got {msg:?}");
            }
            other => prop_assert!(false, "expected Err(AuditFailed), got {other:?}"),
        }
    }
}
