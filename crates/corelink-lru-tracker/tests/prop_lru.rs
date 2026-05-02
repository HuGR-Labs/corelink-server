//! Property tests pinning the load-bearing invariants of
//! `corelink-lru-tracker` at 10k iterations per check (PR-gate; nightly
//! 100k via env var override).
//!
//! Coverage map (per WI-S07-004 §6.1.10 + spec contract §7.5 + §10.s07.5):
//!
//! - `prop_record_then_flush_persists_latest` — for any sequence of
//!   records on the same digest, flush results in the latest
//!   `accessed_at_ms` wins (round-trip with eviction's `BlobMetaSoftDeleteStore`).
//!   This is the cross-WI integration with WI-S07-002.
//! - `prop_coalescing_reduces_write_amplification` — N records on the
//!   same `(tenant, digest)` pre-flush yield ≤ 1 D1 UPDATE on flush.
//! - `prop_tenant_isolation` — tenant A records NEVER affect tenant B
//!   blob_meta (CTRL-ISO-005).
//! - `prop_overflow_drops_oldest` — queue at cap → oldest dropped,
//!   newest preserved (FIFO).
//! - `prop_drift_metric_monotone_or_zero` — drift_ms after flush is 0
//!   (queue drained) OR strictly less than before.
//! - `prop_consistency_violation_detected_when_drift_exceeds_threshold`
//!   — per-row UPDATE catching up by more than the threshold fires the
//!   violation arm (R-S07-3 + spec contract §6 DoD).
//! - `prop_audit_emit_per_decision_arm` — every Recorded / Coalesced
//!   decision emits exactly one canonical audit record.
//! - `prop_idempotent_flush` — flushing an empty queue is a no-op.
//! - `prop_boundary_values` — queue cap-1 / cap / cap+1; drift at
//!   59s/60s/61s; batch_size at 250.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_eviction::{
    BlobLruRow, EvictionBlobDigest, EvictionRegion, InMemoryBlobMetaSoftDeleteStore,
};
use corelink_lru_tracker::{
    canonical_audit_event_strings, canonical_metric_names, FrozenLruClock,
    InMemoryLruAuditSink, InMemoryLruMetrics, InMemoryLruTracker, LruConfig,
    LruEventType, LruMetricKind, LruTracker,
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

fn dig(seed: u8) -> EvictionBlobDigest {
    let bytes = [seed; 32];
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    EvictionBlobDigest::new(s).unwrap()
}

fn row(seed: u8, last_accessed_ms: u64) -> BlobLruRow {
    BlobLruRow {
        digest: dig(seed),
        size_bytes: 1,
        created_at_ms: 1,
        last_accessed_at_ms: last_accessed_ms,
        deleted_at_ms: None,
    }
}

type Tracker = InMemoryLruTracker<
    InMemoryBlobMetaSoftDeleteStore,
    InMemoryLruAuditSink,
    InMemoryLruMetrics,
    FrozenLruClock,
>;

type Fixture = (
    Tracker,
    Arc<InMemoryBlobMetaSoftDeleteStore>,
    Arc<InMemoryLruAuditSink>,
    Arc<InMemoryLruMetrics>,
);

fn fresh_at(now_ms: u64) -> Fixture {
    let blob = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let audit = Arc::new(InMemoryLruAuditSink::new());
    let metrics = Arc::new(InMemoryLruMetrics::new());
    let clock = Arc::new(FrozenLruClock::new(now_ms));
    let tracker = InMemoryLruTracker::with_defaults(
        Arc::clone(&blob),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    (tracker, blob, audit, metrics)
}

fn fresh_with_config(now_ms: u64, config: LruConfig) -> Fixture {
    let blob = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let audit = Arc::new(InMemoryLruAuditSink::new());
    let metrics = Arc::new(InMemoryLruMetrics::new());
    let clock = Arc::new(FrozenLruClock::new(now_ms));
    let tracker = InMemoryLruTracker::new(
        Arc::clone(&blob),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
        config,
    );
    (tracker, blob, audit, metrics)
}

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 4);
    assert!(s.contains(&"corelink.lru.access_recorded"));
    assert!(s.contains(&"corelink.lru.batch_flushed"));
    assert!(s.contains(&"corelink.lru.batch_failed"));
    assert!(s.contains(&"corelink.lru.consistency_violation_detected"));
}

#[test]
fn canonical_metric_names_pinned() {
    let m = canonical_metric_names();
    assert_eq!(m.len(), 6);
    assert!(m.contains(&"corelink.lru.records_total"));
    assert!(m.contains(&"corelink.lru.coalesced_total"));
    assert!(m.contains(&"corelink.lru.dropped_total"));
    assert!(m.contains(&"corelink.lru.batch_flush_duration_ms"));
    assert!(m.contains(&"corelink.lru.drift_ms"));
    assert!(m.contains(&"corelink.lru.consistency_violation_total"));
}

#[test]
fn lru_config_canonical_constants() {
    let c = LruConfig::canonical();
    assert_eq!(c.queue_size_max(), 10_000);
    assert_eq!(c.batch_size(), 250);
    assert_eq!(c.flush_interval_ms(), 30_000);
    assert_eq!(c.drift_violation_threshold_ms(), 60_000);
    assert_eq!(c.refresh_threshold_ms(), 60_000);
}

// ---- Boundary cases ------------------------------------------------

#[test]
fn boundary_queue_cap_minus_one_does_not_drop() {
    let cfg = LruConfig::with_overrides(3, 3, 5_000, 60_000, 60_000).unwrap();
    let (tracker, _b, _a, metrics) = fresh_with_config(1000, cfg);
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(2), 1001)
        .unwrap();
    // Queue at cap-1 (2 of 3); next NEW key fits without drop.
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(3), 1002)
        .unwrap();
    assert_eq!(tracker.queue_size(), 3);
    assert_eq!(metrics.counter_total(LruMetricKind::DroppedTotal), 0);
}

#[test]
fn boundary_queue_cap_plus_one_drops_one() {
    let cfg = LruConfig::with_overrides(3, 3, 5_000, 60_000, 60_000).unwrap();
    let (tracker, _b, _a, metrics) = fresh_with_config(1000, cfg);
    for i in 1_u8..=3 {
        tracker
            .record_access(
                ten_a(),
                EvictionRegion::Sam,
                &dig(i),
                1000 + u64::from(i),
            )
            .unwrap();
    }
    // 4th NEW key triggers drop-oldest.
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(4), 1004)
        .unwrap();
    assert_eq!(tracker.queue_size(), 3);
    assert_eq!(metrics.counter_total(LruMetricKind::DroppedTotal), 1);
}

#[test]
fn boundary_drift_at_threshold_does_not_fire_violation() {
    let cfg = LruConfig::with_overrides(100, 50, 5_000, 60_000, 100).unwrap();
    let (tracker, blob, _a, metrics) = fresh_with_config(60_000, cfg);
    blob.push_row(ten_a(), row(1, 0)).unwrap();
    // Lag = 60_000 - 0 = 60_000 ms == threshold; the guard fires only
    // when `lag > threshold`. Boundary inclusive at threshold.
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 60_000)
        .unwrap();
    let r = tracker.flush_batch(60_000).unwrap();
    assert_eq!(r.consistency_violations, 0);
    assert_eq!(
        metrics.counter_total(LruMetricKind::ConsistencyViolationTotal),
        0
    );
}

#[test]
fn boundary_drift_one_ms_above_threshold_fires_violation() {
    let cfg = LruConfig::with_overrides(100, 50, 5_000, 60_000, 100).unwrap();
    let (tracker, blob, _a, metrics) = fresh_with_config(60_001, cfg);
    blob.push_row(ten_a(), row(1, 0)).unwrap();
    // Lag = 60_001 - 0 = 60_001 ms; one ms above threshold.
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 60_001)
        .unwrap();
    let r = tracker.flush_batch(60_001).unwrap();
    assert!(r.consistency_violations >= 1);
    assert!(
        metrics.counter_total(LruMetricKind::ConsistencyViolationTotal) >= 1
    );
}

#[test]
fn boundary_batch_size_250_caps_drain() {
    let cfg = LruConfig::with_overrides(500, 250, 5_000, 60_000, 100).unwrap();
    let (tracker, blob, _a, _m) = fresh_with_config(1000, cfg);
    for i in 0_u8..251 {
        blob.push_row(ten_a(), row(i, 1)).unwrap();
        tracker
            .record_access(
                ten_a(),
                EvictionRegion::Sam,
                &dig(i),
                1000 + u64::from(i),
            )
            .unwrap();
    }
    assert_eq!(tracker.queue_size(), 251);
    let r = tracker.flush_batch(2000).unwrap();
    assert_eq!(r.rows_drained, 250);
    assert_eq!(tracker.queue_size(), 1);
}

// ---- Property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Round-trip: for any sequence of strictly-increasing `accessed_at_ms`
    /// records on the same `(tenant, digest)`, the flush persists the
    /// LATEST sample to blob_meta.
    #[test]
    fn prop_record_then_flush_persists_latest(
        n in 1_u64..50,
        start in 1_u64..1_000_000,
        // Use a small refresh window so each record overwrites the
        // previous (out-of-window).
        gap in 1000_u64..10_000,
    ) {
        let cfg = LruConfig::with_overrides(
            10_000, 250, 5_000, 60_000, /* refresh */ 100,
        )
        .unwrap();
        let (tracker, blob, _a, _m) =
            fresh_with_config(start + n * gap + 1, cfg);
        blob.push_row(ten_a(), row(1, start - 1)).unwrap();
        for i in 0..n {
            // The frozen clock returns the same now_ms for every call;
            // out-of-window dedup short-circuit only applies when the
            // existing entry is within refresh_threshold_ms of NOW.
            // To force overwrites, we use accessed_at_ms strictly
            // greater than existing at every step.
            let access = start + i * gap;
            tracker
                .record_access(
                    ten_a(),
                    EvictionRegion::Sam,
                    &dig(1),
                    access,
                )
                .unwrap();
        }
        // Flush.
        let _ = tracker
            .flush_batch(start + n * gap + 1)
            .unwrap();
        let r = blob.snapshot(ten_a(), &dig(1)).unwrap();
        // The LATEST sample wins.
        let expected_latest = start + (n - 1) * gap;
        prop_assert_eq!(r.last_accessed_at_ms, expected_latest);
    }

    /// Coalescing: N records on the same `(tenant, digest)` collapse
    /// to ≤ 1 D1 UPDATE on flush.
    #[test]
    fn prop_coalescing_reduces_write_amplification(
        n in 2_u64..200,
    ) {
        let (tracker, blob, _a, _m) = fresh_at(1000);
        blob.push_row(ten_a(), row(1, 1)).unwrap();
        for i in 0..n {
            tracker
                .record_access(
                    ten_a(),
                    EvictionRegion::Sam,
                    &dig(1),
                    1000 + i,
                )
                .unwrap();
        }
        // Queue still has 1 entry (coalesced).
        prop_assert_eq!(tracker.queue_size(), 1);
        // Flush results in exactly 1 row drained.
        let r = tracker.flush_batch(2000).unwrap();
        prop_assert_eq!(r.rows_drained, 1);
    }

    /// Tenant isolation: tenant A records NEVER affect tenant B
    /// blob_meta.
    #[test]
    fn prop_tenant_isolation(
        a_access in 100_u64..1_000_000,
        b_access_initial in 100_u64..1_000_000,
    ) {
        let (tracker, blob, _a, _m) = fresh_at(2_000_000);
        blob.push_row(ten_a(), row(1, 1)).unwrap();
        blob.push_row(ten_b(), row(1, b_access_initial)).unwrap();
        tracker
            .record_access(ten_a(), EvictionRegion::Sam, &dig(1), a_access)
            .unwrap();
        tracker.flush_batch(2_000_000).unwrap();
        let r_a = blob.snapshot(ten_a(), &dig(1)).unwrap();
        let r_b = blob.snapshot(ten_b(), &dig(1)).unwrap();
        // Tenant A advanced (or held — depends on whether a_access > 1).
        // Tenant B is UNCHANGED by tenant A's records.
        prop_assert_eq!(r_b.last_accessed_at_ms, b_access_initial);
        // Tenant A's row reflects the recorded access if newer.
        if a_access > 1 {
            prop_assert_eq!(r_a.last_accessed_at_ms, a_access);
        }
    }

    /// Overflow: queue at cap; new key drops the OLDEST FIFO entry.
    /// The newest entry is preserved.
    #[test]
    fn prop_overflow_drops_oldest(
        cap in 2_usize..20,
        extra in 1_u8..30,
    ) {
        let cfg = LruConfig::with_overrides(
            cap, cap, 5_000, 60_000, /* refresh */ 100,
        )
        .unwrap();
        let (tracker, _b, _a, metrics) =
            fresh_with_config(1000, cfg);
        // Insert cap + extra distinct keys (all u8 distinct so cap+extra <= 256).
        let total = cap.saturating_add(usize::from(extra));
        prop_assume!(total <= 250);
        for i in 0..total {
            let seed = u8::try_from(i).unwrap_or(0);
            tracker
                .record_access(
                    ten_a(),
                    EvictionRegion::Sam,
                    &dig(seed),
                    1000 + i as u64,
                )
                .unwrap();
        }
        prop_assert_eq!(tracker.queue_size(), cap);
        // Total drops = extra (each new beyond cap drops one).
        prop_assert_eq!(
            metrics.counter_total(LruMetricKind::DroppedTotal),
            u64::from(extra)
        );
        // Newest entry IS in the queue.
        let newest_seed = u8::try_from(total - 1).unwrap_or(0);
        prop_assert!(tracker
            .buffered_accessed_at_ms(
                ten_a(),
                EvictionRegion::Sam,
                &dig(newest_seed)
            )
            .is_some());
        // Oldest entry IS NOT in the queue.
        prop_assert!(tracker
            .buffered_accessed_at_ms(
                ten_a(),
                EvictionRegion::Sam,
                &dig(0)
            )
            .is_none());
    }

    /// Drift metric: after a flush that drains the queue, drift_ms
    /// records 0 (queue empty) OR the observed value before drain.
    /// Either way, after flush + before next record, queue is empty
    /// so a second flush observes drift = 0.
    #[test]
    fn prop_drift_metric_monotone_or_zero(
        access in 1_u64..1_000,
        delta in 100_u64..10_000,
    ) {
        let (tracker, blob, _a, metrics) = fresh_at(access + delta);
        blob.push_row(ten_a(), row(1, 0)).unwrap();
        tracker
            .record_access(ten_a(), EvictionRegion::Sam, &dig(1), access)
            .unwrap();
        let r1 = tracker.flush_batch(access + delta).unwrap();
        // First flush observes drift = delta.
        prop_assert_eq!(r1.drift_ms, delta);
        // Second flush (no new records) observes drift = 0.
        let r2 = tracker.flush_batch(access + delta + 1).unwrap();
        prop_assert_eq!(r2.drift_ms, 0);
        // Histogram captured both.
        let samples = metrics.drift_samples();
        prop_assert!(samples.len() >= 2);
        prop_assert!(samples.contains(&0));
    }

    /// Per-row consistency violation detected when prev → new lag
    /// exceeds the configured drift threshold.
    #[test]
    fn prop_consistency_violation_detected_when_drift_exceeds_threshold(
        prev in 1_u64..1000,
        // pick a `new` strictly greater than prev + threshold so the
        // per-row guard fires. threshold = 1000.
        gap_above_threshold in 1_u64..10_000,
    ) {
        let cfg = LruConfig::with_overrides(
            100,
            50,
            5_000,
            /* threshold */ 1000,
            /* refresh */ 100,
        )
        .unwrap();
        let new = prev + 1000 + gap_above_threshold;
        let (tracker, blob, _a, metrics) =
            fresh_with_config(new + 1, cfg);
        blob.push_row(ten_a(), row(1, prev)).unwrap();
        tracker
            .record_access(ten_a(), EvictionRegion::Sam, &dig(1), new)
            .unwrap();
        let r = tracker.flush_batch(new + 1).unwrap();
        prop_assert!(r.consistency_violations >= 1);
        prop_assert!(
            metrics
                .counter_total(LruMetricKind::ConsistencyViolationTotal)
                >= 1
        );
    }

    /// Audit emit per decision arm: every Recorded / Coalesced
    /// decision emits exactly one canonical AccessRecorded record.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        n in 1_u64..50,
        access_step in 1_u64..200,
    ) {
        let (tracker, _b, audit, _m) = fresh_at(1_000_000);
        for i in 0..n {
            tracker
                .record_access(
                    ten_a(),
                    EvictionRegion::Sam,
                    &dig(1),
                    1000 + i * access_step,
                )
                .unwrap();
        }
        let emitted =
            audit.snapshot_of(LruEventType::AccessRecorded).len();
        prop_assert_eq!(emitted, n as usize);
    }

    /// Idempotent flush: flushing an empty queue is a no-op (one
    /// audit, drift=0).
    #[test]
    fn prop_idempotent_flush(
        n in 1_u64..20,
    ) {
        let (tracker, _b, audit, metrics) = fresh_at(1000);
        for _ in 0..n {
            let r = tracker.flush_batch(1000).unwrap();
            prop_assert_eq!(r.rows_drained, 0);
            prop_assert_eq!(r.drift_ms, 0);
        }
        prop_assert_eq!(
            audit.snapshot_of(LruEventType::BatchFlushed).len(),
            n as usize
        );
        // Every drift sample is 0.
        let samples = metrics.drift_samples();
        for s in samples {
            prop_assert_eq!(s, 0);
        }
    }
}
