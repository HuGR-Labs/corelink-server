#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
use std::sync::Arc;

use uuid::Uuid;

use corelink_eviction::{
    BlobLruRow, EvictionBlobDigest, EvictionRegion, InMemoryBlobMetaSoftDeleteStore,
};

use crate::lru_tracker::audit::{FailingLruAuditSink, InMemoryLruAuditSink};
use crate::lru_tracker::clock::{CountingLruClock, FrozenLruClock};
use crate::lru_tracker::metrics::{InMemoryLruMetrics, LruMetricKind};
use crate::lru_tracker::{
    InMemoryLruTracker, LruConfig, LruDecision, LruError, LruEventType, LruFlushResult, LruTracker,
};

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

// ---- record_access basic surface --------------------------------

#[test]
fn record_access_inserts_new_entry() {
    let (tracker, _b, audit, metrics) = fresh_at(1000);
    let dec = tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    assert!(matches!(
        dec,
        LruDecision::Recorded {
            was_existing: false,
            ..
        }
    ));
    assert_eq!(tracker.queue_size(), 1);
    assert_eq!(audit.snapshot_of(LruEventType::AccessRecorded).len(), 1);
    assert_eq!(metrics.counter_total(LruMetricKind::RecordsTotal), 1);
}

#[test]
fn record_access_within_refresh_window_coalesces() {
    // Default refresh_threshold_ms = 60_000.
    let (tracker, _b, audit, metrics) = fresh_at(1000);
    // T0: enqueue at accessed_ms=1000 (queue holds entry at 1000).
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    // T1: 30s later (now - existing = 30_000 < 60_000) — coalesce.
    let clock = Arc::new(FrozenLruClock::new(1000 + 30_000));
    let blob = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let audit2 = Arc::new(InMemoryLruAuditSink::new());
    let metrics2 = Arc::new(InMemoryLruMetrics::new());
    // We can't swap clocks on the same tracker easily; use a
    // separate tracker constructed with the advanced clock and
    // pre-seeded queue via record then re-record approach.
    let tracker2 = InMemoryLruTracker::with_defaults(blob, audit2, metrics2, clock);
    // Pre-seed at the older clock value implicitly by recording
    // first.
    // Instead, exercise on a single tracker by stepping the clock
    // via CountingLruClock.
    let _ = tracker2;
    drop(audit);
    drop(metrics);

    // Cleaner: use CountingLruClock with set_to.
    let clock = Arc::new(CountingLruClock::new(1000));
    let blob = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let audit = Arc::new(InMemoryLruAuditSink::new());
    let metrics = Arc::new(InMemoryLruMetrics::new());
    let t = InMemoryLruTracker::with_defaults(
        Arc::clone(&blob),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&clock),
    );
    t.record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    clock.set_to(1000 + 30_000);
    let dec = t
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 31_000)
        .unwrap();
    assert!(matches!(dec, LruDecision::Coalesced { .. }));
    // Coalesced metric incremented.
    assert_eq!(metrics.counter_total(LruMetricKind::CoalescedTotal), 1);
    // RecordsTotal incremented only for the FIRST record.
    assert_eq!(metrics.counter_total(LruMetricKind::RecordsTotal), 1);
}

#[test]
fn record_access_after_refresh_window_overwrites_with_coalesce() {
    let clock = Arc::new(CountingLruClock::new(1000));
    let blob = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let audit = Arc::new(InMemoryLruAuditSink::new());
    let metrics = Arc::new(InMemoryLruMetrics::new());
    let t = InMemoryLruTracker::with_defaults(
        Arc::clone(&blob),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&clock),
    );
    t.record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    // Step BEYOND refresh window (60s + 1ms).
    clock.set_to(1000 + 60_001);
    let dec = t
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 61_001)
        .unwrap();
    assert!(matches!(
        dec,
        LruDecision::Recorded {
            was_existing: true,
            ..
        }
    ));
    // The `was_existing=true` arm bumps coalesced + records totals.
    assert_eq!(metrics.counter_total(LruMetricKind::CoalescedTotal), 1);
    assert_eq!(metrics.counter_total(LruMetricKind::RecordsTotal), 2);
    assert_eq!(t.queue_size(), 1);
    assert_eq!(
        t.buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(1)),
        Some(61_001)
    );
}

#[test]
fn record_access_out_of_order_coalesces() {
    let (tracker, _b, _a, metrics) = fresh_at(1000);
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5000)
        .unwrap();
    // Out-of-order (older sample) → coalesce.
    let dec = tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 4000)
        .unwrap();
    assert!(matches!(dec, LruDecision::Coalesced { .. }));
    assert_eq!(metrics.counter_total(LruMetricKind::CoalescedTotal), 1);
    // Queue still has the LATER sample.
    assert_eq!(
        tracker.buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(1)),
        Some(5000)
    );
}

#[test]
fn record_access_audit_failure_does_not_mutate_queue() {
    let blob = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let audit = Arc::new(FailingLruAuditSink::new());
    let metrics = Arc::new(InMemoryLruMetrics::new());
    let clock = Arc::new(FrozenLruClock::new(1000));
    let t = InMemoryLruTracker::with_defaults(
        Arc::clone(&blob),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    let err = t
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap_err();
    assert!(matches!(err, LruError::Audit(_)));
    assert_eq!(t.queue_size(), 0);
}

// ---- queue overflow / drop-oldest -------------------------------

#[test]
fn record_access_overflow_drops_oldest() {
    let cfg = LruConfig::with_overrides(2, 2, 5_000, 60_000, 60_000).unwrap();
    let (tracker, _b, _a, metrics) = fresh_with_config(1000, cfg);
    // Fill queue to cap (2).
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(2), 1001)
        .unwrap();
    // Third record should drop the OLDEST (digest 1).
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(3), 1002)
        .unwrap();
    assert_eq!(tracker.queue_size(), 2);
    // Oldest (dig 1) is dropped; dig 2 + dig 3 remain.
    assert!(tracker
        .buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(1))
        .is_none());
    assert!(tracker
        .buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(2))
        .is_some());
    assert!(tracker
        .buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(3))
        .is_some());
    // Dropped metric fired once with reason=queue_full.
    assert_eq!(metrics.counter_total(LruMetricKind::DroppedTotal), 1);
}

#[test]
fn record_access_overflow_existing_key_does_not_drop() {
    let cfg = LruConfig::with_overrides(2, 2, 5_000, 60_000, 60_000).unwrap();
    let (tracker, _b, _a, metrics) = fresh_with_config(1000, cfg);
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(2), 1001)
        .unwrap();
    // Re-record dig(1) within refresh window → coalesces (no
    // overflow drop).
    let dec = tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1500)
        .unwrap();
    assert!(matches!(dec, LruDecision::Coalesced { .. }));
    assert_eq!(tracker.queue_size(), 2);
    assert_eq!(metrics.counter_total(LruMetricKind::DroppedTotal), 0);
}

// ---- flush_batch ------------------------------------------------

#[test]
fn flush_batch_empty_queue_is_noop() {
    let (tracker, _b, audit, metrics) = fresh_at(1000);
    let r = tracker.flush_batch(1000).unwrap();
    assert_eq!(r.rows_drained, 0);
    assert_eq!(r.drift_ms, 0);
    // Audit fires for the BatchFlushed envelope (one per call).
    assert_eq!(audit.snapshot_of(LruEventType::BatchFlushed).len(), 1);
    // Histograms record one zero-sample.
    assert_eq!(metrics.drift_samples(), vec![0]);
}

#[test]
fn flush_batch_writes_through_to_blob_meta() {
    let (tracker, blob, audit, _m) = fresh_at(1000);
    // Pre-seed blob_meta with the row.
    blob.push_row(ten_a(), row(1, 100)).unwrap();
    // Record access at later instant.
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5000)
        .unwrap();
    let r = tracker.flush_batch(5_000).unwrap();
    assert_eq!(r.rows_drained, 1);
    assert_eq!(r.rows_updated, 1);
    assert_eq!(tracker.queue_size(), 0);
    // Verify the underlying row advanced.
    let r2 = blob.snapshot(ten_a(), &dig(1)).unwrap();
    assert_eq!(r2.last_accessed_at_ms, 5000);
    // BatchFlushed audit fired.
    assert_eq!(audit.snapshot_of(LruEventType::BatchFlushed).len(), 1);
}

#[test]
fn flush_batch_skipped_when_blob_already_newer() {
    let (tracker, blob, _a, _m) = fresh_at(1000);
    // Pre-seed blob_meta with last_accessed_at_ms = 6000 (NEWER
    // than the recorded access).
    blob.push_row(ten_a(), row(1, 6000)).unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5000)
        .unwrap();
    let r = tracker.flush_batch(5_000).unwrap();
    assert_eq!(r.rows_drained, 1);
    assert_eq!(r.rows_skipped, 1);
    // No consistency violation (the LAG of 5000 is in the OPPOSITE
    // direction — blob is AHEAD of recorded access; this is the
    // out-of-order write case, not a flush failure).
    assert_eq!(r.consistency_violations, 0);
}

#[test]
fn flush_batch_per_row_violation_when_prev_to_new_lag_exceeds_threshold() {
    // Custom config with small drift threshold so the per-row
    // guard fires.
    let cfg = LruConfig::with_overrides(100, 50, 5_000, 1_000, /* refresh */ 100).unwrap();
    let (tracker, blob, audit, metrics) = fresh_with_config(100_000, cfg);
    // Pre-seed blob_meta with an OLD value (100). The recorded
    // access at 5_000 advances by 4_900 — more than the
    // 1_000-ms drift threshold; per-row guard fires.
    blob.push_row(ten_a(), row(1, 100)).unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5_000)
        .unwrap();
    let r = tracker.flush_batch(5_000).unwrap();
    assert_eq!(r.rows_updated, 1);
    assert!(
        r.consistency_violations >= 1,
        "expected >= 1 consistency violation, got {}",
        r.consistency_violations
    );
    // Audit + metric counter both reflect the violation.
    assert!(!audit
        .snapshot_of(LruEventType::ConsistencyViolationDetected)
        .is_empty());
    assert!(metrics.counter_total(LruMetricKind::ConsistencyViolationTotal) >= 1);
}

#[test]
fn flush_batch_per_flush_watermark_violation_when_oldest_too_old() {
    // Custom config with small drift threshold so the per-flush
    // watermark guard fires when the oldest pending entry has
    // lingered.
    let cfg = LruConfig::with_overrides(100, 50, 5_000, 1_000, /* refresh */ 100).unwrap();
    let (tracker, _blob, audit, metrics) = fresh_with_config(100_000, cfg);
    // Record at very old accessed_ms = 100 (no row in blob_meta).
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(99), 100)
        .unwrap();
    // Flush at now = 100_000; drift = 99_900 ms; threshold = 1_000.
    let r = tracker.flush_batch(100_000).unwrap();
    assert!(r.consistency_violations >= 1);
    assert!(!audit
        .snapshot_of(LruEventType::ConsistencyViolationDetected)
        .is_empty());
    assert!(metrics.counter_total(LruMetricKind::ConsistencyViolationTotal) >= 1);
}

#[test]
fn flush_batch_no_violation_when_drift_under_threshold() {
    // Default config (60_000 ms threshold).
    let (tracker, blob, _audit, metrics) = fresh_at(1000);
    blob.push_row(ten_a(), row(1, 4900)).unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5_000)
        .unwrap();
    let r = tracker.flush_batch(5_000).unwrap();
    assert_eq!(r.consistency_violations, 0);
    assert_eq!(
        metrics.counter_total(LruMetricKind::ConsistencyViolationTotal),
        0
    );
}

#[test]
fn flush_batch_already_resolved_when_row_absent() {
    let (tracker, _b, _a, _m) = fresh_at(1000);
    // Record without seeding blob_meta.
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(99), 5000)
        .unwrap();
    let r = tracker.flush_batch(5_000).unwrap();
    assert_eq!(r.rows_drained, 1);
    assert_eq!(r.rows_already_resolved, 1);
}

#[test]
fn flush_batch_drift_ms_observed() {
    let (tracker, blob, _a, metrics) = fresh_at(1000);
    blob.push_row(ten_a(), row(1, 1)).unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 100)
        .unwrap();
    // Flush at now=5_000; oldest_pending_access_ms = 100; drift =
    // 4_900.
    let r = tracker.flush_batch(5_000).unwrap();
    assert_eq!(r.drift_ms, 4_900);
    assert_eq!(metrics.drift_samples(), vec![4_900]);
}

#[test]
fn flush_batch_respects_batch_size_cap() {
    let cfg = LruConfig::with_overrides(100, 2, 5_000, 60_000, 60_000).unwrap();
    let (tracker, blob, _a, _m) = fresh_with_config(1000, cfg);
    for i in 1..=5 {
        blob.push_row(ten_a(), row(i, 1)).unwrap();
        tracker
            .record_access(ten_a(), EvictionRegion::Sam, &dig(i), 1000 + u64::from(i))
            .unwrap();
    }
    let r = tracker.flush_batch(2000).unwrap();
    // Only batch_size (2) drained; remaining 3 left in queue.
    assert_eq!(r.rows_drained, 2);
    assert_eq!(tracker.queue_size(), 3);
}

#[test]
fn flush_batch_fifo_order_preserved() {
    let cfg = LruConfig::with_overrides(100, 2, 5_000, 60_000, 60_000).unwrap();
    let (tracker, blob, _a, _m) = fresh_with_config(1000, cfg);
    for i in 1..=5 {
        blob.push_row(ten_a(), row(i, 1)).unwrap();
        tracker
            .record_access(ten_a(), EvictionRegion::Sam, &dig(i), 1000 + u64::from(i))
            .unwrap();
    }
    // First flush drains dig(1) + dig(2).
    tracker.flush_batch(2000).unwrap();
    // dig(1) should be persisted; dig(3..=5) still queued.
    assert!(tracker
        .buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(1))
        .is_none());
    assert!(tracker
        .buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(2))
        .is_none());
    assert!(tracker
        .buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(3))
        .is_some());
}

// ---- tenant isolation -------------------------------------------

#[test]
fn tenant_isolation_record_access() {
    let (tracker, blob, _a, _m) = fresh_at(1000);
    blob.push_row(ten_a(), row(1, 100)).unwrap();
    blob.push_row(ten_b(), row(1, 100)).unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5000)
        .unwrap();
    // Tenant B's queue entry is absent.
    assert!(tracker
        .buffered_accessed_at_ms(ten_b(), EvictionRegion::Sam, &dig(1))
        .is_none());
    tracker.flush_batch(5_000).unwrap();
    // Tenant A's row advanced; tenant B's row UNCHANGED.
    let a = blob.snapshot(ten_a(), &dig(1)).unwrap();
    let b = blob.snapshot(ten_b(), &dig(1)).unwrap();
    assert_eq!(a.last_accessed_at_ms, 5000);
    assert_eq!(b.last_accessed_at_ms, 100);
}

// ---- buffered_accessed_at_ms surface ---------------------------

#[test]
fn buffered_accessed_at_ms_returns_latest_record() {
    let (tracker, _b, _a, _m) = fresh_at(1000);
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5000)
        .unwrap();
    // Within refresh window → coalesce; existing stays at 5000.
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5500)
        .unwrap();
    assert_eq!(
        tracker.buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(1)),
        Some(5000)
    );
}

#[test]
fn buffered_accessed_at_ms_returns_none_after_flush() {
    let (tracker, blob, _a, _m) = fresh_at(1000);
    blob.push_row(ten_a(), row(1, 100)).unwrap();
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 5000)
        .unwrap();
    tracker.flush_batch(5_000).unwrap();
    assert!(tracker
        .buffered_accessed_at_ms(ten_a(), EvictionRegion::Sam, &dig(1))
        .is_none());
}

// ---- queue_size + is_empty -------------------------------------

#[test]
fn queue_size_starts_zero() {
    let (tracker, _b, _a, _m) = fresh_at(1000);
    assert_eq!(tracker.queue_size(), 0);
    assert!(tracker.is_empty());
}

#[test]
fn queue_size_advances_on_record() {
    let (tracker, _b, _a, _m) = fresh_at(1000);
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(1), 1000)
        .unwrap();
    assert_eq!(tracker.queue_size(), 1);
    tracker
        .record_access(ten_a(), EvictionRegion::Sam, &dig(2), 1001)
        .unwrap();
    assert_eq!(tracker.queue_size(), 2);
}

// ---- LruFlushResult::empty surface -----------------------------

#[test]
fn flush_result_empty_const() {
    let r = LruFlushResult::empty(42);
    assert_eq!(r.flushed_at_ms, 42);
    assert_eq!(r.rows_drained, 0);
    assert_eq!(r.drift_ms, 0);
    assert_eq!(r.consistency_violations, 0);
}
