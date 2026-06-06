//! [`BillingReconciler`] trait + [`InMemoryBillingReconciler`]
//! orchestrator.
//!
//! ## Run pipeline (per (tenant, billing_period) cron tick)
//!
//! For each input [`ReconcileSnapshot`]:
//!
//! 1. **Audit `run_started` BEFORE state observation** (S-07 P1-1
//!    canonical lift + Lote 10.6bis pattern). Audit failure aborts the
//!    run (NO drift compute, NO history INSERT, NO Stripe-pause
//!    attempt).
//! 2. **Compute drift**: `(max_drift_pct, primary_layer,
//!    drift_record_count)` via the canonical pairwise primitive.
//! 3. **Classify decision** via the 4-tier ladder + auto-fix
//!    dual-condition gate carved INSIDE the Quiet tier:
//!    - `drift_pct ≤ QUIET_THRESHOLD` → `NoDrift` (or `AutoFixed` when
//!      drift > 0 AND the gate fires);
//!    - `QUIET_THRESHOLD < drift_pct ≤ SEV3_TO_SEV2_THRESHOLD` →
//!      `TicketSev3`;
//!    - `SEV3_TO_SEV2_THRESHOLD < drift_pct ≤ SEV2_TO_SEV1_THRESHOLD`
//!      → `PageSev2`;
//!    - `drift_pct > SEV2_TO_SEV1_THRESHOLD` → `PageSev1AutoPaused`.
//! 4. **Audit `<decision>` BEFORE state mutation** (fail-CLOSED
//!    envelope on every decision arm). Audit failure aborts before
//!    drift-history INSERT + Stripe-pause attempt.
//! 5. **Drift-history INSERT** (always — even Quiet runs land a row
//!    so the auditor evidence trail is uninterrupted).
//! 6. **SEV-1 arm only**: invoke
//!    [`StripeSubmissionControl::pause`]; the `pause_acked` flag in
//!    the decision reflects the outcome. Pause backend failure
//!    surfaces as [`crate::error::ReconcileError::StripePause`] AFTER
//!    the canonical audit row + the drift-history row landed.
//!
//! ## Audit-fail-CLOSED at the trait surface
//!
//! Per WI-S10-004 §6.1 + sprint contract §14.s10.1, every decision
//! arm fires its canonical audit BEFORE the state-mutating step:
//!
//! - `run_started` fires BEFORE any drift compute.
//! - `<decision>` (NoDrift / AutoFixed / TicketFiled / PageDispatched
//!   / StripePaused) fires BEFORE the drift-history INSERT (the only
//!   state mutation on the Quiet / SEV-3 / SEV-2 arms).
//! - `stripe_paused` fires BEFORE the pause control surface call (the
//!   second state mutation on the SEV-1 arm).
//!
//! Audit failure on any arm aborts the orchestrator + propagates
//! [`crate::error::ReconcileError::Audit`] (caller sees no state
//! mutation past the point of the failure).
//!
//! ## F-001 closure
//!
//! The orchestrator holds the audit sink + drift-history ledger +
//! Stripe-pause control surface as `Arc` handles passed at
//! construction; per-instance state lives inside those `Arc`'d
//! primitives. Tests instantiate fresh orchestrators per case so
//! cross-test contamination is structurally impossible.

use std::sync::Arc;

use uuid::Uuid;

use crate::audit::{
    audit_event_for_decision, ReconcileAuditEventType, ReconcileAuditRecord, ReconcileAuditSink,
};
use crate::drift::{auto_fix_gate_fires, compute_drift_record_count, compute_max_drift};
use crate::error::ReconcileError;
use crate::event::{ReconcileConfig, ReconcileDecision, ReconcileSnapshot};
use crate::history::{DriftHistoryLedger, DriftHistoryRow};
use crate::stripe_pause::{StripePauseOutcome, StripeSubmissionControl};

/// Reconciliation worker trait. Production wiring composes the
/// `CronDOBillingReconciler` Cloudflare Durable Object cron-trigger
/// (daily 02:00 UTC per region) deferred to WI-S10-007 PRR ship gate
/// per the `trait-abstraction-defer` charter pattern.
pub trait BillingReconciler: Send + Sync + core::fmt::Debug {
    /// Run reconciliation for a single (tenant, billing_period)
    /// snapshot. The orchestrator computes drift, classifies the
    /// decision per the canonical 4-tier ladder, audits BEFORE every
    /// state mutation (fail-CLOSED envelope), and persists the
    /// drift-history row. The SEV-1 arm additionally pauses Stripe
    /// usage_record submissions until operator clearance.
    ///
    /// # Errors
    ///
    /// - [`ReconcileError::Audit`] when the audit envelope rejects
    ///   any decision arm (fail-CLOSED at the trait surface).
    /// - [`ReconcileError::DriftHistory`] when the drift-history
    ///   ledger rejects the INSERT (D1 backend failure / canonical
    ///   PK collision with diverged content).
    /// - [`ReconcileError::StripePause`] when the SEV-1 arm cannot
    ///   flip the Stripe-submission control flag (the canonical
    ///   audit row + the drift-history row already landed; the
    ///   alert is dispatched).
    /// - [`ReconcileError::Internal`] when a per-instance mutex is
    ///   poisoned.
    fn reconcile(
        &self,
        snapshot: &ReconcileSnapshot,
        billing_period: &str,
        run_started_at: u64,
    ) -> Result<ReconcileDecision, ReconcileError>;
}

/// In-memory billing reconciler orchestrator. Composes the audit
/// sink + drift-history ledger + Stripe-pause control surface via
/// `Arc` handles; all three are generic over their trait so test
/// fakes (e.g. [`crate::audit::FailingReconcileAuditSink`] +
/// [`crate::history::FailingDriftHistoryLedger`] +
/// [`crate::stripe_pause::FailingStripeSubmissionControl`]) compose
/// directly at construction.
#[derive(Clone)]
pub struct InMemoryBillingReconciler<A, H, P>
where
    A: ReconcileAuditSink + 'static,
    H: DriftHistoryLedger + 'static,
    P: StripeSubmissionControl + 'static,
{
    audit: Arc<A>,
    history: Arc<H>,
    stripe_pause: Arc<P>,
    config: ReconcileConfig,
}

impl<A, H, P> core::fmt::Debug for InMemoryBillingReconciler<A, H, P>
where
    A: ReconcileAuditSink + 'static,
    H: DriftHistoryLedger + 'static,
    P: StripeSubmissionControl + 'static,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryBillingReconciler")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<A, H, P> InMemoryBillingReconciler<A, H, P>
where
    A: ReconcileAuditSink + 'static,
    H: DriftHistoryLedger + 'static,
    P: StripeSubmissionControl + 'static,
{
    /// Construct a reconciler with the canonical config defaults.
    pub fn new(audit: Arc<A>, history: Arc<H>, stripe_pause: Arc<P>) -> Self {
        Self {
            audit,
            history,
            stripe_pause,
            config: ReconcileConfig::default(),
        }
    }

    /// Construct a reconciler with an explicit [`ReconcileConfig`].
    pub fn with_config(
        audit: Arc<A>,
        history: Arc<H>,
        stripe_pause: Arc<P>,
        config: ReconcileConfig,
    ) -> Self {
        Self {
            audit,
            history,
            stripe_pause,
            config,
        }
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<A> {
        &self.audit
    }

    /// Borrow the drift-history ledger.
    #[must_use]
    pub fn history(&self) -> &Arc<H> {
        &self.history
    }

    /// Borrow the Stripe-pause control surface.
    #[must_use]
    pub fn stripe_pause(&self) -> &Arc<P> {
        &self.stripe_pause
    }

    /// Borrow the config snapshot.
    #[must_use]
    pub const fn config(&self) -> ReconcileConfig {
        self.config
    }

    fn audit_record(
        event_type: ReconcileAuditEventType,
        tenant_id: Uuid,
        billing_period: &str,
        primary_layer: Option<crate::event::ReconcileLayerKind>,
        now_ms: u64,
        context: String,
    ) -> ReconcileAuditRecord {
        ReconcileAuditRecord {
            event_type,
            tenant_id: Some(tenant_id),
            billing_period: billing_period.to_string(),
            primary_layer,
            now_ms,
            context,
        }
    }
}

impl<A, H, P> BillingReconciler for InMemoryBillingReconciler<A, H, P>
where
    A: ReconcileAuditSink + 'static,
    H: DriftHistoryLedger + 'static,
    P: StripeSubmissionControl + 'static,
{
    fn reconcile(
        &self,
        snapshot: &ReconcileSnapshot,
        billing_period: &str,
        run_started_at: u64,
    ) -> Result<ReconcileDecision, ReconcileError> {
        let tenant_id = snapshot.tenant_id;
        // 1. RunStarted audit BEFORE drift compute (S-07 P1-1
        //    canonical fail-CLOSED envelope discipline).
        self.audit.emit(Self::audit_record(
            ReconcileAuditEventType::RunStarted,
            tenant_id,
            billing_period,
            None,
            run_started_at,
            format!("reconcile run_started tenant={tenant_id}"),
        ))?;

        // 2. Compute drift (pure function over the snapshot).
        let (max_drift_pct, primary_layer) = compute_max_drift(*snapshot);
        let drift_record_count = compute_drift_record_count(*snapshot);

        // 3. Classify decision per the canonical 4-tier ladder + the
        //    auto-fix dual-condition gate carved INSIDE the Quiet
        //    tier.
        //
        //    The `<=` on the lower-bound + `>` on the upper-bound is
        //    canonical Prometheus boundary semantics (mirrors
        //    WI-S10-002 PeriodWindow inclusive-start exclusive-end):
        //    boundary `drift_pct = QUIET_THRESHOLD` is Quiet;
        //    `drift_pct = SEV3_THRESHOLD` is SEV-3 (one-sided
        //    consistency).
        let decision = if max_drift_pct <= self.config.quiet_threshold() {
            // Inside the Quiet tier — carve out AutoFixed only when
            // there's actual drift AND the dual-condition gate fires.
            // Why: the gate's record-count arm is meaningful only
            // when drift > 0; routing a true-zero-drift run through
            // AutoFixed would invert the audit semantics.
            if max_drift_pct > 0.0
                && auto_fix_gate_fires(max_drift_pct, drift_record_count, &self.config)
            {
                ReconcileDecision::AutoFixed {
                    max_drift_pct,
                    drift_record_count,
                }
            } else {
                ReconcileDecision::NoDrift { max_drift_pct }
            }
        } else if max_drift_pct <= self.config.sev3_to_sev2_threshold() {
            ReconcileDecision::TicketSev3 {
                max_drift_pct,
                primary_layer,
            }
        } else if max_drift_pct <= self.config.sev2_to_sev1_threshold() {
            ReconcileDecision::PageSev2 {
                max_drift_pct,
                primary_layer,
            }
        } else {
            // SEV-1 arm: pause_acked is provisionally true; the
            // canonical audit lands BEFORE the pause attempt; the
            // pause outcome (or backend failure) is reflected on the
            // returned decision.
            ReconcileDecision::PageSev1AutoPaused {
                max_drift_pct,
                primary_layer,
                pause_acked: false,
            }
        };

        // 4. Decision-arm audit BEFORE state mutation.
        let audit_event = audit_event_for_decision(decision);
        let primary_for_audit = match decision {
            ReconcileDecision::NoDrift { .. } => None,
            ReconcileDecision::AutoFixed { .. } => Some(primary_layer),
            ReconcileDecision::TicketSev3 { primary_layer, .. }
            | ReconcileDecision::PageSev2 { primary_layer, .. }
            | ReconcileDecision::PageSev1AutoPaused { primary_layer, .. } => Some(primary_layer),
        };
        self.audit.emit(Self::audit_record(
            audit_event,
            tenant_id,
            billing_period,
            primary_for_audit,
            run_started_at,
            format!(
                "decision={} max_drift_pct={} drift_record_count={}",
                decision.as_str(),
                max_drift_pct,
                drift_record_count
            ),
        ))?;

        // 5. Drift-history INSERT (always — Quiet runs included so
        //    the auditor evidence trail is unbroken).
        self.history.insert(&DriftHistoryRow {
            tenant_id,
            billing_period: billing_period.to_string(),
            run_started_at,
            decision,
            context: format!(
                "max_drift_pct={max_drift_pct} primary_layer={primary_layer} drift_record_count={drift_record_count}"
            ),
        })?;

        // 6. SEV-1 arm: invoke Stripe-submission pause.
        //    Per the canonical fail-CLOSED envelope: the audit row
        //    landed in step 4 + the drift-history row landed in step
        //    5. The pause attempt is the last side-effect; if it
        //    fails the alert is already dispatched + the operator
        //    triages.
        if let ReconcileDecision::PageSev1AutoPaused {
            max_drift_pct,
            primary_layer,
            ..
        } = decision
        {
            let outcome = self.stripe_pause.pause(tenant_id, billing_period)?;
            let acked = matches!(
                outcome,
                StripePauseOutcome::Acked | StripePauseOutcome::AlreadyPaused
            );
            return Ok(ReconcileDecision::PageSev1AutoPaused {
                max_drift_pct,
                primary_layer,
                pause_acked: acked,
            });
        }

        Ok(decision)
    }
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
    use crate::audit::{FailingReconcileAuditSink, InMemoryReconcileAuditSink};
    use crate::event::{LayerTotals, ReconcileLayerKind};
    use crate::history::{FailingDriftHistoryLedger, InMemoryDriftHistoryLedger};
    use crate::stripe_pause::{FailingStripeSubmissionControl, InMemoryStripeSubmissionControl};

    type Reconciler = InMemoryBillingReconciler<
        InMemoryReconcileAuditSink,
        InMemoryDriftHistoryLedger,
        InMemoryStripeSubmissionControl,
    >;

    fn fresh() -> (
        Reconciler,
        Arc<InMemoryReconcileAuditSink>,
        Arc<InMemoryDriftHistoryLedger>,
        Arc<InMemoryStripeSubmissionControl>,
    ) {
        let audit = Arc::new(InMemoryReconcileAuditSink::new());
        let history = Arc::new(InMemoryDriftHistoryLedger::new());
        let stripe_pause = Arc::new(InMemoryStripeSubmissionControl::new());
        let r = InMemoryBillingReconciler::new(
            Arc::clone(&audit),
            Arc::clone(&history),
            Arc::clone(&stripe_pause),
        );
        (r, audit, history, stripe_pause)
    }

    fn snap(t: Uuid, l1: u128, l2: u128, l3: u128) -> ReconcileSnapshot {
        ReconcileSnapshot::new(
            t,
            LayerTotals::new(l1, 1),
            LayerTotals::new(l2, 1),
            LayerTotals::new(l3, 1),
        )
    }

    #[test]
    fn no_drift_when_three_layers_match_exactly() {
        let (r, audit, history, stripe) = fresh();
        let t = Uuid::now_v7();
        let s = snap(t, 1000, 1000, 1000);
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        let no_drift = matches!(dec, ReconcileDecision::NoDrift { .. });
        assert!(no_drift);
        assert_eq!(history.len(), 1);
        assert_eq!(stripe.paused_count(), 0);
        // Two audit rows: RunStarted + NoDrift.
        assert_eq!(audit.len(), 2);
    }

    #[test]
    fn ticket_sev3_at_canonical_boundary() {
        let (r, audit, history, stripe) = fresh();
        let t = Uuid::now_v7();
        // Drift = 0.0005 (between 0.0001 Quiet ceiling and 0.001 SEV-2
        // floor) → SEV-3 ticket.
        // Use 9995 vs 10000 = 0.0005.
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(9995, 1),
            LayerTotals::new(10_000, 1),
            LayerTotals::new(10_000, 1),
        );
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        let sev3 = matches!(dec, ReconcileDecision::TicketSev3 { .. });
        assert!(sev3);
        assert_eq!(history.len(), 1);
        assert_eq!(stripe.paused_count(), 0);
        assert_eq!(
            audit
                .snapshot_of(ReconcileAuditEventType::TicketFiled)
                .len(),
            1
        );
    }

    #[test]
    fn page_sev2_in_canonical_band() {
        let (r, audit, history, stripe) = fresh();
        let t = Uuid::now_v7();
        // Drift = 0.005 (between 0.001 SEV-2 floor and 0.01 SEV-1
        // floor) → SEV-2 page.
        // Use 995 vs 1000 = 0.005.
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(995, 1),
            LayerTotals::new(1000, 1),
            LayerTotals::new(1000, 1),
        );
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        let sev2 = matches!(dec, ReconcileDecision::PageSev2 { .. });
        assert!(sev2);
        assert_eq!(history.len(), 1);
        assert_eq!(stripe.paused_count(), 0);
        assert_eq!(
            audit
                .snapshot_of(ReconcileAuditEventType::PageDispatched)
                .len(),
            1
        );
    }

    #[test]
    fn page_sev1_pauses_stripe_at_threshold_breach() {
        let (r, audit, history, stripe) = fresh();
        let t = Uuid::now_v7();
        // Drift > 1% (canonical SEV-1 boundary). 95 vs 100 = 5%.
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(95, 1),
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 1),
        );
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        match dec {
            ReconcileDecision::PageSev1AutoPaused { pause_acked, .. } => {
                assert!(pause_acked);
            }
            other => unreachable!("{other:?}"),
        }
        assert_eq!(stripe.paused_count(), 1);
        assert!(stripe.is_paused(t, "2026-05").unwrap());
        assert_eq!(history.len(), 1);
        assert_eq!(
            audit
                .snapshot_of(ReconcileAuditEventType::StripePaused)
                .len(),
            1
        );
    }

    #[test]
    fn audit_failure_run_started_aborts_no_state_mutation() {
        let audit: Arc<FailingReconcileAuditSink> = Arc::new(FailingReconcileAuditSink::new());
        let history: Arc<InMemoryDriftHistoryLedger> = Arc::new(InMemoryDriftHistoryLedger::new());
        let stripe: Arc<InMemoryStripeSubmissionControl> =
            Arc::new(InMemoryStripeSubmissionControl::new());
        let r = InMemoryBillingReconciler::new(
            Arc::clone(&audit),
            Arc::clone(&history),
            Arc::clone(&stripe),
        );
        let t = Uuid::now_v7();
        let s = snap(t, 100, 100, 100);
        let err = r.reconcile(&s, "2026-05", 100).unwrap_err();
        let audit_err = matches!(err, ReconcileError::Audit(_));
        assert!(audit_err);
        assert!(history.is_empty());
        assert_eq!(stripe.paused_count(), 0);
    }

    #[test]
    fn drift_history_failure_after_audits_propagates() {
        let audit: Arc<InMemoryReconcileAuditSink> = Arc::new(InMemoryReconcileAuditSink::new());
        let history: Arc<FailingDriftHistoryLedger> = Arc::new(FailingDriftHistoryLedger::new());
        let stripe: Arc<InMemoryStripeSubmissionControl> =
            Arc::new(InMemoryStripeSubmissionControl::new());
        let r = InMemoryBillingReconciler::new(
            Arc::clone(&audit),
            Arc::clone(&history),
            Arc::clone(&stripe),
        );
        let t = Uuid::now_v7();
        let s = snap(t, 100, 100, 100);
        let err = r.reconcile(&s, "2026-05", 100).unwrap_err();
        let history_err = matches!(err, ReconcileError::DriftHistory(_));
        assert!(history_err);
        // Both audit rows already landed (RunStarted + NoDrift) per
        // the canonical fail-CLOSED envelope discipline.
        assert_eq!(audit.len(), 2);
        assert_eq!(stripe.paused_count(), 0);
    }

    #[test]
    fn stripe_pause_failure_after_audits_propagates() {
        let audit: Arc<InMemoryReconcileAuditSink> = Arc::new(InMemoryReconcileAuditSink::new());
        let history: Arc<InMemoryDriftHistoryLedger> = Arc::new(InMemoryDriftHistoryLedger::new());
        let stripe: Arc<FailingStripeSubmissionControl> =
            Arc::new(FailingStripeSubmissionControl::new());
        let r = InMemoryBillingReconciler::new(
            Arc::clone(&audit),
            Arc::clone(&history),
            Arc::clone(&stripe),
        );
        let t = Uuid::now_v7();
        // 95 vs 100 = 5% drift → SEV-1.
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(95, 1),
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 1),
        );
        let err = r.reconcile(&s, "2026-05", 100).unwrap_err();
        let pause_err = matches!(err, ReconcileError::StripePause(_));
        assert!(pause_err);
        // Audit + history landed BEFORE the pause attempt.
        assert_eq!(history.len(), 1);
        assert_eq!(
            audit
                .snapshot_of(ReconcileAuditEventType::StripePaused)
                .len(),
            1
        );
    }

    #[test]
    fn auto_fixed_carved_inside_quiet_tier_when_gate_fires() {
        let (r, audit, history, _stripe) = fresh();
        let t = Uuid::now_v7();
        // Drift between 0 and 0.0001 with record_count ≤ 5 = AutoFixed
        // arm. Use 99_999 vs 100_000 = 0.00001 (10× under Quiet
        // ceiling); record_count diff 1.
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(99_999, 1),
            LayerTotals::new(100_000, 2),
            LayerTotals::new(100_000, 2),
        );
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        let auto = matches!(dec, ReconcileDecision::AutoFixed { .. });
        assert!(auto);
        assert_eq!(
            audit.snapshot_of(ReconcileAuditEventType::AutoFixed).len(),
            1
        );
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn no_drift_at_exact_zero_drift_uses_no_drift_arm() {
        let (r, audit, _history, _stripe) = fresh();
        let t = Uuid::now_v7();
        let s = snap(t, 100, 100, 100);
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        let no_drift = matches!(dec, ReconcileDecision::NoDrift { .. });
        assert!(no_drift);
        // No AutoFixed audit row even though the gate would
        // mathematically pass (drift = 0 + count = 0).
        assert!(audit
            .snapshot_of(ReconcileAuditEventType::AutoFixed)
            .is_empty());
    }

    #[test]
    fn idempotent_rerun_same_period_same_decision() {
        let (r, audit, history, _stripe) = fresh();
        let t = Uuid::now_v7();
        let s = snap(t, 100, 100, 100);
        let dec1 = r.reconcile(&s, "2026-05", 100).unwrap();
        let dec2 = r.reconcile(&s, "2026-05", 100).unwrap();
        assert_eq!(dec1, dec2);
        // Same canonical PK → second insert is idempotent; ledger
        // still has 1 row.
        assert_eq!(history.len(), 1);
        // Both runs emitted RunStarted + NoDrift = 4 audit rows.
        assert_eq!(audit.len(), 4);
    }

    #[test]
    fn config_override_threshold_routes_decision() {
        let audit = Arc::new(InMemoryReconcileAuditSink::new());
        let history = Arc::new(InMemoryDriftHistoryLedger::new());
        let stripe = Arc::new(InMemoryStripeSubmissionControl::new());
        // Tighter ladder: quiet 1e-6, sev3 1e-5, sev1 1e-4.
        let cfg = ReconcileConfig::new(5, 1e-6, 1e-6, 1e-5, 1e-4).unwrap();
        let r = InMemoryBillingReconciler::with_config(
            Arc::clone(&audit),
            Arc::clone(&history),
            Arc::clone(&stripe),
            cfg,
        );
        let t = Uuid::now_v7();
        // Drift = 0.001 (1e-3) → past sev1 (1e-4) under tighter
        // ladder; SEV-1.
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(999, 1),
            LayerTotals::new(1000, 1),
            LayerTotals::new(1000, 1),
        );
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        let sev1 = matches!(dec, ReconcileDecision::PageSev1AutoPaused { .. });
        assert!(sev1);
    }

    #[test]
    fn tenant_isolation_drift_routes_per_tenant() {
        let (r, _audit, history, stripe) = fresh();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        // tenant 1: SEV-1 (5% drift); tenant 2: NoDrift.
        let s1 = ReconcileSnapshot::new(
            t1,
            LayerTotals::new(95, 1),
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 1),
        );
        let s2 = snap(t2, 100, 100, 100);
        let d1 = r.reconcile(&s1, "2026-05", 100).unwrap();
        let d2 = r.reconcile(&s2, "2026-05", 100).unwrap();
        let s1_pause = matches!(d1, ReconcileDecision::PageSev1AutoPaused { .. });
        let s2_no = matches!(d2, ReconcileDecision::NoDrift { .. });
        assert!(s1_pause);
        assert!(s2_no);
        // Stripe paused only for tenant 1.
        assert!(stripe.is_paused(t1, "2026-05").unwrap());
        assert!(!stripe.is_paused(t2, "2026-05").unwrap());
        // Two history rows (one per tenant).
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn primary_layer_routes_layer3_for_layer3_drift() {
        let (r, audit, _history, _stripe) = fresh();
        let t = Uuid::now_v7();
        // Layer 1 + Layer 2 match; Layer 3 diverges 5% → SEV-1
        // primary = Layer3Stripe.
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 1),
            LayerTotals::new(95, 1),
        );
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        match dec {
            ReconcileDecision::PageSev1AutoPaused { primary_layer, .. } => {
                assert_eq!(primary_layer, ReconcileLayerKind::Layer3Stripe);
            }
            other => unreachable!("{other:?}"),
        }
        let stripe_paused = audit.snapshot_of(ReconcileAuditEventType::StripePaused);
        assert_eq!(stripe_paused.len(), 1);
        assert_eq!(
            stripe_paused[0].primary_layer,
            Some(ReconcileLayerKind::Layer3Stripe)
        );
    }

    #[test]
    fn zero_input_no_panic_returns_no_drift() {
        let (r, _audit, history, _stripe) = fresh();
        let t = Uuid::now_v7();
        // All three layers zero — degenerate input; canonical
        // NoDrift.
        let s = snap(t, 0, 0, 0);
        let dec = r.reconcile(&s, "2026-05", 100).unwrap();
        let no_drift = matches!(dec, ReconcileDecision::NoDrift { .. });
        assert!(no_drift);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn stripe_pause_idempotent_on_rerun_sev1() {
        let (r, _audit, _history, stripe) = fresh();
        let t = Uuid::now_v7();
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(95, 1),
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 1),
        );
        // First run pauses; second run sees AlreadyPaused but still
        // returns ack=true on the decision (the canonical idempotent
        // SEV-1 re-fire arm).
        r.reconcile(&s, "2026-05", 100).unwrap();
        let dec2 = r.reconcile(&s, "2026-05", 200).unwrap();
        match dec2 {
            ReconcileDecision::PageSev1AutoPaused { pause_acked, .. } => {
                assert!(pause_acked);
            }
            other => unreachable!("{other:?}"),
        }
        assert_eq!(stripe.paused_count(), 1);
    }
}
