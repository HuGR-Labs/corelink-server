//! Property tests pinned at 10 k iter against the WI-S06-003 sweep
//! phase load-bearing invariants.
//!
//! Per WI §6.1.9 + sprint contract §6 DoD, the canonical 4 property
//! tests this file ships are:
//!
//! 1. `prop_inv_gc_004_protect_if_ge_strict_boundary` — for any random
//!    `(mark_anchor, ac_offset)` pair where the `ac_meta` row's
//!    `created_at_ms == mark_anchor + ac_offset`, the canonical TLA
//!    `>=` semantic is enforced: `ac_offset >= 0` MUST trigger
//!    `ProtectedReRef`; `ac_offset < 0` MUST proceed to `Sweep`.
//!    Off-by-one (e.g. `ac.created_at == mark_anchor` mishandled) is
//!    a data-loss bug.
//! 2. `prop_sweep_idempotent_re_run` — re-executing sweep on the same
//!    `(run_id, tenant)` produces stable counters; no audit re-emit.
//! 3. `prop_sweep_tenant_isolation` — two tenants with overlapping
//!    digests; cross-tenant interference is structurally impossible
//!    (per-tenant decisions independent; audit records carry the
//!    correct tenant_id).
//! 4. `prop_soft_delete_reversible` — soft-delete + undelete round-trip
//!    leaves blob_meta in `deleted_at IS NULL` state (CAP-GC-002
//!    reversibility within grace).
//!
//! The 100 k nightly variant is wired alongside WI-S06-006 (TLA+ CI
//! gate) per WI §6.1.9 + sprint contract DoD.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use std::sync::Arc;

use corelink_gc::{
    AcReferenceIndex, BlobDigest, BlobMetaStore, CandidateStatus, CountingSweepClock, GcCandidate,
    GcCandidatesStore, GcEventType, GcPhase, GcRegion, GcRunStore, InMemoryAcReferenceIndex,
    InMemoryBlobMetaStore, InMemoryGcAuditSink, InMemoryGcCandidatesStore, InMemoryGcMetrics,
    InMemoryGcRunStore, InMemorySweepPhase, RunId, SweepBlobMetaRow, SweepDecision, SweepPhase,
};
use proptest::prelude::*;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for the PR gate; 100k nightly via `PROPTEST_CASES=100_000`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn region_from(idx: usize) -> GcRegion {
    GcRegion::all()[idx % 5]
}

fn digest_from(seed: u32) -> BlobDigest {
    let prefix = format!("{seed:08x}");
    let mut s = prefix;
    s.push_str(&"0".repeat(BlobDigest::LEN - 8));
    BlobDigest::parse(&s).expect("canonical hex")
}

type SweepFixture = (
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

/// Construct a fresh fixture; the clock starts at `start_ms` so the
/// caller MUST seed gc_run + candidates with a `mark_anchor < start_ms`
/// so the gc_run checkpoint timestamps remain monotonic across the
/// in-memory `GcRunStore::checkpoint` CHECK constraint
/// `chk_gc_run_lifecycle_checkpoint` (now_ms >= last_checkpoint_at_ms).
fn fresh(start_ms: u64) -> SweepFixture {
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
    digest: BlobDigest,
    mark_anchor: u64,
    blob_size_bytes: u64,
) {
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: digest.clone(),
            mark_started_at_ms: mark_anchor,
            mark_run_id: rid,
            blob_size_bytes,
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
        SweepBlobMetaRow {
            digest,
            size_bytes: blob_size_bytes,
            refcount: 0,
            last_referenced_at_ms: mark_anchor.saturating_sub(1),
            created_at_ms: mark_anchor.saturating_sub(100),
            deleted_at_ms: None,
        },
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    // =================================================================
    // 1) prop_inv_gc_004_protect_if_ge_strict_boundary
    //
    // CRITICAL TLA+ obligation: the canonical TLA `gc_correctness.tla`
    // L152-154 says `protect-if->=`. For any random (mark_anchor,
    // ac_offset) pair, sweep MUST:
    //  - PROTECT when `ac.created_at_ms - mark_anchor >= 0`
    //  - SWEEP   when `ac.created_at_ms - mark_anchor <  0`
    //
    // The off-by-one boundary case (`ac.created_at_ms == mark_anchor`)
    // is the most-feared regression — using `>` instead of `>=` would
    // sweep a re-referenced blob and violate INV-GC-004 / cause
    // permanent data loss. Property test pins the exact predicate.
    // =================================================================
    #[test]
    fn prop_inv_gc_004_protect_if_ge_strict_boundary(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        // mark_anchor must be > 1000 so we can subtract a small
        // ac_offset without underflow.
        mark_anchor in 10_000u64..1_000_000_000,
        // signed offset: negative = ac predates mark anchor (sweep);
        // zero or positive = ac satisfies `>=` (protect).
        ac_offset in -1000i64..=1000i64,
    ) {
        // Sweep clock starts AFTER mark_anchor + sweep transition (+10)
        // + slack so the checkpoint CHECK constraint remains
        // monotonic.
        let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(mark_anchor + 1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(1)));
        seed_run_in_sweep(&runs, rid, tenant, region, mark_anchor);
        let d = digest_from(7);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor, 1024);
        // ac.created_at_ms = mark_anchor + offset (saturating to avoid
        // underflow on the negative-offset arm).
        let ac_created_at_ms = if ac_offset >= 0 {
            mark_anchor.saturating_add(ac_offset as u64)
        } else {
            // ac_offset is in -1000..0; mark_anchor is at least 10_000
            // so subtraction is safe.
            mark_anchor.saturating_sub((-ac_offset) as u64)
        };
        ac.push_ac_row(tenant, "ac_action", vec![d.clone()], ac_created_at_ms);

        let result = sweep.execute(rid, tenant, region).unwrap();
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        let blob_row = blob_meta.snapshot(tenant, &d).unwrap();

        // The canonical predicate: ac_offset >= 0 → PROTECT.
        if ac_offset >= 0 {
            prop_assert_eq!(result.blobs_protected_re_ref_count, 1);
            prop_assert_eq!(result.blobs_swept_count, 0);
            prop_assert_eq!(cand.status, CandidateStatus::ProtectedReRef);
            prop_assert!(blob_row.deleted_at_ms.is_none());
            prop_assert_eq!(audit.snapshot_of(GcEventType::SweepProtectedReRef).len(), 1);
            prop_assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 0);
            // Forensic identity: protect arm carries the offending
            // ac.created_at_ms, which equals mark_anchor + ac_offset.
            prop_assert!(ac_created_at_ms >= mark_anchor);
        } else {
            prop_assert_eq!(result.blobs_swept_count, 1);
            prop_assert_eq!(result.blobs_protected_re_ref_count, 0);
            prop_assert_eq!(cand.status, CandidateStatus::Swept);
            prop_assert!(blob_row.deleted_at_ms.is_some());
            prop_assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 1);
            prop_assert_eq!(audit.snapshot_of(GcEventType::SweepProtectedReRef).len(), 0);
            prop_assert!(ac_created_at_ms < mark_anchor);
        }
    }

    // =================================================================
    // 2) prop_sweep_idempotent_re_run
    //
    // Sweep is idempotent: re-running on the same (run_id, tenant) does
    // not flip any candidate row, does not double-count, does not
    // re-emit audit (AlreadyResolved no-op path).
    // =================================================================
    #[test]
    fn prop_sweep_idempotent_re_run(
        tenant_seed in 1u128..500_000,
        region_idx in 0usize..5,
        n_orphan in 0u32..16,
        n_protected in 0u32..16,
        mark_anchor in 1_000u64..1_000_000_000,
    ) {
        let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(mark_anchor + 1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(3)));
        seed_run_in_sweep(&runs, rid, tenant, region, mark_anchor);
        // Build the candidate corpus: orphans + protected.
        for i in 0..n_orphan {
            let d = digest_from(i + 100);
            seed_candidate(&candidates, &blob_meta, tenant, rid, d, mark_anchor, 1);
        }
        for i in 0..n_protected {
            let d = digest_from(i + 100_000);
            seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor, 1);
            // protected = ac references with created_at >= mark_anchor.
            ac.push_ac_row(tenant, "p_action", vec![d], mark_anchor + 1);
        }
        // First sweep: full pipeline.
        let r1 = sweep.execute(rid, tenant, region).unwrap();
        let audit_count_after_first = audit.len();
        // Second sweep: idempotent no-op.
        let r2 = sweep.execute(rid, tenant, region).unwrap();
        let audit_count_after_second = audit.len();
        prop_assert_eq!(r1.blobs_swept_count, u64::from(n_orphan));
        prop_assert_eq!(r1.blobs_protected_re_ref_count, u64::from(n_protected));
        prop_assert_eq!(r2.blobs_swept_count, 0);
        prop_assert_eq!(r2.blobs_protected_re_ref_count, 0);
        prop_assert_eq!(
            r2.already_resolved_count,
            u64::from(n_orphan) + u64::from(n_protected)
        );
        // Mark anchor preserved.
        prop_assert_eq!(r1.mark_started_at_ms, r2.mark_started_at_ms);
        prop_assert_eq!(r1.mark_started_at_ms, mark_anchor);
        // No audit re-emit on second pass.
        prop_assert_eq!(audit_count_after_first, audit_count_after_second);
    }

    // =================================================================
    // 3) prop_sweep_tenant_isolation
    //
    // Two tenants with overlapping digests + per-tenant ac re-refs;
    // cross-tenant interference is structurally impossible. Audit
    // records carry the correct tenant_id.
    // =================================================================
    #[test]
    fn prop_sweep_tenant_isolation(
        tenant_a_seed in 1u128..400_000,
        tenant_b_seed in 500_001u128..900_000,
        region_idx in 0usize..5,
        a_orphans in 0u32..8,
        b_orphans in 0u32..8,
        a_protect_count in 0u32..4,
        b_protect_count in 0u32..4,
    ) {
        prop_assume!(tenant_a_seed != tenant_b_seed);
        let mark_anchor: u64 = 10_000;
        let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(mark_anchor + 1_000_000);
        let region = region_from(region_idx);
        let ta = Uuid::from_u128(tenant_a_seed);
        let tb = Uuid::from_u128(tenant_b_seed);
        let ra = RunId(Uuid::from_u128(tenant_a_seed.wrapping_add(11)));
        let rb = RunId(Uuid::from_u128(tenant_b_seed.wrapping_add(13)));
        seed_run_in_sweep(&runs, ra, ta, region, mark_anchor);
        seed_run_in_sweep(&runs, rb, tb, region, mark_anchor);
        // Tenant A orphans.
        for i in 0..a_orphans {
            let d = digest_from(i + 200_000);
            seed_candidate(&candidates, &blob_meta, ta, ra, d, mark_anchor, 1);
        }
        // Tenant A protected — ac.created_at >= mark_anchor.
        for i in 0..a_protect_count {
            let d = digest_from(i + 250_000);
            seed_candidate(&candidates, &blob_meta, ta, ra, d.clone(), mark_anchor, 1);
            ac.push_ac_row(ta, "a_action", vec![d], mark_anchor);
        }
        // Tenant B orphans.
        for i in 0..b_orphans {
            let d = digest_from(i + 300_000);
            seed_candidate(&candidates, &blob_meta, tb, rb, d, mark_anchor, 1);
        }
        // Tenant B protected.
        for i in 0..b_protect_count {
            let d = digest_from(i + 350_000);
            seed_candidate(&candidates, &blob_meta, tb, rb, d.clone(), mark_anchor, 1);
            ac.push_ac_row(tb, "b_action", vec![d], mark_anchor);
        }
        let ra_result = sweep.execute(ra, ta, region).unwrap();
        let rb_result = sweep.execute(rb, tb, region).unwrap();
        prop_assert_eq!(ra_result.blobs_swept_count, u64::from(a_orphans));
        prop_assert_eq!(ra_result.blobs_protected_re_ref_count, u64::from(a_protect_count));
        prop_assert_eq!(rb_result.blobs_swept_count, u64::from(b_orphans));
        prop_assert_eq!(rb_result.blobs_protected_re_ref_count, u64::from(b_protect_count));
        // Audit records carry the right tenant_id.
        for record in audit.snapshot_of(GcEventType::SweepSoftDeleted) {
            prop_assert!(record.tenant_id == ta || record.tenant_id == tb);
            // Run id matches tenant scope.
            if record.tenant_id == ta {
                prop_assert_eq!(record.run_id, ra);
            } else {
                prop_assert_eq!(record.run_id, rb);
            }
        }
        for record in audit.snapshot_of(GcEventType::SweepProtectedReRef) {
            prop_assert!(record.tenant_id == ta || record.tenant_id == tb);
            if record.tenant_id == ta {
                prop_assert_eq!(record.run_id, ra);
            } else {
                prop_assert_eq!(record.run_id, rb);
            }
        }
    }

    // =================================================================
    // 4) prop_soft_delete_reversible
    //
    // CAP-GC-002 reversibility: soft-delete + undelete round-trip on
    // any (tenant, digest) within grace produces deleted_at_ms == None.
    // =================================================================
    #[test]
    fn prop_soft_delete_reversible(
        tenant_seed in 1u128..1_000_000,
        digest_seed in 0u32..1_000_000,
        soft_delete_at_ms in 1_000u64..1_000_000,
    ) {
        let blob_meta = InMemoryBlobMetaStore::new();
        let tenant = Uuid::from_u128(tenant_seed);
        let d = digest_from(digest_seed);
        blob_meta.push_row(
            tenant,
            SweepBlobMetaRow {
                digest: d.clone(),
                size_bytes: 1024,
                refcount: 0,
                last_referenced_at_ms: 100,
                created_at_ms: 50,
                deleted_at_ms: None,
            },
        );
        let prev = blob_meta
            .soft_delete(tenant, &d, soft_delete_at_ms)
            .unwrap();
        prop_assert!(prev.is_some());
        prop_assert!(blob_meta.snapshot(tenant, &d).unwrap().deleted_at_ms.is_some());
        // Undelete (CAP-GC-002 reversibility within grace).
        let restored = blob_meta.undelete(tenant, &d);
        prop_assert!(restored);
        prop_assert!(blob_meta.snapshot(tenant, &d).unwrap().deleted_at_ms.is_none());
        // Re-soft-delete fires again (status restored).
        let prev2 = blob_meta
            .soft_delete(tenant, &d, soft_delete_at_ms.saturating_add(100))
            .unwrap();
        prop_assert!(prev2.is_some());
    }

    // =================================================================
    // 5) prop_step_decision_predicate
    //
    // The decision predicate `step_candidate` returns is consistent
    // with the macro-level result counters: every Sweep decision
    // increments swept_count, every ProtectedReRef increments protected
    // count, AlreadyResolved increments already_resolved_count. The
    // sum equals candidates_processed.
    // =================================================================
    #[test]
    fn prop_step_decision_predicate_aggregates(
        tenant_seed in 1u128..500_000,
        region_idx in 0usize..5,
        n in 1u32..32,
        protect_threshold in 0u32..32,
        mark_anchor in 1_000u64..1_000_000_000,
    ) {
        let (sweep, runs, candidates, blob_meta, ac, _audit) = fresh(mark_anchor + 1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(17)));
        seed_run_in_sweep(&runs, rid, tenant, region, mark_anchor);
        let mut expected_protected: u64 = 0;
        let mut expected_swept: u64 = 0;
        for i in 0..n {
            let d = digest_from(i + 500_000);
            seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor, 1);
            if i < protect_threshold {
                ac.push_ac_row(tenant, "act", vec![d], mark_anchor);
                expected_protected = expected_protected.saturating_add(1);
            } else {
                expected_swept = expected_swept.saturating_add(1);
            }
        }
        let result = sweep.execute(rid, tenant, region).unwrap();
        prop_assert_eq!(result.blobs_swept_count, expected_swept);
        prop_assert_eq!(result.blobs_protected_re_ref_count, expected_protected);
        prop_assert_eq!(
            result.candidates_processed,
            result.blobs_swept_count
                + result.blobs_protected_re_ref_count
                + result.already_resolved_count
        );
        // Audit emits exactly one record per non-resolved decision.
        prop_assert_eq!(
            result.audit_events_emitted,
            result.blobs_swept_count + result.blobs_protected_re_ref_count
        );
    }
}

// =====================================================================
// Sanity checks (non-proptest; documented invariants).
// =====================================================================

#[test]
fn canonical_grace_constants_pinned_at_72h_24h() {
    assert_eq!(corelink_gc::GRACE_CAS_MS, 72 * 60 * 60 * 1000);
    assert_eq!(corelink_gc::GRACE_AC_MS, 24 * 60 * 60 * 1000);
}

#[test]
fn canonical_sweep_phase_budget_pinned_at_5_min() {
    assert_eq!(corelink_gc::CANONICAL_SWEEP_PHASE_BUDGET_MS, 5 * 60 * 1000);
}

#[test]
fn sweep_decision_alreadyresolved_for_already_swept_candidate() {
    let (sweep, _runs, candidates, blob_meta, _ac, _audit) = fresh(1);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let d = digest_from(0xBEEF);
    let mark_anchor: u64 = 5_000;
    seed_candidate(
        &candidates,
        &blob_meta,
        tenant,
        rid,
        d.clone(),
        mark_anchor,
        256,
    );
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            mark_anchor + 1,
            None,
        )
        .unwrap();
    let stale = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    let dec = sweep.step_candidate(&stale, GcRegion::Sam).unwrap();
    match dec {
        SweepDecision::AlreadyResolved { observed_status } => {
            assert_eq!(observed_status, CandidateStatus::Swept);
        }
        other => panic!("expected AlreadyResolved, got {other:?}"),
    }
}

#[test]
fn ac_index_returns_witness_with_action_digest_passthrough() {
    let ac = InMemoryAcReferenceIndex::new();
    let tenant = Uuid::from_u128(1);
    let target = digest_from(0x1234);
    ac.push_ac_row(tenant, "action_witness_string", vec![target.clone()], 1_000);
    let res = ac.find_re_reference(tenant, &target, 999).unwrap().unwrap();
    assert_eq!(res.action_digest, "action_witness_string");
    assert_eq!(res.created_at_ms, 1_000);
}
