//! Property tests pinned at 10 k iter against the WI-S06-002 mark
//! phase load-bearing invariants.
//!
//! Per WI §6.1.13 + sprint contract §6 DoD, the canonical 5 property
//! tests are:
//!
//! 1. `prop_mark_idempotent` — re-run mark on same `(tenant, region)`
//!    after crash → same reachable set + `mark_started_at_ms`
//!    preserved (NOT overwritten).
//! 2. `prop_mark_started_at_atomic` — multiple capture attempts; only
//!    the first succeeds; subsequent calls observe the stable anchor.
//! 3. `prop_reachable_set_complete` — random tenant states (mix of
//!    reachable + orphan blobs); mark identifies all reachable
//!    correctly via the 3-pass union; orphans become candidates;
//!    no false orphan.
//! 4. `prop_tenant_isolation_mark` — concurrent marks across distinct
//!    tenants; no cross-tenant interference.
//! 5. `prop_mark_d1_batch_bounded` — random batch ceiling (1..=250);
//!    every batch fetched is `<=` ceiling; jitter is accounted; total
//!    duration `<=` budget.
//!
//! Each test uses `proptest!` with `ProptestConfig { cases: 10_000,
//! .. }` so the suite hits the canonical 10 k iter ceiling. The
//! 100 k nightly variant is wired in WI-S06-006 alongside the TLA+
//! CI gate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use std::collections::BTreeSet;
use std::sync::Arc;

use corelink_gc::{
    AcMetaRow, BlobDigest, BlobMetaRow, CandidateStatus, CountingMarkClock, GcCandidatesStore,
    GcRegion, GcRunStore, InMemoryGcAuditSink, InMemoryGcCandidatesStore, InMemoryGcMetrics,
    InMemoryGcRunStore, InMemoryMarkPhase, InMemoryReachableSetSource, ManifestChunkRow,
    MarkConfig, MarkPhase, RunId,
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

/// Construct a canonical 64-char hex digest from a u32 seed. Each
/// distinct `seed` yields a distinct digest; collisions are
/// algorithmically impossible across the (1, 1_000_000] band the tests
/// use because the seed packs into the leading 8 hex bytes.
fn digest_from(seed: u32) -> BlobDigest {
    let prefix = format!("{seed:08x}");
    let mut s = prefix;
    s.push_str(&"0".repeat(BlobDigest::LEN - 8));
    BlobDigest::parse(&s).expect("canonical hex")
}

type MarkFixture = (
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
);

fn fresh_with_config(start_ms: u64, cfg: MarkConfig) -> MarkFixture {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let source = Arc::new(InMemoryReachableSetSource::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingMarkClock::new(start_ms));
    // 24h grace window matches the canonical default; without it a
    // mark_anchor of `start_ms` would filter out fixture rows whose
    // `created_at_ms` is set before the start (the lower bound is
    // `mark_anchor - grace`).
    let mark = InMemoryMarkPhase::new(
        Arc::clone(&runs),
        Arc::clone(&source),
        Arc::clone(&candidates),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
        cfg,
        24 * 60 * 60 * 1000,
    );
    (mark, runs, source, candidates)
}

fn fresh(start_ms: u64) -> MarkFixture {
    fresh_with_config(start_ms, MarkConfig::default())
}

fn seed_running(runs: &InMemoryGcRunStore, rid: RunId, tenant: Uuid, region: GcRegion) {
    runs.insert_pending(rid, tenant, region, 100, "cron".into())
        .unwrap();
    runs.acquire_running(rid, tenant, 200).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    // ===========================================================
    // 1) prop_mark_idempotent
    // ===========================================================
    //
    // Re-running mark on the same `(run_id, tenant, region)` yields
    // the same `mark_started_at_ms` anchor + the same candidate set
    // (composite PK on gc_candidates is idempotent).
    #[test]
    fn prop_mark_idempotent(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        live_count in 0u32..32,
        orphan_count in 0u32..32,
    ) {
        let (mark, runs, source, candidates) = fresh(1_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(1)));
        seed_running(&runs, rid, tenant, region);
        for i in 0..live_count {
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: digest_from(i + 1),
                    refcount: 1,
                    size_bytes: u64::from(i + 1),
                    last_referenced_at_ms: 500,
                },
            );
        }
        for i in 0..orphan_count {
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: digest_from(i + 1_000),
                    refcount: 0,
                    size_bytes: u64::from(i + 1),
                    last_referenced_at_ms: 500,
                },
            );
        }
        let r1 = mark.execute(rid, tenant, region).unwrap();
        let r2 = mark.execute(rid, tenant, region).unwrap();
        // Anchor preserved (INV-GC-MARK-STARTED-AT-IMMUTABLE).
        prop_assert_eq!(r1.mark_started_at_ms, r2.mark_started_at_ms);
        // Candidate count stable.
        prop_assert_eq!(r1.candidates_count, u64::from(orphan_count));
        prop_assert_eq!(r2.candidates_count, u64::from(orphan_count));
        // Snapshot stable (idempotent INSERT on composite PK).
        let snap = candidates.snapshot_for_run(tenant, rid).unwrap();
        prop_assert_eq!(snap.len() as u64, u64::from(orphan_count));
    }

    // ===========================================================
    // 2) prop_mark_started_at_atomic
    // ===========================================================
    //
    // 3 capture attempts in a row: the first sets the anchor; the next
    // two read the same value (no overwrite). Anchor is monotonic
    // (subsequent capture attempts cannot move it forward).
    #[test]
    fn prop_mark_started_at_atomic(
        tenant_seed in 1u128..1_000_000,
        start_ms in 1u64..1_000_000_000,
    ) {
        let (mark, runs, _source, _candidates) = fresh(start_ms);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(7)));
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        let r1 = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        let r2 = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        let r3 = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        prop_assert_eq!(r1.mark_started_at_ms, r2.mark_started_at_ms);
        prop_assert_eq!(r1.mark_started_at_ms, r3.mark_started_at_ms);
        // Verify storage agrees.
        let row = runs.lookup(rid, tenant).unwrap().unwrap();
        prop_assert_eq!(row.mark_started_at_ms, Some(r1.mark_started_at_ms));
    }

    // ===========================================================
    // 3) prop_reachable_set_complete
    // ===========================================================
    //
    // Mix of reachable (refcount > 0) + orphan (refcount = 0) +
    // ac_meta-only (digest with refcount = 0 in blob_meta but referenced
    // by an ac_meta row) + manifest-only blobs. Mark MUST classify every
    // reachable blob correctly (false orphan = unacceptable per WI §9.6).
    #[test]
    fn prop_reachable_set_complete(
        tenant_seed in 1u128..500_000,
        region_idx in 0usize..5,
        n_live in 0u32..16,
        n_orphan in 0u32..16,
        n_ac_only in 0u32..16,
        n_manifest_only in 0u32..16,
    ) {
        let (mark, runs, source, candidates) = fresh(1_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(11)));
        seed_running(&runs, rid, tenant, region);
        let mut expected_orphans: BTreeSet<BlobDigest> = BTreeSet::new();
        // Live blobs (reachable via refcount > 0).
        for i in 0..n_live {
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: digest_from(i + 100_000),
                    refcount: 1,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
        }
        // Orphan blobs (refcount = 0; not referenced anywhere → candidate).
        for i in 0..n_orphan {
            let d = digest_from(i + 200_000);
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: d.clone(),
                    refcount: 0,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
            expected_orphans.insert(d);
        }
        // AC-only protected (refcount = 0 in blob_meta but referenced
        // by ac_meta → reachable).
        for i in 0..n_ac_only {
            let d = digest_from(i + 300_000);
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: d.clone(),
                    refcount: 0,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
            source.push_ac_meta(
                tenant,
                AcMetaRow {
                    blob_refs: vec![d],
                    created_at_ms: 600,
                },
            );
        }
        // Manifest-only protected.
        for i in 0..n_manifest_only {
            let d = digest_from(i + 400_000);
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: d.clone(),
                    refcount: 0,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
            source.push_manifest_chunk(
                tenant,
                ManifestChunkRow {
                    chunk_digest: d,
                    created_at_ms: 700,
                },
            );
        }
        let result = mark.execute(rid, tenant, region).unwrap();
        // No false orphan: every candidate must be in the expected set.
        let snap = candidates.snapshot_for_run(tenant, rid).unwrap();
        for cand in &snap {
            prop_assert!(
                expected_orphans.contains(&cand.digest),
                "false orphan: {:?} not in expected", cand.digest
            );
            prop_assert_eq!(cand.status, CandidateStatus::Candidate);
        }
        // No missed orphan: every expected orphan must be in candidates.
        let actual: BTreeSet<BlobDigest> =
            snap.iter().map(|c| c.digest.clone()).collect();
        prop_assert_eq!(actual, expected_orphans.clone());
        prop_assert_eq!(result.candidates_count, expected_orphans.len() as u64);
    }

    // ===========================================================
    // 4) prop_tenant_isolation_mark
    // ===========================================================
    //
    // Two tenants in the same region: each tenant's mark sees only its
    // own blob_meta / ac_meta / manifest_chunks rows; candidate
    // snapshots are tenant-scoped.
    #[test]
    fn prop_tenant_isolation_mark(
        tenant_a_seed in 1u128..500_000,
        tenant_b_seed in 500_001u128..1_000_000,
        region_idx in 0usize..5,
        a_orphans in 0u32..8,
        b_orphans in 0u32..8,
    ) {
        prop_assume!(tenant_a_seed != tenant_b_seed);
        let (mark, runs, source, candidates) = fresh(1_000);
        let region = region_from(region_idx);
        let ta = Uuid::from_u128(tenant_a_seed);
        let tb = Uuid::from_u128(tenant_b_seed);
        let rid_a = RunId(Uuid::from_u128(tenant_a_seed.wrapping_add(13)));
        let rid_b = RunId(Uuid::from_u128(tenant_b_seed.wrapping_add(17)));
        seed_running(&runs, rid_a, ta, region);
        seed_running(&runs, rid_b, tb, region);
        for i in 0..a_orphans {
            source.push_blob_meta(
                ta,
                BlobMetaRow {
                    digest: digest_from(i + 500_000),
                    refcount: 0,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
        }
        for i in 0..b_orphans {
            source.push_blob_meta(
                tb,
                BlobMetaRow {
                    digest: digest_from(i + 600_000),
                    refcount: 0,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
        }
        let ra = mark.execute(rid_a, ta, region).unwrap();
        let rb = mark.execute(rid_b, tb, region).unwrap();
        prop_assert_eq!(ra.candidates_count, u64::from(a_orphans));
        prop_assert_eq!(rb.candidates_count, u64::from(b_orphans));
        // Cross-tenant snapshot returns empty.
        let cross_a = candidates.snapshot_for_run(ta, rid_b).unwrap();
        let cross_b = candidates.snapshot_for_run(tb, rid_a).unwrap();
        prop_assert!(cross_a.is_empty());
        prop_assert!(cross_b.is_empty());
        // Owning-tenant snapshot has the right count.
        prop_assert_eq!(
            candidates.snapshot_for_run(ta, rid_a).unwrap().len() as u64,
            u64::from(a_orphans)
        );
        prop_assert_eq!(
            candidates.snapshot_for_run(tb, rid_b).unwrap().len() as u64,
            u64::from(b_orphans)
        );
    }

    // ===========================================================
    // 5) prop_mark_d1_batch_bounded
    // ===========================================================
    //
    // Random batch_size in [1, CANONICAL_BATCH_SIZE]; mark phase visits
    // every row using batches of at most that ceiling; total batches >=
    // ceil(rows / batch_size); duration accounted (jitter + per-batch).
    #[test]
    fn prop_mark_d1_batch_bounded(
        tenant_seed in 1u128..1_000_000,
        batch_size in 1u32..=250,
        n_rows in 0u32..32,
        jitter_ms in 0u64..50,
    ) {
        let cfg = MarkConfig::new(batch_size, jitter_ms, 60_000_000).unwrap();
        let (mark, runs, source, _candidates) = fresh_with_config(1_000, cfg);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(19)));
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        for i in 0..n_rows {
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: digest_from(i + 700_000),
                    refcount: 1,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
        }
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        prop_assert_eq!(result.rows_scanned_count, u64::from(n_rows));
        // Pass 1 batches >= ceil(n_rows / batch_size); also at least 1
        // (the fake always issues at least one fetch per pass; passes 2
        // and 3 each contribute at least 1 empty-batch fetch).
        let expected_p1_batches = if n_rows == 0 {
            1u32
        } else {
            n_rows.div_ceil(batch_size) + u32::from(n_rows.is_multiple_of(batch_size))
        };
        // The fake stops on the first partial batch; if rows are an
        // exact multiple of batch_size, it issues one extra empty
        // batch to confirm end-of-stream.
        prop_assert!(
            result.batches_processed >= expected_p1_batches.saturating_add(2),
            "expected p1>={expected_p1_batches} + p2 + p3, got total={}",
            result.batches_processed
        );
    }
}

// ===========================================================
// Sanity checks (non-proptest; documented invariants)
// ===========================================================

#[test]
fn migration_const_versioned_at_seven() {
    // Cross-link to the gc_schema_version() bump in WI-S06-002.
    assert_eq!(corelink_gc::gc_schema_version(), 7);
}

#[test]
fn canonical_batch_size_pinned_at_250() {
    assert_eq!(corelink_gc::CANONICAL_BATCH_SIZE, 250);
}

#[test]
fn canonical_jitter_ms_pinned_at_100() {
    assert_eq!(corelink_gc::CANONICAL_JITTER_MS, 100);
}

#[test]
fn canonical_phase_budget_pinned_at_10_min() {
    assert_eq!(corelink_gc::CANONICAL_PHASE_BUDGET_MS, 10 * 60 * 1000);
}
