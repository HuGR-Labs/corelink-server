use super::*;

use crate::audit::{GcEventType, InMemoryGcAuditSink};
use crate::mark::{BlobDigest, CandidateStatus, GcCandidate, InMemoryGcCandidatesStore};
use crate::metrics::InMemoryGcMetrics;
use crate::physical_delete::{
    CountingPhysicalDeleteClock, InMemoryBlobMetaPurgeStore, InMemoryR2Delete, PurgeState,
};
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

type Deps = (
    Arc<InMemoryGcRunStore>,
    Arc<InMemoryGcCandidatesStore>,
    Arc<InMemoryBlobMetaPurgeStore>,
    Arc<InMemoryR2Delete>,
    Arc<InMemoryGcAuditSink>,
    Arc<InMemoryGcMetrics>,
    Arc<CountingPhysicalDeleteClock>,
    Arc<InMemoryGcSweepReportSink>,
);

fn fresh_deps(start_ms: u64) -> Deps {
    (
        Arc::new(InMemoryGcRunStore::new()),
        Arc::new(InMemoryGcCandidatesStore::new()),
        Arc::new(InMemoryBlobMetaPurgeStore::new()),
        Arc::new(InMemoryR2Delete::new()),
        Arc::new(InMemoryGcAuditSink::new()),
        Arc::new(InMemoryGcMetrics::new()),
        Arc::new(CountingPhysicalDeleteClock::new(start_ms)),
        Arc::new(InMemoryGcSweepReportSink::new()),
    )
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

/// Seed a `Swept` candidate with its matching `blob_meta` purge row
/// (refcount, soft-deleted at `deleted_at_ms`) + R2 key.
#[allow(
    clippy::too_many_arguments,
    reason = "test fixture: collapses sweep→physical-delete handoff setup into one call"
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
    refcount: u32,
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
    blob_meta.push_soft_deleted(tenant, d.clone(), key, size_bytes, deleted_at_ms);
    if refcount != 0 {
        assert!(blob_meta.set_refcount(tenant, &d, refcount));
    }
}

#[test]
fn mode_from_env_defaults_dry_run_and_fails_closed() {
    // Default + unset → DryRun.
    assert_eq!(GcSweepMode::default(), GcSweepMode::DryRun);
    assert_eq!(GcSweepMode::from_env_value(None), GcSweepMode::DryRun);
    // Only literal true / 1 enable live.
    assert_eq!(
        GcSweepMode::from_env_value(Some("true")),
        GcSweepMode::LiveDelete
    );
    assert_eq!(
        GcSweepMode::from_env_value(Some("TRUE")),
        GcSweepMode::LiveDelete
    );
    assert_eq!(
        GcSweepMode::from_env_value(Some(" 1 ")),
        GcSweepMode::LiveDelete
    );
    // Everything else fails closed to DryRun.
    for v in ["false", "0", "", "yes", "garbage", "truthy"] {
        assert_eq!(
            GcSweepMode::from_env_value(Some(v)),
            GcSweepMode::DryRun,
            "value {v:?} must fail closed to DryRun"
        );
    }
    assert!(!GcSweepMode::DryRun.is_live());
    assert!(GcSweepMode::LiveDelete.is_live());
}

#[test]
fn dry_run_with_zero_candidates_reports_zero_without_deleting() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let tenant = Uuid::from_u128(8);
    let region = GcRegion::Sam;
    let rid = RunId(Uuid::from_u128(0x5eee));
    seed_run_in_physical_delete(&runs, rid, tenant, region, 300);
    let cfg = PhysicalDeleteConfig::new(10, 10, 60_000).unwrap();
    let runner = GcSweepRunner::new(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&clock),
        Arc::clone(&report_sink),
        cfg,
        GcSweepMode::DryRun,
    );

    let report = runner.run(rid, tenant, region).unwrap();

    assert_eq!(report.candidates_scanned, 0);
    assert_eq!(report.reclaimable_count, 0);
    assert_eq!(report.reclaimable_bytes, 0);
    assert_eq!(report.deleted_count, 0);
    assert_eq!(report.deleted_bytes, 0);
    assert!(report.reclaimable_keys.is_empty());
    assert!(audit.snapshot_of(GcEventType::PhysicalDeleted).is_empty());
    assert_eq!(report_sink.len(), 1);
}

#[test]
fn dry_run_lists_candidates_and_deletes_nothing() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let tenant = Uuid::from_u128(7);
    let region = GcRegion::Sam;
    let rid = RunId(Uuid::from_u128(0x5eed));
    seed_run_in_physical_delete(&runs, rid, tenant, region, 300);
    let d = digest(1);
    // Soft-deleted long ago (post-grace), refcount 0 → reclaimable.
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        region,
        rid,
        d.clone(),
        300,
        4_096,
        0, // deleted at t=0; clock starts at 1_000 >> grace? no — grace huge
        0,
    );
    // Use a tiny grace so t=0 deleted_at is post-grace at clock ~1000.
    let cfg = PhysicalDeleteConfig::new(10, 10, 60_000).unwrap();
    let runner = GcSweepRunner::new(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&clock),
        Arc::clone(&report_sink),
        cfg,
        GcSweepMode::DryRun,
    );

    let key = r2_key_for(tenant, &d);
    let report = runner.run(rid, tenant, region).unwrap();

    // Classified reclaimable + listed.
    assert_eq!(report.mode, GcSweepMode::DryRun);
    assert_eq!(report.candidates_scanned, 1);
    assert_eq!(report.reclaimable_count, 1);
    assert_eq!(report.reclaimable_bytes, 4_096);
    assert_eq!(report.reclaimable_keys, vec![key.clone()]);
    // ZERO deletes — the load-bearing dry-run invariant.
    assert_eq!(report.deleted_count, 0);
    assert_eq!(report.deleted_bytes, 0);
    // R2 object still present (no DeleteObject call).
    assert!(r2.contains(tenant, region, &key));
    // D1 blob_meta row preserved (no purge).
    assert!(blob_meta.contains(tenant, &d));
    // Candidate row NOT transitioned (still Swept).
    let c = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(c.status, CandidateStatus::Swept);
    // No physical_deleted audit emitted in dry-run.
    assert!(audit.snapshot_of(GcEventType::PhysicalDeleted).is_empty());
    // Exactly one report event emitted.
    assert_eq!(report_sink.len(), 1);
}

#[test]
fn live_mode_deletes_only_reclaimable_and_never_a_live_object() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let tenant = Uuid::from_u128(9);
    let region = GcRegion::Iad;
    let rid = RunId(Uuid::from_u128(0xc0ffee));
    seed_run_in_physical_delete(&runs, rid, tenant, region, 300);

    let d_reclaim = digest(1); // post-grace, refcount 0 → DELETE
    let d_live = digest(2); // refcount > 0 → live blob, NEVER delete
    let d_grace = digest(3); // grace pending → NEVER delete this tick

    // Tiny grace so the "old" deletes are post-grace, but the grace
    // candidate (deleted "now") is still pending.
    let cfg = PhysicalDeleteConfig::new(50, 50, 600_000).unwrap();

    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        region,
        rid,
        d_reclaim.clone(),
        300,
        1_000,
        0,
        0,
    );
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        region,
        rid,
        d_live.clone(),
        300,
        2_000,
        0,
        3, // live re-reference: refcount 3
    );
    // grace-pending: soft-deleted at ~1_050 (clock will be ~1_05x).
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        region,
        rid,
        d_grace.clone(),
        300,
        3_000,
        1_050,
        0,
    );

    let runner = GcSweepRunner::new(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&clock),
        Arc::clone(&report_sink),
        cfg,
        GcSweepMode::LiveDelete,
    );

    let k_reclaim = r2_key_for(tenant, &d_reclaim);
    let k_live = r2_key_for(tenant, &d_live);
    let k_grace = r2_key_for(tenant, &d_grace);

    let report = runner.run(rid, tenant, region).unwrap();

    // Exactly one object deleted (the reclaimable one).
    assert_eq!(report.deleted_count, 1);
    assert_eq!(report.deleted_bytes, 1_000);
    assert_eq!(report.skipped_refcount_non_zero, 1);
    assert_eq!(report.skipped_grace_pending, 1);

    // The reclaimable object is gone from R2 + D1.
    assert!(!r2.contains(tenant, region, &k_reclaim));
    assert!(!blob_meta.contains(tenant, &d_reclaim));

    // The LIVE blob (refcount>0) is untouched in R2 + D1.
    assert!(r2.contains(tenant, region, &k_live));
    assert!(blob_meta.contains(tenant, &d_live));

    // The grace-pending blob is untouched in R2 + D1.
    assert!(r2.contains(tenant, region, &k_grace));
    assert!(blob_meta.contains(tenant, &d_grace));

    // One physical_deleted audit for the one purge.
    assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 1);
    assert_eq!(report_sink.len(), 1);
}

#[test]
fn fail_closed_on_source_error_deletes_nothing() {
    // A blob_meta store whose lookup errors mid-classification.
    #[derive(Debug)]
    struct FailingPurgeStore;
    impl BlobMetaPurgeStore for FailingPurgeStore {
        fn lookup_purge_state(
            &self,
            _tenant_id: Uuid,
            _digest: &BlobDigest,
        ) -> Result<Option<PurgeState>, PhysicalDeleteError> {
            Err(PhysicalDeleteError::Backend(
                "injected source error".to_owned(),
            ))
        }
        fn conditional_purge(
            &self,
            _tenant_id: Uuid,
            _digest: &BlobDigest,
            _now_ms: u64,
            _grace_period_ms: u64,
        ) -> Result<bool, PhysicalDeleteError> {
            Err(PhysicalDeleteError::Backend(
                "injected source error".to_owned(),
            ))
        }
    }

    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta = Arc::new(FailingPurgeStore);
    let r2 = Arc::new(InMemoryR2Delete::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingPhysicalDeleteClock::new(1_000));
    let report_sink = Arc::new(InMemoryGcSweepReportSink::new());

    let tenant = Uuid::from_u128(11);
    let region = GcRegion::Lhr;
    let rid = RunId(Uuid::from_u128(0xbad));
    seed_run_in_physical_delete(&runs, rid, tenant, region, 300);
    let d = digest(1);
    // Candidate is Swept so classification reaches the failing lookup.
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: d.clone(),
            mark_started_at_ms: 300,
            mark_run_id: rid,
            blob_size_bytes: 10,
            blob_last_referenced_at_ms: 299,
            status: CandidateStatus::Candidate,
            created_at_ms: 300,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            300,
            None,
        )
        .unwrap();
    let key = r2_key_for(tenant, &d);
    r2.seed(tenant, region, key.clone());

    let runner = GcSweepRunner::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&clock),
        Arc::clone(&report_sink),
        GcSweepMode::DryRun,
    );

    let err = runner.run(rid, tenant, region).unwrap_err();
    assert!(matches!(err, GcSweepError::PhysicalDelete(_)));
    // Fail-closed: nothing deleted, no report emitted.
    assert!(r2.contains(tenant, region, &key));
    assert!(report_sink.is_empty());
}

#[test]
fn unknown_run_fails_closed() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let runner = GcSweepRunner::with_defaults(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        Arc::clone(&report_sink),
        GcSweepMode::DryRun,
    );
    let err = runner
        .run(
            RunId(Uuid::from_u128(0xdead)),
            Uuid::from_u128(1),
            GcRegion::Syd,
        )
        .unwrap_err();
    assert!(matches!(err, GcSweepError::PhysicalDelete(_)));
    assert!(report_sink.is_empty());
}

#[test]
fn region_mismatch_fails_closed_before_candidate_scan() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let tenant = Uuid::from_u128(0x71);
    let rid = RunId(Uuid::from_u128(0x72));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, 300);
    let runner = GcSweepRunner::with_defaults(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        Arc::clone(&report_sink),
        GcSweepMode::DryRun,
    );

    let err = runner.run(rid, tenant, GcRegion::Iad).unwrap_err();
    assert!(matches!(
        err,
        GcSweepError::PhysicalDelete(PhysicalDeleteError::RegionMismatch { .. })
    ));
    assert!(report_sink.is_empty());
}

#[test]
fn dry_run_fails_closed_when_phase_budget_is_exceeded() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let tenant = Uuid::from_u128(0x73);
    let region = GcRegion::Sam;
    let rid = RunId(Uuid::from_u128(0x74));
    seed_run_in_physical_delete(&runs, rid, tenant, region, 300);
    let cfg = PhysicalDeleteConfig::new(10, 10, 1).unwrap();
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: digest(1),
            mark_started_at_ms: 300,
            mark_run_id: rid,
            blob_size_bytes: 1,
            blob_last_referenced_at_ms: 299,
            status: CandidateStatus::Candidate,
            created_at_ms: 300,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    let runner = GcSweepRunner::new(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        Arc::clone(&report_sink),
        cfg,
        GcSweepMode::DryRun,
    );

    let err = runner.run(rid, tenant, region).unwrap_err();
    assert!(matches!(
        err,
        GcSweepError::PhysicalDelete(PhysicalDeleteError::PhaseBudgetExceeded { .. })
    ));
    assert!(report_sink.is_empty());
}

/// Build a minimal but fully-populated [`SweepReport`] for sink tests.
fn sample_report() -> SweepReport {
    SweepReport {
        mode: GcSweepMode::DryRun,
        run_id: RunId(Uuid::from_u128(0xfeed)),
        tenant_id: Uuid::from_u128(42),
        region: GcRegion::Iad,
        candidates_scanned: 3,
        reclaimable_count: 2,
        reclaimable_bytes: 8_192,
        deleted_count: 0,
        deleted_bytes: 0,
        skipped_grace_pending: 1,
        skipped_refcount_non_zero: 0,
        already_resolved: 0,
        reclaimable_keys: vec![r2_key_for(Uuid::from_u128(42), &digest(1))],
        duration_ms: 5,
        now_ms: 1_005,
    }
}

#[test]
fn in_memory_sink_snapshot_and_is_empty_round_trip() {
    let sink = InMemoryGcSweepReportSink::new();

    // Fresh sink: empty, zero-length, empty snapshot.
    // (kills `is_empty -> true` only if it also flips to false later,
    //  and kills `snapshot -> vec![]` via the populated round-trip.)
    assert!(sink.is_empty());
    assert_eq!(sink.len(), 0);
    assert!(sink.snapshot().is_empty());

    let report = sample_report();
    sink.emit_report(&report).unwrap();

    // After one emit: NOT empty (kills `is_empty -> true`), len 1.
    assert!(!sink.is_empty());
    assert_eq!(sink.len(), 1);

    // snapshot returns the real captured report, not vec![]
    // (kills `snapshot -> Vec::new()`).
    let snap = sink.snapshot();
    assert_eq!(snap.len(), 1);
    let got = &snap[0];
    assert_eq!(got.mode, report.mode);
    assert_eq!(got.run_id, report.run_id);
    assert_eq!(got.tenant_id, report.tenant_id);
    assert_eq!(got.region, report.region);
    assert_eq!(got.candidates_scanned, report.candidates_scanned);
    assert_eq!(got.reclaimable_count, report.reclaimable_count);
    assert_eq!(got.reclaimable_bytes, report.reclaimable_bytes);
    assert_eq!(got.skipped_grace_pending, report.skipped_grace_pending);
    assert_eq!(got.reclaimable_keys, report.reclaimable_keys);

    // A second emit accumulates (snapshot reflects every report).
    sink.emit_report(&report).unwrap();
    assert_eq!(sink.len(), 2);
    assert_eq!(sink.snapshot().len(), 2);
}

#[test]
fn gc_sweep_runner_debug_renders_struct_name_and_mode() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let runner = GcSweepRunner::with_defaults(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        report_sink,
        GcSweepMode::DryRun,
    );

    // The real Debug impl is `debug_struct("GcSweepRunner").field("mode", ..)`;
    // the mutant returns `Ok(())` → an empty string → these fail.
    let rendered = format!("{runner:?}");
    assert!(
        rendered.contains("GcSweepRunner"),
        "debug output missing struct name: {rendered:?}"
    );
    assert!(
        rendered.contains("mode"),
        "debug output missing mode field: {rendered:?}"
    );
}
