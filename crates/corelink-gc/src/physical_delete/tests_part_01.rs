use super::*;

use crate::audit::InMemoryGcAuditSink;
use crate::mark::InMemoryGcCandidatesStore;
use crate::metrics::InMemoryGcMetrics;
use crate::run::InMemoryGcRunStore;

fn digest(seed: u32) -> BlobDigest {
    let prefix = format!("{seed:08x}");
    let mut s = prefix;
    s.push_str(&"0".repeat(BlobDigest::LEN - 8));
    BlobDigest::parse(&s).expect("canonical hex")
}

fn r2_key_for(tenant_id: Uuid, digest: &BlobDigest) -> String {
    format!("cas/{tenant_id}/{digest}")
}

type Fixture = (
    InMemoryPhysicalDeletePhase<
        InMemoryGcRunStore,
        InMemoryGcCandidatesStore,
        InMemoryBlobMetaPurgeStore,
        InMemoryR2Delete,
        InMemoryGcAuditSink,
        InMemoryGcMetrics,
        CountingPhysicalDeleteClock,
    >,
    Arc<InMemoryGcRunStore>,
    Arc<InMemoryGcCandidatesStore>,
    Arc<InMemoryBlobMetaPurgeStore>,
    Arc<InMemoryR2Delete>,
    Arc<InMemoryGcAuditSink>,
);

fn fresh(start_ms: u64) -> Fixture {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta_purge = Arc::new(InMemoryBlobMetaPurgeStore::new());
    let r2 = Arc::new(InMemoryR2Delete::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingPhysicalDeleteClock::new(start_ms));
    let phase = InMemoryPhysicalDeletePhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta_purge),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    (phase, runs, candidates, blob_meta_purge, r2, audit)
}

fn seed_run_in_physical_delete(
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
    runs.transition_phase(rid, tenant, GcPhase::PhysicalDelete, mark_anchor + 20)
        .unwrap();
}

/// Seed a `Swept` candidate with the matching `blob_meta` purge row
/// (refcount=0, soft-deleted at `deleted_at_ms`) and the matching
/// R2 key. Mirrors the sweep→physical-delete handoff.
#[allow(
    clippy::too_many_arguments,
    reason = "test fixture: 10 fixture-state arguments collapsing to a single setup helper kept readable inline rather than via a builder type"
)]
fn seed_swept_candidate(
    candidates: &InMemoryGcCandidatesStore,
    blob_meta: &InMemoryBlobMetaPurgeStore,
    r2: &InMemoryR2Delete,
    tenant: Uuid,
    region: GcRegion,
    rid: RunId,
    d: BlobDigest,
    mark_anchor: u64,
    size_bytes: u64,
    deleted_at_ms: u64,
) {
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: d.clone(),
            mark_started_at_ms: mark_anchor,
            mark_run_id: rid,
            blob_size_bytes: size_bytes,
            blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
            status: CandidateStatus::Candidate,
            created_at_ms: mark_anchor,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    // Sweep would have flipped the candidate to Swept; we drive the
    // transition here for fixture clarity.
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            deleted_at_ms,
            None,
        )
        .unwrap();
    let key = r2_key_for(tenant, &d);
    r2.seed(tenant, region, key.clone());
    blob_meta.push_soft_deleted(tenant, d, key, size_bytes, deleted_at_ms);
}

#[test]
fn canonical_phase_budget_pinned() {
    // 30 min p99 @ 100k candidates per (tenant, region) hourly tick
    // — sprint contract §5.4 R-S06-9.1.
    assert_eq!(CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS, 30 * 60 * 1000);
}

#[test]
fn config_default_canonical() {
    let cfg = PhysicalDeleteConfig::default();
    assert_eq!(cfg.grace_cas_ms(), GRACE_CAS_MS);
    assert_eq!(cfg.grace_ac_ms(), GRACE_AC_MS);
    assert_eq!(
        cfg.phase_budget_ms(),
        CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS
    );
}

#[test]
fn config_rejects_inverted_grace() {
    let err = PhysicalDeleteConfig::new(GRACE_AC_MS, GRACE_CAS_MS, 1).unwrap_err();
    assert!(matches!(err, PhysicalDeleteError::Backend(_)));
}

#[test]
fn config_rejects_zero_budget() {
    assert!(PhysicalDeleteConfig::new(GRACE_CAS_MS, GRACE_AC_MS, 0).is_err());
}

#[test]
fn r2_delete_idempotent_returns_not_found_on_replay() {
    let r2 = InMemoryR2Delete::new();
    let tenant = Uuid::from_u128(1);
    let key = "cas/abc/d";
    r2.seed(tenant, GcRegion::Sam, key);
    assert_eq!(
        r2.delete(tenant, GcRegion::Sam, key).unwrap(),
        R2DeleteOutcome::Deleted
    );
    // Idempotent re-call: NotFound; orchestrator treats as success.
    assert_eq!(
        r2.delete(tenant, GcRegion::Sam, key).unwrap(),
        R2DeleteOutcome::NotFound
    );
    // Third re-call: still NotFound (idempotent retry-safe).
    assert_eq!(
        r2.delete(tenant, GcRegion::Sam, key).unwrap(),
        R2DeleteOutcome::NotFound
    );
}

#[test]
fn r2_delete_tenant_isolation() {
    // Tenant A and Tenant B both seed the same key under the same
    // region; deleting under Tenant A's scope MUST NOT remove
    // Tenant B's key.
    let r2 = InMemoryR2Delete::new();
    let ta = Uuid::from_u128(1);
    let tb = Uuid::from_u128(2);
    let key = "cas/x/y";
    r2.seed(ta, GcRegion::Sam, key);
    r2.seed(tb, GcRegion::Sam, key);
    assert_eq!(
        r2.delete(ta, GcRegion::Sam, key).unwrap(),
        R2DeleteOutcome::Deleted
    );
    // Tenant B's key still present.
    assert!(r2.contains(tb, GcRegion::Sam, key));
    // Tenant A's key absent.
    assert!(!r2.contains(ta, GcRegion::Sam, key));
}

#[test]
fn happy_path_post_grace_purge() {
    let mark_anchor = 1_000;
    let deleted_at_ms = mark_anchor + 100;
    // Now is well past grace.
    let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
    let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xabcd);
    let key = r2_key_for(tenant, &d);
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        GcRegion::Sam,
        rid,
        d.clone(),
        mark_anchor,
        4096,
        deleted_at_ms,
    );

    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.candidates_processed, 1);
    assert_eq!(result.blobs_deleted_count, 1);
    assert_eq!(result.blobs_skipped_grace_pending, 0);
    assert_eq!(result.blobs_skipped_refcount_non_zero, 0);
    assert_eq!(result.bytes_reclaimed, 4096);
    // R2 key removed.
    assert!(!r2.contains(tenant, GcRegion::Sam, &key));
    // blob_meta row removed.
    assert!(!blob_meta.contains(tenant, &d));
    // gc_candidate flipped to PhysicallyDeleted.
    let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(cand.status, CandidateStatus::PhysicallyDeleted);
    // Audit emitted.
    assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 1);
}

#[test]
fn skipped_grace_pending_strict_boundary() {
    // EXACTLY at boundary: now - deleted_at_ms == grace_period_ms
    // — strict `>` says SKIP (not yet expired).
    // CountingPhysicalDeleteClock advances +1 per call: phase_start
    // + now_for_probe + step_candidate's now = 3 ticks consumed
    // before the gate evaluates. Set start so that the
    // step_candidate tick lands EXACTLY at grace boundary
    // (= deleted_at_ms + GRACE_CAS_MS), where strict `>` skips.
    let mark_anchor = 1_000;
    let deleted_at_ms = mark_anchor + 100;
    let now_start = deleted_at_ms + GRACE_CAS_MS - 2; // step_candidate tick = boundary.
    let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xbb);
    let key = r2_key_for(tenant, &d);
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        GcRegion::Sam,
        rid,
        d.clone(),
        mark_anchor,
        512,
        deleted_at_ms,
    );

    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_deleted_count, 0);
    assert_eq!(result.blobs_skipped_grace_pending, 1);
    assert_eq!(result.bytes_reclaimed, 0);
    // R2 key still present.
    assert!(r2.contains(tenant, GcRegion::Sam, &key));
    // blob_meta row still present.
    assert!(blob_meta.contains(tenant, &d));
    // gc_candidate still Swept.
    let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(cand.status, CandidateStatus::Swept);
    // No audit emit on skipped path.
    assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
}

#[test]
fn skipped_refcount_non_zero_race_protection() {
    // Customer CAS write incremented refcount in the race window
    // between sweep and physical-delete; the conditional predicate
    // skips the row.
    let mark_anchor = 1_000;
    let deleted_at_ms = mark_anchor + 100;
    let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
    let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xabba);
    let key = r2_key_for(tenant, &d);
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        GcRegion::Sam,
        rid,
        d.clone(),
        mark_anchor,
        2048,
        deleted_at_ms,
    );
    // Race: customer CAS write bumped refcount to 1.
    assert!(blob_meta.set_refcount(tenant, &d, 1));

    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_deleted_count, 0);
    assert_eq!(result.blobs_skipped_refcount_non_zero, 1);
    // R2 NOT called (orchestrator skips before R2 step on refcount
    // pre-check).
    assert!(r2.contains(tenant, GcRegion::Sam, &key));
    // blob_meta row still present.
    assert!(blob_meta.contains(tenant, &d));
    // gc_candidate still Swept (no transition fired).
    let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(cand.status, CandidateStatus::Swept);
    // No audit emit.
    assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
}

#[test]
fn idempotent_re_run_no_double_purge() {
    let mark_anchor = 1_000;
    let deleted_at_ms = mark_anchor + 100;
    let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
    let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xcafe);
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        GcRegion::Sam,
        rid,
        d,
        mark_anchor,
        1024,
        deleted_at_ms,
    );

    let r1 = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    let r2_result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(r1.blobs_deleted_count, 1);
    assert_eq!(r2_result.blobs_deleted_count, 0);
    assert_eq!(r2_result.already_resolved_count, 1);
    // Audit emitted ONCE.
    assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 1);
}

#[test]
fn tenant_isolation_no_cross_purge() {
    let mark_anchor = 1_000;
    let deleted_at_ms = mark_anchor + 100;
    let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
    let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
    let ta = Uuid::from_u128(1);
    let tb = Uuid::from_u128(2);
    let ra = RunId(Uuid::from_u128(10));
    let rb = RunId(Uuid::from_u128(11));
    seed_run_in_physical_delete(&runs, ra, ta, GcRegion::Sam, mark_anchor);
    seed_run_in_physical_delete(&runs, rb, tb, GcRegion::Sam, mark_anchor);
    let d = digest(0xdead);
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        ta,
        GcRegion::Sam,
        ra,
        d.clone(),
        mark_anchor,
        128,
        deleted_at_ms,
    );
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tb,
        GcRegion::Sam,
        rb,
        d.clone(),
        mark_anchor,
        128,
        deleted_at_ms,
    );
    // Tenant B has a customer re-upload (refcount = 1) — tenant A
    // is unaffected.
    assert!(blob_meta.set_refcount(tb, &d, 1));

    let result_a = phase.execute(ra, ta, GcRegion::Sam).unwrap();
    let result_b = phase.execute(rb, tb, GcRegion::Sam).unwrap();
    assert_eq!(result_a.blobs_deleted_count, 1);
    assert_eq!(result_b.blobs_deleted_count, 0);
    assert_eq!(result_b.blobs_skipped_refcount_non_zero, 1);
    // Audit captures the per-tenant decisions.
    let purged = audit.snapshot_of(GcEventType::PhysicalDeleted);
    assert_eq!(purged.len(), 1);
    assert_eq!(purged[0].tenant_id, ta);
    // Tenant A's row removed; tenant B's row preserved.
    assert!(!blob_meta.contains(ta, &d));
    assert!(blob_meta.contains(tb, &d));
}

#[test]
fn region_mismatch_rejected() {
    let (phase, runs, _c, _b, _r, _a) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let mark_anchor = 500;
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let err = phase.execute(rid, tenant, GcRegion::Iad).unwrap_err();
    assert!(matches!(err, PhysicalDeleteError::RegionMismatch { .. }));
}

#[test]
fn run_not_found_rejected() {
    let (phase, _runs, _c, _b, _r, _a) = fresh(1_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(99));
    let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, PhysicalDeleteError::RunStore(_)));
}

#[test]
fn cross_tenant_run_returns_not_found() {
    // The Layer 4 envelope is tested at the run-store seam:
    // gc_run.lookup(rid, wrong_tenant) returns Ok(None) → NotFound.
    let (phase, runs, _c, _b, _r, _a) = fresh(1_000);
    let tenant_a = Uuid::from_u128(1);
    let tenant_b = Uuid::from_u128(2);
    let rid = RunId(Uuid::from_u128(99));
    seed_run_in_physical_delete(&runs, rid, tenant_a, GcRegion::Sam, 500);
    let err = phase.execute(rid, tenant_b, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, PhysicalDeleteError::RunStore(_)));
}

#[test]
fn already_resolved_for_physically_deleted_candidate() {
    // A candidate already in PhysicallyDeleted state is observed
    // as AlreadyResolved by step_candidate.
    let (phase, _runs, candidates, blob_meta, r2, _audit) = fresh(1_000_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let d = digest(0x4242);
    let mark_anchor = 500;
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        GcRegion::Sam,
        rid,
        d.clone(),
        mark_anchor,
        64,
        mark_anchor + 100,
    );
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Swept,
            CandidateStatus::PhysicallyDeleted,
            mark_anchor + 200,
            None,
        )
        .unwrap();
    let row = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    let dec = phase.step_candidate(&row, GcRegion::Sam).unwrap();
    match dec {
        PhysicalDeleteDecision::AlreadyResolved { observed_status } => {
            assert_eq!(observed_status, CandidateStatus::PhysicallyDeleted);
        }
        other => panic!("expected AlreadyResolved, got {other:?}"),
    }
}
