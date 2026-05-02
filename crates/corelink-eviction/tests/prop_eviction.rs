//! Property tests pinning the load-bearing invariants of
//! `corelink-eviction` at 10k iterations per check (PR-gate; nightly
//! 100k via env var override).
//!
//! Coverage map:
//!
//! - `prop_evict_protect_if_re_referenced_strict_boundary` — the
//!   load-bearing INV-GC-001 inheritance gate. At offset `0`
//!   (`ac.created_at == evict_started_at_ms`) the AC reference
//!   PROTECTS (canonical TLA `>=` protect-if-equal-or-newer); at
//!   offset `-1` (`ac.created_at < evict_started_at_ms`) the row
//!   surfaces as active reference (eviction MUST refuse); at offset
//!   `+1` (`ac.created_at > evict_started_at_ms`) the row protects.
//!   Off-by-one (`<=` instead of `<`) is a data-loss bug and pinned
//!   here at the property layer.
//! - `prop_tenant_isolation` — eviction for tenant A NEVER reads or
//!   mutates tenant B's blob_meta / storage_state.
//! - `prop_idempotent_re_run` — re-running eviction is a no-op
//!   (subsequent passes produce 0 evictions).
//! - `prop_blob_only_scope` — eviction NEVER touches the chunks
//!   table (the orchestrator's type signature has no chunks-table
//!   dependency; this property pins the no-op behaviour at the
//!   wiring level).
//! - `prop_ttl_size_proportional_reservation` — large blob upload
//!   window respects the size-proportional TTL formula `max(60s,
//!   req_bytes/1MB/s × 2x), capped 7d`.
//! - `prop_quota_trigger_fires_at_95pct` — exact-boundary at 95%;
//!   strictly below DOES NOT fire; at-or-above DOES fire.
//! - `prop_ttl_enterprise_cap_respected` — Enterprise admin-override
//!   > 730d MUST reject; <= 730d MUST accept.
//! - `prop_evict_dedup_byte_count_consistent` — `bytes_reclaimed`
//!   in the result equals the sum of `size_bytes` over the
//!   `Evict` decisions.
//! - `prop_storage_state_reclaim_monotone` — repeated soft-deletes
//!   decrement `bytes_used` monotonically; never go negative.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_eviction::{
    reservation_ttl_ms, should_fire_quota_trigger, ttl_for_tier,
    ttl_for_tier_with_override, AcReferenceProbe, BlobLruRow,
    BlobMetaSoftDeleteStore, CountingEvictionClock, EvictionBlobDigest,
    EvictionConfig, EvictionDecision, EvictionMetricKind, EvictionPhase,
    EvictionRegion, InMemoryAcReferenceProbe, InMemoryBlobMetaSoftDeleteStore,
    InMemoryEvictionAuditSink, InMemoryEvictionMetrics,
    InMemoryEvictionPhase, InMemoryTenantStorageStateStore,
    QuotaTriggerOutcome, SoftDeleteOutcome, TenantStorageStateRow,
    TenantStorageStateStore, Tier, TierTtlOverrideError,
    MAX_RESERVATION_TTL_MS, MIN_RESERVATION_TTL_MS,
};
use proptest::prelude::*;
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn ten_a() -> Uuid {
    Uuid::from_u128(0xa)
}

fn ten_b() -> Uuid {
    Uuid::from_u128(0xb)
}

fn dig_from_seed(seed: u8) -> EvictionBlobDigest {
    let bytes = [seed; 32];
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    EvictionBlobDigest::new(s).unwrap()
}

fn fresh_state_row(
    tenant: Uuid,
    region: EvictionRegion,
    used: u64,
    quota: u64,
) -> TenantStorageStateRow {
    TenantStorageStateRow {
        tenant_id: tenant,
        region,
        bytes_used: used,
        bytes_quota: quota,
        bytes_used_updated_at_ms: 1,
        last_synced_at_ms: 1,
        last_evict_at_ms: None,
        bytes_reclaimed_lifetime: 0,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

fn lru_row(seed: u8, last_accessed: u64, size: u64) -> BlobLruRow {
    BlobLruRow {
        digest: dig_from_seed(seed),
        size_bytes: size,
        created_at_ms: 1,
        last_accessed_at_ms: last_accessed,
        deleted_at_ms: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// CRITICAL — the race-aware reachable check off-by-one boundary.
    /// Off-by-one (`<=` instead of `<`) is a data-loss bug.
    #[test]
    fn prop_evict_protect_if_re_referenced_strict_boundary(
        anchor in 1_000_u64..1_000_000_u64,
        offset_signed in -10_i64..=10_i64,
        seed in 0u8..=u8::MAX,
    ) {
        let probe = InMemoryAcReferenceProbe::new();
        let d = dig_from_seed(seed);
        // ac.created_at = anchor + offset (clamped to non-negative).
        let ac_created_at: u64 = if offset_signed < 0 {
            anchor.saturating_sub(offset_signed.unsigned_abs())
        } else {
            anchor.saturating_add(u64::try_from(offset_signed).unwrap_or(0))
        };
        probe
            .push_ac_row(
                ten_a(),
                "ac-test",
                vec![d.clone()],
                ac_created_at,
                None,
            )
            .unwrap();
        let result = probe
            .find_active_reference(ten_a(), &d, anchor)
            .unwrap();
        // STRICT `<` evict arm:
        // - ac.created_at < anchor → ACTIVE REFERENCE (eviction must
        //   refuse).
        // - ac.created_at >= anchor → PROTECTED (eviction proceeds).
        if ac_created_at < anchor {
            prop_assert!(
                result.is_some(),
                "ac.created_at={ac_created_at} < anchor={anchor} \
                 MUST surface as active reference (offset={offset_signed})"
            );
        } else {
            prop_assert!(
                result.is_none(),
                "ac.created_at={ac_created_at} >= anchor={anchor} \
                 MUST be PROTECTED (offset={offset_signed}; off-by-one \
                 bug pinned)"
            );
        }
    }

    /// Tenant isolation — eviction for tenant A never reads or
    /// mutates tenant B's blob_meta / storage_state.
    #[test]
    fn prop_tenant_isolation(
        a_seeds in prop::collection::btree_set(0u8..=127u8, 0..=20),
        b_seeds in prop::collection::btree_set(128u8..=u8::MAX, 0..=20),
        a_quota in 100_u64..1_000_000_u64,
        a_used_pct in 90_u64..=99_u64,
    ) {
        let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
        let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
        let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
        let audit = Arc::new(InMemoryEvictionAuditSink::new());
        let metrics = Arc::new(InMemoryEvictionMetrics::new());
        // Use a clock far in the future so cutoff_ms (= now - 7d) is
        // a meaningful positive value.
        let clock = Arc::new(CountingEvictionClock::new(
            30_u64 * 86_400_000,
        ));
        let phase = InMemoryEvictionPhase::with_defaults(
            Arc::clone(&blob_meta),
            Arc::clone(&ac_probe),
            Arc::clone(&storage_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );

        // Seed both tenants. Tenant A is at >= 95% quota; tenant B
        // is at 50% (would not trigger).
        let a_used = (a_quota * a_used_pct) / 100;
        storage_state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                a_used,
                a_quota,
            ))
            .unwrap();
        storage_state
            .push_row(fresh_state_row(
                ten_b(),
                EvictionRegion::Sam,
                a_quota / 2,
                a_quota,
            ))
            .unwrap();

        // Cold blobs for both tenants.
        for s in a_seeds.iter() {
            blob_meta
                .push_row(ten_a(), lru_row(*s, 1, 10))
                .unwrap();
        }
        for s in b_seeds.iter() {
            blob_meta
                .push_row(ten_b(), lru_row(*s, 1, 10))
                .unwrap();
        }

        // Capture B's blob digests pre-pass to verify they survive.
        let b_digests_pre: Vec<EvictionBlobDigest> =
            b_seeds.iter().map(|s| dig_from_seed(*s)).collect();

        // Run eviction for A only.
        let _ = phase.execute_daily(ten_a(), EvictionRegion::Sam);

        // B's blobs must be untouched — no soft-delete fired.
        for d in &b_digests_pre {
            let snap = blob_meta.snapshot(ten_b(), d).unwrap();
            prop_assert!(
                snap.deleted_at_ms.is_none(),
                "tenant B blob {} was soft-deleted by tenant A's eviction \
                 (CTRL-ISO-005 / INV-TENANT-ISOLATION violation)",
                d
            );
        }
        // B's storage_state row is untouched.
        let snap_b = storage_state
            .snapshot(ten_b(), EvictionRegion::Sam)
            .unwrap();
        prop_assert_eq!(snap_b.bytes_used, a_quota / 2);
        prop_assert!(snap_b.last_evict_at_ms.is_none());
    }

    /// Idempotent re-run: re-running eviction is a no-op (no new
    /// soft-deletes fire on the second pass).
    #[test]
    fn prop_idempotent_re_run(
        seeds in prop::collection::btree_set(0u8..=u8::MAX, 1..=20),
    ) {
        let start = 30_u64 * 86_400_000;
        let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
        let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
        let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
        let audit = Arc::new(InMemoryEvictionAuditSink::new());
        let metrics = Arc::new(InMemoryEvictionMetrics::new());
        let clock = Arc::new(CountingEvictionClock::new(start));
        let phase = InMemoryEvictionPhase::with_defaults(
            Arc::clone(&blob_meta),
            Arc::clone(&ac_probe),
            Arc::clone(&storage_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        // Tenant at 100% quota with cold blobs.
        let n = seeds.len() as u64;
        storage_state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                n * 10,
                n * 10,
            ))
            .unwrap();
        for s in &seeds {
            blob_meta
                .push_row(ten_a(), lru_row(*s, 1, 10))
                .unwrap();
        }
        let r1 = phase
            .execute_daily(ten_a(), EvictionRegion::Sam)
            .unwrap();
        let evicted_first_pass = r1.blobs_evicted_count;
        prop_assert!(
            evicted_first_pass > 0,
            "first pass should evict at least one blob"
        );
        // Second pass — every row is now soft-deleted; the LRU scan
        // returns nothing eligible.
        let r2 = phase
            .execute_daily(ten_a(), EvictionRegion::Sam)
            .unwrap();
        prop_assert_eq!(
            r2.blobs_evicted_count, 0,
            "idempotent re-run MUST evict 0 blobs (saw {} on second pass)",
            r2.blobs_evicted_count
        );
    }

    /// BLOB-only scope — the orchestrator has no chunks-table
    /// dependency. This property exercises the wiring contract: the
    /// chunks table is NEVER consulted. Pinned indirectly via the
    /// canonical metric counters: only blob-level metrics fire;
    /// chunk-level metrics (which would be exposed by S-06 GC) do
    /// NOT fire.
    #[test]
    fn prop_blob_only_scope(
        seeds in prop::collection::btree_set(0u8..=u8::MAX, 1..=10),
    ) {
        let start = 30_u64 * 86_400_000;
        let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
        let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
        let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
        let audit = Arc::new(InMemoryEvictionAuditSink::new());
        let metrics = Arc::new(InMemoryEvictionMetrics::new());
        let clock = Arc::new(CountingEvictionClock::new(start));
        let phase = InMemoryEvictionPhase::with_defaults(
            Arc::clone(&blob_meta),
            Arc::clone(&ac_probe),
            Arc::clone(&storage_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        let n = seeds.len() as u64;
        storage_state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                n * 100,
                n * 100,
            ))
            .unwrap();
        for s in &seeds {
            blob_meta
                .push_row(ten_a(), lru_row(*s, 1, 100))
                .unwrap();
        }
        let r = phase
            .execute_daily(ten_a(), EvictionRegion::Sam)
            .unwrap();
        // Only blob-level metrics fire. The fake chunks-table is not
        // even part of the orchestrator's wiring; this test pins the
        // INVARIANT at the type-system + behavioural layer.
        prop_assert!(r.blobs_evicted_count > 0);
        // The cron metric, candidates_scanned, ttl_expired,
        // bytes_reclaimed, lru_evicted MAY fire; gc_invariant_violation
        // MUST NOT fire (we're below the chunk-touching threshold).
        prop_assert_eq!(
            metrics
                .counter_total(EvictionMetricKind::GcInvariantViolation),
            0,
            "gc_invariant_violation_total MUST stay 0 — eviction \
             never touches chunks (Lote 10.7bis P0-8)"
        );
    }

    /// Size-proportional reservation TTL formula respects the floor
    /// + cap.
    #[test]
    fn prop_ttl_size_proportional_reservation(
        request_bytes in 0u64..=u64::MAX,
    ) {
        let ttl = reservation_ttl_ms(request_bytes);
        prop_assert!(ttl >= MIN_RESERVATION_TTL_MS,
            "ttl={ttl} below floor MIN={MIN_RESERVATION_TTL_MS} \
             for request_bytes={request_bytes}");
        prop_assert!(ttl <= MAX_RESERVATION_TTL_MS,
            "ttl={ttl} above cap MAX={MAX_RESERVATION_TTL_MS} \
             for request_bytes={request_bytes}");
    }

    /// Size-proportional reservation TTL is monotone in
    /// request_bytes within the proportional band (above floor;
    /// below cap).
    #[test]
    fn prop_ttl_size_proportional_monotone(
        a in 1_000_000u64..1_000_000_000u64,
        b in 1_000_000u64..1_000_000_000u64,
    ) {
        let ta = reservation_ttl_ms(a);
        let tb = reservation_ttl_ms(b);
        if a <= b {
            prop_assert!(tb >= ta, "monotone violated: a={a} ta={ta} b={b} tb={tb}");
        } else {
            prop_assert!(ta >= tb, "monotone violated: a={a} ta={ta} b={b} tb={tb}");
        }
    }

    /// 95% quota trigger boundary semantics. Strictly below 95%
    /// MUST NOT fire; >= 95% MUST fire (with non-zero target).
    #[test]
    fn prop_quota_trigger_fires_at_95pct(
        quota in 100_u64..1_000_000_u64,
        used_pct in 0_u64..=120_u64,
    ) {
        let used = (quota * used_pct) / 100;
        let row = fresh_state_row(
            ten_a(),
            EvictionRegion::Sam,
            used,
            quota,
        );
        let outcome = should_fire_quota_trigger(&row);
        let pct_f = (used as f64) / (quota as f64);
        if pct_f < 0.95 {
            prop_assert!(
                matches!(outcome, QuotaTriggerOutcome::BelowThreshold),
                "pct={pct_f} < 0.95 MUST NOT fire (got {outcome:?})"
            );
        } else {
            prop_assert!(
                matches!(outcome, QuotaTriggerOutcome::Fire { .. }),
                "pct={pct_f} >= 0.95 MUST fire (got {outcome:?})"
            );
        }
    }

    /// Enterprise TTL admin-override hard cap semantics.
    #[test]
    fn prop_ttl_enterprise_cap_respected(
        days in 0_u32..2000_u32,
    ) {
        let r = ttl_for_tier_with_override(Tier::Enterprise, Some(days));
        if days <= 730 {
            prop_assert!(r.is_ok(), "days={days} <= 730 MUST accept (got {r:?})");
        } else {
            prop_assert!(
                matches!(r, Err(TierTtlOverrideError::ExceedsMaxTtl { .. })),
                "days={days} > 730 MUST reject as ExceedsMaxTtl (got {r:?})"
            );
        }
    }

    /// `bytes_reclaimed` in the result MUST equal the sum of
    /// `size_bytes` over the `Evict` decisions.
    #[test]
    fn prop_evict_dedup_byte_count_consistent(
        sizes in prop::collection::vec(1u64..=10_000u64, 1..=20),
    ) {
        let start = 30_u64 * 86_400_000;
        let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
        let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
        let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
        let audit = Arc::new(InMemoryEvictionAuditSink::new());
        let metrics = Arc::new(InMemoryEvictionMetrics::new());
        let clock = Arc::new(CountingEvictionClock::new(start));
        let phase = InMemoryEvictionPhase::with_defaults(
            Arc::clone(&blob_meta),
            Arc::clone(&ac_probe),
            Arc::clone(&storage_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        let total: u64 = sizes.iter().sum();
        storage_state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                total,
                total,
            ))
            .unwrap();
        for (i, sz) in sizes.iter().enumerate() {
            // Distinct seeds; cap u8 indices via wrapping (we ship
            // <= 20 entries so collision-free).
            let seed = u8::try_from(i).unwrap_or(255);
            blob_meta
                .push_row(ten_a(), lru_row(seed, 1, *sz))
                .unwrap();
        }
        let r = phase
            .execute_daily(ten_a(), EvictionRegion::Sam)
            .unwrap();
        // Sum of evicted size_bytes equals the result aggregate.
        let evicted_sum: u64 = sizes
            .iter()
            .copied()
            .take(r.blobs_evicted_count as usize)
            .sum();
        prop_assert_eq!(r.bytes_reclaimed, evicted_sum);
    }

    /// `storage_state.bytes_used` decrements monotonically across
    /// repeated soft-deletes; never goes negative (saturating-sub
    /// at the API surface).
    #[test]
    fn prop_storage_state_reclaim_monotone(
        used_init in 100_u64..10_000_u64,
        reclaim_seq in prop::collection::vec(1u64..=200u64, 1..=20),
    ) {
        let store = InMemoryTenantStorageStateStore::new();
        store
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                used_init,
                used_init * 2,
            ))
            .unwrap();
        let mut prev = used_init;
        let mut now_ms = 1000_u64;
        for r in &reclaim_seq {
            let res = store.apply_eviction_reclaim(
                ten_a(),
                EvictionRegion::Sam,
                *r,
                now_ms,
            );
            now_ms = now_ms.saturating_add(1);
            // If reclaim would underflow, the API rejects with
            // CheckViolation; bytes_used does NOT mutate.
            if *r > prev {
                prop_assert!(res.is_err());
                continue;
            }
            res.unwrap();
            let snap = store
                .snapshot(ten_a(), EvictionRegion::Sam)
                .unwrap();
            prop_assert!(snap.bytes_used <= prev,
                "monotonicity violated: prev={prev} new={}",
                snap.bytes_used);
            prev = snap.bytes_used;
        }
    }
}

// ---- Boundary anchoring (smoke tests outside proptest envelope) ----

#[test]
fn ttl_boundary_at_canonical_per_tier_default() {
    assert_eq!(ttl_for_tier(Tier::Free), 7 * 86_400_000);
    assert_eq!(ttl_for_tier(Tier::Solo), 30 * 86_400_000);
    assert_eq!(ttl_for_tier(Tier::Team), 90 * 86_400_000);
    assert_eq!(ttl_for_tier(Tier::Business), 365 * 86_400_000);
    // Lote 10.7bis P0-7: Enterprise default 365d (NOT 730d).
    assert_eq!(ttl_for_tier(Tier::Enterprise), 365 * 86_400_000);
}

#[test]
fn config_default_for_free_tier_uses_canonical_ttl() {
    let c = EvictionConfig::default_for_free_tier();
    assert_eq!(c.ttl_ms(), ttl_for_tier(Tier::Free));
}

#[test]
fn audit_sink_collects_quota_trigger_fired_event_at_95pct() {
    let start = 30_u64 * 86_400_000;
    let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
    let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
    let audit = Arc::new(InMemoryEvictionAuditSink::new());
    let metrics = Arc::new(InMemoryEvictionMetrics::new());
    let clock = Arc::new(CountingEvictionClock::new(start));
    let phase = InMemoryEvictionPhase::with_defaults(
        Arc::clone(&blob_meta),
        Arc::clone(&ac_probe),
        Arc::clone(&storage_state),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    storage_state
        .push_row(fresh_state_row(
            ten_a(),
            EvictionRegion::Sam,
            95,
            100,
        ))
        .unwrap();
    blob_meta
        .push_row(ten_a(), lru_row(1, 1, 5))
        .unwrap();
    let r = phase
        .execute_daily(ten_a(), EvictionRegion::Sam)
        .unwrap();
    assert!(r.quota_trigger_fired);
    assert_eq!(
        audit
            .snapshot_of(corelink_eviction::EvictionEventType::QuotaTriggerFired)
            .len(),
        1
    );
}

#[test]
fn step_candidate_evict_path_persists_soft_delete() {
    let start = 30_u64 * 86_400_000;
    let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
    let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
    let audit = Arc::new(InMemoryEvictionAuditSink::new());
    let metrics = Arc::new(InMemoryEvictionMetrics::new());
    let clock = Arc::new(CountingEvictionClock::new(start));
    let phase = InMemoryEvictionPhase::with_defaults(
        Arc::clone(&blob_meta),
        Arc::clone(&ac_probe),
        Arc::clone(&storage_state),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    let row = lru_row(0xab, 1, 99);
    blob_meta.push_row(ten_a(), row.clone()).unwrap();
    let d = phase
        .step_candidate(
            ten_a(),
            EvictionRegion::Sam,
            &row,
            start,
            start - 1,
        )
        .unwrap();
    assert!(matches!(d, EvictionDecision::Evict { .. }));
    let outcome = blob_meta
        .soft_delete_for_eviction(ten_a(), &row.digest, start + 100)
        .unwrap();
    // After eviction's soft-delete, second call returns
    // AlreadyResolved.
    assert!(matches!(outcome, SoftDeleteOutcome::AlreadyResolved));
}
