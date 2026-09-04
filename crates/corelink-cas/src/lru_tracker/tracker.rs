//! LRU tracker orchestrator — wires `BlobMetaSoftDeleteStore` (write
//! surface), `LruAuditSink`, `LruMetricsObserver`, `LruClock`, and
//! `LruConfig` into the canonical async-batch pipeline.
//!
//! ## Hot-path call envelope (`record_access`)
//!
//! For each `(tenant_id, region, digest, accessed_at_ms)`:
//!
//! 1. **Validate** input (digest hex shape via
//!    [`corelink_eviction::EvictionBlobDigest`] inherited type).
//! 2. **Acquire** the per-instance `Mutex` (mirrors DO actor model).
//! 3. **Refresh-threshold short-circuit**: if an existing queue entry
//!    for `(tenant, digest)` has `accessed_at_ms >= now -
//!    refresh_threshold_ms`, return [`LruDecision::Coalesced`] (the
//!    write was within the dedup window; bump `coalesced_total` ONLY
//!    if the new sample's `accessed_at_ms > existing.accessed_at_ms`
//!    for monotonicity).
//! 4. **Overflow handling**: if queue size `>= queue_size_max` and
//!    `(tenant, digest)` is not already present, drop the OLDEST entry
//!    (FIFO) and bump `dropped_total{reason=queue_full}`. Returns
//!    [`LruDecision::Dropped`] for the dropped key only when the
//!    enqueue would have failed (the spec is explicit: drop OLDEST,
//!    enqueue NEWEST — so the call's own decision is `Recorded`).
//! 5. **Audit emit** BEFORE state mutation (`AccessRecorded`); audit
//!    failure aborts the enqueue (fail-closed envelope).
//! 6. **Metrics emit** AFTER audit succeeds (`records_total` +
//!    `coalesced_total` if the entry already existed within the
//!    refresh window).
//!
//! ## Flush envelope (`flush_batch`)
//!
//! 1. **Acquire** the per-instance `Mutex`.
//! 2. **Drain** up to `batch_size` entries (FIFO; oldest first).
//! 3. **Capture** drift_ms = `now_ms - oldest_pending_access_ms`
//!    BEFORE the drain (zero when queue is empty).
//! 4. **Per-row** call
//!    [`BlobMetaSoftDeleteStore::update_last_accessed_at_ms`]; the
//!    SQL conditional UPDATE filters out non-monotone writes.
//! 5. **Consistency-violation guard** (per-row + per-flush; R-S07-3 +
//!    spec contract §6 DoD): two triggers fire
//!    `corelink.lru.consistency_violation_detected` + bump
//!    `corelink_lru_consistency_violation_total`:
//!    - **Pre-flush row drift**: for any row drained where
//!      `entry.accessed_at_ms - existing.last_accessed_at_ms >
//!      drift_violation_threshold_ms` AND the conditional UPDATE
//!      ALSO fired the [`LruUpdateOutcome::Updated`] arm (D1 was
//!      catching up after a long gap; the previous flush failed to
//!      persist a recorded access). The lag carried by the audit is
//!      `entry.accessed_at_ms - prev_ms`.
//!    - **Per-flush drift watermark**: when the OLDEST pending entry
//!      has lingered in the queue longer than
//!      `drift_violation_threshold_ms` (i.e.,
//!      `now_ms - oldest_pending_access_ms > threshold`). Production
//!      wiring uses this to wake oncall when D1 batch flushes are
//!      silently failing (queue accumulation; bounded buffer about
//!      to overflow).
//! 6. **Audit emit** `BatchFlushed` AFTER the per-row UPDATEs (the
//!    UPDATE side fires the Skipped/AlreadyResolved arm cleanly even
//!    when the audit emit fails; production rolls back the audit
//!    insert in the same D1 batch as the per-row UPDATEs so the
//!    audit-emit ⇔ flush atomicity contract holds).
//! 7. **Metrics emit** `batch_flush_duration_ms` + `drift_ms`
//!    histograms.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use corelink_eviction::{
    BlobMetaSoftDeleteStore, EvictionBlobDigest, EvictionRegion, LruUpdateOutcome,
};

use crate::lru_tracker::audit::{LruAuditRecord, LruAuditSink, LruEventType};
use crate::lru_tracker::clock::LruClock;
use crate::lru_tracker::config::LruConfig;
use crate::lru_tracker::error::LruError;
use crate::lru_tracker::metrics::{LruDropReason, LruMetricsObserver};

// ============================================================================
//  Decision + Outcome.
// ============================================================================

/// Per-call decision produced by [`LruTracker::record_access`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LruDecision {
    /// Access enqueued (or coalesced into an existing entry whose
    /// `accessed_at_ms` advanced).
    Recorded {
        /// Wall-clock instant the enqueue fired.
        recorded_at_ms: u64,
        /// Whether THIS record overwrote an existing entry's
        /// `accessed_at_ms` (true) OR created a new entry (false).
        was_existing: bool,
    },
    /// Refresh-threshold short-circuit OR new-sample-not-monotonic
    /// short-circuit: an existing entry for `(tenant, digest)` already
    /// covers a write within the dedup window. Counted as
    /// `coalesced_total` (the call is the second + later record on the
    /// same key).
    Coalesced {
        /// Wall-clock instant the coalesce decision fired.
        decided_at_ms: u64,
        /// `accessed_at_ms` already in the queue for this key.
        existing_accessed_at_ms: u64,
    },
    /// Bounded queue at capacity AND the new record's enqueue would
    /// drop the OLDEST entry. The new record IS enqueued (per spec
    /// §6.1.4); the dropped key is reported via the audit/metrics
    /// path. This arm is reserved for FIFO overflow signals (the
    /// orchestrator uses [`LruDecision::Recorded`] with `was_existing=false` for
    /// the call's own decision).
    Dropped {
        /// Wall-clock instant the drop fired.
        dropped_at_ms: u64,
        /// The drop reason (always `queue_full` for this arm).
        reason: LruDropReason,
    },
}

/// Aggregate outcome of one [`LruTracker::flush_batch`] execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LruFlushResult {
    /// Wall-clock instant the flush fired.
    pub flushed_at_ms: u64,
    /// Number of rows drained from the queue (≤ `batch_size`).
    pub rows_drained: u64,
    /// Number of rows where the conditional UPDATE fired
    /// (`LruUpdateOutcome::Updated`).
    pub rows_updated: u64,
    /// Number of rows where the conditional UPDATE was skipped
    /// (out-of-order; `LruUpdateOutcome::Skipped`).
    pub rows_skipped: u64,
    /// Number of rows where the row was absent OR soft-deleted
    /// (`LruUpdateOutcome::AlreadyResolved`).
    pub rows_already_resolved: u64,
    /// Drift observed at flush start (`now_ms -
    /// oldest_pending_access_ms`; 0 when queue was empty).
    pub drift_ms: u64,
    /// Number of consistency-violation events emitted (R-S07-3).
    pub consistency_violations: u64,
}

impl LruFlushResult {
    /// Empty result anchor used for cooldown-skipped flush passes.
    #[must_use]
    pub const fn empty(flushed_at_ms: u64) -> Self {
        Self {
            flushed_at_ms,
            rows_drained: 0,
            rows_updated: 0,
            rows_skipped: 0,
            rows_already_resolved: 0,
            drift_ms: 0,
            consistency_violations: 0,
        }
    }
}

// ============================================================================
//  LruTracker trait.
// ============================================================================

/// Trait surfaced by every LRU-tracker backend (production CF DO
/// singleton + Tower wiring / in-memory fake).
pub trait LruTracker: Send + Sync + core::fmt::Debug {
    /// Hot-path enqueue: record an access for `(tenant, region, digest)`
    /// at `accessed_at_ms`. The orchestrator coalesces records for the
    /// same key within the refresh-threshold window.
    ///
    /// # Errors
    ///
    /// Surface as [`LruError`]. The fail-closed envelope: when the
    /// audit emit fires the [`LruError::Audit`] arm, the queue is NOT
    /// mutated.
    fn record_access(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        digest: &EvictionBlobDigest,
        accessed_at_ms: u64,
    ) -> Result<LruDecision, LruError>;

    /// Drain up to `batch_size` queued entries and apply the
    /// conditional monotone UPDATE via the wired
    /// [`BlobMetaSoftDeleteStore`]. Returns the aggregate flush result.
    ///
    /// # Errors
    ///
    /// Surface as [`LruError`]. The fail-closed envelope: when the
    /// audit emit fires, the per-row UPDATEs already fired (production
    /// rolls back the D1 batch on emit failure; the in-memory fake
    /// surfaces the error after the per-row mutation).
    fn flush_batch(&self, now_ms: u64) -> Result<LruFlushResult, LruError>;

    /// Snapshot the current queue size. Used by the production binding
    /// to drive the auto-flush trigger when the queue crosses
    /// `batch_size`.
    fn queue_size(&self) -> usize;

    /// Idempotent check — true when the queue is drained.
    fn is_empty(&self) -> bool {
        self.queue_size() == 0
    }
}

// ============================================================================
//  InMemoryLruTracker — pure-logic orchestrator.
// ============================================================================

/// Queue entry — one per `(tenant_id, region, digest)` (region is part
/// of the key so a CDN that re-routes a tenant across regions does not
/// coalesce stale entries from a different region).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct LruKey {
    tenant_id: Uuid,
    region: EvictionRegion,
    digest: EvictionBlobDigest,
}

/// Queue value — `accessed_at_ms` (latest-write-wins per coalescing
/// semantic) + `enqueue_seq` (FIFO ordering for overflow drop-oldest).
#[derive(Clone, Debug, PartialEq, Eq)]
struct LruEntry {
    accessed_at_ms: u64,
    enqueue_seq: u64,
}

#[derive(Debug, Default)]
struct LruState {
    /// Coalescing map — at most one entry per `(tenant, region, digest)`.
    queue: BTreeMap<LruKey, LruEntry>,
    /// Monotone enqueue sequence counter — drives FIFO drop-oldest on
    /// overflow.
    next_seq: u64,
}

/// In-memory LRU tracker (the canonical pure-logic skeleton).
///
/// All state mutation flows through a per-instance `Mutex<LruState>`
/// to serialise concurrent decisions (mirrors production DO actor
/// model).
pub struct InMemoryLruTracker<B, A, M, C>
where
    B: BlobMetaSoftDeleteStore,
    A: LruAuditSink,
    M: LruMetricsObserver,
    C: LruClock,
{
    blob_meta: Arc<B>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<C>,
    config: LruConfig,
    state: Mutex<LruState>,
}

impl<B, A, M, C> core::fmt::Debug for InMemoryLruTracker<B, A, M, C>
where
    B: BlobMetaSoftDeleteStore,
    A: LruAuditSink,
    M: LruMetricsObserver,
    C: LruClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryLruTracker")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<B, A, M, C> InMemoryLruTracker<B, A, M, C>
where
    B: BlobMetaSoftDeleteStore,
    A: LruAuditSink,
    M: LruMetricsObserver,
    C: LruClock,
{
    /// Construct with the canonical default config.
    pub fn with_defaults(blob_meta: Arc<B>, audit: Arc<A>, metrics: Arc<M>, clock: Arc<C>) -> Self {
        Self {
            blob_meta,
            audit,
            metrics,
            clock,
            config: LruConfig::canonical(),
            state: Mutex::new(LruState::default()),
        }
    }

    /// Construct with an explicit config.
    pub fn new(
        blob_meta: Arc<B>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<C>,
        config: LruConfig,
    ) -> Self {
        Self {
            blob_meta,
            audit,
            metrics,
            clock,
            config,
            state: Mutex::new(LruState::default()),
        }
    }

    /// Snapshot the current config.
    #[must_use]
    pub const fn config(&self) -> &LruConfig {
        &self.config
    }

    /// Snapshot the current `accessed_at_ms` for `(tenant, region,
    /// digest)` if it exists in the queue. Used by the production
    /// `last_accessed_at_authoritative` lookup (DO buffered + D1 base
    /// UNION) — eviction worker checks the buffer FIRST before falling
    /// back to D1 (race-correctness load-bearing per WI §1 + §6.1.7).
    #[must_use]
    pub fn buffered_accessed_at_ms(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        digest: &EvictionBlobDigest,
    ) -> Option<u64> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let key = LruKey {
            tenant_id,
            region,
            digest: digest.clone(),
        };
        g.queue.get(&key).map(|e| e.accessed_at_ms)
    }

    /// Snapshot the queue size (cheap; under the per-instance mutex).
    fn queue_size_locked(state: &LruState) -> usize {
        state.queue.len()
    }
}

impl<B, A, M, C> LruTracker for InMemoryLruTracker<B, A, M, C>
where
    B: BlobMetaSoftDeleteStore,
    A: LruAuditSink,
    M: LruMetricsObserver,
    C: LruClock,
{
    fn record_access(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        digest: &EvictionBlobDigest,
        accessed_at_ms: u64,
    ) -> Result<LruDecision, LruError> {
        let now_ms = self.clock.now_ms();
        let key = LruKey {
            tenant_id,
            region,
            digest: digest.clone(),
        };

        let mut state = self
            .state
            .lock()
            .map_err(|_| LruError::Backend("lru state mutex poisoned".to_string()))?;

        // Refresh-threshold short-circuit + monotone-coalesce.
        // If an existing entry covers a write within the dedup window
        // OR if the new sample is not strictly newer, we coalesce
        // (no enqueue mutation; bump the coalesced metric AFTER the
        // audit succeeds).
        if let Some(existing) = state.queue.get(&key) {
            // Within refresh window: skip update entirely.
            let within_refresh_window =
                now_ms.saturating_sub(existing.accessed_at_ms) < self.config.refresh_threshold_ms();
            // Out-of-order or equal sample: coalesce.
            let not_strictly_newer = accessed_at_ms <= existing.accessed_at_ms;
            if within_refresh_window || not_strictly_newer {
                // Audit emit BEFORE state mutation (none here, but the
                // contract is consistent).
                self.audit.emit(LruAuditRecord {
                    event_type: LruEventType::AccessRecorded,
                    tenant_id: Some(tenant_id),
                    region,
                    rows: 1,
                    drift_ms: None,
                    now_ms,
                })?;
                self.metrics.record_coalesced()?;
                return Ok(LruDecision::Coalesced {
                    decided_at_ms: now_ms,
                    existing_accessed_at_ms: existing.accessed_at_ms,
                });
            }
        }

        // The (tenant, region, digest) is either absent OR has an
        // older sample that we want to overwrite. Determine if we
        // need to drop-oldest before insert.
        let queue_full = Self::queue_size_locked(&state) >= self.config.queue_size_max();
        let already_present = state.queue.contains_key(&key);

        // Audit emit BEFORE state mutation (fail-closed envelope).
        self.audit.emit(LruAuditRecord {
            event_type: LruEventType::AccessRecorded,
            tenant_id: Some(tenant_id),
            region,
            rows: 1,
            drift_ms: None,
            now_ms,
        })?;

        // Overflow handling — drop OLDEST FIFO if we'd exceed cap and
        // this is a NEW key.
        if queue_full && !already_present {
            // Find the entry with the smallest `enqueue_seq` (oldest
            // FIFO position) and drop it.
            let oldest_key_opt = state
                .queue
                .iter()
                .min_by_key(|(_, e)| e.enqueue_seq)
                .map(|(k, _)| k.clone());
            if let Some(oldest_key) = oldest_key_opt {
                let _ = state.queue.remove(&oldest_key);
                self.metrics.record_dropped(LruDropReason::QueueFull)?;
            }
        }

        // Insert / overwrite.
        let was_existing = already_present;
        let next_seq = state.next_seq;
        state.next_seq = state.next_seq.saturating_add(1);
        state.queue.insert(
            key,
            LruEntry {
                accessed_at_ms,
                enqueue_seq: next_seq,
            },
        );

        // Coalesced metric fires when we overwrote an existing entry
        // with a strictly-newer sample (the monotone-advance path).
        if was_existing {
            self.metrics.record_coalesced()?;
        }

        // Records-total fires for every Recorded decision (per WI
        // §6.1.9 metric).
        self.metrics.record_recorded(tenant_id, region)?;

        Ok(LruDecision::Recorded {
            recorded_at_ms: now_ms,
            was_existing,
        })
    }

    fn flush_batch(&self, now_ms: u64) -> Result<LruFlushResult, LruError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LruError::Backend("lru state mutex poisoned".to_string()))?;

        // Empty queue is a no-op (idempotent flush).
        if state.queue.is_empty() {
            // Emit a zero-row batch_flushed audit so the production
            // wiring's flush-emit cadence is observable. The audit is
            // emitted EXACTLY once per `flush_batch` invocation.
            self.audit.emit(LruAuditRecord {
                event_type: LruEventType::BatchFlushed,
                tenant_id: None,
                region: EvictionRegion::Sam,
                rows: 0,
                drift_ms: Some(0),
                now_ms,
            })?;
            self.metrics.observe_flush_duration_ms(0)?;
            self.metrics.observe_drift_ms(0)?;
            return Ok(LruFlushResult::empty(now_ms));
        }

        // Compute drift_ms BEFORE drain (drift = now - oldest pending
        // access ms; saturating_sub for monotonicity safety).
        let oldest_access_ms = state
            .queue
            .values()
            .map(|e| e.accessed_at_ms)
            .min()
            .unwrap_or(now_ms);
        let drift_ms = now_ms.saturating_sub(oldest_access_ms);

        // Drain up to batch_size FIFO entries (oldest enqueue_seq
        // first). The collect-then-remove pattern works around the
        // BTreeMap mutable-borrow constraint in the per-row branch.
        let batch_size = self.config.batch_size();
        let mut to_drain: Vec<(LruKey, LruEntry)> = state
            .queue
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Stable sort by enqueue_seq ASC (FIFO).
        to_drain.sort_by_key(|(_, e)| e.enqueue_seq);
        to_drain.truncate(batch_size);

        let mut rows_drained = 0_u64;
        let mut rows_updated = 0_u64;
        let mut rows_skipped = 0_u64;
        let mut rows_already_resolved = 0_u64;
        let mut consistency_violations = 0_u64;

        // Per-flush drift-watermark guard: the oldest pending entry
        // has lingered in the queue longer than the threshold; the
        // queue is failing to catch up to D1 (likely D1 backpressure
        // OR previous batch flush failure). Emit ONE
        // ConsistencyViolationDetected for the per-flush watermark.
        let drift_watermark_violation = drift_ms > self.config.drift_violation_threshold_ms();
        if drift_watermark_violation {
            self.audit.emit(LruAuditRecord {
                event_type: LruEventType::ConsistencyViolationDetected,
                tenant_id: None,
                region: EvictionRegion::Sam,
                rows: 1,
                drift_ms: Some(drift_ms),
                now_ms,
            })?;
            // Use Uuid::nil() as the tenant label for the per-flush
            // watermark (the queue may span tenants; the metric is
            // still tenant-scoped).
            self.metrics.record_consistency_violation(Uuid::nil())?;
            consistency_violations = consistency_violations.saturating_add(1);
        }

        // Per-row conditional UPDATE.
        for (key, entry) in to_drain {
            // Remove from queue BEFORE the UPDATE (mirrors atomic D1
            // batch envelope; on failure production wiring rolls back
            // both the D1 UPDATE and the queue removal via DO storage
            // transaction).
            let _ = state.queue.remove(&key);
            rows_drained = rows_drained.saturating_add(1);
            let outcome = self.blob_meta.update_last_accessed_at_ms(
                key.tenant_id,
                &key.digest,
                entry.accessed_at_ms,
            )?;
            match outcome {
                LruUpdateOutcome::Updated { prev_ms, new_ms } => {
                    rows_updated = rows_updated.saturating_add(1);
                    // Per-row drift guard: D1 was catching up across a
                    // wide gap (`new - prev > threshold`). Signals the
                    // PREVIOUS flush failed to persist a recorded
                    // access. Pinned by
                    // `prop_consistency_violation_detected_when_drift_exceeds_threshold`.
                    let lag = new_ms.saturating_sub(prev_ms);
                    if lag > self.config.drift_violation_threshold_ms() {
                        self.audit.emit(LruAuditRecord {
                            event_type: LruEventType::ConsistencyViolationDetected,
                            tenant_id: Some(key.tenant_id),
                            region: key.region,
                            rows: 1,
                            drift_ms: Some(lag),
                            now_ms,
                        })?;
                        self.metrics.record_consistency_violation(key.tenant_id)?;
                        consistency_violations = consistency_violations.saturating_add(1);
                    }
                }
                LruUpdateOutcome::Skipped { .. } => {
                    rows_skipped = rows_skipped.saturating_add(1);
                }
                LruUpdateOutcome::AlreadyResolved => {
                    rows_already_resolved = rows_already_resolved.saturating_add(1);
                }
                _ => {
                    // `LruUpdateOutcome` is `#[non_exhaustive]`; future
                    // arms surface as a Backend error to the caller so
                    // a silent miscount does not leak.
                    return Err(LruError::Backend(
                        "unhandled LruUpdateOutcome variant".to_string(),
                    ));
                }
            }
        }

        // Audit emit AFTER the per-row UPDATEs (carries the batch
        // count + drift_ms).
        self.audit.emit(LruAuditRecord {
            event_type: LruEventType::BatchFlushed,
            tenant_id: None,
            region: EvictionRegion::Sam,
            rows: rows_drained,
            drift_ms: Some(drift_ms),
            now_ms,
        })?;

        // Metrics emit (in-memory fake reports duration as 0; the
        // production binding substitutes a real timer).
        self.metrics.observe_flush_duration_ms(0)?;
        self.metrics.observe_drift_ms(drift_ms)?;

        Ok(LruFlushResult {
            flushed_at_ms: now_ms,
            rows_drained,
            rows_updated,
            rows_skipped,
            rows_already_resolved,
            drift_ms,
            consistency_violations,
        })
    }

    fn queue_size(&self) -> usize {
        match self.state.lock() {
            Ok(g) => g.queue.len(),
            Err(p) => p.into_inner().queue.len(),
        }
    }
}
#[cfg(test)]
#[path = "tracker_tests.rs"]
mod tests;
