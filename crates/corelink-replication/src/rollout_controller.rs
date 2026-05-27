//! `corelink-rollout-controller` — progressive rollout controller
//! implementing PAT-PROGRESSIVE-ROLLOUT-001 (WI-S13-005, S-13 admin
//! plane, HIGH_RISK lane).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the progressive rollout controller the production CF
//! Durable Object per-env singleton will satisfy (real Cloudflare
//! gradual deploy API + D1 atomic state + SEV alert pipeline + real
//! OTel spans), plus an in-memory orchestrator
//! (`InMemoryRolloutController`) that exercises every load-bearing
//! invariant the production wiring relies on. Property tests pinned at
//! 10k iterations (PR gate; 100k via `PROPTEST_CASES`) cover:
//! - `prop_rollout_stage_progression` — advance only when gate criteria
//!   met; never skip a stage (FM-200 blast-radius protection).
//! - `prop_rollout_auto_rollback_triggers` — any of the 3 independent
//!   triggers fires rollback (error rate / SLO burn / p99 latency).
//! - `prop_rollout_budget_cap_enforced` — cumulative consumed > 30% →
//!   freeze (false-positive flood protection).
//! - `prop_rollout_concurrent_blocked` — concurrent start same env →
//!   exactly 1 succeeds.
//!
//! # Modules
//!
//! 1. [`types`] — [`RolloutStage`], [`RolloutStatus`],
//!    [`AutoRollbackTrigger`], [`DeployArtifact`], [`AdminActor`],
//!    [`RolloutHandle`], [`GateMetrics`], [`RolloutDecision`],
//!    [`NextAction`].
//! 2. [`error`] — [`RolloutError`] `#[non_exhaustive]` taxonomy.
//! 3. [`controller`] — [`RolloutController`] trait +
//!    [`InMemoryRolloutController`] orchestrator.
//! 4. [`budget`] — [`BudgetTracker`] monthly cap 30% measured-burn
//!    enforcement + [`BudgetRecord`].
//! 5. [`auto_rollback`] — [`AutoRollbackDriver`] 3-trigger + 5-min
//!    sustained threshold.
//! 6. [`state_machine`] — state machine primitive + transition
//!    validator.
//! 7. [`audit`] — [`RolloutAuditEventType`] `#[non_exhaustive]` +
//!    [`RolloutAuditSink`] trait + [`InMemoryRolloutAuditSink`] +
//!    [`FailingRolloutAuditSink`].
//! 8. [`metrics`] — canonical metric name constants (7 metrics per
//!    §6.1.6).
//!
//! # Why `trait + fake` here, production DO in S-20
//!
//! S-13 lands without Cloudflare Durable Object staging account
//! provisioned (HARD inflection point per
//! `corelink_autonomous_execution_charter.md`). The in-memory fake
//! covers the algorithmic invariants that production wiring would
//! expose: 4-stage state machine correctness; gate criteria
//! evaluation; auto-rollback trigger logic; monthly budget cap
//! 30% measured-burn enforcement; Cosign signature gate
//! (INV-SUPPLY-SIGNED-DEPLOY); concurrent start blocked via D1
//! UNIQUE simulation; audit-emit-BEFORE-mutation fail-CLOSED envelope
//! per Lote 10.6bis pattern + S-07 P1-1 fix + S-09 inheritance. The
//! live `RolloutControllerSingletonDO-{env}` Durable Object + real
//! Cloudflare gradual deploy API + D1 schema
//! (`rollout_state` + `rollout_budget_consumption`) + OTel spans
//! run alongside S-20 GA gate (staging account provisioned).
//!
//! # Invariants enforced
//!
//! - **INV-SUPPLY-SIGNED-DEPLOY** (HIGH, S-12 herdada): rollout starts
//!   only on Cosign-signed deploy artifacts; unsigned → `UnsignedDeploy`
//!   rejection.
//! - **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH, S-12 herdada): rollout
//!   artifact carries Rekor log index.
//! - **INV-AUDIT-APPEND-ONLY** (CRITICAL, S-09 herdada): rollout state
//!   transitions append-only via audit event emission.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL, S-03 herdada):
//!   audit emitted BEFORE state mutation on every decision arm; audit
//!   failure aborts the transition.
//! - **INV-ROLLOUT-SINGLE-ACTIVE** (HIGH, novel): D1 `UNIQUE
//!   (status='active')` → at most 1 active rollout per env; concurrent
//!   start returns `RolloutInFlight`.
//! - **INV-ROLLOUT-BUDGET-CAP** (HIGH, novel): auto-rollback consumes
//!   ≤ 30% monthly error budget; exceedance → freeze + SEV-2.
//! - **INV-ROLLOUT-NO-STAGE-SKIP** (HIGH, novel): stage advance
//!   enforces `RolloutStage::next()`; skip → 403 + audit.
//! - **INV-ROLLOUT-COSIGN-GATE** (HIGH, S-12 herdada): deploy without
//!   Cosign signature rejected at `start()`.

// Crate-level lints (`forbid(unsafe_code)`, `deny(missing_docs)`,
// `deny(missing_debug_implementations)`) are inherited from the
// `corelink-replication` umbrella crate root + Cargo `[lints.rust]`
// stanza. Wave-35 Phase 2 absorption (per
// `specs/_audits/2026-05-26-w35-p2-replication-absorption.md`)
// physically relocated this module from the standalone
// `corelink-rollout-controller` crate into
// `corelink-replication::rollout_controller`; charter constraints
// (audit-fail-CLOSED, `#[non_exhaustive]`, no unwrap/expect/panic
// outside tests) preserved verbatim.

pub mod audit;
pub mod auto_rollback;
pub mod budget;
pub mod controller;
pub mod error;
pub mod metrics;
pub mod state_machine;
pub mod types;

pub use audit::{
    canonical_rollout_audit_event_strings, FailingRolloutAuditSink, InMemoryRolloutAuditSink,
    RolloutAuditEventType, RolloutAuditRecord, RolloutAuditSink,
};
pub use auto_rollback::{AutoRollbackDriver, SustainedTrigger};
pub use budget::{BudgetRecord, BudgetTracker, InMemoryBudgetTracker};
pub use controller::{InMemoryRolloutController, RolloutController};
pub use error::RolloutError;
pub use metrics::{
    METRIC_BUDGET_CONSUMED_RATIO, METRIC_DETECTION_DURATION_SECONDS,
    METRIC_DWELL_SECONDS, METRIC_FREEZE_TOTAL, METRIC_ROLLBACK_DURATION_SECONDS,
    METRIC_ROLLOUT_AUTO_ROLLBACK_TOTAL, METRIC_ROLLOUT_STAGE_GAUGE,
};
pub use state_machine::{RolloutStateMachine, TransitionResult};
pub use types::{
    AdminActor, AutoRollbackTrigger, DeployArtifact, GateMetrics, NextAction, RolloutDecision,
    RolloutHandle, RolloutStage, RolloutStatus,
};

/// Crate schema version constant. Mirrors WI-S13-005 §6.1.5 D1
/// migration slot (`migrations/d1/0026_rollout_state_and_budget.sql`).
#[must_use]
pub const fn rollout_controller_schema_version() -> u32 {
    24
}
