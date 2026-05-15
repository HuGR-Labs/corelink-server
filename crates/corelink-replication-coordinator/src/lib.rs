//! `corelink-replication-coordinator` — Multi-region replication coordinator
//! (R-PREP wave-15 follow-on to DEBT-011 wave 11+13, gated by DR-16 wave-14).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! this crate ships the **pure-logic skeleton** of the multi-region
//! replication coordinator the production Cloudflare DO singleton-lock will
//! satisfy (real DO heartbeat + DO-singleton primary-promotion lock + Neon
//! multi-region + audit bus emit), plus an in-memory orchestrator that
//! exercises every load-bearing invariant the production wiring relies on.
//!
//! Specifically the crate ships:
//!
//! 1. The [`state`] module ships [`RegionRole`] `#[non_exhaustive]`
//!    3-canonical (`Primary` / `HotStandby` / `Replica`) + the
//!    canonical 24 h hot-standby cool-down constant
//!    [`HOT_STANDBY_COOLDOWN_SECONDS`] (= 86_400) per
//!    `specs/03_architecture/resilience_patterns.md §Failback`.
//!
//! 2. The [`heartbeat`] module ships [`Heartbeat`] +
//!    [`HeartbeatRegistry`] + [`InMemoryHeartbeatRegistry`] (per-instance
//!    `Arc<Mutex<>>`; F-001 closure). Per-region last-seen tracking
//!    + [`HEARTBEAT_STALE_SECONDS`] (= 30) staleness threshold (matches the
//!    replica-worker tick cadence per DEBT-011 P0-002).
//!
//! 3. The [`lag`] module ships [`LagBundle`] — the 4-domain lag view
//!    consumed by the coordinator: `r2_seconds` + `d1_seconds` + `kv_seconds`
//!    + `neon_seconds` — and the per-domain SLO ceilings
//!    `SLO_REPLICATION_LAG_{R2,D1,KV,NEON}_SECONDS` (bound from wave-14
//!    DR-16). `LagBundle::within_slo()` returns `true` only when all four
//!    domains are within their respective ceilings (Neon is informational
//!    per P2-001 so it is checked against its soft ceiling but always
//!    considered "within SLO" for promotion-readiness purposes).
//!
//! 4. The [`audit`] module ships [`CoordinatorAuditEventType`]
//!    `#[non_exhaustive]` 4-canonical
//!    (`corelink.failover.region_promoted.v1` /
//!    `corelink.failover.region_demoted.v1` /
//!    `corelink.failover.failback_blocked.v1` /
//!    `corelink.failover.failback_committed.v1`) +
//!    [`CoordinatorAuditSink`] trait + [`InMemoryCoordinatorAuditSink`] +
//!    [`FailingCoordinatorAuditSink`] adversarial fixture (the latter
//!    pins the audit-emit-fail-CLOSED ordering — state MUST NOT mutate
//!    when audit emit fails; verified by integration tests).
//!
//! 5. The [`error`] module ships [`CoordinatorError`] `#[non_exhaustive]`
//!    taxonomy (`UnknownRegion` / `NoEligibleReplica` /
//!    `SplitBrainRejected` / `PrimaryStillEligible` /
//!    `CooldownNotElapsed` / `Audit` / `Internal`).
//!
//! 6. The [`coordinator`] module ships [`ReplicationCoordinator`] trait +
//!    [`InMemoryReplicationCoordinator`] orchestrator. The decision tree:
//!
//!    - `route_write(region)`: if `region` role == `Primary` AND heartbeat
//!      fresh AND lag within SLO → admit. Otherwise return the canonical
//!      error.
//!    - `evaluate(region, now_ms)`: returns one of
//!      [`PromotionDecision`]:
//!         - `KeepPrimary` (primary healthy)
//!         - `PromoteReplica(replica)` (primary stale heartbeat AND at
//!           least one replica `lag < SLO_TARGET` for **all** monitored
//!           domains)
//!         - `NoEligibleReplica` (primary stale BUT every replica also
//!           breaches SLO — escalate to runbook, do NOT auto-promote;
//!           safer to surface stale reads than partition-split-brain).
//!    - `promote(primary, replica, now_ms)`: emits
//!      `region_promoted.v1` BEFORE mutation; demotes old primary to
//!      `HotStandby` with `cooldown_start_ms = now_ms`; rejects with
//!      `SplitBrainRejected` if any other region is already `Primary`
//!      simultaneously (INV-FAILOVER-NO-SPLIT-BRAIN).
//!    - `failback(region, now_ms)`: only allowed after
//!      `now_ms - cooldown_start_ms >= HOT_STANDBY_COOLDOWN_SECONDS * 1000`.
//!      Emits `failback_committed.v1` BEFORE the role flip back to
//!      `Primary`.
//!    - `replication_status()`: returns the full multi-region view
//!      consumed by `/health` and customer dashboards.
//!
//!    A **singleton DO lock** is modelled by the per-instance `Mutex` —
//!    in production this is satisfied by the Cloudflare Durable Object
//!    singleton ID (deferred per charter).
//!
//! # Failback 24 h hot-standby cool-down
//!
//! Canonical per `specs/03_architecture/resilience_patterns.md`. After a
//! `promote` event, the demoted region stays in `HotStandby` for 24 h
//! (synthetic GETs allowed; writes blocked) before
//! [`ReplicationCoordinator::failback`] is allowed to re-promote it.
//! Premature failback returns [`CoordinatorError::CooldownNotElapsed`].
//!
//! # INV-FAILOVER-NO-SPLIT-BRAIN
//!
//! The coordinator holds a singleton lock during the promotion sequence
//! (modelled here as the per-instance `Mutex`; production = DO singleton
//! ID per `failover_no_split_brain.tla`). Concurrent promotion attempts
//! from different "callers" against the same coordinator instance are
//! linearized by the lock; a second caller observing an already-promoted
//! topology returns [`CoordinatorError::SplitBrainRejected`]. Integration
//! tests exercise the rejection path.
//!
//! # Audit fail-CLOSED ordering (S-06 P0-2 lesson)
//!
//! `lookup → emit_audit → mutate_state`
//!
//! State is **NEVER** mutated if audit emit fails. Tests verify the role
//! map UNCHANGED on audit emit failure.
//!
//! # wasm32-unknown-unknown compatibility
//!
//! This crate is `wasm32-unknown-unknown` clean. No `tokio` in `src/`,
//! no `ring`, no C toolchain dependencies, all deps pure-Rust.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::format_in_format_args)]
#![allow(clippy::doc_lazy_continuation)]

pub mod audit;
pub mod coordinator;
pub mod error;
pub mod heartbeat;
pub mod lag;
pub mod state;

pub use audit::{
    CoordinatorAuditEventType, CoordinatorAuditRecord, CoordinatorAuditSink,
    FailingCoordinatorAuditSink, InMemoryCoordinatorAuditSink,
};
pub use coordinator::{
    InMemoryReplicationCoordinator, PromotionDecision, RegionStatus, ReplicationCoordinator,
    ReplicationStatus,
};
pub use error::CoordinatorError;
pub use heartbeat::{
    Heartbeat, HeartbeatRegistry, InMemoryHeartbeatRegistry, HEARTBEAT_STALE_SECONDS,
};
pub use lag::{
    LagBundle, SLO_REPLICATION_LAG_D1_SECONDS, SLO_REPLICATION_LAG_KV_SECONDS,
    SLO_REPLICATION_LAG_NEON_SECONDS, SLO_REPLICATION_LAG_R2_SECONDS,
};
pub use state::{RegionRole, HOT_STANDBY_COOLDOWN_SECONDS};

// Re-export the canonical Region so consumers don't need to depend on
// `corelink-replica-worker` directly.
pub use corelink_replica_worker::Region;
