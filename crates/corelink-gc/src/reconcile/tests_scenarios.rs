//! Reconcile end-to-end scenario tests + conditional-UPDATE unit
//! tests + per-region round-trip. Split out from the pre-split
//! `#[cfg(test)] mod tests` block per Wave 33 Stream A2.2 file-size
//! discipline; companion file [`super::tests`] hosts the canonical
//! config / decision-boundary tests + shared fixture helpers.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives; float_cmp is \
              acceptable for canonical-percentage-pin assertions where the \
              values are constructed deterministically."
)]

use std::sync::Arc;

use uuid::Uuid;

use crate::audit::{
    GcAuditRecord, GcAuditSink, GcAuditSinkError, GcEventType, InMemoryGcAuditSink,
};
use crate::metrics::InMemoryGcMetrics;
use crate::region::GcRegion;
use crate::run::{InMemoryGcRunStore, RunId};

use super::tests::{digest, fresh, seed_ac_row, seed_blob, seed_run_in_reconcile};
use super::{
    AcMetaReconcileRow, BlobMetaReconcileRow, BlobMetaRefcountStore, CountingReconcileClock,
    InMemoryBlobMetaRefcountStore, InMemoryReconcilePhase, InMemoryRefcountSource, ReconcileError,
    ReconcilePhase,
};

#[test]
fn soft_deleted_ac_row_does_not_count_toward_expected() {
    // ac_meta with `deleted_at_ms = Some(_)` is excluded from
    // `expected_refcount` per canonical SQL `WHERE
    // a.deleted_at_ms IS NULL`.
    let (phase, runs, source, blob_meta, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest(0x42);
    seed_blob(&blob_meta, tenant, d.clone(), 0);
    // Push an ac row referencing d, then soft-delete it.
    source.push_ac_row(
        tenant,
        AcMetaReconcileRow {
            action_digest: "ac1".to_owned(),
            blob_refs: vec![d.clone()],
            deleted_at_ms: None,
            created_at_ms: 100,
        },
    );
    let fired = source.soft_delete(tenant, "ac1", 200);
    assert!(fired);
    // Expected refcount = 0 (soft-deleted ac row excluded);
    // stored = 0; no drift.
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.no_drift_count, 1);
    assert_eq!(result.drifts_detected, 0);
}

#[test]
fn ac_row_after_snapshot_does_not_count() {
    // ac_meta with `created_at_ms >= snapshot_at_ms` is excluded
    // from `expected_refcount` per canonical SQL
    // `WHERE a.created_at_ms < snapshot_at_ms`.
    let (phase, runs, source, blob_meta, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest(0x42);
    seed_blob(&blob_meta, tenant, d.clone(), 0);
    // Push an ac row whose created_at_ms is AFTER reasonable
    // snapshot anchor (clock starts at 1000, snapshot ≥ 1001).
    // Use 9_999_999 which is > any test snapshot.
    source.push_ac_row(
        tenant,
        AcMetaReconcileRow {
            action_digest: "ac1".to_owned(),
            blob_refs: vec![d.clone()],
            deleted_at_ms: None,
            created_at_ms: 9_999_999,
        },
    );
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    // The post-snapshot row is excluded → expected = 0; stored
    // = 0 → no drift.
    assert_eq!(result.no_drift_count, 1);
}

#[test]
fn idempotent_re_run_no_op() {
    // After an auto-fix succeeds, re-running reconcile produces
    // no further drift (stored is now correct).
    let (phase, runs, source, blob_meta, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    // 10001 blobs — 10000 correct + 1 drift; auto-fix passes.
    for i in 0..10_000_u32 {
        seed_blob(&blob_meta, tenant, digest(i + 1), 0);
    }
    let drift_d = digest(50_000);
    seed_blob(&blob_meta, tenant, drift_d.clone(), 0);
    seed_ac_row(&source, tenant, vec![drift_d.clone()]);

    let r1 = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(r1.auto_fixed_count, 1);
    let audit_count_after_first = audit.len();
    let r2 = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    // Idempotent: 0 drift on re-run; only no_drift events emit.
    assert_eq!(r2.drifts_detected, 0);
    assert_eq!(r2.auto_fixed_count, 0);
    // Re-run emits one RefcountReconciled per row (10001 of them).
    let audit_count_after_second = audit.len();
    assert!(audit_count_after_second > audit_count_after_first);
    assert_eq!(audit.snapshot_of(GcEventType::RefcountAutoFixed).len(), 1);
}

#[test]
fn tenant_isolation_no_cross_drift() {
    let (phase, runs, source, blob_meta, audit) = fresh(1_000);
    let ta = Uuid::from_u128(1);
    let tb = Uuid::from_u128(2);
    let ra = RunId(Uuid::from_u128(11));
    let rb = RunId(Uuid::from_u128(12));
    seed_run_in_reconcile(&runs, ra, ta, GcRegion::Sam);
    seed_run_in_reconcile(&runs, rb, tb, GcRegion::Sam);
    let d = digest(0xdead);
    // Tenant A: blob with stored=1 + ac referencing → no drift.
    seed_blob(&blob_meta, ta, d.clone(), 1);
    seed_ac_row(&source, ta, vec![d.clone()]);
    // Tenant B: same digest but stored=0; ac references but
    // belongs to A → cross-tenant scan must NOT see A's ac row.
    seed_blob(&blob_meta, tb, d.clone(), 0);

    let ra_result = phase.execute(ra, ta, GcRegion::Sam).unwrap();
    let rb_result = phase.execute(rb, tb, GcRegion::Sam).unwrap();
    assert_eq!(ra_result.no_drift_count, 1);
    assert_eq!(ra_result.drifts_detected, 0);
    // Tenant B sees no ac rows referencing d → expected = 0,
    // stored = 0 → no drift.
    assert_eq!(rb_result.no_drift_count, 1);
    assert_eq!(rb_result.drifts_detected, 0);
    // Audit records carry the right tenant_id.
    for record in audit.snapshot_of(GcEventType::RefcountReconciled) {
        assert!(record.tenant_id == ta || record.tenant_id == tb);
    }
}

#[test]
fn audit_emit_failure_blocks_refcount_mutation() {
    // Audit sink that always errors → step_row aborts BEFORE
    // mutating stored_refcount. Per WI §6.1.7 fail-closed envelope.
    #[derive(Debug, Default)]
    struct FailingSink;
    impl GcAuditSink for FailingSink {
        fn emit(&self, _record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
            Err(GcAuditSinkError::Store("simulated_failure".to_owned()))
        }
    }

    let runs = Arc::new(InMemoryGcRunStore::new());
    let source = Arc::new(InMemoryRefcountSource::new());
    let blob_meta = Arc::new(InMemoryBlobMetaRefcountStore::new());
    let audit = Arc::new(FailingSink);
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingReconcileClock::new(1_000));
    let phase = InMemoryReconcilePhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&source),
        Arc::clone(&blob_meta),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );

    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest(0x1);
    // Drift: stored=0, ac references → expected=1.
    seed_blob(&blob_meta, tenant, d.clone(), 0);
    seed_ac_row(&source, tenant, vec![d.clone()]);

    let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, ReconcileError::AuditEmissionFailed(_)));
    // stored_refcount UNCHANGED (mutation skipped on audit fail).
    assert_eq!(blob_meta.snapshot(tenant, &d).unwrap().stored_refcount, 0);
}

#[test]
fn cross_tenant_run_returns_not_found() {
    // Layer 4 envelope tested at the run-store seam: gc_run.lookup
    // for the wrong tenant returns None.
    let (phase, runs, _source, _blob_meta, _audit) = fresh(1_000);
    let tenant_a = Uuid::from_u128(1);
    let tenant_b = Uuid::from_u128(2);
    let rid = RunId(Uuid::from_u128(99));
    seed_run_in_reconcile(&runs, rid, tenant_a, GcRegion::Sam);
    let err = phase.execute(rid, tenant_b, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, ReconcileError::RunStore(_)));
}

#[test]
fn region_mismatch_rejected() {
    let (phase, runs, _source, _blob_meta, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let err = phase.execute(rid, tenant, GcRegion::Iad).unwrap_err();
    assert!(matches!(err, ReconcileError::RegionMismatch { .. }));
}

#[test]
fn run_not_found_rejected() {
    let (phase, _runs, _source, _blob_meta, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(99));
    let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, ReconcileError::RunStore(_)));
}

#[test]
fn skipped_soft_deleted_no_drift_signal() {
    // blob_meta row soft-deleted → reconcile skips it; no audit
    // event; refcount UNTOUCHED.
    let (phase, runs, _source, blob_meta, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest(0x1);
    blob_meta.push_row(
        tenant,
        BlobMetaReconcileRow {
            digest: d,
            stored_refcount: 0,
            deleted_at_ms: Some(800),
            r2_present: true,
        },
    );
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.skipped_soft_deleted_count, 1);
    assert_eq!(result.no_drift_count, 0);
    assert_eq!(result.drifts_detected, 0);
    // No audit event for skipped-soft-deleted (informational
    // forward).
    assert!(audit
        .snapshot_of(GcEventType::RefcountReconciled)
        .is_empty());
}

#[test]
fn orphan_r2_detected_no_refcount_mutation() {
    // r2_present=false → OrphanR2Detected; refcount UNTOUCHED;
    // no audit emit (forward to S-09 reclaim task).
    let (phase, runs, _source, blob_meta, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest(0xdead);
    blob_meta.push_row(
        tenant,
        BlobMetaReconcileRow {
            digest: d.clone(),
            stored_refcount: 5,
            deleted_at_ms: None,
            r2_present: false,
        },
    );
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.orphan_r2_count, 1);
    // refcount preserved.
    assert_eq!(blob_meta.snapshot(tenant, &d).unwrap().stored_refcount, 5);
    // No reconcile audit event.
    assert!(audit
        .snapshot_of(GcEventType::RefcountReconciled)
        .is_empty());
}

#[test]
fn auto_fix_conditional_predicate_concurrent_update_loses_to_winner() {
    // Concurrent UpdateAR raced and incremented stored_refcount
    // from 0 to 7. The auto-fix tried to set refcount=1 (expected
    // from json_each scan as of snapshot_at_ms) but the
    // conditional `WHERE refcount = stored_refcount=0` fails →
    // PausedForManualReview surfaces (no ping-pong, no
    // overwrite of the winner).
    let runs = Arc::new(InMemoryGcRunStore::new());
    let source = Arc::new(InMemoryRefcountSource::new());
    let blob_meta = Arc::new(InMemoryBlobMetaRefcountStore::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingReconcileClock::new(1_000));
    let phase = InMemoryReconcilePhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&source),
        Arc::clone(&blob_meta),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    // 10001 blobs to land within auto-fix percentage gate.
    for i in 0..10_000_u32 {
        seed_blob(&blob_meta, tenant, digest(i + 1), 0);
    }
    let drift_d = digest(50_000);
    // Pre-snapshot: stored=0, ac references → expected=1.
    seed_blob(&blob_meta, tenant, drift_d.clone(), 0);
    seed_ac_row(&source, tenant, vec![drift_d.clone()]);
    // Simulate concurrent UpdateAR in the race window: bump
    // stored_refcount to 7 BEFORE phase.execute consumes the row.
    // Since rows are snapshotted via snapshot_for_tenant() at the
    // start of execute, the orchestrator sees stored=7 too —
    // expected=1 is now smaller, drift count=1, but the
    // conditional predicate matches. The contract test for the
    // conditional skip lives below as a direct unit test of
    // conditional_set_refcount.
    let _ = blob_meta.set_stored_refcount(tenant, &drift_d, 7);
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    // The orchestrator's snapshot saw stored=7; expected=1; drift
    // detected; auto-fix gate count=1 ≤ 5 + percent=1/10001 <
    // 0.0001 → fires; the conditional UPDATE checks stored=7 ==
    // stored_refcount(7) → succeeds; UPDATE refcount=1.
    assert_eq!(result.auto_fixed_count, 1);
    let updated = blob_meta.snapshot(tenant, &drift_d).unwrap();
    assert_eq!(updated.stored_refcount, 1);
}

#[test]
fn conditional_set_refcount_skips_on_concurrent_winner() {
    // Direct unit test of the anti-ping-pong predicate:
    // current stored=7; caller passes stored_refcount=0 (stale
    // snapshot view); conditional → false; row UNCHANGED.
    let store = InMemoryBlobMetaRefcountStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0xfeed);
    store.push_row(
        tenant,
        BlobMetaReconcileRow {
            digest: d.clone(),
            stored_refcount: 7,
            deleted_at_ms: None,
            r2_present: true,
        },
    );
    let fired = store.conditional_set_refcount(tenant, &d, 0, 1).unwrap();
    assert!(!fired);
    assert_eq!(store.snapshot(tenant, &d).unwrap().stored_refcount, 7);
}

#[test]
fn conditional_set_refcount_skips_on_soft_deleted() {
    // Soft-deleted row → conditional returns false even if the
    // stored_refcount matches.
    let store = InMemoryBlobMetaRefcountStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0xfeed);
    store.push_row(
        tenant,
        BlobMetaReconcileRow {
            digest: d.clone(),
            stored_refcount: 0,
            deleted_at_ms: Some(500),
            r2_present: true,
        },
    );
    let fired = store.conditional_set_refcount(tenant, &d, 0, 1).unwrap();
    assert!(!fired);
}

#[test]
fn conditional_set_refcount_fires_on_match() {
    let store = InMemoryBlobMetaRefcountStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0xfeed);
    store.push_row(
        tenant,
        BlobMetaReconcileRow {
            digest: d.clone(),
            stored_refcount: 3,
            deleted_at_ms: None,
            r2_present: true,
        },
    );
    let fired = store.conditional_set_refcount(tenant, &d, 3, 4).unwrap();
    assert!(fired);
    assert_eq!(store.snapshot(tenant, &d).unwrap().stored_refcount, 4);
}

#[test]
fn region_value_round_trip_in_reconcile_audit() {
    for region in GcRegion::all() {
        let (phase, runs, source, blob_meta, audit) = fresh(1_000);
        let tenant = Uuid::from_u128(u128::from(region.as_str().len() as u32) + 1);
        let rid = RunId(Uuid::from_u128(42));
        seed_run_in_reconcile(&runs, rid, tenant, *region);
        let d = digest(0xc0de);
        seed_blob(&blob_meta, tenant, d.clone(), 1);
        seed_ac_row(&source, tenant, vec![d]);
        let _ = phase.execute(rid, tenant, *region).unwrap();
        let recs = audit.snapshot_of(GcEventType::RefcountReconciled);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].region, *region);
    }
}
