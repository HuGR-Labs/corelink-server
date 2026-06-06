//! Multi-burn-rate alert orchestrator: composes calculator + audit
//! sink + PagerDuty dispatcher behind the canonical
//! audit-emit-BEFORE-mutation fail-CLOSED envelope.
//!
//! ## Audit-fail-CLOSED envelope
//!
//! Per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (HIGH; lift from S-07
//! P1-1 fix plus Lote 10.6bis pattern): every state mutation +
//! external dispatch is preceded by the corresponding audit emit;
//! audit failure aborts the alert path + returns
//! [`SloError::Audit`]. The envelope ordering is:
//!
//! 1. `BurnRateEvaluated` audit emit (informational; surfaces inputs +
//!    decision).
//! 2. Branch on `AlertDecision::is_quiet()`:
//!    - quiet: emit `AlertQuiet` + return; no dispatch.
//!    - non-quiet: emit `AlertFired`.
//! 3. Branch on `AlertDecision::is_page()`:
//!    - page: emit `PageDispatched` BEFORE dispatch attempt; on
//!      audit failure abort with `SloError::Audit`; on dispatch
//!      failure return `SloError::Dispatcher` (the audit row records
//!      the intent — fail-OPEN at the dispatcher boundary so the
//!      alert is recoverable).
//!    - ticket: emit `TicketFiled`; no dispatch (business-hours
//!      review queue; production wiring routes via WI-S13 admin
//!      plane forward).
//!
//! ## Per-tenant ledger
//!
//! Per `INV-TENANT-ISOLATION`: the orchestrator carries a per-tenant
//! ledger so concurrent burn-rate evaluation on tenant A never
//! perturbs tenant B's flapping state. The ledger lives under a
//! per-instance `Arc<Mutex<>>` (F-001 closure; NO process-global
//! `static LazyLock`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{SloAuditEmitError, SloAuditEventType, SloAuditRecord, SloAuditSink};
use crate::calculator::{BurnRateCalculator, BurnRateSample};
use crate::decision::AlertDecision;
use crate::definition::SloDefinition;
use crate::error::SloError;
use crate::pagerduty::{
    PagerDutyDispatcher, PagerDutyEvent, PagerDutyEventAction, PagerDutyServiceKey,
};
use crate::window::BurnRateWindow;

/// Outcome of a single [`MultiBurnRateAlert::evaluate`] call: the
/// canonical alert decision + the dedup key used for PagerDuty
/// dispatch (empty when the decision is quiet) + whether the
/// dispatcher was called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertEvaluation {
    /// Canonical alert decision (see [`AlertDecision`]).
    pub decision: AlertDecision,
    /// Dedup key used for PagerDuty dispatch
    /// (`{sli_slug}:{window_slug}:{tenant_id}`); empty string on
    /// quiet arms.
    pub pagerduty_dedup_key: String,
    /// Whether the dispatcher was called (true on page arms only;
    /// false on ticket + quiet).
    pub dispatched_to_pagerduty: bool,
    /// Number of distinct (sli, window) tuples evaluated for this
    /// tenant in the orchestrator's lifetime (per-tenant ledger
    /// audit count). Used by chaos / property tests to validate
    /// per-tenant isolation.
    pub tenant_evaluation_count: u64,
}

/// Specialized [`AlertEvaluation`] result when the dispatcher
/// produces a transport failure: the alert decision was canonical
/// and the audit row was committed BEFORE the dispatch attempt, so
/// the alert is recoverable — production wiring increments
/// `corelink_alerts_dispatched_total{dispatch="fail"}` SEV-2 and
/// retries via Twilio fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertEvaluationOutcome {
    /// The canonical evaluation with `dispatched_to_pagerduty`
    /// reflecting the actual dispatch attempt's outcome.
    pub evaluation: AlertEvaluation,
    /// Whether the dispatcher accepted the event (`true`) or
    /// rejected it (`false`; production wiring fails OPEN to Twilio
    /// fallback).
    pub dispatcher_accepted: bool,
}

#[derive(Default, Debug)]
struct PerTenantLedger {
    evaluation_count: u64,
}

/// Multi-burn-rate alert orchestrator. Composes
/// [`BurnRateCalculator`] (pure-logic decision) with an
/// [`SloAuditSink`] (audit-emit-BEFORE-mutation fail-CLOSED envelope)
/// and a [`PagerDutyDispatcher`] (dispatch-on-page IFF
/// `AlertDecision::is_page()`).
#[derive(Clone, Debug)]
pub struct MultiBurnRateAlert<A, P>
where
    A: SloAuditSink,
    P: PagerDutyDispatcher,
{
    audit_sink: Arc<A>,
    dispatcher: Arc<P>,
    service_key: PagerDutyServiceKey,
    calculator: BurnRateCalculator,
    tenant_ledger: Arc<Mutex<HashMap<String, PerTenantLedger>>>,
}

impl<A, P> MultiBurnRateAlert<A, P>
where
    A: SloAuditSink,
    P: PagerDutyDispatcher,
{
    /// Construct a fresh orchestrator bound to the given audit sink +
    /// PagerDuty dispatcher + canonical service routing key.
    #[must_use]
    pub fn new(audit_sink: Arc<A>, dispatcher: Arc<P>, service_key: PagerDutyServiceKey) -> Self {
        Self {
            audit_sink,
            dispatcher,
            service_key,
            calculator: BurnRateCalculator::new(),
            tenant_ledger: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Evaluate a single (slo, window, tenant_id, sample) tuple per
    /// the canonical Google SRE Workbook Ch 5 Table 4 boundary
    /// discipline + the audit-emit-BEFORE-mutation envelope.
    ///
    /// Returns [`AlertEvaluation`] when the dispatcher accepts (or
    /// when the arm is quiet / ticket; no dispatch). Returns
    /// [`SloError::Audit`] on audit failure (the alert is fail-CLOSED;
    /// no dispatch attempted). Returns [`SloError::Dispatcher`] on
    /// dispatcher transport failure (the audit row IS committed; the
    /// alert is fail-OPEN at the dispatcher boundary so the alert is
    /// recoverable via Twilio fallback).
    ///
    /// # Errors
    ///
    /// - [`SloError::Audit`] on any audit sink failure.
    /// - [`SloError::Dispatcher`] on dispatcher transport failure.
    /// - [`SloError::Internal`] on mutex poisoning of the per-tenant
    ///   ledger.
    pub fn evaluate(
        &self,
        slo: SloDefinition,
        window: BurnRateWindow,
        tenant_id: &str,
        sample: BurnRateSample,
        now_ms: u64,
    ) -> Result<AlertEvaluation, SloError> {
        let decision = self.calculator.decide(slo, window, sample);
        let dedup_key = if decision.is_quiet() {
            String::new()
        } else {
            format!("{}:{}:{}", slo.sli.slug(), window.slug(), tenant_id)
        };

        // 1. burn_rate_evaluated audit emit (informational).
        self.emit_audit(SloAuditRecord {
            event_type: SloAuditEventType::BurnRateEvaluated,
            sli_slug: slo.sli.slug(),
            window_slug: window.slug(),
            decision_slug: decision.slug(),
            tenant_id: tenant_id.to_string(),
            pagerduty_dedup_key: dedup_key.clone(),
            now_ms,
        })?;

        // 2. Branch on decision + emit + (optionally) dispatch.
        let dispatched = if decision.is_quiet() {
            self.emit_audit(SloAuditRecord {
                event_type: SloAuditEventType::AlertQuiet,
                sli_slug: slo.sli.slug(),
                window_slug: window.slug(),
                decision_slug: decision.slug(),
                tenant_id: tenant_id.to_string(),
                pagerduty_dedup_key: dedup_key.clone(),
                now_ms,
            })?;
            false
        } else {
            self.emit_audit(SloAuditRecord {
                event_type: SloAuditEventType::AlertFired,
                sli_slug: slo.sli.slug(),
                window_slug: window.slug(),
                decision_slug: decision.slug(),
                tenant_id: tenant_id.to_string(),
                pagerduty_dedup_key: dedup_key.clone(),
                now_ms,
            })?;
            if decision.is_page() {
                self.dispatch_page(slo, window, tenant_id, decision, dedup_key.clone(), now_ms)?
            } else {
                self.emit_audit(SloAuditRecord {
                    event_type: SloAuditEventType::TicketFiled,
                    sli_slug: slo.sli.slug(),
                    window_slug: window.slug(),
                    decision_slug: decision.slug(),
                    tenant_id: tenant_id.to_string(),
                    pagerduty_dedup_key: dedup_key.clone(),
                    now_ms,
                })?;
                false
            }
        };

        // 3. Per-tenant ledger advance (audit row already committed
        // BEFORE this state mutation per S-07 P1-1 fix).
        let tenant_evaluation_count = {
            let mut g = self
                .tenant_ledger
                .lock()
                .map_err(|_| SloError::Internal("tenant ledger mutex poisoned".to_string()))?;
            let entry = g
                .entry(tenant_id.to_string())
                .or_insert_with(PerTenantLedger::default);
            entry.evaluation_count = entry.evaluation_count.saturating_add(1);
            entry.evaluation_count
        };

        Ok(AlertEvaluation {
            decision,
            pagerduty_dedup_key: dedup_key,
            dispatched_to_pagerduty: dispatched,
            tenant_evaluation_count,
        })
    }

    /// Evaluate a single tuple but tolerate dispatcher transport
    /// failure (return the canonical [`AlertEvaluationOutcome`] with
    /// `dispatcher_accepted` false). Audit failure still aborts
    /// fail-CLOSED.
    ///
    /// # Errors
    ///
    /// - [`SloError::Audit`] on audit sink failure.
    /// - [`SloError::Internal`] on mutex poisoning.
    pub fn evaluate_dispatcher_fail_open(
        &self,
        slo: SloDefinition,
        window: BurnRateWindow,
        tenant_id: &str,
        sample: BurnRateSample,
        now_ms: u64,
    ) -> Result<AlertEvaluationOutcome, SloError> {
        match self.evaluate(slo, window, tenant_id, sample, now_ms) {
            Ok(evaluation) => Ok(AlertEvaluationOutcome {
                evaluation,
                dispatcher_accepted: true,
            }),
            Err(SloError::Dispatcher(_)) => {
                // Re-derive the canonical decision so the caller
                // surfaces the same decision shape even when the
                // dispatcher rejected.
                let decision = self.calculator.decide(slo, window, sample);
                let dedup_key = if decision.is_quiet() {
                    String::new()
                } else {
                    format!("{}:{}:{}", slo.sli.slug(), window.slug(), tenant_id)
                };
                let tenant_evaluation_count = {
                    let g = self.tenant_ledger.lock().map_err(|_| {
                        SloError::Internal("tenant ledger mutex poisoned".to_string())
                    })?;
                    g.get(tenant_id).map_or(0, |e| e.evaluation_count)
                };
                Ok(AlertEvaluationOutcome {
                    evaluation: AlertEvaluation {
                        decision,
                        pagerduty_dedup_key: dedup_key,
                        dispatched_to_pagerduty: false,
                        tenant_evaluation_count,
                    },
                    dispatcher_accepted: false,
                })
            }
            Err(other) => Err(other),
        }
    }

    fn dispatch_page(
        &self,
        slo: SloDefinition,
        window: BurnRateWindow,
        tenant_id: &str,
        decision: AlertDecision,
        dedup_key: String,
        now_ms: u64,
    ) -> Result<bool, SloError> {
        let summary = format!(
            "{} burn-rate {} → {}",
            slo.sli.slug(),
            window.slug(),
            decision.slug()
        );
        let runbook_url = format!(
            "https://corelink.io/runbooks/RB-SLO-{}-{}.md",
            slo.sli.slug(),
            window.slug()
        );

        // Audit emit BEFORE dispatch attempt — the audit row records
        // the intent so even a dispatcher transport failure leaves a
        // canonical compliance trail.
        self.emit_audit(SloAuditRecord {
            event_type: SloAuditEventType::PageDispatched,
            sli_slug: slo.sli.slug(),
            window_slug: window.slug(),
            decision_slug: decision.slug(),
            tenant_id: tenant_id.to_string(),
            pagerduty_dedup_key: dedup_key.clone(),
            now_ms,
        })?;

        let event = PagerDutyEvent {
            service: self.service_key,
            event_action: PagerDutyEventAction::Trigger,
            dedup_key,
            severity: decision.severity_label(),
            summary,
            source: "slo-orchestrator",
            runbook_url,
        };
        self.dispatcher.dispatch(event)?;
        Ok(true)
    }

    fn emit_audit(&self, record: SloAuditRecord) -> Result<(), SloError> {
        let res: Result<(), SloAuditEmitError> = self.audit_sink.emit(record);
        res.map_err(SloError::from)
    }

    /// Snapshot the evaluation count for a single tenant (per-tenant
    /// ledger introspection for chaos / property tests).
    #[must_use]
    pub fn tenant_evaluation_count(&self, tenant_id: &str) -> u64 {
        match self.tenant_ledger.lock() {
            Ok(g) => g.get(tenant_id).map_or(0, |e| e.evaluation_count),
            Err(p) => p
                .into_inner()
                .get(tenant_id)
                .map_or(0, |e| e.evaluation_count),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{FailingSloAuditSink, InMemorySloAuditSink};
    use crate::definition::Sli;
    use crate::pagerduty::{FailingPagerDutyDispatcher, InMemoryPagerDutyDispatcher};

    fn slo_999() -> SloDefinition {
        SloDefinition::new(Sli::AvailCasGet, 0.999).unwrap()
    }

    fn fixture() -> (
        Arc<InMemorySloAuditSink>,
        Arc<InMemoryPagerDutyDispatcher>,
        MultiBurnRateAlert<InMemorySloAuditSink, InMemoryPagerDutyDispatcher>,
    ) {
        let audit = Arc::new(InMemorySloAuditSink::new());
        let dispatcher = Arc::new(InMemoryPagerDutyDispatcher::new());
        let alert = MultiBurnRateAlert::new(
            Arc::clone(&audit),
            Arc::clone(&dispatcher),
            PagerDutyServiceKey::ProdUs,
        );
        (audit, dispatcher, alert)
    }

    #[test]
    fn quiet_decision_emits_burn_rate_and_quiet_audit_no_dispatch() {
        let (audit, dispatcher, alert) = fixture();
        let s = BurnRateSample::new(0, 1_000_000);
        let outcome = alert
            .evaluate(slo_999(), BurnRateWindow::Fast1h, "t1", s, 1)
            .unwrap();
        assert_eq!(outcome.decision, AlertDecision::Quiet);
        assert!(!outcome.dispatched_to_pagerduty);
        assert!(outcome.pagerduty_dedup_key.is_empty());
        let snap = audit.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].event_type, SloAuditEventType::BurnRateEvaluated);
        assert_eq!(snap[1].event_type, SloAuditEventType::AlertQuiet);
        assert_eq!(dispatcher.attempt_count(), 0);
    }

    #[test]
    fn page_decision_dispatches_and_emits_4_audit_arms() {
        let (audit, dispatcher, alert) = fixture();
        // 99.9 % SLO × 14.4 = 1.44 % threshold; 5 % >> threshold.
        let s = BurnRateSample::new(50, 1_000);
        let outcome = alert
            .evaluate(slo_999(), BurnRateWindow::Fast1h, "t1", s, 1)
            .unwrap();
        assert_eq!(outcome.decision, AlertDecision::PageSev0);
        assert!(outcome.dispatched_to_pagerduty);
        assert_eq!(outcome.pagerduty_dedup_key, "SLI-AVAIL-CAS-GET:fast_1h:t1");
        let snap = audit.snapshot();
        let arms: Vec<SloAuditEventType> = snap.iter().map(|r| r.event_type).collect();
        assert_eq!(
            arms,
            vec![
                SloAuditEventType::BurnRateEvaluated,
                SloAuditEventType::AlertFired,
                SloAuditEventType::PageDispatched,
            ]
        );
        assert_eq!(dispatcher.attempt_count(), 1);
        assert_eq!(dispatcher.open_incident_count(), 1);
    }

    #[test]
    fn ticket_decision_does_not_dispatch_emits_3_audit_arms() {
        let (audit, dispatcher, alert) = fixture();
        // 0.1 % × 3 = 0.3 % threshold; 0.5 % > threshold (slow_24h
        // → ticket_sev2).
        let s = BurnRateSample::new(5, 1_000);
        let outcome = alert
            .evaluate(slo_999(), BurnRateWindow::Slow24h, "t1", s, 1)
            .unwrap();
        assert_eq!(outcome.decision, AlertDecision::TicketSev2);
        assert!(!outcome.dispatched_to_pagerduty);
        let snap = audit.snapshot();
        let arms: Vec<SloAuditEventType> = snap.iter().map(|r| r.event_type).collect();
        assert_eq!(
            arms,
            vec![
                SloAuditEventType::BurnRateEvaluated,
                SloAuditEventType::AlertFired,
                SloAuditEventType::TicketFiled,
            ]
        );
        assert_eq!(dispatcher.attempt_count(), 0);
    }

    #[test]
    fn audit_failure_aborts_fail_closed() {
        let audit = Arc::new(FailingSloAuditSink::new());
        let dispatcher = Arc::new(InMemoryPagerDutyDispatcher::new());
        let alert =
            MultiBurnRateAlert::new(audit, Arc::clone(&dispatcher), PagerDutyServiceKey::ProdUs);
        let s = BurnRateSample::new(50, 1_000);
        let err = alert
            .evaluate(slo_999(), BurnRateWindow::Fast1h, "t1", s, 1)
            .unwrap_err();
        assert!(matches!(err, SloError::Audit(_)));
        assert_eq!(
            dispatcher.attempt_count(),
            0,
            "audit failure must NOT lead to dispatch"
        );
    }

    #[test]
    fn dispatcher_failure_returns_dispatcher_arm_audit_committed() {
        let audit = Arc::new(InMemorySloAuditSink::new());
        let dispatcher = Arc::new(FailingPagerDutyDispatcher::new());
        let alert =
            MultiBurnRateAlert::new(Arc::clone(&audit), dispatcher, PagerDutyServiceKey::ProdUs);
        let s = BurnRateSample::new(50, 1_000);
        let err = alert
            .evaluate(slo_999(), BurnRateWindow::Fast1h, "t1", s, 1)
            .unwrap_err();
        assert!(matches!(err, SloError::Dispatcher(_)));
        // Audit row IS committed BEFORE dispatcher transport failure.
        let arms: Vec<SloAuditEventType> = audit.snapshot().iter().map(|r| r.event_type).collect();
        assert_eq!(
            arms,
            vec![
                SloAuditEventType::BurnRateEvaluated,
                SloAuditEventType::AlertFired,
                SloAuditEventType::PageDispatched,
            ]
        );
    }

    #[test]
    fn dispatcher_failure_fail_open_variant_returns_canonical_outcome() {
        let audit = Arc::new(InMemorySloAuditSink::new());
        let dispatcher = Arc::new(FailingPagerDutyDispatcher::new());
        let alert =
            MultiBurnRateAlert::new(Arc::clone(&audit), dispatcher, PagerDutyServiceKey::ProdUs);
        let s = BurnRateSample::new(50, 1_000);
        let outcome = alert
            .evaluate_dispatcher_fail_open(slo_999(), BurnRateWindow::Fast1h, "t1", s, 1)
            .unwrap();
        assert_eq!(outcome.evaluation.decision, AlertDecision::PageSev0);
        assert!(!outcome.dispatcher_accepted);
        assert!(!outcome.evaluation.dispatched_to_pagerduty);
    }

    #[test]
    fn tenant_evaluation_counts_isolated_per_tenant() {
        let (_audit, _dispatcher, alert) = fixture();
        let s = BurnRateSample::new(0, 1_000);
        for _ in 0..3 {
            alert
                .evaluate(slo_999(), BurnRateWindow::Fast1h, "tA", s, 1)
                .unwrap();
        }
        for _ in 0..7 {
            alert
                .evaluate(slo_999(), BurnRateWindow::Fast1h, "tB", s, 1)
                .unwrap();
        }
        assert_eq!(alert.tenant_evaluation_count("tA"), 3);
        assert_eq!(alert.tenant_evaluation_count("tB"), 7);
        assert_eq!(alert.tenant_evaluation_count("missing"), 0);
    }

    #[test]
    fn dedup_key_canonical_format_pinned() {
        let (_audit, dispatcher, alert) = fixture();
        let s = BurnRateSample::new(50, 1_000);
        alert
            .evaluate(slo_999(), BurnRateWindow::Fast1h, "tenant-x", s, 1)
            .unwrap();
        let attempts = dispatcher.snapshot_attempts();
        assert_eq!(attempts.len(), 1);
        let first = attempts.first().unwrap();
        assert_eq!(first.dedup_key, "SLI-AVAIL-CAS-GET:fast_1h:tenant-x");
        assert_eq!(first.severity, "sev0");
        assert_eq!(first.service, PagerDutyServiceKey::ProdUs);
    }

    #[test]
    fn long_burn_does_not_page() {
        let (_audit, dispatcher, alert) = fixture();
        // Sample at 0.2 % > 0.1 % long_3d threshold.
        let s = BurnRateSample::new(2, 1_000);
        let outcome = alert
            .evaluate(slo_999(), BurnRateWindow::Long3d, "t1", s, 1)
            .unwrap();
        assert_eq!(outcome.decision, AlertDecision::TicketSev3);
        assert!(!outcome.decision.is_page());
        assert!(!outcome.dispatched_to_pagerduty);
        assert_eq!(dispatcher.attempt_count(), 0);
    }

    #[test]
    fn fast_burn_pages_immediately() {
        let (_audit, dispatcher, alert) = fixture();
        let s = BurnRateSample::new(50, 1_000);
        let outcome = alert
            .evaluate(slo_999(), BurnRateWindow::Fast1h, "t1", s, 1)
            .unwrap();
        assert!(outcome.decision.is_page());
        assert_eq!(dispatcher.open_incident_count(), 1);
    }

    #[test]
    fn dedup_key_idempotent_across_repeated_evaluations() {
        let (_audit, dispatcher, alert) = fixture();
        let s = BurnRateSample::new(50, 1_000);
        for _ in 0..5 {
            alert
                .evaluate(slo_999(), BurnRateWindow::Fast1h, "t1", s, 1)
                .unwrap();
        }
        assert_eq!(dispatcher.attempt_count(), 5);
        assert_eq!(
            dispatcher.open_incident_count(),
            1,
            "same dedup key MUST collapse to single incident"
        );
    }
}
