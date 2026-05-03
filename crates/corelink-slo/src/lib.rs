//! `corelink-slo` — multi-burn-rate SLO alerting + PagerDuty
//! dispatcher primitive (WI-S09-006).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the multi-burn-rate alert decision tree plus the
//! PagerDuty dispatcher trait surface every production HTTPS
//! PagerDuty Events API v2 path will satisfy (Prometheus rule
//! ingestion, auto-quarantine flapping cron, Twilio fallback,
//! Terraform IaC), plus an in-memory orchestrator that exercises
//! every load-bearing invariant the production wiring relies on.
//! Property tests pinned at 10k iter against the orchestrator cover
//! the canonical Google SRE Workbook Ch 5 Table 4 multi-window
//! multi-burn-rate matrix (the load-bearing falsifiability target;
//! a calibration drift on the burn-rate multipliers would silently
//! regress alert recall).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`window`] module ships [`BurnRateWindow`]
//!    `#[non_exhaustive]` 4-canonical (`Fast1h` / `Medium6h` /
//!    `Slow24h` / `Long3d` per Google SRE Workbook Ch 5 Table 4) +
//!    `threshold_multiplier()` (14.4× / 6× / 3× / 1× canonical) +
//!    `for_duration_seconds()` (alert hold-down per window) +
//!    `prometheus_range_label()` (PromQL `[5m]`/`[30m]`/`[1h]`/`[6h]`
//!    range vector label).
//! 2. The [`decision`] module ships [`AlertDecision`]
//!    `#[non_exhaustive]` 5-canonical (`Quiet` / `TicketSev3` /
//!    `TicketSev2` / `PageSev1` / `PageSev0`) + `severity_label()`
//!    canonical SEV-string mapping per `observability_model.md §3.1` +
//!    `is_page()` / `is_ticket()` / `is_quiet()` predicates.
//! 3. The [`definition`] module ships [`SloDefinition`] (sli /
//!    target_pct / error_budget_pct / window_days) + [`Sli`]
//!    `#[non_exhaustive]` canonical SLI taxonomy
//!    (`AvailCasGet` / `AvailCasPut` / `AvailAcLookup` / `AvailAuth` /
//!    `LatencyCasGetP99` / `DedupRatio` / `RateLimitWithinQuota` per
//!    `slo_catalog.md`).
//! 4. The [`calculator`] module ships [`BurnRateCalculator`] +
//!    [`BurnRateSample`] (`errors_in_window` / `total_in_window`) +
//!    `error_rate_in_window` pure-logic + `decide` per Google SRE
//!    Workbook Table 4 boundary discipline.
//! 5. The [`pagerduty`] module ships [`PagerDutyDispatcher`] trait +
//!    [`InMemoryPagerDutyDispatcher`] (dedup-key idempotent per
//!    PagerDuty Events API v2 §dedup_key) + [`FailingPagerDutyDispatcher`]
//!    adversarial fixture + [`PagerDutyEvent`] envelope.
//! 6. The [`alert`] module ships [`MultiBurnRateAlert`] orchestrator
//!    (per-instance `Arc<Mutex<>>` per-tenant flapping ledger +
//!    dispatch-on-page IFF AlertDecision::is_page() + audit-emit-
//!    BEFORE-mutation fail-CLOSED envelope on every emit arm).
//! 7. The [`audit`] module ships [`SloAuditEventType`]
//!    `#[non_exhaustive]` 5-canonical taxonomy + [`SloAuditRecord`] +
//!    [`SloAuditSink`] trait + [`InMemorySloAuditSink`] +
//!    [`FailingSloAuditSink`].
//! 8. The [`error`] module ships [`SloError`] `#[non_exhaustive]`
//!    taxonomy (`Audit` / `Dispatcher` / `InvalidSloDefinition` /
//!    `Internal`).
//!
//! # Why multi-burn-rate is `trait + fake` here, real PagerDuty
//! # Events API v2 in WI-S09-007
//!
//! S-09 lands without Cloudflare Worker secrets bound to PagerDuty
//! API token (no remote + Cloudflare Workers + Pager­Duty staging are
//! HARD inflection points per `corelink_autonomous_execution_charter.md`).
//! The fake covers the algorithmic invariants that a production wiring
//! bug would expose: per-tenant ledger isolation; dedup-key idempotency
//! under repeated dispatch of the same alert tuple; canonical Google
//! SRE Workbook Table 4 boundary discipline (14.4× / 6× / 3× / 1× per
//! window); audit-of-audit fail-CLOSED envelope on every decision arm.
//! The live PagerDuty Events API v2 HTTPS POST + Prometheus rule
//! ingestion (`promtool test rules`) + auto-quarantine flapping cron +
//! Twilio fallback + Terraform IaC for 3 services (staging / prod-us /
//! prod-eu) run alongside WI-S09-007 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - Google SRE Workbook Ch 5 Table 4 boundary discipline (HIGH; WI
//!   §1 invariant 1; sprint contract §5.5 R-S09-13): for any (sli,
//!   window, error_rate), the canonical decision matches Table 4
//!   exactly. Pinned by `prop_alert_decision_canonical_table4` (10k
//!   iter PR gate; nightly 100k via `PROPTEST_CASES` env var override
//!   per S-07 P1-2 fix).
//! - INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; lift from S-07 P1-1
//!   fix plus Lote 10.6bis pattern): audit-of-audit emit BEFORE state
//!   mutation on every decision arm (`burn_rate_evaluated` /
//!   `alert_fired` / `alert_quiet` / `page_dispatched` /
//!   `ticket_filed`); audit failure aborts the alert emit + returns a
//!   typed error. Pinned by `prop_audit_emit_per_decision_arm`.
//! - INV-TENANT-ISOLATION (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): per-tenant flapping ledger; tenant A's burn rate
//!   never affects tenant B's decisions. Pinned by
//!   `prop_tenant_isolation`.
//! - PagerDuty Events API v2 dedup-key idempotency (informational
//!   invariant): same `(sli, window, tenant)` tuple dispatched twice
//!   under PageSev0/1 collapses to one incident via canonical
//!   dedup-key string. Pinned by
//!   `prop_pagerduty_dispatch_idempotent_dedup_key`.
//! - Long-burn-no-page (informational invariant; Google SRE canonical):
//!   3d×1× decision is `TicketSev3`, never any `Page*` arm. Pinned by
//!   `prop_long_burn_does_not_page`.
//! - Fast-burn-pages (informational invariant; Google SRE canonical):
//!   1h×14.4× decision is `PageSev0` or `PageSev1`. Pinned by
//!   `prop_fast_burn_pages_immediately`.
//! - Idempotent-no-burn (informational invariant): error_rate=0 is
//!   always `Quiet`. Pinned by `prop_idempotent_no_burn`.
//! - Burn-rate-math-correct (informational invariant): for any
//!   (errors, total) with total > 0, `error_rate = errors/total`.
//!   Pinned by `prop_burn_rate_calculation_correct`.
//!
//! # Production wiring (deferred to WI-S09-007)
//!
//! - PagerDuty Events API v2 HTTPS POST (`/v2/enqueue` JSON envelope:
//!   `routing_key` + `dedup_key` + `event_action` + `payload`) via
//!   `worker::send_future` fire-and-forget per Lote 10.7bis R5 P0-3
//!   (NEVER `tokio::spawn`).
//! - Terraform IaC for 3 PagerDuty services (staging / prod-us /
//!   prod-eu) per sprint contract §5.5 R-S09-14 (Lote 10.9bis P0-F
//!   corrected from prior 5).
//! - Prometheus rule ingestion via `promtool test rules
//!   dashboards/alerts/dash-slo-multi-burn.yml` CI gate at
//!   `.github/workflows/alert-rules-validate.yml`.
//! - Auto-quarantine flapping cron (sprint contract §14.s09.2): weekly
//!   `sum_over_time(increase(ALERTS_FOR_STATE{alertstate="firing"}[5m])[7d:5m]) > 3`
//!   query then silence policy applied via PagerDuty API; 5-Why
//!   post-mortem mandatory before un-quarantine.
//! - Twilio backup SMS fallback when PagerDuty API outage.
//! - SLO coverage CI hook: every SLI in `slo_catalog.md` MUST have
//!   multi-burn-rate alert in `dashboards/alerts/dash-slo-multi-burn.yml`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod alert;
pub mod audit;
pub mod calculator;
pub mod decision;
pub mod definition;
pub mod error;
pub mod pagerduty;
pub mod window;

pub use alert::{AlertEvaluation, AlertEvaluationOutcome, MultiBurnRateAlert};
pub use audit::{
    canonical_slo_audit_event_strings, FailingSloAuditSink, InMemorySloAuditSink,
    SloAuditEmitError, SloAuditEventType, SloAuditRecord, SloAuditSink,
};
pub use calculator::{BurnRateCalculator, BurnRateSample};
pub use decision::{canonical_alert_decisions, AlertDecision};
pub use definition::{canonical_slis, Sli, SloDefinition};
pub use error::{SloError, SloPagerDutyDispatchError};
pub use pagerduty::{
    canonical_pagerduty_actions, FailingPagerDutyDispatcher, InMemoryPagerDutyDispatcher,
    PagerDutyDispatcher, PagerDutyEvent, PagerDutyEventAction, PagerDutyServiceKey,
};
pub use window::{canonical_burn_rate_windows, BurnRateWindow};

/// Crate canonical schema version constant.
#[must_use]
pub const fn slo_schema_version() -> u32 {
    1
}
