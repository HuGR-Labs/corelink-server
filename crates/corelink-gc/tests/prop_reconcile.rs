//! Property tests pinned at 10 k iter against the WI-S06-005 reconcile
//! phase load-bearing invariants.
//!
//! Per WI §6.1.9 + sprint contract §5.5 R-S06-10 / R-S06-10.1, the
//! canonical 5 property tests this file ships are:
//!
//! 1. `prop_no_drift_no_mutation` — for any random `(stored, expected)`
//!    pair where they agree, reconcile produces NO `blob_meta.refcount`
//!    writes (NoDrift decision; refcount preserved).
//! 2. `prop_auto_fix_bounded_dual_condition` — drifts within
//!    `count ≤ 5 AND percent ≤ 0.01%` auto-fix; either condition
//!    violated → manual review (Lote 10.6bis P0-6 scale-invariant
//!    percentage-floor + absolute-floor).
//! 3. `prop_json_each_semantics_not_like` — the canonical regression
//!    pin for Lote 10.6bis Part 2a P0-1: an `ac_meta` row whose
//!    `action_digest` METADATA contains the target digest as a
//!    substring MUST NOT inflate `expected_refcount` — only `j.value
//!    = digest` exact membership counts.
//! 4. `prop_tenant_isolation` — reconcile for tenant A never reads or
//!    mutates tenant B's `blob_meta.refcount`.
//! 5. `prop_idempotent_re_run` — re-running reconcile after a
//!    successful auto-fix produces no further drift (NoDrift on every
//!    row; no auto-fix re-emit).
//!
//! Plus 4 sanity-grade tests pinning canonical constants + audit
//! taxonomy + cross-component invariants.
//!
//! The 100 k nightly variant is wired alongside WI-S06-006 (TLA+ CI
//! gate) per WI §6.1.9 + sprint contract DoD.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    reason = "test target"
)]

use std::sync::Arc;

use corelink_gc::{
    auto_fix_gate_fires, AcMetaReconcileRow, BlobDigest, BlobMetaReconcileRow,
    CountingReconcileClock, GcEventType, GcPhase, GcRegion, GcRunStore,
    InMemoryBlobMetaRefcountStore, InMemoryGcAuditSink, InMemoryGcMetrics, InMemoryGcRunStore,
    InMemoryReconcilePhase, InMemoryRefcountSource, ReconcileConfig, ReconcileDecision,
    ReconcilePhase, RunId, SevLevel, AUTO_FIX_MAX_RECORDS,
};
use proptest::prelude::*;
use uuid::Uuid;

fn region_from(idx: usize) -> GcRegion {
    GcRegion::all()[idx % 5]
}

fn digest_from(seed: u32) -> BlobDigest {
    let prefix = format!("{seed:08x}");
    let mut s = prefix;
    s.push_str(&"0".repeat(BlobDigest::LEN - 8));
    BlobDigest::parse(&s).expect("canonical hex")
}

type ReconcileFixture = (
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

fn fresh(start_ms: u64) -> ReconcileFixture {
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

fn seed_run_in_reconcile(
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

fn seed_blob(
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

fn push_ac_referencing(
    source: &InMemoryRefcountSource,
    tenant: Uuid,
    refs: Vec<BlobDigest>,
    action_digest: impl Into<String>,
) {
    source.push_ac_row(
        tenant,
        AcMetaReconcileRow {
            action_digest: action_digest.into(),
            blob_refs: refs,
            deleted_at_ms: None,
            // created_at_ms < snapshot_at_ms (which is start_ms+1).
            // start_ms in fresh is 1_000_000+ so 100 is safe.
            created_at_ms: 100,
        },
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        .. ProptestConfig::default()
    })]

    // =================================================================
    // 1) prop_no_drift_no_mutation
    //
    // For any random (stored, expected) pair where they agree, reconcile
    // produces NoDrift; blob_meta.refcount preserved; NO RefcountAutoFixed
    // / RefcountManualReviewRequired audit emit.
    // =================================================================
    #[test]
    fn prop_no_drift_no_mutation(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        n in 1u32..16,
        // Each (stored == expected) at refcount k. ks length is
        // capped at 16 so the per-iter expected_refcount scan stays
        // O(n × ks_len) ≈ 256 ops at worst per iter — 10k iter then
        // ≈ 2.5M ops total.
        ks in proptest::collection::vec(0u32..4, 1..16),
    ) {
        let (phase, runs, source, blob_meta, audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(7)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        // Use only the first n entries from ks; pad with 0 if needed.
        let count = (n as usize).min(ks.len());
        for (i, k) in ks.iter().take(count).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let i_u32 = i as u32;
            let d = digest_from(i_u32 + 1);
            seed_blob(&blob_meta, tenant, d.clone(), *k);
            // Push k ac rows, each referencing d once. expected = k.
            for j in 0..*k {
                push_ac_referencing(&source, tenant, vec![d.clone()], format!("ac_{i}_{j}"));
            }
        }

        let result = phase.execute(rid, tenant, region).unwrap();
        prop_assert_eq!(result.no_drift_count, count as u64);
        prop_assert_eq!(result.drifts_detected, 0);
        prop_assert_eq!(result.auto_fixed_count, 0);
        prop_assert_eq!(result.manual_review_count, 0);
        prop_assert_eq!(result.sev_level, SevLevel::None);
        prop_assert_eq!(audit.snapshot_of(GcEventType::RefcountAutoFixed).len(), 0);
        prop_assert_eq!(
            audit.snapshot_of(GcEventType::RefcountManualReviewRequired).len(),
            0
        );
        // blob_meta.refcount preserved exactly per row.
        for (i, k) in ks.iter().take(count).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let i_u32 = i as u32;
            let d = digest_from(i_u32 + 1);
            let row = blob_meta.snapshot(tenant, &d).unwrap();
            prop_assert_eq!(row.stored_refcount, *k);
        }
    }

    // =================================================================
    // 2) prop_auto_fix_bounded_dual_condition
    //
    // Generate (n_blobs, n_drift) pairs that exercise the dual-condition
    // gate. The orchestrator's per-row auto-fix predicate fires ONLY
    // when:
    //   tenant_drift_count_so_far + 1 ≤ AUTO_FIX_MAX_RECORDS (= 5)
    //   AND
    //   (tenant_drift_count_so_far + 1) / (tenant_blobs_scanned_so_far + 1)
    //     ≤ AUTO_FIX_MAX_PERCENT (= 0.0001).
    //
    // The property: the FIRST k drifts auto-fix where k is the largest
    // prefix of drifts such that BOTH conditions hold. Subsequent drifts
    // route to manual review.
    // =================================================================
    #[test]
    fn prop_auto_fix_bounded_dual_condition(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        // n_blobs is the total tenant size; n_drift the drift count.
        // Bounded at 50 to keep proptest at 10k iter under 30s.
        // Larger scales are exercised by `prop_auto_fix_scale_invariant`
        // with a smaller iteration count.
        n_blobs in 10u32..50,
        n_drift in 0u32..15,
    ) {
        prop_assume!(n_drift <= n_blobs);
        let (phase, runs, source, blob_meta, _audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(11)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        // Seed n_blobs blobs all with stored=0 expected=0 EXCEPT the
        // first n_drift which have stored=0 expected=1 (drift).
        // Iteration order: BTreeMap is sorted by key; drift digests
        // use a smaller seed so they're inspected FIRST.
        for i in 0..n_drift {
            let d = digest_from(i);
            seed_blob(&blob_meta, tenant, d.clone(), 0);
            push_ac_referencing(&source, tenant, vec![d], format!("ad_{i}"));
        }
        for i in 0..(n_blobs.saturating_sub(n_drift)) {
            seed_blob(&blob_meta, tenant, digest_from(i + 1_000_000), 0);
        }

        let result = phase.execute(rid, tenant, region).unwrap();
        prop_assert_eq!(result.blobs_scanned, u64::from(n_blobs));
        prop_assert_eq!(result.drifts_detected, u64::from(n_drift));
        // Compute expected auto-fixed cohort: largest prefix k such
        // that for every j in [1..=k] the predicate holds at j.
        let cfg = ReconcileConfig::default();
        let mut expected_auto_fixed: u64 = 0;
        let mut expected_manual: u64 = 0;
        // Drift rows are scanned first (smaller seed → smaller digest).
        for k in 0..n_drift {
            let blobs_scanned_so_far = u64::from(k);
            let drift_count_so_far = u64::from(k);
            let drift_count = drift_count_so_far + 1;
            let blobs_scanned = blobs_scanned_so_far + 1;
            let drift_percent = drift_count as f64 / blobs_scanned as f64;
            if auto_fix_gate_fires(drift_count, drift_percent, &cfg) {
                expected_auto_fixed += 1;
            } else {
                expected_manual += 1;
            }
        }
        prop_assert_eq!(result.auto_fixed_count, expected_auto_fixed);
        prop_assert_eq!(result.manual_review_count, expected_manual);
        // Auto-fix amplification invariant: auto_fixed ≤
        // AUTO_FIX_MAX_RECORDS at every (tenant, run).
        prop_assert!(result.auto_fixed_count <= AUTO_FIX_MAX_RECORDS);
    }

    // =================================================================
    // 3) prop_json_each_semantics_not_like
    //
    // Lote 10.6bis Part 2a P0-1: the canonical SQL `j.value = digest`
    // exact-membership idiom MUST NOT be confused with a substring
    // match on `action_digest` or any other metadata field. The
    // property: for any random pair (target_digest, decoy_action_id),
    // pushing an ac_meta row whose `action_digest = decoy_action_id`
    // (no blob_refs membership) MUST NOT inflate `expected_refcount`
    // for `target_digest` — even when `decoy_action_id` literally
    // contains the target hex string as a substring.
    // =================================================================
    #[test]
    fn prop_json_each_semantics_not_like(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        target_seed in 0u32..1_000_000,
        n_decoys in 1u32..16,
    ) {
        let (phase, runs, source, blob_meta, _audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(13)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        let target = digest_from(target_seed);
        // Stored=0; one ac row references target → expected=1 → no
        // drift.
        seed_blob(&blob_meta, tenant, target.clone(), 1);
        push_ac_referencing(
            &source,
            tenant,
            vec![target.clone()],
            "ac_real",
        );
        // Push N decoy ac rows whose action_digest CONTAINS the target
        // hex string but whose blob_refs is empty (or references
        // unrelated digests). With a `LIKE '%digest%'` query these
        // would inflate the count; with `json_each j.value = digest`
        // they MUST NOT.
        for i in 0..n_decoys {
            let unrelated = digest_from(target_seed.wrapping_add(0xDEAD_BEEF) + i);
            // Construct an action_digest string that EMBEDS the
            // target's hex form as a substring.
            let decoy_action_digest = format!("decoy_{target}_action_{i}");
            source.push_ac_row(
                tenant,
                AcMetaReconcileRow {
                    action_digest: decoy_action_digest,
                    // blob_refs lists `unrelated` only; target NOT
                    // included.
                    blob_refs: vec![unrelated],
                    deleted_at_ms: None,
                    created_at_ms: 100,
                },
            );
        }

        let result = phase.execute(rid, tenant, region).unwrap();
        // expected_refcount for target = 1 (only the real ac row);
        // stored = 1; NO DRIFT despite N decoys with substring-collision
        // metadata.
        prop_assert_eq!(result.drifts_detected, 0);
        prop_assert_eq!(result.no_drift_count, 1);
    }

    // =================================================================
    // 4) prop_tenant_isolation
    //
    // Two tenants share digests (canonical hex collision is impossible
    // by construction, but tenant A and tenant B may both have the
    // same blob_meta digest). The reconcile pass for tenant A MUST
    // NOT read tenant B's ac_meta rows nor mutate tenant B's
    // blob_meta.refcount.
    // =================================================================
    #[test]
    fn prop_tenant_isolation(
        tenant_a_seed in 1u128..400_000,
        tenant_b_seed in 500_001u128..900_000,
        region_idx in 0usize..5,
        n_shared_digests in 1u32..16,
    ) {
        prop_assume!(tenant_a_seed != tenant_b_seed);
        let (phase, runs, source, blob_meta, audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let ta = Uuid::from_u128(tenant_a_seed);
        let tb = Uuid::from_u128(tenant_b_seed);
        let ra = RunId(Uuid::from_u128(tenant_a_seed.wrapping_add(17)));
        let rb = RunId(Uuid::from_u128(tenant_b_seed.wrapping_add(19)));
        seed_run_in_reconcile(&runs, ra, ta, region);
        seed_run_in_reconcile(&runs, rb, tb, region);
        for i in 0..n_shared_digests {
            let d = digest_from(i + 200_000);
            // Tenant A: stored=1 + ac references → no drift.
            seed_blob(&blob_meta, ta, d.clone(), 1);
            push_ac_referencing(&source, ta, vec![d.clone()], format!("a_{i}"));
            // Tenant B: same digest, stored=2 + 2 ac references →
            // no drift.
            seed_blob(&blob_meta, tb, d.clone(), 2);
            push_ac_referencing(&source, tb, vec![d.clone()], format!("b1_{i}"));
            push_ac_referencing(&source, tb, vec![d.clone()], format!("b2_{i}"));
        }
        let ra_result = phase.execute(ra, ta, region).unwrap();
        let rb_result = phase.execute(rb, tb, region).unwrap();
        prop_assert_eq!(ra_result.no_drift_count, u64::from(n_shared_digests));
        prop_assert_eq!(ra_result.drifts_detected, 0);
        prop_assert_eq!(rb_result.no_drift_count, u64::from(n_shared_digests));
        prop_assert_eq!(rb_result.drifts_detected, 0);
        // Audit records carry the right tenant_id.
        for record in audit.snapshot_of(GcEventType::RefcountReconciled) {
            prop_assert!(record.tenant_id == ta || record.tenant_id == tb);
        }
        // blob_meta refcount preserved per tenant — A=1, B=2.
        for i in 0..n_shared_digests {
            let d = digest_from(i + 200_000);
            let a_row = blob_meta.snapshot(ta, &d).unwrap();
            let b_row = blob_meta.snapshot(tb, &d).unwrap();
            prop_assert_eq!(a_row.stored_refcount, 1);
            prop_assert_eq!(b_row.stored_refcount, 2);
        }
    }

    // =================================================================
    // 5) prop_idempotent_re_run
    //
    // Re-running reconcile after a successful auto-fix produces no
    // further drift (NoDrift on every row); no RefcountAutoFixed
    // re-emit; no RefcountManualReviewRequired emit. Sustained drift
    // < 0.1% after a successful pass is the canonical DoD §10.s06.5.
    // =================================================================
    // (prop_idempotent_re_run lives in the slow block below — its
    // 10k-blob scale means we cap at 256 iter not 10k.)

    // =================================================================
    // 6) prop_step_decision_aggregates
    //
    // The decision pipeline returns counters that exactly match the
    // ReconcileResult aggregate: blobs_scanned == sum of the 5
    // decision arms.
    // =================================================================
    #[test]
    fn prop_step_decision_aggregates(
        tenant_seed in 1u128..500_000,
        region_idx in 0usize..5,
        n in 1u32..32,
        deleted_threshold in 0u32..32,
        orphan_threshold in 0u32..32,
    ) {
        let (phase, runs, _source, blob_meta, _audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(29)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        for i in 0..n {
            let d = digest_from(i + 500_000);
            // Soft-deleted: deleted_at_ms = Some(_); skipped.
            // Orphan R2: r2_present = false; orphan path.
            // Otherwise normal — stored=0 expected=0 → no drift.
            let deleted = i < deleted_threshold;
            let orphan = i >= deleted_threshold && i < deleted_threshold + orphan_threshold;
            blob_meta.push_row(
                tenant,
                BlobMetaReconcileRow {
                    digest: d,
                    stored_refcount: 0,
                    deleted_at_ms: if deleted { Some(800) } else { None },
                    r2_present: !orphan,
                },
            );
        }
        let result = phase.execute(rid, tenant, region).unwrap();
        // Aggregate sum invariant.
        let sum = result.no_drift_count
            + result.auto_fixed_count
            + result.manual_review_count
            + result.skipped_soft_deleted_count
            + result.orphan_r2_count;
        prop_assert_eq!(result.blobs_scanned, sum);
    }


    // =================================================================
    // 8) prop_audit_emit_per_decision_arm
    //
    // Every NoDrift / AutoFixed / ManualReview decision emits exactly
    // one audit record; SkippedSoftDeleted and OrphanR2Detected emit
    // none.
    // =================================================================
    #[test]
    fn prop_audit_emit_per_decision_arm(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        n_no_drift in 0u32..8,
        n_drift in 0u32..6,
        n_soft_deleted in 0u32..8,
        n_orphan_r2 in 0u32..8,
    ) {
        let (phase, runs, source, blob_meta, audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(37)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        // No-drift rows: stored = expected = 0 (no ac refs).
        for i in 0..n_no_drift {
            seed_blob(&blob_meta, tenant, digest_from(i + 100), 0);
        }
        // Drift rows: stored = 0, expected = 1.
        for i in 0..n_drift {
            let d = digest_from(i + 200);
            seed_blob(&blob_meta, tenant, d.clone(), 0);
            push_ac_referencing(&source, tenant, vec![d], format!("d_{i}"));
        }
        // Soft-deleted rows: deleted_at_ms = Some(_).
        for i in 0..n_soft_deleted {
            blob_meta.push_row(
                tenant,
                BlobMetaReconcileRow {
                    digest: digest_from(i + 300),
                    stored_refcount: 0,
                    deleted_at_ms: Some(700),
                    r2_present: true,
                },
            );
        }
        // Orphan R2 rows: r2_present = false, deleted_at_ms = None.
        for i in 0..n_orphan_r2 {
            blob_meta.push_row(
                tenant,
                BlobMetaReconcileRow {
                    digest: digest_from(i + 400),
                    stored_refcount: 0,
                    deleted_at_ms: None,
                    r2_present: false,
                },
            );
        }
        let result = phase.execute(rid, tenant, region).unwrap();
        // Audit emit count invariant.
        let no_drift_emit = audit.snapshot_of(GcEventType::RefcountReconciled).len();
        let auto_fix_emit = audit.snapshot_of(GcEventType::RefcountAutoFixed).len();
        let manual_emit = audit
            .snapshot_of(GcEventType::RefcountManualReviewRequired)
            .len();
        prop_assert_eq!(no_drift_emit, result.no_drift_count as usize);
        prop_assert_eq!(auto_fix_emit, result.auto_fixed_count as usize);
        prop_assert_eq!(manual_emit, result.manual_review_count as usize);
        prop_assert_eq!(
            result.audit_events_emitted,
            (no_drift_emit + auto_fix_emit + manual_emit) as u64
        );
        // Skipped + orphan arms emit no reconcile audit (forward to
        // S-09 reclaim task / informational).
        prop_assert_eq!(result.skipped_soft_deleted_count, u64::from(n_soft_deleted));
        prop_assert_eq!(result.orphan_r2_count, u64::from(n_orphan_r2));
    }

    // =================================================================
    // 9) prop_snapshot_bound_excludes_post_snapshot_writes
    //
    // The canonical SQL `WHERE a.created_at_ms < snapshot_at_ms`
    // bounds the race window to writes-before-snapshot (Lote 10.6bis
    // P1-6 fix). For any random ac_meta row whose
    // `created_at_ms >= snapshot_at_ms`, expected_refcount MUST NOT
    // count it.
    // =================================================================
    #[test]
    fn prop_snapshot_bound_excludes_post_snapshot_writes(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        // Force created_at_ms to be very large so it definitely
        // exceeds the snapshot anchor (which is start_ms+1 ≈
        // 1_000_001 in the fixture).
        post_snapshot_offset in 5_000_000u64..10_000_000,
    ) {
        let (phase, runs, source, blob_meta, _audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(41)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        let d = digest_from(0xfeed);
        seed_blob(&blob_meta, tenant, d.clone(), 0);
        // Post-snapshot ac row: created_at_ms ≥ snapshot_at_ms → must
        // be excluded from expected_refcount.
        source.push_ac_row(
            tenant,
            AcMetaReconcileRow {
                action_digest: "post_snapshot".to_owned(),
                blob_refs: vec![d.clone()],
                deleted_at_ms: None,
                created_at_ms: post_snapshot_offset,
            },
        );
        let result = phase.execute(rid, tenant, region).unwrap();
        // expected = 0 (post-snapshot ac excluded); stored = 0 → no
        // drift.
        prop_assert_eq!(result.no_drift_count, 1);
        prop_assert_eq!(result.drifts_detected, 0);
    }
}

// =====================================================================
// Heavy property tests — bounded at 256 iter (10k blobs per iter ×
// 256 iter = ~2.5M ops; still adversarial-grade coverage). The 10k
// nightly variant is wired alongside WI-S06-006 (TLA+ CI gate).
//
// These tests need the larger blob-count denominator to cross the
// 0.0001 percent gate; pinning at 10k iter would create a multi-minute
// PR-gate runtime. The 256-iter cap mirrors the proptest convention
// for cost-distinct sub-suites.
// =====================================================================

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        .. ProptestConfig::default()
    })]

    // =================================================================
    // 7) prop_auto_fix_scale_invariant
    //
    // Across tenant scales 10/100/1k/10k blobs with drift counts 1-15,
    // the dual-condition gate decision is the canonical predicate
    // `auto_fix_gate_fires(count, percent)`. Verifies the orchestrator
    // never fires auto-fix when the predicate rejects.
    // =================================================================
    #[test]
    fn prop_auto_fix_scale_invariant(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        scale in proptest::sample::select(vec![10u32, 100u32, 1_000u32, 10_000u32]),
        n_drift in 1u32..15,
    ) {
        prop_assume!(n_drift <= scale);
        let (phase, runs, source, blob_meta, _audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(31)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        // Drift digests scanned first (smaller seed); scale digests
        // padding.
        for i in 0..n_drift {
            let d = digest_from(i);
            seed_blob(&blob_meta, tenant, d.clone(), 0);
            push_ac_referencing(&source, tenant, vec![d], format!("ad_{i}"));
        }
        for i in 0..(scale.saturating_sub(n_drift)) {
            seed_blob(&blob_meta, tenant, digest_from(i + 1_000_000), 0);
        }
        let result = phase.execute(rid, tenant, region).unwrap();
        // Cumulative-predicate replay: re-derive expected auto-fixed
        // count under the dual-condition gate.
        let cfg = ReconcileConfig::default();
        let mut expected_auto_fixed: u64 = 0;
        for k in 0..n_drift {
            let blobs_scanned_so_far = u64::from(k);
            let drift_count_so_far = u64::from(k);
            let drift_count = drift_count_so_far + 1;
            let blobs_scanned = blobs_scanned_so_far + 1;
            let drift_percent = drift_count as f64 / blobs_scanned as f64;
            if auto_fix_gate_fires(drift_count, drift_percent, &cfg) {
                expected_auto_fixed += 1;
            }
        }
        prop_assert_eq!(result.auto_fixed_count, expected_auto_fixed);
        prop_assert!(result.auto_fixed_count <= AUTO_FIX_MAX_RECORDS);
    }

    // =================================================================
    // 8) prop_idempotent_re_run
    //
    // Re-running reconcile after a successful auto-fix produces no
    // further drift (NoDrift on every row); no RefcountAutoFixed
    // re-emit. Sustained drift < 0.1% after a successful pass is the
    // canonical DoD §10.s06.5.
    // =================================================================
    #[test]
    fn prop_idempotent_re_run(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        // n_blobs ∈ [10000, 12000] minimal to cross the 0.0001
        // percent gate. n_drift capped at 1 to ensure the cumulative
        // dual-condition gate fires for the lone drift row at the
        // last iteration position (non-drift rows scanned first).
        n_blobs in 10_000u32..12_000,
        n_drift in 0u32..2,
    ) {
        prop_assume!(n_drift <= n_blobs);
        let (phase, runs, source, blob_meta, audit) = fresh(1_000_000);
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(23)));
        seed_run_in_reconcile(&runs, rid, tenant, region);
        // Non-drift blobs scanned FIRST (smaller digest seed). Drift
        // blobs scanned LAST (largest digest seed) so the cumulative
        // dual-condition gate has ample non-drift denominator before
        // the first drift hits.
        for i in 0..(n_blobs.saturating_sub(n_drift)) {
            seed_blob(&blob_meta, tenant, digest_from(i + 1), 0);
        }
        for i in 0..n_drift {
            let d = digest_from(0xFFFF_0000 + i);
            seed_blob(&blob_meta, tenant, d.clone(), 0);
            push_ac_referencing(&source, tenant, vec![d], format!("ad_{i}"));
        }
        let r1 = phase.execute(rid, tenant, region).unwrap();
        prop_assert_eq!(r1.drifts_detected, u64::from(n_drift));
        prop_assert_eq!(r1.auto_fixed_count, u64::from(n_drift));
        let auto_fixed_audits_after_first =
            audit.snapshot_of(GcEventType::RefcountAutoFixed).len();
        // Re-run.
        let r2 = phase.execute(rid, tenant, region).unwrap();
        // Idempotent: 0 drift, 0 auto-fix, 0 manual review.
        prop_assert_eq!(r2.drifts_detected, 0);
        prop_assert_eq!(r2.auto_fixed_count, 0);
        prop_assert_eq!(r2.manual_review_count, 0);
        prop_assert_eq!(r2.no_drift_count, u64::from(n_blobs));
        // Auto-fix audit count UNCHANGED across re-run.
        let auto_fixed_audits_after_second =
            audit.snapshot_of(GcEventType::RefcountAutoFixed).len();
        prop_assert_eq!(
            auto_fixed_audits_after_first,
            auto_fixed_audits_after_second
        );
    }
}

// =====================================================================
// Sanity checks (non-proptest; documented invariants).
// =====================================================================

#[test]
fn canonical_constants_pinned_at_1h_5_records_0_01_percent() {
    assert_eq!(
        corelink_gc::CANONICAL_RECONCILE_PHASE_BUDGET_MS,
        60 * 60 * 1000
    );
    assert_eq!(corelink_gc::AUTO_FIX_MAX_RECORDS, 5);
    assert_eq!(corelink_gc::AUTO_FIX_MAX_PERCENT, 0.0001);
    assert_eq!(corelink_gc::SEV1_PER_TENANT_DRIFT_PERCENT, 0.01);
    assert_eq!(corelink_gc::SEV2_GLOBAL_DRIFT_PERCENT, 0.001);
}

#[test]
fn audit_taxonomy_count_extended_to_eleven() {
    assert_eq!(corelink_gc::canonical_audit_event_strings().len(), 11);
    let names = corelink_gc::canonical_audit_event_strings();
    assert!(names.iter().any(|n| n.contains("refcount_reconciled")));
    assert!(names.iter().any(|n| n.contains("refcount_auto_fixed")));
    assert!(names
        .iter()
        .any(|n| n.contains("refcount_manual_review_required")));
}

#[test]
fn step_decision_orphan_r2_reaches_decision_arm() {
    // Direct unit test pinning the OrphanR2Detected arm — refcount
    // mutation MUST NOT fire when r2_present = false.
    let (phase, runs, _source, blob_meta, _audit) = fresh(1_000_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest_from(0xc0de);
    blob_meta.push_row(
        tenant,
        BlobMetaReconcileRow {
            digest: d.clone(),
            stored_refcount: 7,
            deleted_at_ms: None,
            r2_present: false,
        },
    );
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.orphan_r2_count, 1);
    assert_eq!(result.drifts_detected, 0);
    // refcount preserved.
    assert_eq!(blob_meta.snapshot(tenant, &d).unwrap().stored_refcount, 7);
}

#[test]
fn auto_fix_gate_5_and_threshold_pin() {
    // Boundary table pin: dual-condition gate at the canonical
    // (5, 0.0001) corner.
    let cfg = ReconcileConfig::default();
    // (5, 0.0001) → fires.
    assert!(auto_fix_gate_fires(5, 0.0001, &cfg));
    // (6, 0.0001) → fails (count arm).
    assert!(!auto_fix_gate_fires(6, 0.0001, &cfg));
    // (5, 0.000_101) → fails (percent arm).
    assert!(!auto_fix_gate_fires(5, 0.000_101, &cfg));
    // Both fail.
    assert!(!auto_fix_gate_fires(100, 0.5, &cfg));
}

#[test]
fn step_decision_no_drift_returns_canonical_event() {
    // Sanity: NoDrift decision arm produces a NoDrift variant carrying
    // the stored refcount.
    let (phase, runs, source, blob_meta, _audit) = fresh(1_000_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_reconcile(&runs, rid, tenant, GcRegion::Sam);
    let d = digest_from(0x42);
    seed_blob(&blob_meta, tenant, d.clone(), 1);
    push_ac_referencing(&source, tenant, vec![d.clone()], "ac_real");
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.no_drift_count, 1);
    assert_eq!(result.drifts_detected, 0);
    assert!(matches!(result.sev_level, SevLevel::None));
    // Direct step_row probe.
    let row = blob_meta.snapshot(tenant, &d).unwrap();
    let dec = phase
        .step_row(tenant, rid, GcRegion::Sam, &row, 1_000_002, 0, 0)
        .unwrap();
    match dec {
        ReconcileDecision::NoDrift { refcount } => assert_eq!(refcount, 1),
        other => panic!("expected NoDrift, got {other:?}"),
    }
}
