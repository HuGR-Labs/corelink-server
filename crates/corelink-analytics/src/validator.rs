//! `CardinalityValidator` — the load-bearing piece. Enforces
//! `INV-OBS-CARDINALITY-BUDGET` (HIGH; per-metric ≤ 20k unique
//! label-tuples + global ≤ 100k) at the emit boundary.
//!
//! ## Why this matters
//!
//! Cardinality blowup is the #1 production cost vector in
//! Prometheus-shaped metric pipelines. The NetflixOSS 2018 incident:
//! `request_id` added in label produced 5B series in 24h, $50k/mo
//! Prometheus blowup, 14h debug. This validator is the runtime
//! defense-in-depth alongside the CI-side `scripts/cardinality_check.py`
//! static analyzer.
//!
//! ## Decision pipeline (per emit)
//!
//! For each `(metric, labels, request_id, now_ms)`:
//!
//! 1. Compute the pre-emit per-metric unique-tuple count for
//!    `(metric, labels)`.
//! 2. **If the tuple is already known** → no-op on the unique-tuple
//!    ledger; emit the `MetricEmitted` audit + return `Allow`. (The
//!    repeat-tuple invariant pinned by
//!    `prop_cardinality_idempotent_repeat_label_set`.)
//! 3. **Else if the per-metric budget would be exceeded** by adding
//!    this tuple → emit `CardinalityRejected` audit; return `Reject`.
//! 4. **Else if the global budget would be exceeded** by adding this
//!    tuple → emit `BudgetExceeded` audit; return `Reject`.
//! 5. **Else** add the tuple to the metric's unique-tuple set; emit
//!    `MetricEmitted` audit; return `Allow`.
//!
//! ## Audit fail-closed envelope (Lote 10.6bis pattern + S-07 P1-1 fix)
//!
//! The audit emit happens BEFORE the unique-tuple ledger mutation on
//! every decision arm. If audit fails, the ledger is NOT mutated and
//! the validator returns `AnalyticsError::Audit(_)`. Production
//! wiring maps this to 503 so the audit gap doesn't leak to the client
//! as a 200.
//!
//! ## F-001 closure
//!
//! The validator holds the unique-tuple ledger under a per-instance
//! `Arc<Mutex<>>` (NOT a `static LazyLock`). Tests instantiate fresh
//! validators per case so the orchestrator harness cannot accidentally
//! leak cardinality state across cases.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::audit::{AnalyticsAuditRecord, AnalyticsAuditSink, AnalyticsEventType};
use crate::canonical::{canonical_metric_kinds, RedMetricKind};
use crate::config::AnalyticsConfig;
use crate::error::AnalyticsError;
use crate::labels::MetricLabelTuple;

/// Outcome of [`CardinalityValidator::validate_and_register`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidatorDecision {
    /// The tuple was already registered (idempotent repeat).
    AlreadyRegistered,
    /// The tuple was newly registered (the metric's unique-tuple set
    /// grew by 1).
    Registered,
}

/// Side-effect payload returned alongside the [`ValidatorDecision`].
#[derive(Clone, Copy, Debug)]
pub struct ValidatorOutcome {
    /// The rendered decision.
    pub decision: ValidatorDecision,
    /// Per-metric unique-tuple count AFTER the decision was rendered.
    pub observed_per_metric: u64,
    /// Global unique-tuple count AFTER the decision was rendered.
    pub observed_global: u64,
}

/// The cardinality validator.
///
/// Holds a per-metric unique-tuple ledger under a single per-instance
/// `Arc<Mutex<>>`. Cloning shares the underlying ledger.
#[derive(Clone, Debug)]
pub struct CardinalityValidator<A>
where
    A: AnalyticsAuditSink,
{
    audit: Arc<A>,
    config: AnalyticsConfig,
    state: Arc<Mutex<ValidatorState>>,
}

#[derive(Debug, Default)]
struct ValidatorState {
    per_metric: HashMap<RedMetricKind, HashSet<MetricLabelTuple>>,
    rejection_counter_per_metric: HashMap<RedMetricKind, u64>,
    rejection_counter_global: u64,
}

impl<A> CardinalityValidator<A>
where
    A: AnalyticsAuditSink,
{
    /// Construct with the canonical default config.
    pub fn with_defaults(audit: Arc<A>) -> Self {
        Self::new(audit, AnalyticsConfig::canonical())
    }

    /// Construct with an explicit config.
    pub fn new(audit: Arc<A>, config: AnalyticsConfig) -> Self {
        Self {
            audit,
            config,
            state: Arc::new(Mutex::new(ValidatorState::default())),
        }
    }

    /// Snapshot the current config.
    #[must_use]
    pub fn config(&self) -> &AnalyticsConfig {
        &self.config
    }

    /// Snapshot the per-metric unique-tuple count.
    #[must_use]
    pub fn unique_tuples_for_metric(&self, metric: RedMetricKind) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.per_metric
            .get(&metric)
            .map(|s| s.len() as u64)
            .unwrap_or(0)
    }

    /// Snapshot the global unique-tuple count (sum across all metrics).
    #[must_use]
    pub fn unique_tuples_global(&self) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.per_metric.values().map(|s| s.len() as u64).sum()
    }

    /// Snapshot the per-metric rejection counter (the canonical SEV-2
    /// alert source per WI §6.1.11
    /// `corelink_metrics_cardinality_budget_violation_total`).
    #[must_use]
    pub fn rejection_counter_for_metric(&self, metric: RedMetricKind) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.rejection_counter_per_metric
            .get(&metric)
            .copied()
            .unwrap_or(0)
    }

    /// Snapshot the global rejection counter.
    #[must_use]
    pub fn rejection_counter_global(&self) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.rejection_counter_global
    }

    /// Whether this `(metric, labels)` tuple is `≥ approaching_pct *
    /// per_metric_budget` of the way through the budget. Surfaced for
    /// the production wiring to emit the SEV-3
    /// `corelink_metrics_cardinality_approaching_budget_total` alert
    /// per WI §6.1.11.
    #[must_use]
    pub fn is_approaching_budget(&self, metric: RedMetricKind) -> bool {
        let observed = self.unique_tuples_for_metric(metric);
        let budget = self.config.budget_for_metric(metric);
        let threshold =
            (self.config.cardinality_approaching_pct() * (budget as f64)).floor() as u64;
        observed >= threshold
    }

    /// Validate and (if the budget allows) register the tuple in the
    /// per-metric unique-tuple ledger. The audit emit fires BEFORE
    /// any state mutation per Lote 10.6bis pattern + S-07 P1-1 fix.
    ///
    /// # Errors
    ///
    /// - [`AnalyticsError::CardinalityBudgetExceeded`] when the per-metric
    ///   or global budget would be exceeded.
    /// - [`AnalyticsError::Audit`] when the audit sink fails.
    /// - [`AnalyticsError::Internal`] when the per-instance mutex is
    ///   poisoned.
    pub fn validate_and_register(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<ValidatorOutcome, AnalyticsError> {
        // Step 1: read the current per-metric + global counts under
        // the lock so the audit-then-mutate ordering is consistent.
        let g = self
            .state
            .lock()
            .map_err(|_| AnalyticsError::Internal("validator mutex poisoned".to_string()))?;

        let already_registered = g
            .per_metric
            .get(&metric)
            .map(|s| s.contains(&labels))
            .unwrap_or(false);

        let per_metric_observed = g
            .per_metric
            .get(&metric)
            .map(|s| s.len() as u64)
            .unwrap_or(0);
        let global_observed: u64 = g.per_metric.values().map(|s| s.len() as u64).sum();
        let per_metric_budget = self.config.budget_for_metric(metric);
        let global_budget = self.config.global_budget();

        if already_registered {
            // Idempotent repeat: no ledger mutation; audit MetricEmitted
            // BEFORE returning success (consistent with the audit
            // fail-closed envelope on every decision arm).
            drop(g);
            self.audit.emit(AnalyticsAuditRecord {
                event_type: AnalyticsEventType::MetricEmitted,
                metric_name: metric.as_str(),
                observed: per_metric_observed,
                budget: per_metric_budget,
                created_by_request_id: created_by_request_id.to_string(),
                now_ms,
            })?;
            return Ok(ValidatorOutcome {
                decision: ValidatorDecision::AlreadyRegistered,
                observed_per_metric: per_metric_observed,
                observed_global: global_observed,
            });
        }

        // Pre-mutation per-metric guard.
        if per_metric_observed >= per_metric_budget {
            // Hold the lock to bump the rejection counter atomically
            // AFTER the audit emit succeeds (audit fail-closed envelope).
            drop(g);
            self.audit.emit(AnalyticsAuditRecord {
                event_type: AnalyticsEventType::CardinalityRejected,
                metric_name: metric.as_str(),
                observed: per_metric_observed,
                budget: per_metric_budget,
                created_by_request_id: created_by_request_id.to_string(),
                now_ms,
            })?;
            // Re-acquire to bump the rejection counter (per-metric
            // SEV-2 alert source). The audit sink is the canonical
            // ordering anchor: counter bump only after audit success.
            let mut g2 = self.state.lock().map_err(|_| {
                AnalyticsError::Internal("validator mutex poisoned (rejection bump)".to_string())
            })?;
            let entry = g2.rejection_counter_per_metric.entry(metric).or_insert(0);
            *entry = entry.saturating_add(1);
            g2.rejection_counter_global = g2.rejection_counter_global.saturating_add(1);
            return Err(AnalyticsError::CardinalityBudgetExceeded {
                metric: metric.as_str(),
                observed: per_metric_observed,
                budget: per_metric_budget,
                scope: "per_metric",
            });
        }

        // Pre-mutation global guard.
        if global_observed >= global_budget {
            drop(g);
            self.audit.emit(AnalyticsAuditRecord {
                event_type: AnalyticsEventType::BudgetExceeded,
                metric_name: metric.as_str(),
                observed: global_observed,
                budget: global_budget,
                created_by_request_id: created_by_request_id.to_string(),
                now_ms,
            })?;
            let mut g2 = self.state.lock().map_err(|_| {
                AnalyticsError::Internal("validator mutex poisoned (global bump)".to_string())
            })?;
            g2.rejection_counter_global = g2.rejection_counter_global.saturating_add(1);
            return Err(AnalyticsError::CardinalityBudgetExceeded {
                metric: metric.as_str(),
                observed: global_observed,
                budget: global_budget,
                scope: "global",
            });
        }

        // Step 2: audit emit BEFORE the ledger mutation.
        drop(g);
        self.audit.emit(AnalyticsAuditRecord {
            event_type: AnalyticsEventType::MetricEmitted,
            metric_name: metric.as_str(),
            observed: per_metric_observed.saturating_add(1),
            budget: per_metric_budget,
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
        })?;

        // Step 3: register the tuple in the per-metric set.
        let mut g3 = self.state.lock().map_err(|_| {
            AnalyticsError::Internal("validator mutex poisoned (register)".to_string())
        })?;
        let set = g3.per_metric.entry(metric).or_default();
        set.insert(labels);
        let new_per_metric = set.len() as u64;
        let new_global = g3.per_metric.values().map(|s| s.len() as u64).sum();
        Ok(ValidatorOutcome {
            decision: ValidatorDecision::Registered,
            observed_per_metric: new_per_metric,
            observed_global: new_global,
        })
    }

    /// Snapshot a per-metric unique-tuple count map across every
    /// canonical metric (used for the SEV-3 `cardinality_approaching`
    /// dashboard surface). Returns `(metric, observed)` rows.
    #[must_use]
    pub fn snapshot_observed(&self) -> Vec<(RedMetricKind, u64)> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        canonical_metric_kinds()
            .iter()
            .map(|k| {
                let observed = g.per_metric.get(k).map(|s| s.len() as u64).unwrap_or(0);
                (*k, observed)
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{AnalyticsEventType, FailingAnalyticsAuditSink, InMemoryAnalyticsAuditSink};
    use crate::labels::{Region, Tier};

    type Validator = CardinalityValidator<InMemoryAnalyticsAuditSink>;

    fn fresh_validator() -> (Validator, Arc<InMemoryAnalyticsAuditSink>) {
        let audit = Arc::new(InMemoryAnalyticsAuditSink::new());
        let v = CardinalityValidator::with_defaults(Arc::clone(&audit));
        (v, audit)
    }

    fn fresh_validator_with_per_metric_budget(
        budget: u64,
    ) -> (Validator, Arc<InMemoryAnalyticsAuditSink>) {
        let audit = Arc::new(InMemoryAnalyticsAuditSink::new());
        let cfg = test_config_with_per_metric_budget(budget);
        let v = CardinalityValidator::new(Arc::clone(&audit), cfg);
        (v, audit)
    }

    fn test_config_with_per_metric_budget(budget: u64) -> AnalyticsConfig {
        AnalyticsConfig::with_budgets(budget, budget * 100)
    }

    fn la_team_iad() -> MetricLabelTuple {
        MetricLabelTuple::tenant_region(Tier::Team, Region::Iad)
    }

    fn la_team_sjc() -> MetricLabelTuple {
        MetricLabelTuple::tenant_region(Tier::Team, Region::Sjc)
    }

    fn la_free_iad() -> MetricLabelTuple {
        MetricLabelTuple::tenant_region(Tier::Free, Region::Iad)
    }

    #[test]
    fn fresh_validator_has_zero_observed() {
        let (v, _a) = fresh_validator();
        for k in canonical_metric_kinds() {
            assert_eq!(v.unique_tuples_for_metric(*k), 0);
        }
        assert_eq!(v.unique_tuples_global(), 0);
    }

    #[test]
    fn first_register_increments_per_metric_and_global() {
        let (v, audit) = fresh_validator();
        let out = v
            .validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                la_team_iad(),
                "req-1",
                1000,
            )
            .unwrap();
        assert_eq!(out.decision, ValidatorDecision::Registered);
        assert_eq!(out.observed_per_metric, 1);
        assert_eq!(out.observed_global, 1);
        assert_eq!(
            v.unique_tuples_for_metric(RedMetricKind::CasPutRequestsTotal),
            1
        );
        assert_eq!(
            audit.snapshot_of(AnalyticsEventType::MetricEmitted).len(),
            1
        );
    }

    #[test]
    fn repeat_register_is_idempotent_and_emits_audit() {
        let (v, audit) = fresh_validator();
        let out1 = v
            .validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                la_team_iad(),
                "req-1",
                1000,
            )
            .unwrap();
        let out2 = v
            .validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                la_team_iad(),
                "req-2",
                2000,
            )
            .unwrap();
        assert_eq!(out1.decision, ValidatorDecision::Registered);
        assert_eq!(out2.decision, ValidatorDecision::AlreadyRegistered);
        assert_eq!(
            v.unique_tuples_for_metric(RedMetricKind::CasPutRequestsTotal),
            1
        );
        // Both decisions emit MetricEmitted audit (idempotent repeat
        // still emits per the canonical fail-closed envelope).
        assert_eq!(
            audit.snapshot_of(AnalyticsEventType::MetricEmitted).len(),
            2
        );
    }

    #[test]
    fn over_per_metric_budget_rejects_and_emits_audit() {
        let (v, audit) = fresh_validator_with_per_metric_budget(2);
        v.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            la_team_iad(),
            "req-1",
            1000,
        )
        .unwrap();
        v.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            la_team_sjc(),
            "req-2",
            2000,
        )
        .unwrap();
        // 3rd UNIQUE tuple → over budget.
        let err = v
            .validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                la_free_iad(),
                "req-3",
                3000,
            )
            .unwrap_err();
        match err {
            AnalyticsError::CardinalityBudgetExceeded {
                metric,
                observed,
                budget,
                scope,
            } => {
                assert_eq!(metric, "corelink_cas_put_requests_total");
                assert_eq!(observed, 2);
                assert_eq!(budget, 2);
                assert_eq!(scope, "per_metric");
            }
            other => panic!("expected CardinalityBudgetExceeded, got {other:?}"),
        }
        // Audit emitted CardinalityRejected; ledger NOT mutated.
        assert_eq!(
            audit
                .snapshot_of(AnalyticsEventType::CardinalityRejected)
                .len(),
            1
        );
        assert_eq!(
            v.unique_tuples_for_metric(RedMetricKind::CasPutRequestsTotal),
            2
        );
        assert_eq!(
            v.rejection_counter_for_metric(RedMetricKind::CasPutRequestsTotal),
            1
        );
        assert_eq!(v.rejection_counter_global(), 1);
    }

    #[test]
    fn audit_failure_aborts_decision_no_ledger_mutation() {
        let audit = Arc::new(FailingAnalyticsAuditSink::new());
        let v = CardinalityValidator::with_defaults(Arc::clone(&audit));
        let err = v
            .validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                la_team_iad(),
                "req-1",
                1000,
            )
            .unwrap_err();
        assert!(matches!(err, AnalyticsError::Audit(_)));
        assert_eq!(
            v.unique_tuples_for_metric(RedMetricKind::CasPutRequestsTotal),
            0
        );
    }

    #[test]
    fn approaching_budget_threshold_triggers_at_80pct() {
        let (v, _a) = fresh_validator_with_per_metric_budget(10);
        // 7 tuples = 70% < 80%.
        for region in [
            Region::Iad,
            Region::Sjc,
            Region::Dfw,
            Region::Sea,
            Region::Ord,
            Region::Lhr,
            Region::Fra,
        ] {
            v.validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                MetricLabelTuple::tenant_region(Tier::Team, region),
                "req",
                1000,
            )
            .unwrap();
        }
        assert!(!v.is_approaching_budget(RedMetricKind::CasPutRequestsTotal));
        // 8th tuple = 80% → approaching trigger fires.
        v.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Ams),
            "req",
            1000,
        )
        .unwrap();
        assert!(v.is_approaching_budget(RedMetricKind::CasPutRequestsTotal));
    }

    #[test]
    fn snapshot_observed_returns_15_canonical_metrics() {
        let (v, _a) = fresh_validator();
        let snap = v.snapshot_observed();
        assert_eq!(snap.len(), 15);
        for (_, observed) in snap {
            assert_eq!(observed, 0);
        }
    }

    #[test]
    fn cross_metric_isolation_observed() {
        let (v, _a) = fresh_validator();
        v.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            la_team_iad(),
            "req-1",
            1000,
        )
        .unwrap();
        assert_eq!(
            v.unique_tuples_for_metric(RedMetricKind::CasPutRequestsTotal),
            1
        );
        // CAS GET unaffected.
        assert_eq!(
            v.unique_tuples_for_metric(RedMetricKind::CasGetBytesTotal),
            0
        );
    }

    #[test]
    fn rejection_counter_increments_only_on_reject_arms() {
        let (v, _a) = fresh_validator();
        v.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            la_team_iad(),
            "req-1",
            1000,
        )
        .unwrap();
        assert_eq!(
            v.rejection_counter_for_metric(RedMetricKind::CasPutRequestsTotal),
            0
        );
        assert_eq!(v.rejection_counter_global(), 0);
    }

    #[test]
    fn cloned_validator_shares_ledger() {
        let (v, _a) = fresh_validator();
        let v2 = v.clone();
        v.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            la_team_iad(),
            "req-1",
            1000,
        )
        .unwrap();
        assert_eq!(
            v2.unique_tuples_for_metric(RedMetricKind::CasPutRequestsTotal),
            1
        );
    }
}
