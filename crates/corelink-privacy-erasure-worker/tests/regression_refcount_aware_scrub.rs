//! Regression test for S-07 dedup safety: blob shared between 3
//! tenants → erasure for tenant 1 must NOT break tenants 2 + 3 (per
//! WI-S11-002 §10.2 T-2.5 + §28 R-003 CRITICAL cross-tenant break
//! prevention).
//!
//! AC-003: subject_unaffiliated → refcount decrement only;
//! subject_dedicated → blob deleted.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_privacy_erasure_worker::{
    canonical_in_memory_adapters, BackendErasureAdapter, BackendKind, ErasureRequest, ErasureSalt,
    ErasureWorker, InMemoryRow, InMemoryErasureAuditSink, InMemoryErasureIdempotencyLedger,
    InMemoryErasureWorker,
};
use uuid::Uuid;

fn fixed(seed: u8) -> Uuid {
    let mut b = [0u8; 16];
    for (i, x) in b.iter_mut().enumerate() {
        *x = seed.wrapping_mul(13).wrapping_add(i as u8);
    }
    Uuid::from_bytes(b)
}

#[test]
fn r2_cas_shared_blob_3_tenants_isolation() {
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let r2_cas = adapters_typed
        .iter()
        .find(|a| a.kind() == BackendKind::R2Cas)
        .unwrap()
        .clone();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(audit, ledger, adapters_dyn).unwrap();

    let tenant_1 = fixed(1);
    let subject_t1 = fixed(11);

    let tenant_2 = fixed(2);
    let subject_t2 = fixed(12);

    let tenant_3 = fixed(3);
    let subject_t3 = fixed(13);

    // Insert a "shared blob" with refcount=3 for each tenant slot.
    // The orchestrator's R2Cas adapter decrements refcount for the
    // canonical (tenant, subject) being erased; the OTHER tenants'
    // metadata edges are untouched (separate map slots).
    for (t, s) in [
        (tenant_1, subject_t1),
        (tenant_2, subject_t2),
        (tenant_3, subject_t3),
    ] {
        r2_cas.insert_rows(
            t,
            s,
            vec![InMemoryRow::new(b"shared".to_vec()).with_refcount(3)],
        );
    }

    // Erase tenant 1.
    let req = ErasureRequest::new(
        fixed(100),
        tenant_1,
        subject_t1,
        ErasureSalt::synthetic_for_test(7),
        1_000,
    );
    worker.process_erasure(&req, 1_000).unwrap();

    // Tenant 1's row decremented (refcount: 3 → 2; row preserved
    // because refcount > 1 → subject_unaffiliated semantic).
    assert_eq!(
        r2_cas.row_count(tenant_1, subject_t1),
        1,
        "tenant 1 row preserved with decremented refcount"
    );

    // Tenants 2 + 3 untouched.
    assert_eq!(
        r2_cas.row_count(tenant_2, subject_t2),
        1,
        "tenant 2 row UNTOUCHED (cross-tenant isolation)"
    );
    assert_eq!(
        r2_cas.row_count(tenant_3, subject_t3),
        1,
        "tenant 3 row UNTOUCHED (cross-tenant isolation)"
    );
}

#[test]
fn r2_cas_dedicated_blob_erased_when_refcount_one() {
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let r2_cas = adapters_typed
        .iter()
        .find(|a| a.kind() == BackendKind::R2Cas)
        .unwrap()
        .clone();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(audit, ledger, adapters_dyn).unwrap();

    let tenant = fixed(1);
    let subject = fixed(2);

    // Single-tenant ownership: refcount=1 → subject_dedicated semantic.
    r2_cas.insert_rows(
        tenant,
        subject,
        vec![InMemoryRow::new(b"dedicated".to_vec()).with_refcount(1)],
    );

    let req = ErasureRequest::new(
        fixed(100),
        tenant,
        subject,
        ErasureSalt::synthetic_for_test(7),
        1_000,
    );
    worker.process_erasure(&req, 1_000).unwrap();

    // Refcount=1 → blob hard-deleted; row count = 0.
    assert_eq!(r2_cas.row_count(tenant, subject), 0);
}

#[test]
fn refcount_aware_at_5_tenants_disjoint_metadata_edges() {
    // Property: erasing tenant N never touches the other N-1 tenants'
    // metadata edges (canonical tenant-isolation invariant absorbed
    // from the S-07 dedup pattern).
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let r2_cas = adapters_typed
        .iter()
        .find(|a| a.kind() == BackendKind::R2Cas)
        .unwrap()
        .clone();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(audit, ledger, adapters_dyn).unwrap();

    let tenants: Vec<(Uuid, Uuid)> = (1..=5u8)
        .map(|seed| (fixed(seed), fixed(seed.wrapping_add(50))))
        .collect();

    for (t, s) in &tenants {
        r2_cas.insert_rows(
            *t,
            *s,
            vec![InMemoryRow::new(b"shared".to_vec()).with_refcount(5)],
        );
    }

    // Erase tenant 0.
    let (t_target, s_target) = tenants.first().unwrap();
    let req = ErasureRequest::new(
        fixed(99),
        *t_target,
        *s_target,
        ErasureSalt::synthetic_for_test(7),
        1_000,
    );
    worker.process_erasure(&req, 1_000).unwrap();

    // Other 4 tenants intact.
    for (t, s) in tenants.iter().skip(1) {
        assert_eq!(r2_cas.row_count(*t, *s), 1);
    }
}
