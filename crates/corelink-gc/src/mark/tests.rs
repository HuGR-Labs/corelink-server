use super::*;

use std::sync::Arc;

use crate::audit::InMemoryGcAuditSink;
use crate::metrics::InMemoryGcMetrics;
use crate::run::InMemoryGcRunStore;

fn digest(seed: u8) -> BlobDigest {
    // 64-char lower-case hex; varies the first byte so each seed is
    // distinct.
    let mut s = format!("{seed:02x}");
    s.push_str(&"0".repeat(BlobDigest::LEN - 2));
    BlobDigest::parse(&s).unwrap()
}

type Fixture = (
    InMemoryMarkPhase<
        InMemoryGcRunStore,
        InMemoryReachableSetSource,
        InMemoryGcCandidatesStore,
        InMemoryGcAuditSink,
        InMemoryGcMetrics,
        CountingMarkClock,
    >,
    Arc<InMemoryGcRunStore>,
    Arc<InMemoryReachableSetSource>,
    Arc<InMemoryGcCandidatesStore>,
    Arc<InMemoryGcAuditSink>,
);

fn fresh(start_ms: u64) -> Fixture {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let source = Arc::new(InMemoryReachableSetSource::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingMarkClock::new(start_ms));
    let mark = InMemoryMarkPhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&source),
        Arc::clone(&candidates),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    (mark, runs, source, candidates, audit)
}

fn seed_running(runs: &InMemoryGcRunStore, rid: RunId, tenant: Uuid, region: GcRegion) {
    runs.insert_pending(rid, tenant, region, 100, "cron".into())
        .unwrap();
    runs.acquire_running(rid, tenant, 200).unwrap();
}

#[test]
fn blob_digest_parse_canonical() {
    let raw = "0".repeat(BlobDigest::LEN);
    let d = BlobDigest::parse(&raw).unwrap();
    assert_eq!(d.as_str(), &raw);
    assert_eq!(format!("{d}"), raw);
}

#[test]
fn blob_digest_rejects_wrong_length() {
    let raw = "0".repeat(63);
    let err = BlobDigest::parse(&raw).unwrap_err();
    assert!(matches!(err, MarkError::InvalidDigest { actual_len: 63 }));
}

#[test]
fn blob_digest_rejects_uppercase() {
    let mut raw = "0".repeat(BlobDigest::LEN - 1);
    raw.insert(0, 'A');
    let err = BlobDigest::parse(&raw).unwrap_err();
    assert!(matches!(err, MarkError::InvalidDigest { .. }));
}

#[test]
fn blob_digest_rejects_non_hex() {
    let mut raw = "0".repeat(BlobDigest::LEN - 1);
    raw.push('z');
    assert!(BlobDigest::parse(&raw).is_err());
}

#[test]
fn mark_config_default_canonical() {
    let cfg = MarkConfig::default();
    assert_eq!(cfg.batch_size(), CANONICAL_BATCH_SIZE);
    assert_eq!(cfg.jitter_ms(), CANONICAL_JITTER_MS);
    assert_eq!(cfg.phase_budget_ms(), CANONICAL_PHASE_BUDGET_MS);
}

#[test]
fn mark_config_rejects_zero_batch() {
    assert!(MarkConfig::new(0, 100, 60_000).is_err());
}

#[test]
fn mark_config_rejects_oversize_batch() {
    assert!(MarkConfig::new(CANONICAL_BATCH_SIZE + 1, 100, 60_000).is_err());
}

#[test]
fn mark_config_rejects_zero_budget() {
    assert!(MarkConfig::new(100, 100, 0).is_err());
}

#[test]
#[allow(
    clippy::const_is_empty,
    reason = "regression guard: catches an accidental clear of \
              MIGRATION_0007_GC_CANDIDATES even when the const is, by \
              construction, the embedded SQL bytes."
)]
fn migration_const_is_non_empty_and_versioned() {
    assert!(!MIGRATION_0007_GC_CANDIDATES.is_empty());
    assert!(MIGRATION_0007_GC_CANDIDATES.contains("migration 0007"));
    assert!(MIGRATION_0007_GC_CANDIDATES.contains("CREATE TABLE IF NOT EXISTS gc_candidates"));
}

#[test]
fn happy_path_zero_blobs_zero_candidates() {
    let (mark, runs, _src, candidates, audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.rows_scanned_count, 0);
    assert_eq!(result.reachable_blobs_count, 0);
    assert_eq!(result.candidates_count, 0);
    // Mark anchor captured.
    let row = runs.lookup(rid, tenant).unwrap().unwrap();
    assert!(row.mark_started_at_ms.is_some());
    assert_eq!(result.mark_started_at_ms, row.mark_started_at_ms.unwrap());
    // No candidate rows for an empty corpus.
    assert_eq!(candidates.snapshot().len(), 0);
    // Audit emit: phase_started + phase_completed.
    let evts = audit.snapshot_of(GcEventType::PhaseTransitioned);
    assert_eq!(evts.len(), 2);
    assert_eq!(evts[0].reason, "mark_phase_started");
    assert_eq!(evts[1].reason, "mark_phase_completed");
}

#[test]
fn live_blob_via_blob_meta_refcount_is_reachable() {
    let (mark, runs, source, candidates, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    // 1 live blob (refcount = 1); reachable.
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(1),
            refcount: 1,
            size_bytes: 1024,
            last_referenced_at_ms: 500,
        },
    );
    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.rows_scanned_count, 1);
    assert_eq!(result.reachable_blobs_count, 1);
    assert_eq!(result.candidates_count, 0);
    assert_eq!(candidates.snapshot().len(), 0);
}

#[test]
fn orphan_blob_becomes_candidate() {
    let (mark, runs, source, candidates, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    // 1 orphan blob (refcount = 0).
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(2),
            refcount: 0,
            size_bytes: 2048,
            last_referenced_at_ms: 500,
        },
    );
    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.reachable_blobs_count, 0);
    assert_eq!(result.candidates_count, 1);
    let snap = candidates.snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].digest, digest(2));
    assert_eq!(snap[0].status, CandidateStatus::Candidate);
    assert_eq!(snap[0].blob_size_bytes, 2048);
    assert_eq!(snap[0].mark_run_id, rid);
    assert_eq!(snap[0].mark_started_at_ms, result.mark_started_at_ms);
}

#[test]
fn ac_meta_outputs_protect_blob_with_zero_refcount() {
    // Blob B with refcount=0 in blob_meta but referenced in ac_meta
    // outputs MUST be reachable (defense-in-depth: 3-pass union).
    let (mark, runs, source, candidates, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(3),
            refcount: 0,
            size_bytes: 1024,
            last_referenced_at_ms: 500,
        },
    );
    source.push_ac_meta(
        tenant,
        AcMetaRow {
            blob_refs: vec![digest(3)],
            created_at_ms: 600,
        },
    );
    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.reachable_blobs_count, 1);
    assert_eq!(result.candidates_count, 0);
    assert_eq!(candidates.snapshot().len(), 0);
}

#[test]
fn manifest_chunks_protect_blob_with_zero_refcount() {
    let (mark, runs, source, candidates, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(4),
            refcount: 0,
            size_bytes: 1024,
            last_referenced_at_ms: 500,
        },
    );
    source.push_manifest_chunk(
        tenant,
        ManifestChunkRow {
            chunk_digest: digest(4),
            created_at_ms: 700,
        },
    );
    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.reachable_blobs_count, 1);
    assert_eq!(result.candidates_count, 0);
    assert_eq!(candidates.snapshot().len(), 0);
}

#[test]
fn idempotent_re_run_preserves_mark_anchor() {
    let (mark, runs, source, candidates, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(5),
            refcount: 0,
            size_bytes: 256,
            last_referenced_at_ms: 500,
        },
    );
    let r1 = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    // Re-run: anchor preserved.
    let r2 = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(r1.mark_started_at_ms, r2.mark_started_at_ms);
    // Idempotent insert: still 1 candidate (composite PK conflict
    // is no-op).
    assert_eq!(candidates.snapshot().len(), 1);
}

#[test]
fn cross_tenant_isolation() {
    let (mark, runs, source, candidates, _audit) = fresh(1_000);
    let rid_a = RunId(uuid::Uuid::from_u128(1));
    let rid_b = RunId(uuid::Uuid::from_u128(2));
    let ta = uuid::Uuid::from_u128(10);
    let tb = uuid::Uuid::from_u128(20);
    seed_running(&runs, rid_a, ta, GcRegion::Sam);
    seed_running(&runs, rid_b, tb, GcRegion::Sam);
    // Tenant A: 1 orphan.
    source.push_blob_meta(
        ta,
        BlobMetaRow {
            digest: digest(6),
            refcount: 0,
            size_bytes: 1,
            last_referenced_at_ms: 500,
        },
    );
    // Tenant B: 1 reachable.
    source.push_blob_meta(
        tb,
        BlobMetaRow {
            digest: digest(7),
            refcount: 1,
            size_bytes: 1,
            last_referenced_at_ms: 500,
        },
    );
    let ra = mark.execute(rid_a, ta, GcRegion::Sam).unwrap();
    let rb = mark.execute(rid_b, tb, GcRegion::Sam).unwrap();
    assert_eq!(ra.candidates_count, 1);
    assert_eq!(rb.candidates_count, 0);
    // A's candidate is per-tenant; B sees none.
    let a_snap = candidates.snapshot_for_run(ta, rid_a).unwrap();
    let b_snap = candidates.snapshot_for_run(tb, rid_b).unwrap();
    assert_eq!(a_snap.len(), 1);
    assert_eq!(b_snap.len(), 0);
    // Cross-tenant snapshot returns empty.
    let cross = candidates.snapshot_for_run(tb, rid_a).unwrap();
    assert!(cross.is_empty());
}

#[test]
fn region_mismatch_rejected() {
    let (mark, runs, _src, _cand, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    // Caller passes Iad — mismatch.
    let err = mark.execute(rid, tenant, GcRegion::Iad).unwrap_err();
    assert!(matches!(err, MarkError::RunStore(_)));
}

#[test]
fn run_not_found_rejected() {
    let (mark, _runs, _src, _cand, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(99));
    let tenant = uuid::Uuid::from_u128(42);
    let err = mark.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(
        err,
        MarkError::RunStore(GcRunStoreError::NotFound(_))
    ));
}

#[test]
fn cross_tenant_lookup_rejected() {
    let (mark, runs, _src, _cand, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let owner = uuid::Uuid::from_u128(42);
    let attacker = uuid::Uuid::from_u128(43);
    seed_running(&runs, rid, owner, GcRegion::Sam);
    // Attacker tenant lookup yields NotFound (lookup returns
    // Ok(None) on cross-tenant per Layer 4 envelope; mark surfaces
    // it as NotFound so the trace is unambiguous).
    let err = mark.execute(rid, attacker, GcRegion::Sam).unwrap_err();
    assert!(matches!(
        err,
        MarkError::RunStore(GcRunStoreError::NotFound(_))
    ));
}

#[test]
fn batched_scan_visits_every_row() {
    // Batch size 3 + 7 rows → 3 batches (3+3+1).
    let (mark, runs, source, candidates, _audit) = {
        let runs = Arc::new(InMemoryGcRunStore::new());
        let source = Arc::new(InMemoryReachableSetSource::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingMarkClock::new(1_000));
        let cfg = MarkConfig::new(3, 1, 60_000).unwrap();
        let mark = InMemoryMarkPhase::new(
            Arc::clone(&runs),
            Arc::clone(&source),
            Arc::clone(&candidates),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
            cfg,
            0,
        );
        (mark, runs, source, candidates, audit)
    };
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    for i in 0..7u8 {
        source.push_blob_meta(
            tenant,
            BlobMetaRow {
                digest: digest(i + 10),
                refcount: 0,
                size_bytes: u64::from(i),
                last_referenced_at_ms: 500,
            },
        );
    }
    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.rows_scanned_count, 7); // pass1=7 + pass2=0 + pass3=0
                                              // Pass1 batches: 3 (3+3+1 → stop on partial).
                                              // Pass2 batches: 1 (empty → stop immediately).
                                              // Pass3 batches: 1 (empty → stop immediately).
    assert_eq!(result.batches_processed, 5);
    assert_eq!(result.candidates_count, 7);
    assert_eq!(candidates.snapshot().len(), 7);
}

#[test]
fn phase_budget_exceeded_surfaces_error() {
    // Tiny budget + jitter so the second batch trips the deadline.
    let runs = Arc::new(InMemoryGcRunStore::new());
    let source = Arc::new(InMemoryReachableSetSource::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingMarkClock::new(1_000));
    let cfg = MarkConfig::new(1, 100_000, 50).unwrap();
    let mark = InMemoryMarkPhase::new(
        Arc::clone(&runs),
        Arc::clone(&source),
        Arc::clone(&candidates),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
        cfg,
        0,
    );
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    for i in 0..5u8 {
        source.push_blob_meta(
            tenant,
            BlobMetaRow {
                digest: digest(i + 30),
                refcount: 1,
                size_bytes: 1,
                last_referenced_at_ms: 500,
            },
        );
    }
    let err = mark.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, MarkError::PhaseBudgetExceeded { .. }));
}

#[test]
fn no_mark_after_tombstone_orphans_only() {
    // Property: if blob_meta has 0 rows but ac_meta references a
    // digest, mark MUST NOT emit that digest as a candidate (no
    // physical row → cannot delete).
    let (mark, runs, source, candidates, _audit) = fresh(1_000);
    let rid = RunId(uuid::Uuid::from_u128(1));
    let tenant = uuid::Uuid::from_u128(42);
    seed_running(&runs, rid, tenant, GcRegion::Sam);
    source.push_ac_meta(
        tenant,
        AcMetaRow {
            blob_refs: vec![digest(50)],
            created_at_ms: 600,
        },
    );
    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    // ac_meta surfaced 1 reachable digest; blob_meta empty so no
    // candidate. Sweep cannot delete what mark never observed.
    assert_eq!(result.reachable_blobs_count, 1);
    assert_eq!(result.candidates_count, 0);
    assert_eq!(candidates.snapshot().len(), 0);
}

#[test]
fn candidate_status_canonical_strings() {
    assert_eq!(CandidateStatus::Candidate.as_str(), "candidate");
    assert_eq!(CandidateStatus::Swept.as_str(), "swept");
    assert_eq!(
        CandidateStatus::PhysicallyDeleted.as_str(),
        "physically_deleted"
    );
    assert_eq!(CandidateStatus::ProtectedReRef.as_str(), "protected_re_ref");
}
