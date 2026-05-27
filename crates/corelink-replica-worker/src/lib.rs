//! `corelink-replica-worker` — Hot Blob Replica Worker + Offline Batch Aggregator
//! (WI-S14-003 — PAT-REGION-FAILOVER-001, HIGH_RISK lane).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! this crate ships the **pure-logic skeleton** of the hot blob replica worker
//! and offline aggregation job the production CF Workers cron-trigger will
//! satisfy (real D1 `hot_blobs` table + R2 cross-region GET/PUT + hash verify +
//! residency routing), plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`region`] module ships [`Region`] `#[non_exhaustive]` 4-canonical
//!    (`Wnam` / `Enam` / `Weur` / `Sam`; APAC/AFR phase 2) + [`TenantTier`]
//!    4-canonical (`Solo` / `Team` / `Business` / `Enterprise`) for the
//!    cardinality-safe live Prometheus label (`tenant_tier × region = 16 séries`;
//!    per INV-OBS-CARDINALITY-BUDGET S-09). Also exports `residency_graph`
//!    — the **static acyclic** failover-sibling map (WNAM↔ENAM, WEUR↔SAM;
//!    acyclic; cross-jurisdiction forbidden per Schrems II + LGPD Art. 33).
//!
//! 2. The [`hot_blob`] module ships [`HotBlob`] + [`ReplicaStatus`]
//!    `#[non_exhaustive]` 5-canonical (`Pending` / `InProgress` / `Replicated`
//!    / `Failed` / `Evicted`) + [`AggregationEntry`] (tenant_id + blob_hash +
//!    bytes_total + access_count_30d).
//!
//! 3. The [`aggregator`] module ships [`OfflineAggregator`] trait +
//!    [`InMemoryOfflineAggregator`] orchestrator (per-instance `Arc<Mutex<>>`;
//!    F-001 closure; **OFFLINE NOT live metric** — audit-emit-BEFORE-mutation
//!    fail-CLOSED per S-06 P0-2 pattern). Also ships the top-1% gate constant
//!    [`TOP_1_PCT_THRESHOLD`] = `0.01` and window constant
//!    [`AGGREGATION_WINDOW_DAYS`] = `30`.
//!    **CRITICAL**: `// DO NOT add tenant_id label — use offline aggregation`
//!    (INV-OBS-CARDINALITY-BUDGET enforced; cardinality validator CI gate).
//!
//! 4. The [`replication`] module ships [`ReplicationWorker`] trait +
//!    [`InMemoryReplicationWorker`] orchestrator. Per-blob: hash verify
//!    post-copy (INV-CAS-INTEGRITY); residency check (`replica_region` in
//!    allowed sibling set per `residency_graph`; INV-REGION-NO-CROSS-LEAK);
//!    exponential backoff retry (5 attempts max); persistent failure = SEV-2
//!    audit emit `corelink.region.replication.failed`. Also ships
//!    [`FailingReplicationWorker`] adversarial fixture.
//!
//! 5. The [`audit`] module ships [`ReplicaAuditEventType`] `#[non_exhaustive]`
//!    8-canonical taxonomy:
//!    - `corelink.region.replication.{started, completed, failed}`
//!    - `corelink.hot_blob.aggregation.{started, completed, failed}`
//!    - `corelink.region.failover.{detected, resolved}`
//!    + [`ReplicaAuditRecord`] + [`ReplicaAuditSink`] trait +
//!      [`InMemoryReplicaAuditSink`] + [`FailingReplicaAuditSink`].
//!
//! 6. The [`error`] module ships [`ReplicaError`] `#[non_exhaustive]`
//!    taxonomy (`Aggregation` / `R2Copy` / `HashMismatch` /
//!    `ResidencyViolation` / `Storage` / `Audit` / `RetryExhausted`).
//!
//! 7. The [`cardinality`] module ships [`LIVE_METRIC_LABEL_CARDINALITY`] = 16
//!    (4 `tenant_tier` × 4 `region` séries; budget-safe) and asserts the
//!    compile-time invariant `NO_TENANT_ID_LABEL` via constant bool.
//!
//! # Cardinality budget critical path (INV-OBS-CARDINALITY-BUDGET)
//!
//! `corelink_cas_get_bytes_total{tenant_tier, region}` — TIER label NOT
//! tenant_id label. 16 séries baseline. Budget-safe.
//!
//! Live metric labeled by `tenant_id` is **FORBIDDEN** — with 10k+ tenants
//! × 100 metrics each = 1M+ unique series; Grafana Mimir tenant limit exceeded;
//! metrics dropped silently. Use offline aggregation (this crate) instead.
//!
//! # Audit fail-CLOSED ordering (S-06 P0-2 lesson)
//!
//! `lookup → emit_audit → mutate_state`
//!
//! State is **NEVER** mutated if audit emit fails. Tests verify state UNCHANGED
//! on audit emit failure.
//!
//! # Residency restriction (INV-REGION-NO-CROSS-LEAK; Schrems II + LGPD)
//!
//! - WNAM ↔ ENAM (US sibling pair; CCPA/PIPEDA jurisdiction)
//! - WEUR ↔ SAM (EU ↔ LGPD; SAM is the only allowed WEUR sibling per
//!   Schrems II — no EU→US transfer without adequacy decision)
//!
//! Cross-jurisdiction replication (WEUR → ENAM or WNAM) is **FORBIDDEN** by
//! static acyclic config enforced in `residency_graph`. Property tests verify
//! zero residency violations across 10k random tenant+region combinations.
//!
//! # INV-CAS-INTEGRITY (hash verify post-copy)
//!
//! Every replication copies blob bytes, then verifies `primary_hash == replica_hash`.
//! Mismatch triggers exponential backoff retry (5 max); persistent = SEV-2 alert
//! + audit emit + `ReplicaError::HashMismatch`.
//!
//! # wasm32-unknown-unknown compatibility
//!
//! This crate is `wasm32-unknown-unknown` clean. No `ring`, no C toolchain
//! dependencies, no `tokio::spawn`. All deps are pure-Rust.

#![forbid(unsafe_code)]

pub mod aggregator;
pub mod audit;
pub mod cardinality;
pub mod coverage;
pub mod error;
pub mod hot_blob;
pub mod metrics;
pub mod region;
pub mod replication;

pub use aggregator::{
    AggregationResult, InMemoryOfflineAggregator, OfflineAggregator, AGGREGATION_WINDOW_DAYS,
    TOP_1_PCT_THRESHOLD,
};
pub use audit::{
    FailingReplicaAuditSink, InMemoryReplicaAuditSink, ReplicaAuditEventType, ReplicaAuditRecord,
    ReplicaAuditSink,
};
pub use cardinality::{LIVE_METRIC_LABEL_CARDINALITY, NO_TENANT_ID_LABEL};
pub use coverage::{
    compute_coverage_ratio, CoverageObservation, FailingHotBlobCoverageSli, HotBlobCoverageSli,
    InMemoryHotBlobCoverageSli, HOT_BLOB_COVERAGE_DEADLINE_SECONDS,
    HOT_BLOB_COVERAGE_TARGET_RATIO, METRIC_HOT_BLOB_REPLICATION_COVERAGE_RATIO,
};
pub use error::ReplicaError;
pub use hot_blob::{AggregationEntry, HotBlob, ReplicaStatus};
pub use metrics::{
    BatchOutcome, BatchOutcomeObservation, FailingReplicationLagSli, InMemoryReplicationLagSli,
    LagObservation, ReplicationDomain, ReplicationLagSli, METRIC_REPLICATION_BATCH_TOTAL,
    METRIC_REPLICATION_LAG_SECONDS, REPLICATION_LAG_BUCKETS_SECONDS,
};
pub use region::{Region, ResidencyGraph, TenantTier, REPLICATION_LAG_P99_SLO_SECS};
pub use replication::{FailingReplicationWorker, InMemoryReplicationWorker, ReplicationWorker};
