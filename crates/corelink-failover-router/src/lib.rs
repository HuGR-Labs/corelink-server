//! `corelink-failover-router` — Read Failover Router Middleware
//! (WI-S14-003 — PAT-REGION-FAILOVER-001, HIGH_RISK lane).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! this crate ships the **pure-logic skeleton** of the failover routing
//! middleware the production Tower layer will satisfy (real health probes +
//! R2 reads from sibling region + 503 write block + customer notification),
//! plus an in-memory orchestrator that exercises every load-bearing invariant.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`health`] module ships [`RegionHealth`] `#[non_exhaustive]`
//!    3-canonical (`Healthy` / `Degraded` / `Down`) + [`FailoverTrigger`]
//!    `#[non_exhaustive]` 3-canonical (`Rate5xx` / `LatencySloViolation` /
//!    `ConsecutiveFailures`) + multi-signal detection thresholds:
//!    - `RATE_5XX_THRESHOLD_PCT` = 1.0 (> 1% = degraded signal)
//!    - `LATENCY_SLO_CEIL_MS` = 300 (p99 > 300ms = degraded signal; SLO-LAT-CAS-GET)
//!    - `CONSECUTIVE_FAILURES_THRESHOLD` = 3 (within 5s window)
//!    - `SUSTAINED_WINDOW_SECS` = 5 (all signals must be sustained 5s)
//!
//! 2. The [`probe`] module ships [`HealthProbe`] trait +
//!    [`InMemoryHealthProbe`] (per-instance `Arc<Mutex<>>`; F-001 closure;
//!    5min synthetic GET cadence simulated) + [`FailingHealthProbe`]
//!    adversarial fixture.
//!
//! 3. The [`router`] module ships [`FailoverRouter`] trait +
//!    [`InMemoryFailoverRouter`] orchestrator. Multi-signal detection:
//!    ALL 3 signals degraded within 5s sustained window = `RegionHealth::Degraded`.
//!    If degraded: route reads to sibling region (per [`ResidencyGraph`]).
//!    Read-only mode: writes return `FailoverError::WriteBlockedDuringFailover`.
//!    Failover overhead ≤ 50ms p99 (SLO-FAILOVER-OVERHEAD).
//!
//! 4. The [`audit`] module ships [`FailoverAuditEventType`] `#[non_exhaustive]`
//!    2-canonical (`corelink.region.failover.{detected, resolved}`) re-exported
//!    from `corelink-replica-worker` + [`FailoverAuditSink`] shim.
//!
//! 5. The [`error`] module ships [`FailoverError`] `#[non_exhaustive]`
//!    taxonomy (`RegionDegraded` / `WriteBlockedDuringFailover` /
//!    `NoReplicaAvailable` / `Audit` / `Internal`).
//!
//! # Failover graph acyclic guarantee
//!
//! Reuses [`corelink_replica_worker::ResidencyGraph`] — same static acyclic
//! map (WNAM↔ENAM, WEUR↔SAM). `ResidencyGraph::is_acyclic()` returns `true`
//! always. Integration test pins this invariant.
//!
//! # SLO targets
//!
//! - Failover overhead ≤ 50ms p99 (`SLO_FAILOVER_OVERHEAD_MS` = 50).
//! - SLO-LAT-CAS-GET p99 < 300ms preserved during region outage chaos test.
//! - Failover detection within 5s of sustained multi-signal degradation.
//!
//! # wasm32-unknown-unknown compatibility
//!
//! This crate is `wasm32-unknown-unknown` clean. No `ring`, no C toolchain,
//! no `tokio::spawn`. All deps are pure-Rust.

#![forbid(unsafe_code)]

pub mod audit;
pub mod error;
pub mod health;
pub mod probe;
pub mod router;

pub use corelink_replica_worker::{Region, ResidencyGraph};

pub use audit::{
    FailingFailoverAuditSink, FailoverAuditEventType, FailoverAuditRecord, FailoverAuditSink,
    InMemoryFailoverAuditSink,
};
pub use error::FailoverError;
pub use health::{
    FailoverTrigger, RegionHealth, RegionHealthSnapshot, CONSECUTIVE_FAILURES_THRESHOLD,
    LATENCY_SLO_CEIL_MS, RATE_5XX_THRESHOLD_PCT, SLO_FAILOVER_OVERHEAD_MS, SUSTAINED_WINDOW_SECS,
};
pub use probe::{FailingHealthProbe, HealthProbe, InMemoryHealthProbe};
pub use router::{FailoverDecision, FailoverRouter, InMemoryFailoverRouter, ReadMode, WriteMode};
