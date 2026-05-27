//! `corelink-oncall` — oncall scheduler + fatigue tracking + PagerDuty
//! integration trait (WI-S17-005).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the oncall rotation + fatigue tracking + handoff
//! decision tree plus the PagerDuty schedule/events client trait
//! surface every production HTTPS PagerDuty Schedule API + Events API
//! v2 path will satisfy (Terraform IaC, Grafana dashboard wiring,
//! webhook auto-handoff), plus an in-memory rotation ledger that
//! exercises every load-bearing invariant the production wiring
//! relies on. Property tests cover the canonical fatigue threshold
//! decision matrix (Lote 10.17 codex P1 fix canonical: alerts alone
//! NÃO suficientes — hard cutoff/protection enforced).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`engineer`] module ships [`Engineer`] + [`EngineerId`]
//!    canonical identifier (opaque string newtype; tenant-agnostic).
//! 2. The [`tier`] module ships [`Tier`] `#[non_exhaustive]`
//!    3-canonical (`Tier1` / `Tier2` / `Tier3` per WI §6.1.1) +
//!    `escalation_delay_seconds()` (5min Tier1→Tier2; 10min
//!    Tier2→Tier3).
//! 3. The [`severity`] module ships [`Severity`] `#[non_exhaustive]`
//!    4-canonical (`Sev0` / `Sev1` / `Sev2` / `Sev3`) + canonical
//!    label.
//! 4. The [`rotation`] module ships [`Rotation`] + [`ShiftId`] +
//!    [`Shift`] (engineer + tier + start_ms + end_ms; 7-day cap
//!    canonical per Google SRE Workbook Ch 8 + WI §6.1.2).
//! 5. The [`page`] module ships [`PageEvent`] (engineer + severity +
//!    ts_ms + correlation_id) + [`FatigueWindow`]
//!    `#[non_exhaustive]` 2-canonical (`Rolling7d` / `Rolling30d`) +
//!    [`FatigueScore`] (count + window).
//! 6. The [`threshold`] module ships [`FatigueThreshold`]
//!    `#[non_exhaustive]` canonical taxonomy + [`HandoffDecision`]
//!    `#[non_exhaustive]` 3-canonical (`KeepCurrent` /
//!    `RotateToBackup` / `MandatoryRotationBlock`) + canonical
//!    decision matrix per Lote 10.17 codex P1 fix (HARD cutoff /
//!    automatic handoff / 1-month block; alerts alone NÃO
//!    suficientes).
//! 7. The [`ledger`] module ships [`RotationLedger`] orchestrator
//!    (per-engineer page log + handoff event log + `Arc<Mutex<>>`
//!    in-memory; audit-emit-BEFORE-mutation fail-CLOSED envelope per
//!    Lote 10.6bis pattern).
//! 8. The [`pagerduty`] module ships [`PagerDutyClient`] trait +
//!    [`InMemoryPagerDutyClient`] (rotation roster + page dispatch
//!    fake) + [`FailingPagerDutyClient`] adversarial fixture.
//! 9.  The [`audit`] module ships [`OncallAuditEventType`]
//!     `#[non_exhaustive]` 5-canonical taxonomy plus
//!     [`OncallAuditRecord`], [`OncallAuditSink`] trait,
//!     [`InMemoryOncallAuditSink`], and [`FailingOncallAuditSink`].
//! 10. The [`error`] module ships [`OncallError`] `#[non_exhaustive]`
//!     taxonomy (`Audit` / `Pagerduty` / `InvalidRotation` /
//!     `Internal`).
//!
//! # Why PagerDuty is `trait + fake` here, real Schedule/Events API
//! # in PRR ship gate
//!
//! WI-S17-005 lands without Cloudflare Worker secrets bound to
//! `PAGERDUTY_API_KEY` (no remote + Cloudflare Workers + PagerDuty
//! staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production wiring bug would expose:
//! 7-day shift cap; 2-week protection-period veto; fatigue threshold
//! canonical matrix (soft alert vs HARD handoff per Lote 10.17 codex
//! P1 fix); audit-of-audit fail-CLOSED envelope on every decision
//! arm.
//!
//! # Invariants enforced
//!
//! - Lote 10.17 codex P1 fix canonical: alerts alone NÃO suficientes;
//!   HARD thresholds trigger automatic rotation handoff + mandatory
//!   recovery rotation skip; pinned by
//!   `prop_fatigue_hard_threshold_handoff`.
//! - 7-day shift cap (Google SRE Workbook Ch 8 + WI §6.1.1): no
//!   single shift exceeds 7 days; pinned by `prop_shift_duration_cap`.
//! - 2-week protection-period veto (WI §6.1.2): an engineer in their
//!   post-shift protection window cannot be assigned a new shift;
//!   pinned by `prop_protection_period_veto`.
//! - Audit-emit-BEFORE-mutation (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER;
//!   Lote 10.6bis pattern): audit failure aborts the ledger mutation
//!   and returns a typed error; pinned by
//!   `prop_audit_emit_before_mutation`.
//! - Tenant-agnostic metrics (CTRL-PRIV-001; INV-OBS-CARDINALITY-
//!   BUDGET-RESPECTED): fatigue metrics carry only `{tier,
//!   in_rotation}` labels; pinned by
//!   `prop_metrics_cardinality_bounded`.
//!
//! # Production wiring (deferred to PRR ship gate)
//!
//! - PagerDuty Schedule API HTTPS PUT for rotation/restriction/
//!   escalation_policy CRUD via `worker::send_future` fire-and-forget
//!   per Lote 10.7bis R5 P0-3 (NEVER `tokio::spawn`).
//! - PagerDuty Events API v2 webhook receiver for automatic handoff
//!   (HARD threshold breach → rotate-to-backup API call).
//! - Terraform IaC for 3 schedules (`corelink-oncall-tier-1` /
//!   `corelink-oncall-tier-2` / `corelink-oncall-tier-3`).
//! - Grafana Cloud dashboard ingestion of
//!   `dashboards/grafana/DASH-ONCALL-FATIGUE.json` via Grafana API.
//! - D1 migration `0036_oncall_pages.sql` apply in production schema.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod engineer;
pub mod error;
#[cfg(feature = "production")]
pub mod events;
pub mod ledger;
pub mod page;
pub mod pagerduty;
pub mod rotation;
pub mod severity;
pub mod threshold;
pub mod tier;

pub use audit::{
    canonical_oncall_audit_event_strings, FailingOncallAuditSink, InMemoryOncallAuditSink,
    OncallAuditEmitError, OncallAuditEventType, OncallAuditRecord, OncallAuditSink,
};
pub use engineer::{Engineer, EngineerId};
pub use error::{OncallError, OncallPagerDutyError};
pub use ledger::{LedgerOutcome, RotationLedger};
pub use page::{FatigueScore, FatigueWindow, PageEvent};
pub use pagerduty::{
    canonical_pagerduty_actions, FailingPagerDutyClient, InMemoryPagerDutyClient,
    PagerDutyAssignment, PagerDutyClient, PagerDutyEventAction, PagerDutyScheduleKey,
};
#[cfg(feature = "production")]
pub use events::{
    backoff_wait, map_severity, Clock, EventAction, HttpPagerDutyClient, HttpResponse,
    HttpTransport, NoopClock, PageContext, PagerDutyAuditSink, PagerDutyEvent,
    ReqwestBlockingTransport, RoutingKey, SendOutcome, StdClock, BACKOFF_BASE_MS, BACKOFF_CAP_MS,
    MAX_RETRIES, PAGERDUTY_EVENTS_V2_URL, PAYLOAD_MAX_BYTES,
};
pub use rotation::{Rotation, Shift, ShiftId};
pub use severity::{canonical_severities, Severity};
pub use threshold::{
    canonical_handoff_decisions, decide_handoff as decide_handoff_via_pure_logic_proxy,
    FatigueThreshold, HandoffDecision,
};
pub use tier::{canonical_tiers, Tier};

/// Crate canonical schema version constant.
#[must_use]
pub const fn oncall_schema_version() -> u32 {
    1
}

/// 7-day shift cap canonical (Google SRE Workbook Ch 8 + WI §6.1.1).
pub const SHIFT_CAP_SECONDS: u64 = 7 * 24 * 60 * 60;

/// 2-week post-shift protection-period canonical (WI §6.1.2).
pub const PROTECTION_PERIOD_SECONDS: u64 = 14 * 24 * 60 * 60;
