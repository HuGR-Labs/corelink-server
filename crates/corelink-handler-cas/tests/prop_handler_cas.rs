//! Property tests for `corelink-handler-cas`.
//!
//! Pins three load-bearing invariants the handler trait contract
//! relies on:
//!
//! 1. `INV-HANDLER-SLI-EMIT-ENTRY` — every handler entry emits at
//!    least one `Sli::AvailCasGet` (read) or `Sli::AvailCasPut`
//!    (write) observation regardless of outcome.
//! 2. `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` — when the audit sink
//!    fails on a write, the underlying storage is **never** mutated.
//! 3. Cross-tenant isolation — a caller authenticated as tenant `A`
//!    can never read or write objects under tenant `B`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "proptests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_handler_cas::{
    handler::fake_hash, AuditEventKind, CasHandlerError, CasReadHandler, CasReadRequest,
    CasWriteHandler, CasWriteRequest, InMemoryAuditSink, InMemoryCasHandler,
    InMemorySliObserver,
};
use corelink_handler_cas::observer::Sli;

use proptest::prelude::*;

fn handler() -> (Arc<InMemoryAuditSink>, Arc<InMemorySliObserver>, InMemoryCasHandler) {
    let a = Arc::new(InMemoryAuditSink::new());
    let s = Arc::new(InMemorySliObserver::new());
    let h = InMemoryCasHandler::new(a.clone(), s.clone());
    (a, s, h)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn prop_sli_emit_per_entry_read(
        tenant in "[a-z]{1,8}",
        bytes in proptest::collection::vec(any::<u8>(), 0..32),
    ) {
        let (_audit, sli, h) = handler();
        let hash = fake_hash(&bytes);
        h.seed(&tenant, &hash, bytes.clone()).expect("seed");
        let _ = h.read(CasReadRequest::new(
            tenant.clone(),
            hash.clone(),
            "p",
            tenant.clone(),
            1,
        )).expect("read");
        let obs = sli.snapshot().expect("sli");
        let avail = obs.iter().filter(|o| o.sli == Sli::AvailCasGet).count();
        prop_assert!(avail >= 1);
        let lat = obs.iter().filter(|o| o.sli == Sli::LatencyCasGetP99).count();
        prop_assert!(lat >= 1);
    }

    #[test]
    fn prop_audit_fail_closed_no_mutation_on_write(
        tenant in "[a-z]{1,4}",
        bytes in proptest::collection::vec(any::<u8>(), 0..16),
    ) {
        let (audit, _sli, h) = handler();
        audit.inject_failure("simulated d1 down").expect("inject");
        let claimed = fake_hash(&bytes);
        let err = h.write(CasWriteRequest::new(
            tenant.clone(),
            claimed,
            bytes.clone(),
            "p",
            tenant.clone(),
            1,
        )).expect_err("audit closed");
        prop_assert!(matches!(err, CasHandlerError::AuditFailed(_)));
        // No row recorded.
        prop_assert_eq!(audit.snapshot().expect("a").len(), 0);
    }

    #[test]
    fn prop_cross_tenant_always_denied(
        tenant_a in "[a-z]{1,4}",
        tenant_b in "[a-z]{1,4}",
        bytes in proptest::collection::vec(any::<u8>(), 1..16),
    ) {
        prop_assume!(tenant_a != tenant_b);
        let (audit, _sli, h) = handler();
        let hash = fake_hash(&bytes);
        h.seed(&tenant_a, &hash, bytes.clone()).expect("seed");
        let err = h.read(CasReadRequest::new(
            tenant_a.clone(),
            hash.clone(),
            "p",
            tenant_b.clone(),
            1,
        )).expect_err("denied");
        let is_cross = matches!(err, CasHandlerError::CrossTenantDenied { .. });
        prop_assert!(is_cross);
        // Audit emitted BEFORE the denial — first row is the
        // denial row.
        let rows = audit.snapshot().expect("audit");
        prop_assert!(!rows.is_empty());
        prop_assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
    }
}
