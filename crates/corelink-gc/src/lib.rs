//! `corelink-gc` — Garbage Collection worker skeleton + scheduler +
//! degrade-mode `gc-pause` emergency stop (WI-S06-001).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the GC worker plane: trait surfaces every production
//! Cloudflare Cron Durable Object binding (WI-S06-006 conformance
//! suite) will satisfy, plus in-memory fakes that exercise every
//! load-bearing invariant the production wiring relies on. Property
//! tests pinned at 10 k iter against the fakes cover the
//! cross-component flow (cron tick → gc_run insert → phase orchestration
//! → degrade-mode probe → audit emit) without spinning up miniflare.
//!
//! Specifically, the crate ships:
//!
//! 1. The canonical SQL artifact `migrations/d1/0006_gc_run.sql`
//!    embedded via [`MIGRATION_0006_GC_RUN`] so production code can
//!    pass the DDL to `wrangler d1 migrations apply` without re-reading
//!    from disk.
//! 2. The [`region`] module pinning the canonical 5-region list
//!    (`sam`, `iad`, `lhr`, `nrt`, `syd`) the schema CHECK enforces.
//! 3. The [`run`] module ships the gc_run row type ([`GcRun`]),
//!    canonical phase + status state machines ([`GcPhase`] +
//!    [`GcStatus`]), the [`GcRunStore`] trait every backend implements,
//!    and the [`InMemoryGcRunStore`] fake whose semantics mirror the
//!    SQL migration byte-for-byte (partial UNIQUE on
//!    `WHERE status = 'running'`, monotone phase transitions,
//!    `mark_started_at_ms` immutability, idempotent resume from
//!    checkpoint).
//! 4. The [`schedule`] module ships [`ScheduleConfig`] (cron schedule,
//!    jitter, region pin, per-tenant batch concurrency cap) and the
//!    deterministic jitter helper used by the in-memory scheduler
//!    (`jitter_ms_for_region`).
//! 5. The [`scheduler`] module ships the [`GcScheduler`] trait + the
//!    [`InMemoryGcScheduler`] fake driving the canonical
//!    `cron tick → list candidate tenants → spawn worker per tenant`
//!    flow.
//! 6. The [`degrade`] module ships [`DegradeMode`] / [`DegradeKind`] +
//!    the [`DegradeProbe`] trait every config-singleton DO will
//!    satisfy (PAT-DEGRADE-001 alignment) + the
//!    [`InMemoryDegradeProbe`] fake whose snapshot semantics mirror
//!    a config-singleton read.
//! 7. The [`audit`] module ships the canonical 4-event
//!    `corelink.gc.*` taxonomy ([`GcEventType`] +
//!    [`GcAuditRecord`]) + the [`GcAuditSink`] trait + the
//!    [`InMemoryGcAuditSink`] capture sink.
//! 8. The [`metrics`] module ships [`GcMetricsObserver`] + the
//!    canonical 6 metrics name list every emission path uses.
//! 9. The [`worker`] module ships the [`GcWorker`] trait every
//!    production worker satisfies + the [`InMemoryGcWorker`] fake
//!    that orchestrates the run lifecycle (Insert → Mark phase
//!    transition → checkpoint → Sweep delegation [WI-S06-002 onwards]
//!    → Completed) honoring the degrade-mode probe at every
//!    transition (≤100 ms next-batch propagation gate).
//! 10. The [`admin`] module ships the [`AdminTriggerOutcome`] +
//!     [`admin_trigger`] helper that surfaces the canonical
//!     `403 / 200 / 501` envelope the S-13 admin plane will satisfy.
//! 11. The [`error`] module ships the canonical [`GcError`] taxonomy
//!     every fallible API surfaces.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - The simulator is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the gc_run table and reports structured
//!   [`GcError`] errors when an invariant fires.

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite) for
/// `gc_run` (WI-S06-001).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0006_GC_RUN: &str = include_str!("../../../migrations/d1/0006_gc_run.sql");

pub mod admin;
pub mod audit;
pub mod degrade;
pub mod error;
pub mod mark;
pub mod metrics;
pub mod physical_delete;
pub mod reconcile;
pub mod region;
pub mod run;
pub mod schedule;
pub mod scheduler;
pub mod sweep;
pub mod worker;

pub use admin::{admin_trigger, AdminTriggerOutcome};
pub use audit::{
    canonical_audit_event_strings, GcAuditRecord, GcAuditSink, GcAuditSinkError, GcEventType,
    InMemoryGcAuditSink,
};
pub use degrade::{
    DegradeKind, DegradeMode, DegradeProbe, DegradeProbeError, InMemoryDegradeProbe,
};
pub use error::GcError;
pub use mark::{
    AcMetaRow, BlobDigest, BlobMetaRow, CandidateStatus, CountingMarkClock, GcCandidate,
    GcCandidatesStore, InMemoryGcCandidatesStore, InMemoryMarkPhase, InMemoryReachableSetSource,
    ManifestChunkRow, MarkClock, MarkConfig, MarkError, MarkPhase, MarkResult, ReachableSetSource,
    CANONICAL_BATCH_SIZE, CANONICAL_JITTER_MS, CANONICAL_PHASE_BUDGET_MS,
    MIGRATION_0007_GC_CANDIDATES,
};
pub use metrics::{
    canonical_metric_names, GcMetricKind, GcMetricsObserver, GcMetricsObserverError,
    InMemoryGcMetrics,
};
pub use reconcile::{
    auto_fix_gate_fires, sev_level_for, AcMetaReconcileRow, BlobMetaReconcileRow,
    BlobMetaRefcountStore, CountingReconcileClock, InMemoryBlobMetaRefcountStore,
    InMemoryReconcilePhase, InMemoryRefcountSource, ReconcileClock, ReconcileConfig,
    ReconcileDecision, ReconcileError, ReconcilePhase, ReconcileResult, RefcountSource, SevLevel,
    AUTO_FIX_MAX_PERCENT, AUTO_FIX_MAX_RECORDS, CANONICAL_RECONCILE_PHASE_BUDGET_MS,
    SEV1_PER_TENANT_DRIFT_PERCENT, SEV2_GLOBAL_DRIFT_PERCENT,
};
pub use region::{GcRegion, UnknownRegion, REGION_LIST};
pub use run::{
    CheckpointDeltas, FailureContext, GcPhase, GcPhaseKind, GcRun, GcRunStore, GcRunStoreError,
    GcStatus, InMemoryGcRunStore, RunId, NULL_MARK_STARTED_AT_MS,
};
pub use schedule::{
    jitter_ms_for_region, ScheduleConfig, DEFAULT_CRON_EXPR, DEFAULT_JITTER_MINUTES,
    DEFAULT_MAX_CONCURRENT_TENANTS, MAX_JITTER_MINUTES,
};
pub use scheduler::{
    CronTickOutcome, GcScheduler, GcSchedulerError, InMemoryGcScheduler, MAX_TENANTS_PER_TICK,
};
pub use sweep::{
    AcReferenceIndex, AcReferenceWitness, BlobMetaRow as SweepBlobMetaRow, BlobMetaStore,
    BlobState, CountingSweepClock, InMemoryAcReferenceIndex, InMemoryBlobMetaStore,
    InMemorySweepPhase, SweepClock, SweepConfig, SweepDecision, SweepError, SweepPhase,
    SweepResult, CANONICAL_SWEEP_PHASE_BUDGET_MS, GRACE_AC_MS, GRACE_CAS_MS,
};
pub use worker::{GcWorker, InMemoryGcWorker, WorkerStepOutcome};

/// Returns the canonical schema version recorded by the latest GC
/// migration in the D1 `gc` domain.
///
/// Version is sequential within the D1 domain; D1 (`blob_meta` =
/// schema 1, `ac_meta` = schema 2, `multipart_chunks_manifest` =
/// schema 3, …, `gc_run` = schema 6, `gc_candidates` = schema 7) and
/// Postgres (`auth` schema) versioning are independent — a schema bump
/// here does NOT increment the Postgres `schema_version` table.
///
/// WI-S06-002 advances the GC domain to schema 7 by adding the
/// `gc_candidates` table embedded as
/// [`crate::MIGRATION_0007_GC_CANDIDATES`].
#[must_use]
pub const fn gc_schema_version() -> u32 {
    7
}
