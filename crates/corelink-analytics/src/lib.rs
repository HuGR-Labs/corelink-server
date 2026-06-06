//! `corelink-analytics` — Worker Analytics Engine RED metrics observer
//! + cardinality validator (WI-S09-001).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the observability emit primitive: trait surfaces every
//! production Cloudflare Workers Analytics Engine binding + Prom remote
//! write shim will satisfy, plus an in-memory observer that exercises
//! every load-bearing invariant the production wiring relies on.
//! Property tests pinned at 10k iter against the orchestrator cover
//! INV-OBS-CARDINALITY-BUDGET (HIGH; per-metric ≤ 20k unique
//! label-tuples + global ≤ 100k), INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
//! (audit fail-closed envelope; emit BEFORE state mutation), and the
//! canonical 9-RED + 6-USE metric kind taxonomy per
//! `observability_model.md §4.2`.
//!
//! Specifically, the crate ships:
//!
//! 1. The canonical SQL artifact
//!    `migrations/d1/0015_analytics_cardinality_budgets.sql` embedded
//!    via [`MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS`] so production
//!    code can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema mirrors the per-metric budget +
//!    observed-tuple ledger; durable reload across worker restarts.
//! 2. The [`canonical`] module ships [`RedMetricKind`]
//!    (`#[non_exhaustive]` 9-RED + 6-USE canonical metric taxonomy:
//!    `corelink_cas_put_requests_total` / `corelink_cas_put_duration_seconds`
//!    / `corelink_cas_get_bytes_total` / `corelink_ac_lookup_requests_total`
//!    / `corelink_gc_runs_total` / `corelink_dedup_ratio` /
//!    `corelink_rate_limit_rejects_total` /
//!    `corelink_privacy_dsr_active_total` /
//!    `corelink_billing_events_emitted_total` + 6 USE Cloudflare
//!    runtime metrics) + the canonical metric-name list
//!    [`canonical_metric_names`] for cross-component regression tests.
//! 3. The [`labels`] module ships [`MetricLabelTuple`] (the canonical
//!    enum-typed label cartesian; `tenant_tier` / `region` / `result` /
//!    `layer` / `reason` / `phase` / `hit_miss` / `op_type` /
//!    `dsr_type` / `bucket` / `database` / `namespace` / `do_class` /
//!    `event_type` per the 9-RED + 6-USE metrics — NEVER raw `tenant_id`
//!    / `trace_id` / `request_id` / `blob_digest` cardinality-explosion
//!    labels) + canonical 5-tier `Tier` mirror.
//! 4. The [`observer`] module ships [`RedMetricsObserver`] trait +
//!    [`InMemoryRedMetrics`] capture sink (RED triple: rate counter,
//!    error counter, duration histogram with canonical buckets per
//!    OpenMetrics 1.0).
//! 5. The [`validator`] module ships [`CardinalityValidator`] (the
//!    load-bearing piece: rejects emits that would push a metric over
//!    its per-metric budget; INV-OBS-CARDINALITY-BUDGET enforcement at
//!    runtime in addition to the CI-side `cardinality_check.py`
//!    validator).
//! 6. The [`audit`] module ships [`AnalyticsEventType`]
//!    (`#[non_exhaustive]` 3-event taxonomy: `corelink.analytics.{
//!    metric_emitted, cardinality_rejected, budget_exceeded}`) +
//!    [`AnalyticsAuditRecord`] + [`AnalyticsAuditSink`] +
//!    [`InMemoryAnalyticsAuditSink`] capture sink (fail-closed envelope
//!    per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 7. The [`error`] module ships the canonical [`AnalyticsError`]
//!    `#[non_exhaustive]` taxonomy.
//! 8. The [`config`] module ships [`AnalyticsConfig`] (knobs pinned to
//!    canonical defaults: per-metric cardinality budget 20k, global
//!    budget 100k, OpenMetrics-canonical 11-bucket histogram boundaries
//!    per Lote 10.9bis P0-I) + per-instance F-001 closure semantics.
//!
//! # Why analytics is `trait + fake` here, real CF Workers Analytics
//! # Engine binding in WI-S09-007
//!
//! S-09 lands without Cloudflare Workers Analytics Engine bindings
//! wired into CI (no remote + Cloudflare Workers + AE staging are HARD
//! inflection points per `corelink_autonomous_execution_charter.md`).
//! The fake covers the algorithmic invariants that a production
//! binding bug would expose: cardinality budget enforcement at the
//! emit boundary; idempotent repeat label-set (same tuple twice = 1
//! unique entry); RED counter monotonicity; histogram bucket placement
//! per OpenMetrics 1.0; tenant-tier isolation (no cross-budget
//! contamination); audit fail-closed envelope on every decision arm.
//! The live AE-binding + Mimir conformance tests run alongside
//! WI-S09-007 (PRR ship gate) once miniflare/wrangler-dev integration
//! tests land.
//!
//! # Cripto-driven invariants enforced
//!
//! - INV-OBS-CARDINALITY-BUDGET (HIGH; spec_contract §8 +
//!   invariant_registry §3.12): per-metric ≤ 20k unique label-tuples
//!   plus global ≤ 100k. Validator rejects emits that would push a
//!   metric over budget; durable mirror in `analytics_cardinality_*`
//!   D1 tables keeps the SEV-2 alert source durable across DO
//!   restarts. Pinned by `prop_cardinality_budget_enforced` plus
//!   `prop_cardinality_idempotent_repeat_label_set`.
//! - INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; lift from WI-S07-003
//!   P1-1 fix plus Lote 10.6bis pattern): audit emit BEFORE state
//!   mutation on every decision arm (`metric_emitted`,
//!   `cardinality_rejected`, `budget_exceeded`); audit failure aborts
//!   the emit plus returns a typed error (production wiring maps to
//!   503 so the audit gap doesn't surface to the client as 200).
//!   Pinned by `prop_audit_emit_per_decision_arm`.
//! - INV-TENANT-ISOLATION (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): the cardinality ledger is metric-scoped (NOT
//!   tenant-scoped). `tenant_id` is FORBIDDEN as a label by the type
//!   system — only `Tier` (5-tier canonical:
//!   free/solo/team/business/enterprise per Lote 10.7bis P0-7) appears
//!   in label tuples. Pinned by `prop_tenant_isolation`.
//! - RED counter monotonicity (informational invariant; OpenMetrics
//!   1.0 spec §counter): rate + errors counters never decrement.
//!   Pinned by `prop_red_rate_monotone` plus `prop_red_errors_monotone`.
//! - OpenMetrics histogram bucket placement (Lote 10.9bis P0-I):
//!   duration observation goes to the canonical bucket boundary per
//!   `AnalyticsConfig::histogram_bucket_boundaries`. Pinned by
//!   `prop_red_duration_histogram_bucket_correct`.
//!
//! # Production wiring (deferred to WI-S09-007)
//!
//! - Analytics Engine binding (`worker::send_future` fire-and-forget
//!   per Lote 10.7bis R5 P0-3; NEVER `tokio::spawn`).
//! - Prom remote write → Grafana Mimir tenant (per-region API key in
//!   CF Worker secret).
//! - CI hook `scripts/cardinality_check.py` static validator (parses
//!   PR diff for `RedMetricKind` enum extensions plus `MetricLabelTuple`
//!   field additions; rejects estimated cartesian above 20k per-metric
//!   or above 100k global; rejects forbidden labels `trace_id`,
//!   `tenant_id`, `request_id`, `blob_digest`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod canonical;
pub mod config;
pub mod error;
pub mod labels;
pub mod observer;
pub mod validator;

pub use audit::{
    canonical_audit_event_strings, AnalyticsAuditRecord, AnalyticsAuditSink,
    AnalyticsAuditSinkError, AnalyticsEventType, FailingAnalyticsAuditSink,
    InMemoryAnalyticsAuditSink,
};
pub use canonical::{
    canonical_metric_names, RedMetricKind, RedMetricKindSet, CANONICAL_METRIC_COUNT,
};
pub use config::{
    AnalyticsConfig, CANONICAL_GLOBAL_BUDGET, CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES,
    CANONICAL_PER_METRIC_BUDGET,
};
pub use error::AnalyticsError;
pub use labels::{
    tier_canonical_list, AcHitMissLabel, BillingEventTypeLabel, DoClassLabel, DsrTypeLabel,
    GcPhaseLabel, KvNamespaceLabel, MetricLabelTuple, R2BucketLabel, R2OpTypeLabel,
    RateLimitLayerLabel, RateLimitReasonLabel, RedResultLabel, Region, Tier, FORBIDDEN_LABEL_NAMES,
};
pub use observer::{FailingRedMetrics, InMemoryRedMetrics, RedMetricsObserver};
pub use validator::{CardinalityValidator, ValidatorDecision, ValidatorOutcome};

/// Canonical SQL DDL for the analytics cardinality budget + observed
/// tuple count durable mirror (D1 migration 0015).
///
/// Production wiring at WI-S09-007 passes this string to
/// `wrangler d1 migrations apply --remote`; the same DDL is replayed
/// during local miniflare integration tests.
pub const MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS: &str =
    include_str!("../../../migrations/d1/0015_analytics_cardinality_budgets.sql");

/// Canonical D1 schema version for the analytics emitter mirror tables.
///
/// Mirrors the migration filename prefix; lifted into a typed surface so
/// the production wiring asserts the binding-side schema version
/// matches the embedded migration before accepting any emit.
#[must_use]
pub const fn analytics_schema_version() -> u32 {
    15
}
