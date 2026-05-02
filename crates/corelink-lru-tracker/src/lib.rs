//! `corelink-lru-tracker` — async-batch `last_accessed_at` hot-path
//! LRU tracker (WI-S07-004).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the LRU tracker plane: trait surfaces every production
//! Cloudflare Durable Object singleton + Tower middleware will satisfy,
//! plus in-memory fakes that exercise every load-bearing invariant the
//! production wiring relies on. Property tests pinned at 10 k iter
//! against the fakes cover write-amplification mitigation via
//! coalescing (R-S07-3 + spec contract §5), bounded queue overflow
//! with FIFO drop-oldest (WI §6.1.4), monotone latest-write-wins per
//! `(tenant, digest)`, bounded LRU drift histogram, INV-LRU-CONSISTENCY
//! violation detection (R-S07-3 + spec contract §6 DoD), tenant-scoped
//! strict isolation (CTRL-ISO-005), audit fail-closed envelope
//! (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`), and idempotent flush
//! semantics.
//!
//! # Why this crate is `trait + fake` here, real DO/D1 in WI-S07-005
//!
//! S-07 lands without Cloudflare DO bindings wired into CI (no remote +
//! Cloudflare Workers + D1 staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! coalescing collapses N records on the same `(tenant, digest)` into
//! one D1 UPDATE (write-amplification mitigation); FIFO drop-oldest on
//! overflow preserves the NEWEST entries; the per-row + per-flush
//! consistency-violation guard fires when D1 last_accessed_at lags the
//! most recent recorded access by more than the configured drift
//! threshold; tenant-scoped strict isolation; idempotent flush.
//! The live-DO + D1 conformance tests run alongside WI-S07-005 (PRR
//! ship gate) once miniflare/wrangler-dev integration tests land.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`audit`] module ships [`LruEventType`] +
//!    [`LruAuditRecord`] + [`LruAuditSink`] +
//!    [`InMemoryLruAuditSink`] capture sink (fail-closed envelope
//!    per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The canonical 4-event
//!    taxonomy is `corelink.lru.{access_recorded, batch_flushed,
//!    batch_failed, consistency_violation_detected}`. The
//!    `ConsistencyViolationDetected` arm IS SEV-1 (operator pager
//!    wake-up).
//! 2. The [`metrics`] module ships [`LruMetricsObserver`] +
//!    [`InMemoryLruMetrics`] capture sink (canonical 6 metrics:
//!    `corelink.lru.records_total{tenant,region}`,
//!    `corelink.lru.coalesced_total`,
//!    `corelink.lru.dropped_total{reason=queue_full|flush_error}`,
//!    `corelink.lru.batch_flush_duration_ms`,
//!    `corelink.lru.drift_ms`,
//!    `corelink.lru.consistency_violation_total{tenant}` — alert
//!    SEV-1 if `> 0` per WI-S07-005 alert taxonomy).
//! 3. The [`error`] module ships the canonical [`LruError`]
//!    `#[non_exhaustive]` taxonomy.
//! 4. The [`config`] module ships [`LruConfig`] (knobs pinned to
//!    canonical values: `queue_size_max=10_000`, `batch_size=250`,
//!    `flush_interval_ms=30_000`, `drift_violation_threshold_ms=60_000`,
//!    `refresh_threshold_ms=60_000`) + per-instance F-001 closure
//!    semantics + invariant validation via `with_overrides`.
//! 5. The [`clock`] module ships [`LruClock`] seam +
//!    [`CountingLruClock`] / [`FrozenLruClock`] fakes.
//! 6. The [`tracker`] module ships [`LruDecision`] `#[non_exhaustive]`
//!    (`Recorded` / `Coalesced` / `Dropped`) + [`LruFlushResult`]
//!    aggregate + [`LruTracker`] trait + [`InMemoryLruTracker`]
//!    orchestrator wired to `corelink-eviction::BlobMetaSoftDeleteStore`
//!    (write surface) + [`LruAuditSink`] + [`LruMetricsObserver`] +
//!    [`LruClock`].
//!
//! # Cripto-driven invariants enforced
//!
//! - **R-S07-3 LRU coalescing** (write-amplification mitigation): the
//!   in-memory queue is keyed by `(tenant_id, region, digest)`; multiple
//!   records on the same key within `flush_interval_ms` collapse to ONE
//!   D1 UPDATE. The refresh-threshold short-circuit further reduces
//!   write amplification within `refresh_threshold_ms`. Pinned by
//!   `prop_coalescing_reduces_write_amplification`.
//! - **INV-LRU-CONSISTENCY** (HIGH; spec_contract §8 §3.18): eviction
//!   respects authoritative `last_accessed_at` via DO buffered + D1
//!   base UNION. The tracker exposes
//!   [`InMemoryLruTracker::buffered_accessed_at_ms`] for the eviction
//!   worker's race-aware `last_accessed_at_authoritative` lookup; the
//!   per-row + per-flush consistency-violation guard fires when D1
//!   last_accessed_at is older than the most recent recorded access by
//!   more than `drift_violation_threshold_ms`. Pinned by
//!   `prop_consistency_violation_detected_when_drift_exceeds_threshold`.
//! - **INV-AC-TTL-MONOTONIC** (HIGH; registry §3.15 inheritance):
//!   `last_accessed_at_ms` strictly monotone — out-of-order records on
//!   the same key are coalesced; the conditional UPDATE
//!   (`WHERE last_accessed_at < ?`) preserves monotonicity at the SQL
//!   layer. Pinned by `prop_record_then_flush_persists_latest`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+): the queue key is
//!   `(tenant_id, region, digest)`; tenant A records NEVER affect
//!   tenant B blob_meta. Pinned by `prop_tenant_isolation`.
//! - **Bounded queue with FIFO drop-oldest**: when the queue reaches
//!   `queue_size_max` AND a NEW key is recorded, the OLDEST entry
//!   (lowest `enqueue_seq`) is dropped + `dropped_total{reason=queue_full}`
//!   bumps. Pinned by `prop_overflow_drops_oldest`.
//! - **Audit fail-closed** (Lote 10.6bis pattern): every Recorded /
//!   Coalesced decision emits an audit BEFORE the queue mutation.
//!   Pinned by `prop_audit_emit_per_decision_arm`.
//! - **Idempotent flush**: flushing an empty queue is a no-op (fires
//!   one BatchFlushed audit + zero drift_ms). Pinned by
//!   `prop_idempotent_flush`.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - **No `tokio`** in `src/` (wasm32-clean lib code; tokio only in
//!   tests if needed).
//! - The fake is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the conditional monotone UPDATE envelope
//!   on `corelink-eviction::BlobMetaSoftDeleteStore::update_last_accessed_at_ms`.
//!
//! # Scope boundary vs WI-S07-002 + WI-S07-005
//!
//! S-07 WI-S07-002 (`corelink-eviction`) ships the eviction worker
//! that consumes `last_accessed_at_ms` from `blob_meta`. THIS WI's
//! tracker WRITES to that same column (via the new
//! `BlobMetaSoftDeleteStore::update_last_accessed_at_ms` conditional
//! UPDATE method). The mutual integration is covered by the
//! `prop_record_then_flush_persists_latest` round-trip property test:
//! a sequence of records on the same digest is flushed; the resulting
//! `blob_meta.last_accessed_at_ms` reflects the LATEST recorded
//! `accessed_at_ms`.
//!
//! WI-S07-005 (PRR ship gate) consolidates the live D1 + DO
//! conformance suite + DASH-DEDUP dashboard + alerts (including the
//! `corelink_lru_consistency_violation_total > 0` SEV-1 alert).
//!
//! # Wiring into the production binding (forward; WI-S07-005 ship gate)
//!
//! The production wiring composes [`LruTracker::record_access`]
//! fire-and-forget on top of the existing `AuthLayer` (WI-S03-003
//! SEALED; provides `AuthCtx` extracted tenant_id) post-CAS GET success
//! via `worker::send_future()` (Lote 10.7bis R5 P0-3 — async-spawn does
//! NOT block the hot path; NEVER `tokio::spawn` (no tokio reactor in
//! CF Workers V8 isolate); NEVER `wasm_bindgen_futures::spawn_local`
//! (browser WASM API; not available in CF Workers runtime)).
//!
//! The DO singleton `lru-tracker-<region>` schedules a 30s alarm
//! that calls `LruTracker::flush_batch(now_ms)`; the alarm is re-armed
//! AT START (Lote 10.4bis lesson) so a DO restart does not stall the
//! flush cadence.

#![forbid(unsafe_code)]

pub mod audit;
pub mod clock;
pub mod config;
pub mod error;
pub mod metrics;
pub mod tracker;

pub use audit::{
    canonical_audit_event_strings, FailingLruAuditSink, InMemoryLruAuditSink,
    LruAuditRecord, LruAuditSink, LruAuditSinkError, LruEventType,
};
pub use clock::{CountingLruClock, FrozenLruClock, LruClock};
pub use config::{
    LruConfig, DEFAULT_BATCH_SIZE, DEFAULT_DRIFT_VIOLATION_THRESHOLD_MS,
    DEFAULT_FLUSH_INTERVAL_MS, DEFAULT_QUEUE_SIZE_MAX,
    DEFAULT_REFRESH_THRESHOLD_MS,
};
pub use error::LruError;
pub use metrics::{
    canonical_metric_names, FailingLruMetrics, InMemoryLruMetrics,
    LruDropReason, LruMetricKind, LruMetricsObserver,
    LruMetricsObserverError,
};
pub use tracker::{
    InMemoryLruTracker, LruDecision, LruFlushResult, LruTracker,
};
