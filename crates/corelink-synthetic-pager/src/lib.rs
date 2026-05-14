//! `corelink-synthetic-pager` — WI-S20-006 weekly synthetic page drill.
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-
//! defer`), this crate ships the **pure-logic skeleton** of the weekly
//! synthetic page drill (cron-triggered synthetic SEV-2 page emit →
//! MTTA measurement against the active region rotation engineer → ack
//! vector classification → drill record persistence) plus the
//! `DrillRecorder` trait surface every production CF Cron Worker
//! adapter + PagerDuty Events API v2 receiver will satisfy.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`region`] module ships [`Region`] `#[non_exhaustive]`
//!    3-canonical (`Americas` / `EMEA` / `APAC`) + follow-the-sun
//!    8-hour shift boundaries (UTC window labels).
//! 2. The [`vector`] module ships [`AckVector`] `#[non_exhaustive]`
//!    3-canonical (`MobilePush` / `Sms` / `Email`) + canonical wire
//!    label.
//! 3. The [`severity`] module ships [`DrillSeverity`]
//!    `#[non_exhaustive]` (`Sev2Synthetic`) — drill incidents carry a
//!    dedicated synthetic-severity that PagerDuty escalation routes
//!    MUST NOT confuse with a production SEV-2.
//! 4. The [`request`] module ships [`SyntheticDrillId`] (validated
//!    `SP-<UUID>` newtype) + [`SyntheticPageRequest`] (target region +
//!    correlation id + emit timestamp).
//! 5. The [`outcome`] module ships [`AckOutcome`] `#[non_exhaustive]`
//!    3-canonical (`Acked` / `Unacked` / `Escalated`) + [`MttaMs`]
//!    newtype + canonical 5-minute MTTA budget cap (300_000 ms per WI
//!    §2.1 + spec contract §10.s20.8 + RB-INCIDENT-ESCALATION-MATRIX).
//! 6. The [`decide`] module ships [`decide_drill_outcome`] pure helper
//!    (per-emit decision tree: acked-within-budget → `Acked`; acked-
//!    over-budget OR escalated-before-ack → `Escalated`; never-acked
//!    within 15 min hard window → `Unacked`).
//! 7. The [`record`] module ships [`DrillRecord`] (drill_id + region +
//!    rotation engineer slug + emit_ts_ms + ack_ts_ms + ack_vector +
//!    mtta_ms + outcome) + [`DrillRecorder`] trait (D1 binding) +
//!    [`InMemoryDrillRecorder`] + [`FailingDrillRecorder`] adversarial
//!    fixture.
//! 8. The [`error`] module ships [`SyntheticDrillError`]
//!    `#[non_exhaustive]` taxonomy (`InvalidDrillId` / `EmptyEngineer`
//!    / `EmitTimestampInFuture` / `AckBeforeEmit` / `Recorder` /
//!    `Internal`).
//!
//! # Why the cron is `trait + fake` here, real CF Cron + PagerDuty in
//! # PRR ship gate
//!
//! WI-S20-006 ships without Cloudflare Worker secrets bound to
//! `PAGERDUTY_API_KEY` + `SYNTHETIC_PAGE_ROUTING_KEY` (no remote +
//! Cloudflare Workers + PagerDuty staging are HARD inflection points
//! per `corelink_autonomous_execution_charter.md`). The fake covers
//! the algorithmic invariants that a production wiring bug would
//! expose: 5-min MTTA budget cap; ack-before-emit rejection;
//! escalation classification when MTTA breached; recorder failure
//! propagation (fail-CLOSED).
//!
//! # Invariants enforced
//!
//! - 5-minute MTTA budget canonical (WI §2.1 + spec contract
//!   §10.s20.8): any ack with `mtta_ms > 300_000` is classified as
//!   `Escalated`; pinned by `prop_mtta_budget_cap_enforced`.
//! - 15-minute hard unack window (RB-INCIDENT-ESCALATION-MATRIX P0
//!   T+15): drills with no ack inside 900_000 ms are classified as
//!   `Unacked`; pinned by `prop_unack_hard_window`.
//! - Ack timestamp monotonicity: `ack_ts_ms >= emit_ts_ms`; pinned by
//!   `prop_ack_after_emit`.
//! - Synthetic severity isolation: production SEV-2 escalation paths
//!   MUST NOT page from a synthetic incident; the synthetic taxonomy
//!   uses a dedicated `Sev2Synthetic` variant routed through a
//!   `synthetic-drill` PagerDuty service.
//!
//! # Production wiring (deferred to PRR ship gate)
//!
//! - CF Cron Worker `[triggers] crons = ["0 14 * * 1"]` weekly trigger
//!   at 14:00 UTC Monday (mid-Americas-shift; sweeps all 3 regions
//!   over a 4-week cycle via region selector cron arg).
//! - PagerDuty Events API v2 POST `synthetic-drill` service routing
//!   key → on-call dispatch.
//! - PagerDuty webhook receiver: ingests ack event → records
//!   `ack_ts_ms` + `ack_vector` to D1 `synthetic_page_drills`.
//! - D1 migration `0042_synthetic_page_drills.sql` apply.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod decide;
pub mod error;
pub mod outcome;
pub mod record;
pub mod region;
pub mod request;
pub mod severity;
pub mod vector;

pub use decide::decide_drill_outcome;
pub use error::SyntheticDrillError;
pub use outcome::{AckOutcome, MttaMs, MTTA_BUDGET_MS, UNACK_HARD_WINDOW_MS};
pub use record::{
    DrillRecord, DrillRecorder, FailingDrillRecorder, InMemoryDrillRecorder,
};
pub use region::{canonical_regions, Region};
pub use request::{SyntheticDrillId, SyntheticPageRequest};
pub use severity::{canonical_drill_severities, DrillSeverity};
pub use vector::{canonical_ack_vectors, AckVector};

/// Crate canonical schema version constant.
#[must_use]
pub const fn synthetic_pager_schema_version() -> u32 {
    1
}
