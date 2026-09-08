use super::*;

use crate::audit::InMemoryGcAuditSink;
use crate::mark::{InMemoryGcCandidatesStore, MarkConfig};
use crate::metrics::InMemoryGcMetrics;
use crate::run::InMemoryGcRunStore;

fn digest(seed: u32) -> BlobDigest {
    let prefix = format!("{seed:08x}");
    let mut s = prefix;
    s.push_str(&"0".repeat(BlobDigest::LEN - 8));
    BlobDigest::parse(&s).expect("canonical hex")
}

type Fixture = (
    InMemorySweepPhase<
        InMemoryGcRunStore,
        InMemoryGcCandidatesStore,
        InMemoryBlobMetaStore,
        InMemoryAcReferenceIndex,
        InMemoryGcAuditSink,
        InMemoryGcMetrics,
        CountingSweepClock,
    >,
    Arc<InMemoryGcRunStore>,
    Arc<InMemoryGcCandidatesStore>,
    Arc<InMemoryBlobMetaStore>,
    Arc<InMemoryAcReferenceIndex>,
    Arc<InMemoryGcAuditSink>,
);

fn fresh(start_ms: u64) -> Fixture {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta = Arc::new(InMemoryBlobMetaStore::new());
    let ac_index = Arc::new(InMemoryAcReferenceIndex::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingSweepClock::new(start_ms));
    let sweep = InMemorySweepPhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&ac_index),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    (sweep, runs, candidates, blob_meta, ac_index, audit)
}

fn seed_run_in_sweep(
    runs: &InMemoryGcRunStore,
    rid: RunId,
    tenant: Uuid,
    region: GcRegion,
    mark_anchor: u64,
) {
    runs.insert_pending(rid, tenant, region, 100, "cron".into())
        .unwrap();
    runs.acquire_running(rid, tenant, 200).unwrap();
    runs.transition_phase(rid, tenant, GcPhase::Mark, mark_anchor)
        .unwrap();
    runs.transition_phase(rid, tenant, GcPhase::Sweep, mark_anchor + 10)
        .unwrap();
}

fn seed_candidate(
    candidates: &InMemoryGcCandidatesStore,
    blob_meta: &InMemoryBlobMetaStore,
    tenant: Uuid,
    rid: RunId,
    d: BlobDigest,
    mark_anchor: u64,
) {
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: d.clone(),
            mark_started_at_ms: mark_anchor,
            mark_run_id: rid,
            blob_size_bytes: 4096,
            blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
            status: CandidateStatus::Candidate,
            created_at_ms: mark_anchor,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    blob_meta.push_row(
        tenant,
        BlobMetaRow {
            digest: d,
            size_bytes: 4096,
            refcount: 0,
            last_referenced_at_ms: mark_anchor.saturating_sub(1),
            created_at_ms: mark_anchor.saturating_sub(100),
            deleted_at_ms: None,
        },
    );
}

#[test]
fn canonical_grace_constants_pinned() {
    assert_eq!(GRACE_CAS_MS, 72 * 60 * 60 * 1000);
    assert_eq!(GRACE_AC_MS, 24 * 60 * 60 * 1000);
    assert_eq!(CANONICAL_SWEEP_PHASE_BUDGET_MS, 5 * 60 * 1000);
}

#[test]
fn sweep_config_default_canonical() {
    let cfg = SweepConfig::default();
    assert_eq!(cfg.grace_cas_ms(), GRACE_CAS_MS);
    assert_eq!(cfg.grace_ac_ms(), GRACE_AC_MS);
    assert_eq!(cfg.phase_budget_ms(), CANONICAL_SWEEP_PHASE_BUDGET_MS);
}

#[test]
fn sweep_config_rejects_inverted_grace() {
    let err = SweepConfig::new(GRACE_AC_MS, GRACE_CAS_MS, 1).unwrap_err();
    assert!(matches!(err, SweepError::Backend(_)));
}

#[test]
fn sweep_config_rejects_zero_budget() {
    assert!(SweepConfig::new(GRACE_CAS_MS, GRACE_AC_MS, 0).is_err());
}

#[test]
fn happy_path_orphan_swept() {
    let (sweep, runs, candidates, blob_meta, _ac, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xabcd);
    seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);

    let result = sweep.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.mark_started_at_ms, mark_anchor);
    assert_eq!(result.candidates_processed, 1);
    assert_eq!(result.blobs_swept_count, 1);
    assert_eq!(result.blobs_protected_re_ref_count, 0);
    assert_eq!(result.bytes_to_be_reclaimed, 4096);
    // blob_meta soft-deleted.
    let row = blob_meta.snapshot(tenant, &d).unwrap();
    assert!(row.deleted_at_ms.is_some());
    // gc_candidate flipped to Swept.
    let cand = candidates
        .lookup(tenant, &d, rid)
        .unwrap()
        .expect("candidate row");
    assert_eq!(cand.status, CandidateStatus::Swept);
    assert!(cand.swept_at_ms.is_some());
    // Audit emitted.
    assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 1);
    assert!(audit
        .snapshot_of(GcEventType::SweepProtectedReRef)
        .is_empty());
}

#[test]
fn protect_re_ref_strict_ge_boundary() {
    // ac.created_at_ms == mark_started_at_ms exactly → protected.
    let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Iad, mark_anchor);
    let d = digest(0x1111);
    seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
    // EXACTLY EQUAL — canonical TLA `>=` says PROTECT.
    ac.push_ac_row(tenant, "action_a", vec![d.clone()], mark_anchor);

    let result = sweep.execute(rid, tenant, GcRegion::Iad).unwrap();
    assert_eq!(result.blobs_swept_count, 0);
    assert_eq!(result.blobs_protected_re_ref_count, 1);
    // blob_meta NOT soft-deleted.
    let row = blob_meta.snapshot(tenant, &d).unwrap();
    assert!(row.deleted_at_ms.is_none());
    // gc_candidate flipped to ProtectedReRef.
    let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(cand.status, CandidateStatus::ProtectedReRef);
    assert!(cand.protected_at_ms.is_some());
    assert!(cand.protected_reason.is_some());
    // Audit emitted.
    assert_eq!(audit.snapshot_of(GcEventType::SweepProtectedReRef).len(), 1);
    assert!(audit.snapshot_of(GcEventType::SweepSoftDeleted).is_empty());
}

#[test]
fn no_protect_when_ac_predates_mark_anchor() {
    let (sweep, runs, candidates, blob_meta, ac, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Lhr, mark_anchor);
    let d = digest(0x2222);
    seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
    // ac.created_at_ms = mark_anchor - 1 → NOT protected.
    ac.push_ac_row(tenant, "action_b", vec![d.clone()], mark_anchor - 1);

    let result = sweep.execute(rid, tenant, GcRegion::Lhr).unwrap();
    assert_eq!(result.blobs_swept_count, 1);
    assert_eq!(result.blobs_protected_re_ref_count, 0);
}

#[test]
fn idempotent_re_run_already_swept() {
    let (sweep, runs, candidates, blob_meta, _ac, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Nrt, mark_anchor);
    let d = digest(0x3333);
    seed_candidate(&candidates, &blob_meta, tenant, rid, d, mark_anchor);

    let _r1 = sweep.execute(rid, tenant, GcRegion::Nrt).unwrap();
    let r2 = sweep.execute(rid, tenant, GcRegion::Nrt).unwrap();
    assert_eq!(r2.blobs_swept_count, 0);
    assert_eq!(r2.already_resolved_count, 1);
    // Audit only emitted ONCE (no re-emit on idempotent re-run).
    assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 1);
}

#[test]
fn mark_anchor_missing_rejected() {
    // gc_run without Mark transition → MarkAnchorMissing.
    let (sweep, runs, _c, _b, _ac, _aud) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    runs.insert_pending(rid, tenant, GcRegion::Sam, 100, "cron".into())
        .unwrap();
    runs.acquire_running(rid, tenant, 200).unwrap();
    let err = sweep.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, SweepError::MarkAnchorMissing { .. }));
}

#[test]
fn region_mismatch_rejected() {
    let (sweep, runs, _c, _b, _ac, _aud) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let err = sweep.execute(rid, tenant, GcRegion::Iad).unwrap_err();
    assert!(matches!(err, SweepError::RegionMismatch { .. }));
}

#[test]
fn run_not_found_rejected() {
    let (sweep, _runs, _c, _b, _ac, _aud) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(99));
    let err = sweep.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, SweepError::RunStore(_)));
}

#[test]
fn cross_tenant_run_returns_not_found() {
    // The Layer 4 envelope is tested at the run-store seam:
    // gc_run.lookup(rid, wrong_tenant) returns Ok(None).
    let (sweep, runs, _c, _b, _ac, _aud) = fresh(1_000);
    let tenant_a = Uuid::from_u128(1);
    let tenant_b = Uuid::from_u128(2);
    let rid = RunId(Uuid::from_u128(99));
    seed_run_in_sweep(&runs, rid, tenant_a, GcRegion::Sam, 500);
    // Tenant B asks for the run id → MarkAnchorMissing/NotFound
    // (the lookup returns None, then the .ok_or maps to NotFound).
    let err = sweep.execute(rid, tenant_b, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, SweepError::RunStore(_)));
}

#[test]
fn tenant_isolation_no_cross_sweep() {
    let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(1_000);
    let ta = Uuid::from_u128(1);
    let tb = Uuid::from_u128(2);
    let ra = RunId(Uuid::from_u128(10));
    let rb = RunId(Uuid::from_u128(11));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, ra, ta, GcRegion::Sam, mark_anchor);
    seed_run_in_sweep(&runs, rb, tb, GcRegion::Sam, mark_anchor);
    let d = digest(0xdead);
    seed_candidate(&candidates, &blob_meta, ta, ra, d.clone(), mark_anchor);
    seed_candidate(&candidates, &blob_meta, tb, rb, d.clone(), mark_anchor);
    // Tenant A has a re-ref; tenant B does not.
    ac.push_ac_row(ta, "action_a", vec![d.clone()], mark_anchor + 1);

    let result_a = sweep.execute(ra, ta, GcRegion::Sam).unwrap();
    let result_b = sweep.execute(rb, tb, GcRegion::Sam).unwrap();
    // A protected; B swept.
    assert_eq!(result_a.blobs_protected_re_ref_count, 1);
    assert_eq!(result_a.blobs_swept_count, 0);
    assert_eq!(result_b.blobs_protected_re_ref_count, 0);
    assert_eq!(result_b.blobs_swept_count, 1);
    // Audit captures the per-tenant decisions.
    let protected_audits = audit.snapshot_of(GcEventType::SweepProtectedReRef);
    let swept_audits = audit.snapshot_of(GcEventType::SweepSoftDeleted);
    assert_eq!(protected_audits.len(), 1);
    assert_eq!(swept_audits.len(), 1);
    assert_eq!(protected_audits[0].tenant_id, ta);
    assert_eq!(swept_audits[0].tenant_id, tb);
    // Tenant A's blob_meta NOT soft-deleted; tenant B's IS.
    assert!(blob_meta.snapshot(ta, &d).unwrap().deleted_at_ms.is_none());
    assert!(blob_meta.snapshot(tb, &d).unwrap().deleted_at_ms.is_some());
}

#[test]
fn undelete_via_reupload_restores_blob_meta() {
    // CAP-GC-002 reversibility — blob_meta soft-deleted within
    // grace; a CAS write handler call resets `deleted_at = NULL`.
    let blob_meta = InMemoryBlobMetaStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0xfeed);
    blob_meta.push_row(
        tenant,
        BlobMetaRow {
            digest: d.clone(),
            size_bytes: 1024,
            refcount: 0,
            last_referenced_at_ms: 100,
            created_at_ms: 50,
            deleted_at_ms: None,
        },
    );
    // Soft-delete.
    let prev = blob_meta
        .soft_delete(tenant, &d, 200)
        .unwrap()
        .expect("soft-delete should fire");
    assert_eq!(prev.size_bytes, 1024);
    // Within grace, customer re-uploads same digest → undelete.
    let restored = blob_meta.undelete(tenant, &d);
    assert!(restored);
    let row = blob_meta.snapshot(tenant, &d).unwrap();
    assert!(row.deleted_at_ms.is_none());
}

#[test]
fn audit_emit_failure_blocks_status_flip() {
    // Use a sink that always errors; assert no soft-delete persists
    // through the candidate status flip when audit fails.
    #[derive(Debug, Default)]
    struct FailingSink;
    impl GcAuditSink for FailingSink {
        fn emit(&self, _record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
            Err(GcAuditSinkError::Store("simulated_failure".to_owned()))
        }
    }

    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta = Arc::new(InMemoryBlobMetaStore::new());
    let ac_index = Arc::new(InMemoryAcReferenceIndex::new());
    let audit = Arc::new(FailingSink);
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingSweepClock::new(1_000));
    let sweep = InMemorySweepPhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&ac_index),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );

    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Syd, mark_anchor);
    let d = digest(0xfeed);
    seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);

    let err = sweep.execute(rid, tenant, GcRegion::Syd).unwrap_err();
    assert!(matches!(err, SweepError::AuditEmissionFailed(_)));
    let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(cand.status, CandidateStatus::Candidate);
    let row = blob_meta.lookup(tenant, &d).unwrap().unwrap();
    assert!(row.deleted_at_ms.is_none());
}

#[test]
fn protect_path_preserves_audit_field_taxonomy() {
    // Audit record emitted on protect arm carries the canonical
    // event type + tenant + region + run_id + protected reason.
    let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Iad, mark_anchor);
    let d = digest(0x7777);
    seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
    ac.push_ac_row(tenant, "action_xyz", vec![d.clone()], mark_anchor + 5);
    let _ = sweep.execute(rid, tenant, GcRegion::Iad).unwrap();
    let recs = audit.snapshot_of(GcEventType::SweepProtectedReRef);
    assert_eq!(recs.len(), 1);
    let r = &recs[0];
    assert_eq!(r.tenant_id, tenant);
    assert_eq!(r.run_id, rid);
    assert_eq!(r.region, GcRegion::Iad);
    assert_eq!(r.from_phase, Some(GcPhase::Sweep));
    assert_eq!(r.to_phase, Some(GcPhase::Sweep));
    assert_eq!(r.reason, "inv_gc_004_protected_re_ref");
}

#[test]
fn step_candidate_preserves_already_resolved_for_swept() {
    let (sweep, _runs, candidates, blob_meta, _ac, _audit) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let d = digest(0x4242);
    let mark_anchor = 500;
    seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            mark_anchor + 100,
            None,
        )
        .unwrap();
    let already_swept = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    let dec = sweep.step_candidate(&already_swept, GcRegion::Sam).unwrap();
    match dec {
        SweepDecision::AlreadyResolved { observed_status } => {
            assert_eq!(observed_status, CandidateStatus::Swept);
        }
        other => panic!("expected AlreadyResolved, got {other:?}"),
    }
}

#[test]
fn ac_index_only_returns_witness_with_matching_blob_ref() {
    let ac = InMemoryAcReferenceIndex::new();
    let tenant = Uuid::from_u128(1);
    let target = digest(0x9999);
    let other = digest(0x0001);
    // ac row references `other` only — no witness on `target`.
    ac.push_ac_row(tenant, "action_o", vec![other.clone()], 500);
    let res = ac.find_re_reference(tenant, &target, 100).unwrap();
    assert!(res.is_none());
    // Push a row referencing `target` at exactly 100 (>=) — witness fires.
    ac.push_ac_row(tenant, "action_t", vec![target.clone()], 100);
    let witness = ac.find_re_reference(tenant, &target, 100).unwrap();
    assert!(witness.is_some());
}

#[test]
fn ac_index_tenant_isolation() {
    let ac = InMemoryAcReferenceIndex::new();
    let ta = Uuid::from_u128(1);
    let tb = Uuid::from_u128(2);
    let d = digest(0x4321);
    ac.push_ac_row(ta, "action_a", vec![d.clone()], 1_000);
    // Tenant B should NOT see tenant A's ac row.
    let res = ac.find_re_reference(tb, &d, 500).unwrap();
    assert!(res.is_none());
    // Tenant A sees it.
    let res = ac.find_re_reference(ta, &d, 500).unwrap();
    assert!(res.is_some());
}

#[test]
fn region_value_round_trip_in_sweep_audit() {
    // Sanity: every region appears in sweep audit emit.
    for region in GcRegion::all() {
        let (sweep, runs, candidates, blob_meta, _ac, audit) = fresh(1_000);
        let tenant = Uuid::from_u128(u128::from(region.as_str().len() as u32) + 1);
        let rid = RunId(Uuid::from_u128(42));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, *region, mark_anchor);
        let d = digest(0xc0de);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d, mark_anchor);
        let _ = sweep.execute(rid, tenant, *region).unwrap();
        let recs = audit.snapshot_of(GcEventType::SweepSoftDeleted);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].region, *region);
    }
}

#[test]
fn cross_module_canonical_constants() {
    // Ensure canonical constants don't drift from sprint contract
    // §5.3 (R-S06-7.1 sweep budget = 5 min) + §5.3 R-S06-6 grace.
    let cfg = MarkConfig::default();
    // Mark phase budget is 10 min canonical; sweep is 5 min — they
    // are independent budget lines, sum ≤ 15 min total per
    // (tenant, region) cron tick.
    assert_eq!(
        cfg.phase_budget_ms() + CANONICAL_SWEEP_PHASE_BUDGET_MS,
        15 * 60 * 1000
    );
}
