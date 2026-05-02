//! Abuse-detection metrics emission trait + InMemory test sink.
//!
//! ## Canonical 5-metric ladder (mirror of WI-S08-004 §6.1.10)
//!
//! - `corelink.abuse.score{tenant_id, tier}` — gauge (latest score per
//!   tenant; observed via the in-memory observer; production wiring
//!   maps to a Prometheus gauge).
//! - `corelink.abuse.tier_count_total{tier=Benign|Suspicious|Malicious}`
//!   — counter; SLI denominator source.
//! - `corelink.abuse.auto_suspend_attempts_total{tenant_id}` — counter;
//!   **alert SEV-1 if > 0; LGPD violation canary** (humane response
//!   sprint contract §7.10.s08.3).
//! - `corelink.abuse.downgrade_applied_total{tenant_id}` — counter;
//!   SLI numerator for SilentDowngrade tier rate.
//! - `corelink.abuse.cross_tenant_feature_leak_total{tenant_id}` —
//!   counter; **alert SEV-1 if > 0; INV-TENANT-ISOLATION canary**
//!   (sprint contract §7.10.s08.4 explicit).

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

/// Canonical metric name list (mirrors WI-S08-004 §6.1.10).
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 5] {
    &[
        "corelink.abuse.score",
        "corelink.abuse.tier_count_total",
        "corelink.abuse.auto_suspend_attempts_total",
        "corelink.abuse.downgrade_applied_total",
        "corelink.abuse.cross_tenant_feature_leak_total",
    ]
}

/// Canonical decision tier label for the `tier_count_total` counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AbuseTierLabel {
    /// Score below SUSPICIOUS_THRESHOLD; no action.
    Benign,
    /// Score in [SUSPICIOUS_THRESHOLD, MALICIOUS_THRESHOLD); silent
    /// downgrade applied.
    Suspicious,
    /// Score ≥ MALICIOUS_THRESHOLD; admin review trigger.
    Malicious,
}

impl AbuseTierLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Benign => "benign",
            Self::Suspicious => "suspicious",
            Self::Malicious => "malicious",
        }
    }
}

/// Canonical metric kinds for the in-memory test sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AbuseMetricKind {
    /// `tier_count_total` counter.
    TierCountTotal,
    /// `auto_suspend_attempts_total` counter (LGPD violation canary).
    AutoSuspendAttemptsTotal,
    /// `downgrade_applied_total` counter.
    DowngradeAppliedTotal,
    /// `cross_tenant_feature_leak_total` counter
    /// (INV-TENANT-ISOLATION canary).
    CrossTenantFeatureLeakTotal,
}

/// Errors surfaced by [`AbuseMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AbuseMetricsObserverError {
    /// Backend transport failure.
    #[error("abuse metrics observer error: {0}")]
    Backend(String),
}

/// Trait surface for the metrics observer. Production wiring composes
/// a Prometheus / OTLP shim; tests use [`InMemoryAbuseMetrics`] for
/// deterministic assertions.
pub trait AbuseMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Record the latest score for a tenant (gauge).
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn observe_score(
        &self,
        tenant_id: Uuid,
        tier: AbuseTierLabel,
        score: f64,
    ) -> Result<(), AbuseMetricsObserverError>;

    /// Record a `tier_count_total{tier=…}` increment.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_tier(
        &self,
        tier: AbuseTierLabel,
    ) -> Result<(), AbuseMetricsObserverError>;

    /// Record an `auto_suspend_attempts_total{tenant_id}` increment.
    /// SEV-1 LGPD violation canary; production wiring fires the
    /// PagerDuty alert when this counter is positive.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_auto_suspend_attempt(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), AbuseMetricsObserverError>;

    /// Record a `downgrade_applied_total{tenant_id}` increment.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_downgrade_applied(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), AbuseMetricsObserverError>;

    /// Record a `cross_tenant_feature_leak_total{tenant_id}` increment.
    /// SEV-1 INV-TENANT-ISOLATION canary; production wiring fires the
    /// PagerDuty alert when this counter is positive.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_cross_tenant_feature_leak(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), AbuseMetricsObserverError>;
}

/// In-memory test metrics observer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryAbuseMetrics {
    counters: std::sync::Arc<Mutex<HashMap<(AbuseMetricKind, Uuid), u64>>>,
    tier_by_label: std::sync::Arc<Mutex<HashMap<AbuseTierLabel, u64>>>,
    score_observations:
        std::sync::Arc<Mutex<HashMap<Uuid, (AbuseTierLabel, f64)>>>,
}

impl InMemoryAbuseMetrics {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total counter (across tenants if applicable).
    #[must_use]
    pub fn counter_total(&self, kind: AbuseMetricKind) -> u64 {
        let g = match self.counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, v)| v)
            .sum()
    }

    /// Counter for a specific tenant.
    #[must_use]
    pub fn counter_for_tenant(
        &self,
        kind: AbuseMetricKind,
        tenant_id: Uuid,
    ) -> u64 {
        let g = match self.counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(kind, tenant_id)).copied().unwrap_or(0)
    }

    /// Tier-total counter for a specific tier label.
    #[must_use]
    pub fn tier_total_for_label(&self, label: AbuseTierLabel) -> u64 {
        let g = match self.tier_by_label.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&label).copied().unwrap_or(0)
    }

    /// Latest observed score for a tenant.
    #[must_use]
    pub fn score_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Option<(AbuseTierLabel, f64)> {
        let g = match self.score_observations.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&tenant_id).copied()
    }
}

impl AbuseMetricsObserver for InMemoryAbuseMetrics {
    fn observe_score(
        &self,
        tenant_id: Uuid,
        tier: AbuseTierLabel,
        score: f64,
    ) -> Result<(), AbuseMetricsObserverError> {
        let mut g = self.score_observations.lock().map_err(|_| {
            AbuseMetricsObserverError::Backend(
                "metrics score observations mutex poisoned".to_string(),
            )
        })?;
        g.insert(tenant_id, (tier, score));
        Ok(())
    }

    fn record_tier(
        &self,
        tier: AbuseTierLabel,
    ) -> Result<(), AbuseMetricsObserverError> {
        let mut g = self.tier_by_label.lock().map_err(|_| {
            AbuseMetricsObserverError::Backend(
                "metrics tier_by_label mutex poisoned".to_string(),
            )
        })?;
        *g.entry(tier).or_insert(0) += 1;
        drop(g);
        let mut g = self.counters.lock().map_err(|_| {
            AbuseMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((AbuseMetricKind::TierCountTotal, Uuid::nil())).or_insert(0) +=
            1;
        Ok(())
    }

    fn record_auto_suspend_attempt(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), AbuseMetricsObserverError> {
        let mut g = self.counters.lock().map_err(|_| {
            AbuseMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((AbuseMetricKind::AutoSuspendAttemptsTotal, tenant_id))
            .or_insert(0) += 1;
        Ok(())
    }

    fn record_downgrade_applied(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), AbuseMetricsObserverError> {
        let mut g = self.counters.lock().map_err(|_| {
            AbuseMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((AbuseMetricKind::DowngradeAppliedTotal, tenant_id))
            .or_insert(0) += 1;
        Ok(())
    }

    fn record_cross_tenant_feature_leak(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), AbuseMetricsObserverError> {
        let mut g = self.counters.lock().map_err(|_| {
            AbuseMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((AbuseMetricKind::CrossTenantFeatureLeakTotal, tenant_id))
            .or_insert(0) += 1;
        Ok(())
    }
}

/// Always-failing observer for adversarial tests of fail-soft metric
/// emission (production wiring may downgrade metric failures to
/// log-and-continue per WI §14.s08.004.10; the orchestrator
/// fail-closes ONLY on audit emit failure, not metric).
#[derive(Debug, Default)]
pub struct FailingAbuseMetrics;

impl FailingAbuseMetrics {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AbuseMetricsObserver for FailingAbuseMetrics {
    fn observe_score(
        &self,
        _t: Uuid,
        _l: AbuseTierLabel,
        _s: f64,
    ) -> Result<(), AbuseMetricsObserverError> {
        Err(AbuseMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_tier(
        &self,
        _l: AbuseTierLabel,
    ) -> Result<(), AbuseMetricsObserverError> {
        Err(AbuseMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_auto_suspend_attempt(
        &self,
        _t: Uuid,
    ) -> Result<(), AbuseMetricsObserverError> {
        Err(AbuseMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_downgrade_applied(
        &self,
        _t: Uuid,
    ) -> Result<(), AbuseMetricsObserverError> {
        Err(AbuseMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_cross_tenant_feature_leak(
        &self,
        _t: Uuid,
    ) -> Result<(), AbuseMetricsObserverError> {
        Err(AbuseMetricsObserverError::Backend("induced".to_string()))
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

    #[test]
    fn canonical_metric_names_count() {
        let names = canonical_metric_names();
        assert_eq!(names.len(), 5);
        for n in names {
            assert!(n.starts_with("corelink.abuse."));
        }
    }

    #[test]
    fn tier_label_strings_distinct() {
        let v = [
            AbuseTierLabel::Benign,
            AbuseTierLabel::Suspicious,
            AbuseTierLabel::Malicious,
        ];
        let mut set = std::collections::HashSet::new();
        for l in v {
            assert!(set.insert(l.as_str()));
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn in_memory_tier_increments_aggregate_and_per_label() {
        let m = InMemoryAbuseMetrics::new();
        m.record_tier(AbuseTierLabel::Benign).unwrap();
        m.record_tier(AbuseTierLabel::Malicious).unwrap();
        m.record_tier(AbuseTierLabel::Benign).unwrap();
        assert_eq!(m.counter_total(AbuseMetricKind::TierCountTotal), 3);
        assert_eq!(m.tier_total_for_label(AbuseTierLabel::Benign), 2);
        assert_eq!(m.tier_total_for_label(AbuseTierLabel::Malicious), 1);
        assert_eq!(m.tier_total_for_label(AbuseTierLabel::Suspicious), 0);
    }

    #[test]
    fn auto_suspend_attempt_counter_per_tenant() {
        let m = InMemoryAbuseMetrics::new();
        let t = Uuid::from_u128(0xa);
        m.record_auto_suspend_attempt(t).unwrap();
        m.record_auto_suspend_attempt(t).unwrap();
        assert_eq!(
            m.counter_for_tenant(AbuseMetricKind::AutoSuspendAttemptsTotal, t),
            2
        );
    }

    #[test]
    fn downgrade_applied_counter_per_tenant() {
        let m = InMemoryAbuseMetrics::new();
        let t = Uuid::from_u128(0xb);
        m.record_downgrade_applied(t).unwrap();
        assert_eq!(
            m.counter_for_tenant(AbuseMetricKind::DowngradeAppliedTotal, t),
            1
        );
    }

    #[test]
    fn cross_tenant_leak_counter_per_tenant() {
        let m = InMemoryAbuseMetrics::new();
        let t = Uuid::from_u128(0xc);
        m.record_cross_tenant_feature_leak(t).unwrap();
        assert_eq!(
            m.counter_for_tenant(
                AbuseMetricKind::CrossTenantFeatureLeakTotal,
                t
            ),
            1
        );
    }

    #[test]
    fn score_observation_overwrites_per_tenant() {
        let m = InMemoryAbuseMetrics::new();
        let t = Uuid::from_u128(0xd);
        m.observe_score(t, AbuseTierLabel::Benign, 0.1).unwrap();
        m.observe_score(t, AbuseTierLabel::Malicious, 0.97).unwrap();
        let (tier, score) = m.score_for_tenant(t).unwrap();
        assert_eq!(tier, AbuseTierLabel::Malicious);
        assert_eq!(score, 0.97);
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingAbuseMetrics::new();
        let err = m.record_tier(AbuseTierLabel::Benign).unwrap_err();
        assert!(matches!(err, AbuseMetricsObserverError::Backend(_)));
    }
}
