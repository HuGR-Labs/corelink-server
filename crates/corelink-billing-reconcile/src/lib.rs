//! `corelink-billing-reconcile` — 3-layer billing reconciliation
//! orchestrator (WI-S10-004).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the daily reconciliation primitive: trait surfaces
//! every production CF Cron Durable Object cron-trigger 02:00 UTC daily
//! per region + real D1 `billing_reconciliation_drift` ledger + real
//! `stripe_submission_state` flag table + R2 reconciliation-report
//! Object Lock 7y archive will satisfy, plus an in-memory orchestrator
//! that exercises every load-bearing invariant the production wiring
//! relies on. Property tests pinned at 10k iter against the
//! orchestrator cover INV-BILLING-RECONCILE-3-LAYER (HIGH; sprint
//! contract §8 NEW; `invariant_registry.md §3.12 line 166`; 3-layer
//! daily 30d clean), INV-BILLING-NO-LOSS Layer 1 (HIGH; `invariant
//! _registry.md §3.9 line 136`; `Σ R2 events qty = Σ counter qty +
//! Σ counter_late qty per (tenant, region, sku, hour)`),
//! INV-BILLING-NO-DUP (HIGH; `invariant_registry.md §3.9 line 137`;
//! re-running reconcile for the same canonical (tenant,
//! billing_period, run_started_at) is idempotent over the canonical
//! 4-tuple), INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; audit envelope
//! BEFORE state mutation on every decision arm), and
//! INV-TENANT-ISOLATION (CRITICAL, TLA+; per-tenant drift never
//! affects another tenant's decision).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`ReconcileLayerKind`]
//!    `#[non_exhaustive]` 3-element taxonomy (Layer1Emit /
//!    Layer2Aggregate / Layer3Stripe), [`ReconcileDecision`]
//!    `#[non_exhaustive]` 5-element taxonomy (NoDrift / AutoFixed /
//!    TicketSev3 / PageSev2 / PageSev1AutoPaused), [`LayerTotals`],
//!    [`ReconcileSnapshot`], and [`ReconcileConfig`] with the
//!    canonical 4-tier ladder constants ([`event::QUIET_THRESHOLD`]
//!    / [`event::SEV3_TO_SEV2_THRESHOLD`] /
//!    [`event::SEV2_TO_SEV1_THRESHOLD`]) plus auto-fix
//!    dual-condition gate constants
//!    ([`event::AUTO_FIX_MAX_RECORDS`] /
//!    [`event::AUTO_FIX_MAX_PERCENT`]).
//! 2. The [`drift`] module ships [`compute_pairwise_drift_pct`] +
//!    [`compute_max_drift`] + [`compute_drift_record_count`] +
//!    [`auto_fix_gate_fires`] — the canonical scale-invariant
//!    primitives (Lote 10.6bis P0-6 inheritance: dual-condition gate
//!    per tenant; drift over `u128::max` denominator preserves
//!    monotone direction `0.0 ≤ drift ≤ 1.0`).
//! 3. The [`history`] module ships [`DriftHistoryLedger`] trait +
//!    [`InMemoryDriftHistoryLedger`] + [`DriftHistoryRow`] +
//!    [`DriftHistoryInsertOutcome`] + [`FailingDriftHistoryLedger`]
//!    (the SOC 2 CC1.4 + GAAP ASC 606 7-year-retained drift evidence
//!    trail; PRIMARY KEY canonical `(tenant_id, billing_period,
//!    run_started_at)`).
//! 4. The [`stripe_pause`] module ships [`StripeSubmissionControl`]
//!    trait + [`InMemoryStripeSubmissionControl`] +
//!    [`StripePauseOutcome`] + [`FailingStripeSubmissionControl`]
//!    (the SEV-1 arm halts further `corelink-billing-stripe`
//!    usage_record submissions until operator clearance; deferred
//!    production binding flips the canonical D1
//!    `stripe_submission_state` flag the WI-S10-003 adapter reads
//!    at every `record_usage` call).
//! 5. The [`audit`] module ships [`audit::ReconcileAuditEventType`]
//!    `#[non_exhaustive]` 6-event taxonomy:
//!    `corelink.billing_reconcile.{run_started, no_drift,
//!    auto_fixed, ticket_filed, page_dispatched, stripe_paused}` +
//!    [`audit::ReconcileAuditRecord`] + [`audit::ReconcileAuditSink`]
//!    trait + [`audit::InMemoryReconcileAuditSink`] capture sink +
//!    [`audit::FailingReconcileAuditSink`] + the
//!    [`audit::audit_event_for_decision`] canonical mapping
//!    (fail-CLOSED envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` from S-07 P1-1 lift +
//!    Lote 10.6bis pattern + S-09 inheritance).
//! 6. The [`reconciler`] module ships [`BillingReconciler`] trait +
//!    [`InMemoryBillingReconciler`] orchestrator (run pipeline:
//!    audit `run_started` → compute drift → classify decision per
//!    the 4-tier ladder → audit `<decision>` BEFORE state mutation
//!    → drift-history INSERT → SEV-1 arm:
//!    `StripeSubmissionControl::pause`).
//! 7. The [`error`] module ships the canonical
//!    [`error::ReconcileError`] `#[non_exhaustive]` taxonomy (audit
//!    / drift-history / Stripe-pause / config / internal) +
//!    [`error::ReconcileAuditSinkError`] +
//!    [`error::ReconcileDriftHistoryError`] +
//!    [`error::ReconcileStripePauseError`].
//!
//! # Why `trait + fake` here, real CF Cron DO + D1 + Stripe-pause API in WI-S10-007
//!
//! S-10 lands without Cloudflare R2 + Cron + D1 atomic batch + real
//! Stripe API quota controls wired into CI (no remote + Cloudflare
//! Workers + R2 staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! 4-tier drift threshold ladder boundary semantics; dual-condition
//! auto-fix gate scale-invariant pairing; per-tenant isolation
//! (cross-tenant drift never spills); SEV-1 arm pauses Stripe
//! submission for the affected `(tenant, billing_period)` only;
//! audit-fail-CLOSED envelope BEFORE every state mutation; idempotent
//! re-run produces the same decision over the canonical 4-tuple PK.
//! The live `BillingReconcilerCron-<region>` per-region Cloudflare
//! Durable Object cron-trigger (daily 02:00 UTC; 5 canonical regions
//! iad/fra/nrt/syd/gru per sprint contract §17), R2 reconciliation-
//! report Object Lock 7y archive
//! (`reconciliation-reports/<region>/YYYY-MM-DD.json`), R2 events
//! Layer 1 input source (WI-S10-001 NDJSON), D1 counter Layer 2 input
//! source (WI-S10-002 `usage_counter` table + chain head re-
//! verification cooperation), Stripe usage_records Layer 3 input
//! source (WI-S10-003 `stripe_idempotency_keys` ledger), real
//! `corelink_time::next_month_first_utc_midnight()` boundary
//! primitive, RB-FM-302 + RB-FM-151 dry-run, statistical drift bounds
//! calibration (n=50+50 + 95% CI per Lote 10.8bis P0-E), 30d clean
//! streak prerequisite gauge, and PagerDuty 3 services dispatch
//! (`corelink-finance` / `corelink-sre` / `corelink-security`) all
//! run alongside WI-S10-007 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-BILLING-RECONCILE-3-LAYER** (HIGH; sprint contract §8 NEW;
//!   `invariant_registry.md §3.12 line 166`): 3-layer daily 30d clean.
//!   The orchestrator's pairwise drift primitive
//!   ([`drift::compute_max_drift`]) compares all three pairs (Layer 1
//!   ↔ Layer 2, Layer 2 ↔ Layer 3, Layer 1 ↔ Layer 3) so a
//!   compose-of-bugs across layers cannot mask the divergence; the
//!   primary-layer routing prefers downstream layers (Layer 3 = SEV-1
//!   priority) so customer-facing impact escalates first. Pinned by
//!   `prop_three_layer_full_coverage` + `prop_drift_threshold_boundaries`.
//! - **INV-BILLING-NO-LOSS Layer 1** (HIGH; `invariant_registry.md
//!   §3.9 line 136`): cooperation with WI-S10-002 — the
//!   reconciliation worker reads the canonical aggregator output as
//!   Layer 2 input + the raw R2 NDJSON as Layer 1 input + flags
//!   divergence > Quiet threshold. Pinned by
//!   `prop_layer1_layer2_divergence_detected_at_correct_threshold`.
//! - **INV-BILLING-NO-DUP** (HIGH; `invariant_registry.md §3.9 line
//!   137`): drift-history ledger PRIMARY KEY canonical `(tenant_id,
//!   billing_period, run_started_at)` UNIQUE pins idempotent re-run
//!   over the watermark; same input snapshot reproduces the same
//!   decision arm. Pinned by `prop_idempotent_rerun_same_period`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07
//!   P1-1 fix + Lote 10.6bis pattern + S-09 inheritance + WI-S10-002
//!   /003 inheritance): every reconciliation decision arm fires its
//!   canonical audit BEFORE state mutation; audit failure aborts the
//!   run + propagates as [`error::ReconcileError::Audit`]. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): per-tenant decision routing; tenant A's drift
//!   never escalates tenant B's submission state. Pinned by
//!   `prop_tenant_isolation`.
//! - **Auto-fix dual-condition gate** (Lote 10.6bis P0-6
//!   scale-invariant): auto-fix fires ONLY when `drift_record_count
//!   ≤ 5 AND drift_pct ≤ 0.01%`. The two-floor design is
//!   scale-invariant: a 100-row tenant with 5 drifts = 5% (catastrophic
//!   — the percentage arm rejects); a 100k-row tenant with 5 drifts
//!   = 0.005% (negligible — both arms pass). Pinned by
//!   `prop_auto_fix_dual_condition_gate`.
//! - **Float-precision floor** (informational; `u128 → f64` cast
//!   precision-loss bounded): the drift metric is precision-bounded
//!   by `2^-43` (`u128 → f64` mantissa loss); orders of magnitude
//!   below the canonical Quiet threshold `10^-4` so the precision
//!   floor never produces a false-negative drift detection.
//!
//! # Production wiring (deferred to WI-S10-007)
//!
//! - `BillingReconcilerCron-<region>` per-region Cloudflare Durable
//!   Object cron-trigger (daily 02:00 UTC; 5 canonical regions
//!   iad/fra/nrt/syd/gru per sprint contract §17 inheritance).
//! - R2 reconciliation-report Object Lock Governance Mode 7y archive
//!   at `reconciliation-reports/<region>/YYYY-MM-DD.json` per Lote
//!   10.5bis path conventions (SOC 2 CC1.4 + GAAP ASC 606 evidence
//!   trail).
//! - D1 atomic batch (drift_history INSERT + stripe_submission_state
//!   UPDATE in the same `db.batch`) per WI-S10-004 §6.1.8.
//! - Layer 1 input via WI-S10-001 R2 NDJSON aggregate (Σ qty per
//!   `(tenant, region, sku, hour)`).
//! - Layer 2 input via WI-S10-002 `usage_counter` table + chain head
//!   re-verification cooperation (BLAKE3 chain digest recomputation
//!   from raw events; tampering signal SEV-1).
//! - Layer 3 input via WI-S10-003 `stripe_idempotency_keys` ledger
//!   (Σ total_qty_text per (tenant, billing_period)) + Stripe API
//!   pull cooperation via fetch_invoice (FM-151 backoff + queue
//!   fallback).
//! - PagerDuty 3 services dispatch: `corelink-sre` (Layer 1/2 SEV-2;
//!   Layer 3 SEV-1; query failure SEV-1) / `corelink-finance` (drift
//!   `> 0.1%` any layer triage) / `corelink-security` (hash chain
//!   violation SEV-1).
//! - Statistical drift bounds calibration n=50+50 + 95% CI (Lote
//!   10.8bis P0-E inheritance) + drift calibration doc runbook.
//! - 30d clean streak prerequisite gauge
//!   (`corelink_billing_reconcile_clean_streak_days{region}`)
//!   tracking sprint contract §6 DoD.
//! - RB-FM-302 (billing drift) + RB-FM-151 (Stripe outage cooperation
//!   Layer 3) staging dry-run runbooks.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod drift;
pub mod error;
pub mod event;
pub mod history;
pub mod reconciler;
pub mod stripe_pause;

pub use audit::{
    audit_event_for_decision, canonical_reconcile_audit_event_strings,
    FailingReconcileAuditSink, InMemoryReconcileAuditSink, ReconcileAuditEmitError,
    ReconcileAuditEventType, ReconcileAuditRecord, ReconcileAuditSink,
};
pub use drift::{
    auto_fix_gate_fires, compute_drift_record_count, compute_max_drift,
    compute_pairwise_drift_pct,
};
pub use error::{
    ReconcileAuditSinkError, ReconcileDriftHistoryError, ReconcileError,
    ReconcileStripePauseError,
};
pub use event::{
    canonical_reconcile_layer_kinds, LayerTotals, ReconcileConfig, ReconcileDecision,
    ReconcileLayerKind, ReconcileSnapshot, AUTO_FIX_MAX_PERCENT, AUTO_FIX_MAX_RECORDS,
    QUIET_THRESHOLD, SEV2_TO_SEV1_THRESHOLD, SEV3_TO_SEV2_THRESHOLD,
};
pub use history::{
    DriftHistoryInsertOutcome, DriftHistoryLedger, DriftHistoryRow,
    FailingDriftHistoryLedger, InMemoryDriftHistoryLedger,
};
pub use reconciler::{BillingReconciler, InMemoryBillingReconciler};
pub use stripe_pause::{
    FailingStripeSubmissionControl, InMemoryStripeSubmissionControl, StripePauseOutcome,
    StripeSubmissionControl,
};

/// Crate canonical schema version constant. Mirrors WI-S10-004 §1
/// schema versioning policy + the canonical D1 migration slot for the
/// follow-on `billing_reconciliation_drift` + `stripe_submission_state`
/// tables (next slot after `migrations/d1/0018_stripe_idem_keys.sql`;
/// production wiring at WI-S10-007 lands the additive 0019 production
/// migration alongside this WI's drift-history staging table).
#[must_use]
pub const fn reconcile_schema_version() -> u32 {
    19
}
