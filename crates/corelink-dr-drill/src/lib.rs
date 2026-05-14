//! `corelink-dr-drill` — DR Drill Scheduler + CF Region Outage Simulator
//! (WI-S17-002 — STANDARD lane).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! this crate ships the **pure-logic skeleton** of the DR drill scheduler
//! the production CF Worker handler will satisfy (real CF Cron trigger at
//! `0 6 1 1,7 *` Jan 1 + Jul 1 06:00 UTC + chaos-mesh staging integration +
//! D1 `dr_drill_runs` table writes + R2 7y archive), plus an in-memory
//! orchestrator that exercises every load-bearing invariant.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`schedule`] module ships [`DrillCadence`] `#[non_exhaustive]`
//!    2-canonical (`Semestral` / `Annual`) + [`DrillSchedule`] (cron expression
//!    `0 6 1 1,7 *` for `Semestral`; computed window per cycle) +
//!    [`DrillCycle`] `#[non_exhaustive]` 3-canonical (`CfRegionOutage` /
//!    `D1PrimaryLoss` / `ByokKeyCompromise`) + [`DrillScheduler`] trait +
//!    [`InMemoryDrillScheduler`].
//!
//! 2. The [`env_guard`] module ships [`DrillEnv`] `#[non_exhaustive]`
//!    3-canonical (`Staging` / `Test` / `Production`) + [`require_staging`]
//!    helper (rejects `Production` with [`DrillError::ProdEnvForbidden`];
//!    code-level enforcement matching the canonical TS guard in WI §1
//!    cycle_1_cf_region_outage.ts).
//!
//! 3. The [`run`] module ships [`DrillRun`] record (`drill_id` +
//!    `started_at_ms` + `simulated_region` + `failover_target` +
//!    `rto_seconds` + `rpo_seconds` + `slo_impact` + `completed_at_ms` +
//!    `status`) + [`DrillStatus`] `#[non_exhaustive]` 5-canonical
//!    (`Scheduled` / `InProgress` / `Completed` / `Failed` / `Aborted`) +
//!    [`RtoRpoMeasurement`] + [`SloImpact`].
//!
//! 4. The [`outage`] module ships [`CfRegionOutageSimulator`] trait +
//!    [`InMemoryCfRegionOutageSimulator`] (marks region `synthetic_outage`;
//!    routes read/write traffic to sibling region via
//!    `corelink-failover-router::ResidencyGraph`; measures RTO/RPO against
//!    SLO targets: RTO ≤ 1800s + RPO ≤ 60s per WI §6 DoD).
//!
//! 5. The [`error`] module ships [`DrillError`] `#[non_exhaustive]` taxonomy
//!    (`ProdEnvForbidden` / `RegionUnknown` / `NoSiblingAvailable` /
//!    `SloViolation` / `Internal`).
//!
//! # CF Cron expression
//!
//! Semestral cadence = every 6 months = Jan 1 + Jul 1 at 06:00 UTC.
//! Cron: `0 6 1 1,7 *` (minute=0, hour=6, day-of-month=1, month=1 or 7,
//! day-of-week=*). See [`SEMESTRAL_CRON`].
//!
//! # SLO targets (RTO/RPO)
//!
//! - RTO (Recovery Time Objective) ≤ 1800s (30min) — see [`RTO_CEIL_SECONDS`].
//! - RPO (Recovery Point Objective) ≤ 60s — see [`RPO_CEIL_SECONDS`].
//!
//! # Staging-only enforcement
//!
//! Per Lote 10.17 codex P0 canonical fix, env enforcement is code-level
//! (not Gherkin assertion). `require_staging` rejects `Production` with
//! `ProdEnvForbidden` synchronously before any state mutation.
//!
//! # wasm32-unknown-unknown compatibility
//!
//! This crate is `wasm32-unknown-unknown` clean. No `ring`, no C toolchain,
//! no `tokio::spawn`. Pure-Rust deps only.

#![forbid(unsafe_code)]

pub mod env_guard;
pub mod error;
pub mod outage;
pub mod run;
pub mod schedule;

pub use corelink_failover_router::{Region, ResidencyGraph};

pub use env_guard::{require_staging, DrillEnv};
pub use error::DrillError;
pub use outage::{
    CfRegionOutageSimulator, InMemoryCfRegionOutageSimulator, OutageOutcome,
    RTO_CEIL_SECONDS, RPO_CEIL_SECONDS,
};
pub use run::{DrillRun, DrillStatus, RtoRpoMeasurement, SloImpact};
pub use schedule::{
    DrillCadence, DrillCycle, DrillSchedule, DrillScheduler, InMemoryDrillScheduler,
    SEMESTRAL_CRON,
};
