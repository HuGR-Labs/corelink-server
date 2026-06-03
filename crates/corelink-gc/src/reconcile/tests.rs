//! Reconcile unit tests — fixture + config/gate/sev tests + canonical
//! reconcile-decision tests. Consolidated from the pre-split
//! `#[cfg(test)] mod tests` block per Wave 33 Stream A2.2.
//!
//! Larger end-to-end scenario tests live in the sibling
//! [`super::tests_scenarios`] module so each file stays under the L2.10
//! HARD CAP (500 LOC).

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

use crate::audit::{GcEventType, InMemoryGcAuditSink};
use crate::mark::BlobDigest;
use crate::metrics::InMemoryGcMetrics;
use crate::region::GcRegion;
use crate::run::{GcPhase, GcRunStore, InMemoryGcRunStore, RunId};

use super::{
    auto_fix_gate_fires, sev_level_for, AcMetaReconcileRow, BlobMetaReconcileRow,
    CountingReconcileClock, InMemoryBlobMetaRefcountStore, InMemoryReconcilePhase,
    InMemoryRefcountSource, ReconcileConfig, ReconcilePhase, SevLevel, AUTO_FIX_MAX_PERCENT,
    AUTO_FIX_MAX_RECORDS, CANONICAL_RECONCILE_PHASE_BUDGET_MS, SEV1_PER_TENANT_DRIFT_PERCENT,
    SEV2_GLOBAL_DRIFT_PERCENT,
};

pub(super) fn digest(seed: u32) -> BlobDigest {
    let prefix = format!("{seed:08x}");
    let mut s = prefix;
    s.push_str(&"0".repeat(BlobDigest::LEN - 8));
    BlobDigest::parse(&s).expect("canonical hex")
}

pub(super) type Fixture = (
    InMemoryReconcilePhase<
        InMemoryGcRunStore,
        InMemoryRefcountSource,
        InMemoryBlobMetaRefcountStore,
        InMemoryGcAuditSink,
        InMemoryGcMetrics,
        CountingReconcileClock,
    >,
    Arc<InMemoryGcRunStore>,
    Arc<InMemoryRefcountSource>,
    Arc<InMemoryBlobMetaRefcountStore>,
    Arc<InMemoryGcAuditSink>,
);

pub(super) fn fresh(start_ms: u64) -> Fixture {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let refcount_source = Arc::new(InMemoryRefcountSource::new());
    let blob_meta = Arc::new(InMemoryBlobMetaRefcountStore::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingReconcileClock::new(start_ms));
    let phase = InMemoryReconcilePhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&refcount_source),
        Arc::clone(&blob_meta),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    (phase, runs, refcount_source, blob_meta, audit)
}

pub(super) fn seed_run_in_reconcile(
    runs: &InMemoryGcRunStore,
    rid: RunId,
    tenant: Uuid,
    region: GcRegion,
) {
    runs.insert_pending(rid, tenant, region, 100, "cron".into())
        .unwrap();
    runs.acquire_running(rid, tenant, 200).unwrap();
    runs.transition_phase(rid, tenant, GcPhase::Mark, 300)
        .unwrap();
    runs.transition_phase(rid, tenant, GcPhase::Sweep, 400)
        .unwrap();
    runs.transition_phase(rid, tenant, GcPhase::PhysicalDelete, 500)
        .unwrap();
    runs.transition_phase(rid, tenant, GcPhase::Reconcile, 600)
        .unwrap();
}

pub(super) fn seed_blob(
    blob_meta: &InMemoryBlobMetaRefcountStore,
    tenant: Uuid,
    d: BlobDigest,
    stored: u32,
) {
    blob_meta.push_row(
        tenant,
        BlobMetaReconcileRow {
            digest: d,
            stored_refcount: stored,
            deleted_at_ms: None,
            r2_present: true,
        },
    );
}

pub(super) fn seed_ac_row(source: &InMemoryRefcountSource, tenant: Uuid, refs: Vec<BlobDigest>) {
    source.push_ac_row(
        tenant,
        AcMetaReconcileRow {
            action_digest: format!("action_{}", refs.len()),
            blob_refs: refs,
            deleted_at_ms: None,
            created_at_ms: 100,
        },
    );
}

#[test]
fn canonical_constants_pinned() {
    assert_eq!(CANONICAL_RECONCILE_PHASE_BUDGET_MS, 60 * 60 * 1000);
    assert_eq!(AUTO_FIX_MAX_RECORDS, 5);
    assert_eq!(AUTO_FIX_MAX_PERCENT, 0.0001);
    assert_eq!(SEV1_PER_TENANT_DRIFT_PERCENT, 0.01);
    assert_eq!(SEV2_GLOBAL_DRIFT_PERCENT, 0.001);
}

#[test]
fn config_default_canonical() {
    let cfg = ReconcileConfig::default();
    assert_eq!(cfg.auto_fix_max_records(), AUTO_FIX_MAX_RECORDS);
    assert_eq!(cfg.auto_fix_max_percent(), AUTO_FIX_MAX_PERCENT);
    assert_eq!(
        cfg.sev1_per_tenant_drift_percent(),
        SEV1_PER_TENANT_DRIFT_PERCENT
    );
    assert_eq!(cfg.sev2_global_drift_percent(), SEV2_GLOBAL_DRIFT_PERCENT);
    assert_eq!(cfg.phase_budget_ms(), CANONICAL_RECONCILE_PHASE_BUDGET_MS);
}

#[test]
fn config_rejects_zero_budget() {
    assert!(ReconcileConfig::new(5, 0.0001, 0.01, 0.001, 0).is_err());
}

#[test]
fn config_rejects_inverted_sev_thresholds() {
    // SEV-1 must be strictly greater than SEV-2.
    assert!(ReconcileConfig::new(5, 0.0001, 0.001, 0.001, 1).is_err());
    assert!(ReconcileConfig::new(5, 0.0001, 0.0005, 0.001, 1).is_err());
}

#[test]
fn config_rejects_non_finite_percentages() {
    assert!(ReconcileConfig::new(5, f64::NAN, 0.01, 0.001, 1).is_err());
    assert!(ReconcileConfig::new(5, 0.0001, f64::INFINITY, 0.001, 1).is_err());
    assert!(ReconcileConfig::new(5, 0.0001, 0.01, f64::NEG_INFINITY, 1).is_err());
}

#[test]
fn config_rejects_negative_auto_fix_percent() {
    assert!(ReconcileConfig::new(5, -0.0001, 0.01, 0.001, 1).is_err());
}

#[test]
fn auto_fix_gate_dual_condition_count_boundary() {
    let cfg = ReconcileConfig::default();
    // count = 5 AND percent = 0.0001 → fires.
    assert!(auto_fix_gate_fires(5, 0.0001, &cfg));
    // count = 6 → fails (count arm).
    assert!(!auto_fix_gate_fires(6, 0.0001, &cfg));
}

#[test]
fn auto_fix_gate_dual_condition_percent_boundary() {
    let cfg = ReconcileConfig::default();
    // percent = 0.0001 → fires.
    assert!(auto_fix_gate_fires(5, 0.0001, &cfg));
    // percent = 0.000101 (just above) → fails.
    assert!(!auto_fix_gate_fires(5, 0.000_101, &cfg));
}

#[test]
fn auto_fix_gate_either_violation_rejects() {
    let cfg = ReconcileConfig::default();
    // count fail only.
    assert!(!auto_fix_gate_fires(6, 0.000_05, &cfg));
    // percent fail only.
    assert!(!auto_fix_gate_fires(2, 0.5, &cfg));
    // both fail.
    assert!(!auto_fix_gate_fires(100, 0.5, &cfg));
}

#[test]
fn sev_level_for_canonical_thresholds() {
    let cfg = ReconcileConfig::default();
    // No drift = None.
    assert_eq!(sev_level_for(0.0, 0.0, &cfg), SevLevel::None);
    // Just below SEV-2 threshold = None.
    assert_eq!(sev_level_for(0.0009, 0.0009, &cfg), SevLevel::None);
    // SEV-2 fires at 0.1% global drift.
    assert_eq!(sev_level_for(0.005, 0.005, &cfg), SevLevel::Sev2);
    // SEV-1 fires at >1% per-tenant.
    assert_eq!(sev_level_for(0.005, 0.02, &cfg), SevLevel::Sev1);
}

#[test]
fn happy_path_no_drift() {
    // 1 blob; ac_meta references it once; stored = 1 = expected.
    let (phase, runs, source, blob_meta, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest(0x1);
    seed_blob(&blob_meta, tenant, d.clone(), 1);
    seed_ac_row(&source, tenant, vec![d.clone()]);

    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_scanned, 1);
    assert_eq!(result.no_drift_count, 1);
    assert_eq!(result.drifts_detected, 0);
    assert_eq!(result.sev_level, SevLevel::None);
    assert_eq!(audit.snapshot_of(GcEventType::RefcountReconciled).len(), 1);
    assert!(audit.snapshot_of(GcEventType::RefcountAutoFixed).is_empty());
    assert!(audit
        .snapshot_of(GcEventType::RefcountManualReviewRequired)
        .is_empty());
    // blob_meta stored refcount untouched.
    assert_eq!(blob_meta.snapshot(tenant, &d).unwrap().stored_refcount, 1);
}

#[test]
fn auto_fix_small_drift_dual_condition_passes() {
    // 1000 blobs, 1 drift = 0.1% = 0.001 — fails the percentage
    // gate (> 0.0001), so manual review even with count=1. To get
    // both arms passing, need >=10000 blobs at 1 drift.
    let (phase, runs, source, blob_meta, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    // Seed 10001 blobs; the first 10000 are correctly counted
    // (stored=0 expected=0), the last has stored=0 expected=1
    // (drift of 1).
    for i in 0..10_000_u32 {
        let d = digest(i + 1);
        seed_blob(&blob_meta, tenant, d, 0);
    }
    let drift_d = digest(50_000);
    seed_blob(&blob_meta, tenant, drift_d.clone(), 0);
    seed_ac_row(&source, tenant, vec![drift_d.clone()]);

    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_scanned, 10_001);
    assert_eq!(result.drifts_detected, 1);
    // 1 / 10001 ≈ 0.0001 < 0.0001? Equal at boundary; auto-fixes.
    // Actually 1/10001 < 0.0001, so passes percentage gate. Count
    // = 1 ≤ 5, passes count gate. → auto-fixed.
    assert_eq!(result.auto_fixed_count, 1);
    assert_eq!(result.manual_review_count, 0);
    let updated = blob_meta.snapshot(tenant, &drift_d).unwrap();
    assert_eq!(updated.stored_refcount, 1);
}

#[test]
fn manual_review_drift_percent_gate_fails() {
    // 10 blobs, 1 drift = 10% drift — count=1 ≤ 5 (count arm pass);
    // percent=10% > 0.01% (percent arm fail) → manual review.
    let (phase, runs, source, blob_meta, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    for i in 0..9_u32 {
        let d = digest(i + 1);
        seed_blob(&blob_meta, tenant, d, 0);
    }
    let drift_d = digest(50_000);
    seed_blob(&blob_meta, tenant, drift_d.clone(), 0);
    seed_ac_row(&source, tenant, vec![drift_d.clone()]);

    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_scanned, 10);
    assert_eq!(result.drifts_detected, 1);
    assert_eq!(result.auto_fixed_count, 0);
    assert_eq!(result.manual_review_count, 1);
    // SEV-1 because per-tenant drift = 10% > 1%.
    assert_eq!(result.sev_level, SevLevel::Sev1);
    // Stored refcount UNTOUCHED on manual review.
    assert_eq!(
        blob_meta
            .snapshot(tenant, &drift_d)
            .unwrap()
            .stored_refcount,
        0
    );
    assert_eq!(
        audit
            .snapshot_of(GcEventType::RefcountManualReviewRequired)
            .len(),
        1
    );
}

#[test]
fn manual_review_drift_count_gate_fails_at_six() {
    // 1M blobs, 6 drifts: count=6 > 5 (count arm fail);
    // percent=6/1M = 6e-6 < 0.0001 (percent arm pass) → manual
    // review (either-arm-fail).
    let (phase, runs, source, blob_meta, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    // Seed 60_000 non-drift + 6 drift blobs (smaller scale for
    // test speed; 6 / 60_006 ≈ 0.0001 BUT just barely below
    // 0.0001 so percent passes.)
    for i in 0..60_000_u32 {
        let d = digest(i + 1);
        seed_blob(&blob_meta, tenant, d, 0);
    }
    for i in 0..6_u32 {
        let d = digest(800_000 + i);
        seed_blob(&blob_meta, tenant, d.clone(), 0);
        seed_ac_row(&source, tenant, vec![d]);
    }
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_scanned, 60_006);
    assert_eq!(result.drifts_detected, 6);
    // First 5 drifts auto-fix (count cumulative 1..=5; percent
    // tiny). 6th drift: count=6, percent ~= 6/60006 = ~1e-4 right
    // around boundary — but operative is the count arm: 6 > 5,
    // gate fails → manual review.
    assert_eq!(result.auto_fixed_count, 5);
    assert_eq!(result.manual_review_count, 1);
    assert_eq!(
        audit
            .snapshot_of(GcEventType::RefcountManualReviewRequired)
            .len(),
        1
    );
}

#[test]
fn json_each_semantics_substring_collision_ignored() {
    // The cardinal regression: a row where blob_refs is `[abc...]`
    // and a different digest `abc...1234` (substring match would
    // double-count, json_each does NOT). Stored=1 expected=1 →
    // no drift. With LIKE substring this would inflate to 2 →
    // false drift signal → wrong UPDATE.
    let (phase, runs, source, blob_meta, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    // Both digests are canonical 64-char hex; the prefix is
    // shared but the suffix differs — substring `LIKE` would
    // match both.
    let target =
        BlobDigest::parse("abcdef0123456789000000000000000000000000000000000000000000000000")
            .unwrap();
    // Same prefix + different suffix; "abcdef0123" appears as a
    // prefix in both — `LIKE '%target%'` would NOT collision but
    // a different attack vector is "target appears as substring
    // of unrelated digest". Construct a digest whose hex form
    // contains the target hex form by concatenation:
    // target = "abcd…000". An ac row referencing "1abcd…000XYZ"
    // would NOT exist because all digests are 64 chars
    // canonical. So the LIKE attack vector here is *metadata*
    // string scanning (e.g. action_digest) — but the production
    // SQL uses `j.value = digest` which is exact.
    // The test pins the property: an unrelated row whose
    // `action_digest` contains the target prefix MUST NOT inflate
    // expected_refcount.
    seed_blob(&blob_meta, tenant, target.clone(), 1);
    // ac row references target exactly (correct count = 1).
    source.push_ac_row(
        tenant,
        AcMetaReconcileRow {
            action_digest: format!("metadata_contains_{target}_prefix"),
            blob_refs: vec![target.clone()],
            deleted_at_ms: None,
            created_at_ms: 100,
        },
    );

    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.no_drift_count, 1);
    assert_eq!(result.drifts_detected, 0);
    // Stored 1 = expected 1 (json_each membership exact).
    assert_eq!(
        blob_meta.snapshot(tenant, &target).unwrap().stored_refcount,
        1
    );
}
