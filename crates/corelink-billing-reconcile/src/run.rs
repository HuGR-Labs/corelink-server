//! Live reconcile-pass driver (WI-S10-007 closure: the scheduled
//! prod reconciliation runner).
//!
//! This module is the thin, pure-logic seam between the per-region
//! daily cron and the [`crate::reconciler`] orchestrator. The cron
//! workflow (`.github/workflows/billing-reconcile-daily.yml`) extracts
//! the three layer totals per `(tenant, billing_period)` from the REAL
//! prod sources — Layer 1 (Σ R2 usage events) + Layer 2 (Σ D1
//! `usage_counter`) over the Cloudflare D1 HTTP query API, and Layer 3
//! (Σ Stripe usage_records) over the Stripe REST API — assembles them
//! into a [`ReconcileRunInput`] JSON document, and feeds it to the
//! [`crate::bin`] runner (`billing-reconcile-run`). The runner calls
//! [`run_reconcile_pass`] here.
//!
//! ## Why the extraction lives in the workflow, not in `src/`
//!
//! Mirrors the proven `corelink-audit-chain` daily-verify pattern
//! (`.github/workflows/audit-chain-daily-verify.yml` + the
//! `corelink-audit-chain` `verifier` bin): the credentialed HTTP pull
//! lives in the workflow (CF API v4 + Stripe REST), the Rust binary is
//! a pure-logic, `tokio`-free, wasm-clean integrity engine that reads
//! local JSON and runs the canonical comparison. This keeps this crate
//! dependency-light (no async runtime / HTTP client) and keeps the
//! load-bearing drift logic fully unit- + property-testable.
//!
//! ## READ-ONLY + report-only (this WP)
//!
//! The pass is a **comparison + report**. It NEVER mutates Stripe or
//! D1. The orchestrator's SEV-1 arm is wired against the in-memory
//! [`crate::stripe_pause::InMemoryStripeSubmissionControl`] which
//! records the pause *intent* in-process only (a DRY-RUN) — the real
//! D1 `stripe_submission_state` flag (WI-S10-007 production binding) is
//! deliberately NOT touched in this WP. The report surfaces the drift
//! plus the would-pause intent so the operator can act; the auto-pause
//! side-effect stays disabled until the owner promotes it.
//!
//! ## Fail-CLOSED
//!
//! Any reconcile error (audit sink / drift-history ledger / config /
//! internal) aborts the pass and propagates [`ReconcileError`] — the
//! runner maps that to a non-zero exit. A pass NEVER reports a false
//! "no drift" when a source or sink errored.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audit::{InMemoryReconcileAuditSink, ReconcileAuditRecord};
use crate::drift::compute_max_drift;
use crate::error::ReconcileError;
use crate::event::{ReconcileConfig, ReconcileDecision, ReconcileSnapshot};
use crate::history::InMemoryDriftHistoryLedger;
use crate::reconciler::{BillingReconciler, InMemoryBillingReconciler};
use crate::stripe_pause::InMemoryStripeSubmissionControl;

/// Default run-start watermark used when the input omits one. The
/// production cron passes the canonical period midnight so the
/// drift-history PK is stable across re-runs (INV-BILLING-NO-DUP); the
/// default keeps the pure-logic path deterministic for tests + ad-hoc
/// invocations.
pub const DEFAULT_RUN_STARTED_AT_MS: u64 = 0;

/// Canonical clean marker the runner prints on a zero-escalation pass
/// (the cron greps for it).
pub const MARKER_CLEAN: &str = "BILLING_RECONCILE_CLEAN";

/// Canonical drift marker prefix the runner prints per escalated
/// tenant (the cron greps for it + pages).
pub const MARKER_DRIFT: &str = "BILLING_RECONCILE_DRIFT_DETECTED";

/// Canonical fail-CLOSED error marker the runner prints when the pass
/// itself errored (source/sink failure) — distinct from a clean drift
/// detection so the cron can tell "we could not prove anything" apart
/// from "we proved drift".
pub const MARKER_ERROR: &str = "BILLING_RECONCILE_ERROR";

/// Severity of a single tenant's reconcile outcome, ordered ascending
/// (`Clean` < `Sev3` < `Sev2` < `Sev1`) so a threshold comparison is a
/// plain `>=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReconcileSeverity {
    /// Within the canonical Quiet tier (`NoDrift`) or auto-fixed inside
    /// the dual-condition gate (`AutoFixed`) — no escalation.
    Clean,
    /// SEV-3 monitor ticket (`0.01% < drift ≤ 0.1%`).
    Sev3,
    /// SEV-2 page (`0.1% < drift ≤ 1%`).
    Sev2,
    /// SEV-1 page + would-auto-pause Stripe (`drift > 1%`).
    Sev1,
}

impl ReconcileSeverity {
    /// Map a [`ReconcileDecision`] arm to its escalation severity.
    #[must_use]
    pub const fn from_decision(decision: ReconcileDecision) -> Self {
        match decision {
            ReconcileDecision::NoDrift { .. } | ReconcileDecision::AutoFixed { .. } => Self::Clean,
            ReconcileDecision::TicketSev3 { .. } => Self::Sev3,
            ReconcileDecision::PageSev2 { .. } => Self::Sev2,
            ReconcileDecision::PageSev1AutoPaused { .. } => Self::Sev1,
        }
    }

    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Sev3 => "sev3",
            Self::Sev2 => "sev2",
            Self::Sev1 => "sev1",
        }
    }

    /// Parse the `--fail-on` CLI mnemonic. `clean` is accepted as the
    /// "fail on any escalation past Quiet" alias of `sev3` would be too
    /// loose, so the canonical floor mnemonics are `sev3` / `sev2` /
    /// `sev1`.
    #[must_use]
    pub fn parse_floor(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sev3" => Some(Self::Sev3),
            "sev2" => Some(Self::Sev2),
            "sev1" => Some(Self::Sev1),
            _ => None,
        }
    }
}

/// One tenant's three-layer totals + identity. A flattened, JSON-stable
/// shape over [`ReconcileSnapshot`] (which the extraction step emits
/// one-per-tenant).
pub type TenantSnapshotInput = ReconcileSnapshot;

/// Input document for one reconcile pass — the materialization of the
/// real D1 + Stripe sources for a single `(billing_period)` across all
/// tenants the extraction step pulled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileRunInput {
    /// Canonical billing period (`YYYY-MM`) the snapshots cover.
    pub billing_period: String,
    /// Run-start watermark (Unix epoch ms). Optional in the JSON; absent →
    /// `u64::default()` (0), which equals [`DEFAULT_RUN_STARTED_AT_MS`] — the
    /// deterministic no-op default (prod passes an explicit watermark). Using
    /// `#[serde(default)]` rather than a custom default-fn deliberately leaves
    /// no `-> 0` mutation surface for the changed-line mutant gate.
    #[serde(default)]
    pub run_started_at_ms: u64,
    /// Per-tenant three-layer totals.
    pub snapshots: Vec<TenantSnapshotInput>,
}

/// A serializable view of one orchestrator audit record (the audit
/// envelope that fired BEFORE state mutation). Decoupled from
/// [`ReconcileAuditRecord`] so this module does not force `Serialize`
/// onto the core audit taxonomy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileAuditEntry {
    /// Canonical CloudEvents `type` string.
    pub event_type: String,
    /// Tenant id (string form for JSON stability).
    pub tenant_id: Option<String>,
    /// Canonical billing period.
    pub billing_period: String,
    /// Primary drift layer mnemonic, if any.
    pub primary_layer: Option<String>,
    /// Producer wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
    /// Free-form context.
    pub context: String,
}

impl ReconcileAuditEntry {
    fn from_record(r: &ReconcileAuditRecord) -> Self {
        Self {
            event_type: r.event_type.as_str().to_string(),
            tenant_id: r.tenant_id.map(|t| t.to_string()),
            billing_period: r.billing_period.clone(),
            primary_layer: r.primary_layer.map(|l| l.as_str().to_string()),
            now_ms: r.now_ms,
            context: r.context.clone(),
        }
    }
}

/// One tenant's reconcile outcome row in the report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TenantReconcileOutcome {
    /// Tenant id (string form for JSON stability).
    pub tenant_id: String,
    /// Maximum pairwise drift observed (fractional unit; `0.01 = 1%`).
    pub max_drift_pct: f64,
    /// The orchestrator's decision arm.
    pub decision: ReconcileDecision,
    /// Escalation severity of the decision.
    pub severity: ReconcileSeverity,
}

/// The drift report emitted as the audit-event artifact for the pass.
/// Serialized to the `--report` path + archived by the cron.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReconcileReport {
    /// Canonical billing period the pass covered.
    pub billing_period: String,
    /// Run-start watermark used for the pass.
    pub run_started_at_ms: u64,
    /// Number of tenant snapshots reconciled.
    pub tenant_count: usize,
    /// Number of tenants whose decision escalated past the Quiet tier
    /// (severity `>= Sev3`).
    pub escalation_count: usize,
    /// The highest severity observed across all tenants.
    pub max_severity: ReconcileSeverity,
    /// Per-tenant outcomes.
    pub outcomes: Vec<TenantReconcileOutcome>,
    /// The orchestrator audit envelope trail (forensic evidence: every
    /// `run_started` + decision-arm audit that fired BEFORE state
    /// mutation).
    pub audit_trail: Vec<ReconcileAuditEntry>,
}

impl ReconcileReport {
    /// Whether the pass observed zero escalations (every tenant Quiet /
    /// auto-fixed).
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.max_severity == ReconcileSeverity::Clean
    }

    /// Whether the pass should fail the run at the given severity floor
    /// (`max_severity >= floor`). The runner maps `true` to a non-zero
    /// exit + a SEV alert.
    #[must_use]
    pub fn fails_at(&self, floor: ReconcileSeverity) -> bool {
        self.max_severity >= floor
    }
}

/// Run the canonical reconcile decision over every tenant snapshot
/// using the supplied reconciler. Pure over the reconciler trait so a
/// failing-source fake injects fail-CLOSED behavior in tests.
///
/// # Errors
///
/// Propagates the first [`ReconcileError`] any tenant's
/// [`BillingReconciler::reconcile`] returns (fail-CLOSED: the pass
/// aborts rather than emit a partial — and therefore false-confidence —
/// report).
pub fn reconcile_snapshots(
    reconciler: &dyn BillingReconciler,
    input: &ReconcileRunInput,
) -> Result<Vec<TenantReconcileOutcome>, ReconcileError> {
    let mut outcomes = Vec::with_capacity(input.snapshots.len());
    for snapshot in &input.snapshots {
        let decision =
            reconciler.reconcile(snapshot, &input.billing_period, input.run_started_at_ms)?;
        let (max_drift_pct, _primary) = compute_max_drift(*snapshot);
        outcomes.push(TenantReconcileOutcome {
            tenant_id: snapshot.tenant_id.to_string(),
            max_drift_pct,
            decision,
            severity: ReconcileSeverity::from_decision(decision),
        });
    }
    Ok(outcomes)
}

/// Run one full reconcile pass against the canonical in-memory
/// orchestrator (audit sink + drift-history ledger + DRY-RUN
/// Stripe-pause control) and produce the drift [`ReconcileReport`].
///
/// This is the function the `billing-reconcile-run` binary calls. The
/// Stripe-pause control is the in-memory (dry-run) surface so the SEV-1
/// arm records pause *intent* without touching the real Stripe / D1
/// flag — READ-ONLY + report-only per this WP.
///
/// # Errors
///
/// Propagates any [`ReconcileError`] from the orchestrator (fail-CLOSED
/// — never a false clean report).
pub fn run_reconcile_pass(
    input: &ReconcileRunInput,
    config: ReconcileConfig,
) -> Result<ReconcileReport, ReconcileError> {
    let audit = Arc::new(InMemoryReconcileAuditSink::new());
    let history = Arc::new(InMemoryDriftHistoryLedger::new());
    // DRY-RUN: in-memory pause control — records intent in-process,
    // never the production D1 `stripe_submission_state` flag.
    let stripe_pause = Arc::new(InMemoryStripeSubmissionControl::new());
    let reconciler = InMemoryBillingReconciler::with_config(
        Arc::clone(&audit),
        Arc::clone(&history),
        Arc::clone(&stripe_pause),
        config,
    );

    let outcomes = reconcile_snapshots(&reconciler, input)?;

    let max_severity = outcomes
        .iter()
        .map(|o| o.severity)
        .max()
        .unwrap_or(ReconcileSeverity::Clean);
    let escalation_count = outcomes
        .iter()
        .filter(|o| o.severity >= ReconcileSeverity::Sev3)
        .count();
    let audit_trail = audit
        .snapshot()
        .iter()
        .map(ReconcileAuditEntry::from_record)
        .collect();

    Ok(ReconcileReport {
        billing_period: input.billing_period.clone(),
        run_started_at_ms: input.run_started_at_ms,
        tenant_count: outcomes.len(),
        escalation_count,
        max_severity,
        outcomes,
        audit_trail,
    })
}

/// Parse a [`ReconcileRunInput`] from a JSON byte slice.
///
/// # Errors
///
/// Returns [`ReconcileError::Config`] on malformed JSON — fail-CLOSED:
/// an unparseable input is a run error, NEVER an empty "clean" pass.
pub fn parse_input(bytes: &[u8]) -> Result<ReconcileRunInput, ReconcileError> {
    serde_json::from_slice(bytes)
        .map_err(|e| ReconcileError::Config(format!("malformed reconcile input JSON: {e}")))
}

/// Convenience constructor for an empty input (cron no-op: first run
/// after deploy / no extracted snapshots).
#[must_use]
pub fn empty_input(billing_period: &str, run_started_at_ms: u64) -> ReconcileRunInput {
    ReconcileRunInput {
        billing_period: billing_period.to_string(),
        run_started_at_ms,
        snapshots: Vec::new(),
    }
}

/// Build a [`ReconcileSnapshot`] from raw layer totals — small helper
/// the extraction tests + ad-hoc callers use.
#[must_use]
pub fn snapshot(
    tenant_id: Uuid,
    layer1: (u128, u64),
    layer2: (u128, u64),
    layer3: (u128, u64),
) -> ReconcileSnapshot {
    use crate::event::LayerTotals;
    ReconcileSnapshot::new(
        tenant_id,
        LayerTotals::new(layer1.0, layer1.1),
        LayerTotals::new(layer2.0, layer2.1),
        LayerTotals::new(layer3.0, layer3.1),
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives; float_cmp is acceptable for canonical-percentage-pin assertions where the values are constructed deterministically."
)]
mod tests {
    use super::*;
    use crate::audit::InMemoryReconcileAuditSink;
    use crate::history::FailingDriftHistoryLedger;
    use crate::stripe_pause::InMemoryStripeSubmissionControl;

    fn input_with(snapshots: Vec<ReconcileSnapshot>) -> ReconcileRunInput {
        ReconcileRunInput {
            billing_period: "2026-05".to_string(),
            run_started_at_ms: 100,
            snapshots,
        }
    }

    #[test]
    fn zero_drift_pass_is_clean() {
        let t = Uuid::now_v7();
        let input = input_with(vec![snapshot(t, (1000, 1), (1000, 1), (1000, 1))]);
        let report = run_reconcile_pass(&input, ReconcileConfig::default()).unwrap();
        assert!(report.is_clean());
        assert_eq!(report.max_severity, ReconcileSeverity::Clean);
        assert_eq!(report.escalation_count, 0);
        assert_eq!(report.tenant_count, 1);
        assert!(!report.fails_at(ReconcileSeverity::Sev3));
        // Two audit envelopes per tenant: run_started + no_drift.
        assert_eq!(report.audit_trail.len(), 2);
    }

    #[test]
    fn seeded_drift_usage_ne_billed_is_reported_sev1() {
        // Layer 1 + Layer 2 (D1 usage) agree at 100; Layer 3 (Stripe
        // billed) diverges to 95 = 5% drift → SEV-1 (usage != billed).
        let t = Uuid::now_v7();
        let input = input_with(vec![snapshot(t, (100, 1), (100, 1), (95, 1))]);
        let report = run_reconcile_pass(&input, ReconcileConfig::default()).unwrap();
        assert!(!report.is_clean());
        assert_eq!(report.max_severity, ReconcileSeverity::Sev1);
        assert_eq!(report.escalation_count, 1);
        assert!(report.fails_at(ReconcileSeverity::Sev3));
        assert!(report.fails_at(ReconcileSeverity::Sev1));
        let o = &report.outcomes[0];
        assert_eq!(o.severity, ReconcileSeverity::Sev1);
        assert!(matches!(
            o.decision,
            ReconcileDecision::PageSev1AutoPaused { .. }
        ));
        assert!((o.max_drift_pct - 0.05).abs() < 1e-12);
    }

    #[test]
    fn seeded_sev3_drift_escalates_but_not_at_sev1_floor() {
        // 9995 vs 10000 = 0.05% drift → SEV-3 ticket.
        let t = Uuid::now_v7();
        let input = input_with(vec![snapshot(t, (9995, 1), (10_000, 1), (10_000, 1))]);
        let report = run_reconcile_pass(&input, ReconcileConfig::default()).unwrap();
        assert_eq!(report.max_severity, ReconcileSeverity::Sev3);
        assert!(report.fails_at(ReconcileSeverity::Sev3));
        assert!(!report.fails_at(ReconcileSeverity::Sev1));
    }

    #[test]
    fn mixed_tenants_max_severity_dominates() {
        let clean = Uuid::now_v7();
        let drifted = Uuid::now_v7();
        let input = input_with(vec![
            snapshot(clean, (1000, 1), (1000, 1), (1000, 1)),
            snapshot(drifted, (100, 1), (100, 1), (95, 1)),
        ]);
        let report = run_reconcile_pass(&input, ReconcileConfig::default()).unwrap();
        assert_eq!(report.tenant_count, 2);
        assert_eq!(report.escalation_count, 1);
        assert_eq!(report.max_severity, ReconcileSeverity::Sev1);
    }

    #[test]
    fn empty_input_is_clean_noop() {
        let input = empty_input("2026-05", 0);
        let report = run_reconcile_pass(&input, ReconcileConfig::default()).unwrap();
        assert!(report.is_clean());
        assert_eq!(report.tenant_count, 0);
        assert!(!report.fails_at(ReconcileSeverity::Sev3));
    }

    #[test]
    fn source_error_fails_closed_not_silent_clean() {
        // A drift-history ledger failure must propagate as Err — the
        // pass MUST NOT report a false "clean".
        let audit = Arc::new(InMemoryReconcileAuditSink::new());
        let history = Arc::new(FailingDriftHistoryLedger::new());
        let stripe = Arc::new(InMemoryStripeSubmissionControl::new());
        let reconciler = InMemoryBillingReconciler::new(
            Arc::clone(&audit),
            Arc::clone(&history),
            Arc::clone(&stripe),
        );
        let t = Uuid::now_v7();
        let input = input_with(vec![snapshot(t, (1000, 1), (1000, 1), (1000, 1))]);
        let err = reconcile_snapshots(&reconciler, &input).unwrap_err();
        assert!(matches!(err, ReconcileError::DriftHistory(_)));
    }

    #[test]
    fn parse_input_rejects_malformed_json_fail_closed() {
        let err = parse_input(b"{ not json").unwrap_err();
        assert!(matches!(err, ReconcileError::Config(_)));
    }

    #[test]
    fn parse_input_accepts_canonical_and_defaults_watermark() {
        let json = br#"{
            "billing_period": "2026-05",
            "snapshots": [
                {
                    "tenant_id": "018f9b1e-0000-7000-8000-000000000001",
                    "layer1": { "total_qty": 1000, "record_count": 1 },
                    "layer2": { "total_qty": 1000, "record_count": 1 },
                    "layer3": { "total_qty": 1000, "record_count": 1 }
                }
            ]
        }"#;
        let input = parse_input(json).unwrap();
        assert_eq!(input.billing_period, "2026-05");
        assert_eq!(input.run_started_at_ms, DEFAULT_RUN_STARTED_AT_MS);
        assert_eq!(input.snapshots.len(), 1);
    }

    #[test]
    fn report_serde_round_trips() {
        let t = Uuid::now_v7();
        let input = input_with(vec![snapshot(t, (100, 1), (100, 1), (95, 1))]);
        let report = run_reconcile_pass(&input, ReconcileConfig::default()).unwrap();
        let json = serde_json::to_string(&report).unwrap();
        let back: ReconcileReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn severity_floor_parse_pins_mnemonics() {
        assert_eq!(
            ReconcileSeverity::parse_floor("sev3"),
            Some(ReconcileSeverity::Sev3)
        );
        assert_eq!(
            ReconcileSeverity::parse_floor("sev2"),
            Some(ReconcileSeverity::Sev2)
        );
        assert_eq!(
            ReconcileSeverity::parse_floor("SEV1"),
            Some(ReconcileSeverity::Sev1)
        );
        assert_eq!(ReconcileSeverity::parse_floor("bogus"), None);
    }

    #[test]
    fn severity_ordering_ascending() {
        assert!(ReconcileSeverity::Clean < ReconcileSeverity::Sev3);
        assert!(ReconcileSeverity::Sev3 < ReconcileSeverity::Sev2);
        assert!(ReconcileSeverity::Sev2 < ReconcileSeverity::Sev1);
    }

    #[test]
    fn severity_from_decision_maps_every_arm() {
        assert_eq!(
            ReconcileSeverity::from_decision(ReconcileDecision::NoDrift { max_drift_pct: 0.0 }),
            ReconcileSeverity::Clean
        );
        assert_eq!(
            ReconcileSeverity::from_decision(ReconcileDecision::AutoFixed {
                max_drift_pct: 0.00005,
                drift_record_count: 1
            }),
            ReconcileSeverity::Clean
        );
        assert_eq!(
            ReconcileSeverity::from_decision(ReconcileDecision::TicketSev3 {
                max_drift_pct: 0.0005,
                primary_layer: crate::event::ReconcileLayerKind::Layer1Emit,
            }),
            ReconcileSeverity::Sev3
        );
        assert_eq!(
            ReconcileSeverity::from_decision(ReconcileDecision::PageSev2 {
                max_drift_pct: 0.005,
                primary_layer: crate::event::ReconcileLayerKind::Layer2Aggregate,
            }),
            ReconcileSeverity::Sev2
        );
        assert_eq!(
            ReconcileSeverity::from_decision(ReconcileDecision::PageSev1AutoPaused {
                max_drift_pct: 0.05,
                primary_layer: crate::event::ReconcileLayerKind::Layer3Stripe,
                pause_acked: true,
            }),
            ReconcileSeverity::Sev1
        );
    }
}
