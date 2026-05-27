//! Quota-CAS metrics emission trait + InMemory test sink.
//!
//! ## Canonical 5-metric ladder (mirror of WI-S08-003 §6.1.10)
//!
//! - `corelink.quota.cas_check_total{result=allow|deny|race}` —
//!   counter; SLI numerator source (allow + deny within plan;
//!   race-detected NOT counted in error budget).
//! - `corelink.quota.cas_denials_total{tenant_id}` — counter; alert
//!   SEV-1 if positive (canonical 100% hard-block fired).
//! - `corelink.quota.cas_race_detected_total{tenant_id}` — counter;
//!   alert SEV-3 if sustained (race-detection observability lineage).
//! - `corelink.quota.cas_retry_after_secs` — histogram of emitted
//!   Retry-After values (lineage signal — drift between this crate's
//!   canonical formula and any S-07 PROVISIONAL leftover wiring is
//!   visible at the dashboard layer).
//! - `corelink.quota.cas_check_duration_us` — histogram of CAS check
//!   duration (microseconds; SLO ≤ 5ms p99 per WI §22).

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

/// Canonical metric name list (mirrors WI-S08-003 §6.1.10).
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 5] {
    &[
        "corelink.quota.cas_check_total",
        "corelink.quota.cas_denials_total",
        "corelink.quota.cas_race_detected_total",
        "corelink.quota.cas_retry_after_secs",
        "corelink.quota.cas_check_duration_us",
    ]
}

/// Canonical decision result label for the `cas_check_total` counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuotaCasResultLabel {
    /// CAS predicate held; bytes_used updated.
    Allow,
    /// CAS predicate fired the deny arm (100% hard-block).
    Deny,
    /// CAS race detected at write; orchestrator retried (or exhausted).
    Race,
}

impl QuotaCasResultLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Race => "race",
        }
    }
}

/// Canonical metric kinds for the in-memory test sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuotaCasMetricKind {
    /// `cas_check_total` counter.
    CheckTotal,
    /// `cas_denials_total` counter.
    DenialsTotal,
    /// `cas_race_detected_total` counter.
    RaceDetectedTotal,
}

/// Errors surfaced by [`QuotaCasMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaCasMetricsObserverError {
    /// Backend transport failure.
    #[error("quota CAS metrics observer error: {0}")]
    Backend(String),
}

/// Trait surface for the metrics observer. Production wiring composes
/// a Prometheus / OTLP shim; tests use [`InMemoryQuotaCasMetrics`] for
/// deterministic assertions.
pub trait QuotaCasMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Record a `cas_check_total{result=…}` increment.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_check(
        &self,
        label: QuotaCasResultLabel,
    ) -> Result<(), QuotaCasMetricsObserverError>;

    /// Record a `cas_denials_total{tenant_id}` increment (canonical
    /// 100% hard-block fired).
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_denial(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), QuotaCasMetricsObserverError>;

    /// Record a `cas_race_detected_total{tenant_id}` increment.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_race_detected(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), QuotaCasMetricsObserverError>;

    /// Record a `cas_retry_after_secs` histogram observation.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_retry_after_secs(
        &self,
        retry_after_secs: u64,
    ) -> Result<(), QuotaCasMetricsObserverError>;

    /// Record a `cas_check_duration_us` histogram observation.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_check_duration_us(
        &self,
        label: QuotaCasResultLabel,
        duration_us: u64,
    ) -> Result<(), QuotaCasMetricsObserverError>;
}

/// In-memory test metrics observer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryQuotaCasMetrics {
    counters: std::sync::Arc<Mutex<HashMap<(QuotaCasMetricKind, Uuid), u64>>>,
    check_by_label:
        std::sync::Arc<Mutex<HashMap<QuotaCasResultLabel, u64>>>,
    retry_after_observations: std::sync::Arc<Mutex<Vec<u64>>>,
    duration_observations:
        std::sync::Arc<Mutex<Vec<(QuotaCasResultLabel, u64)>>>,
}

impl InMemoryQuotaCasMetrics {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total counter (across tenants if applicable).
    #[must_use]
    pub fn counter_total(&self, kind: QuotaCasMetricKind) -> u64 {
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
        kind: QuotaCasMetricKind,
        tenant_id: Uuid,
    ) -> u64 {
        let g = match self.counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(kind, tenant_id)).copied().unwrap_or(0)
    }

    /// Check-total counter for a specific result label.
    #[must_use]
    pub fn check_total_for_label(&self, label: QuotaCasResultLabel) -> u64 {
        let g = match self.check_by_label.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&label).copied().unwrap_or(0)
    }

    /// Snapshot of all retry-after observations.
    #[must_use]
    pub fn retry_after_snapshot(&self) -> Vec<u64> {
        let g = match self.retry_after_observations.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }

    /// Snapshot of all duration observations.
    #[must_use]
    pub fn duration_snapshot(&self) -> Vec<(QuotaCasResultLabel, u64)> {
        let g = match self.duration_observations.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }
}

impl QuotaCasMetricsObserver for InMemoryQuotaCasMetrics {
    fn record_check(
        &self,
        label: QuotaCasResultLabel,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        // Bump the per-label counter.
        let mut g = self.check_by_label.lock().map_err(|_| {
            QuotaCasMetricsObserverError::Backend(
                "metrics check_by_label mutex poisoned".to_string(),
            )
        })?;
        *g.entry(label).or_insert(0) += 1;
        drop(g);
        // Bump the aggregate CheckTotal counter (tenant nil — aggregate).
        let mut g = self.counters.lock().map_err(|_| {
            QuotaCasMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((QuotaCasMetricKind::CheckTotal, Uuid::nil())).or_insert(0) +=
            1;
        Ok(())
    }

    fn record_denial(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        let mut g = self.counters.lock().map_err(|_| {
            QuotaCasMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((QuotaCasMetricKind::DenialsTotal, tenant_id)).or_insert(0) +=
            1;
        Ok(())
    }

    fn record_race_detected(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        let mut g = self.counters.lock().map_err(|_| {
            QuotaCasMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((QuotaCasMetricKind::RaceDetectedTotal, tenant_id))
            .or_insert(0) += 1;
        Ok(())
    }

    fn record_retry_after_secs(
        &self,
        retry_after_secs: u64,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        let mut g = self.retry_after_observations.lock().map_err(|_| {
            QuotaCasMetricsObserverError::Backend(
                "metrics retry_after mutex poisoned".to_string(),
            )
        })?;
        g.push(retry_after_secs);
        Ok(())
    }

    fn record_check_duration_us(
        &self,
        label: QuotaCasResultLabel,
        duration_us: u64,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        let mut g = self.duration_observations.lock().map_err(|_| {
            QuotaCasMetricsObserverError::Backend(
                "metrics duration mutex poisoned".to_string(),
            )
        })?;
        g.push((label, duration_us));
        Ok(())
    }
}

/// Always-failing observer for adversarial tests of fail-soft metric
/// emission (production wiring may downgrade metric failures to
/// log-and-continue per WI §14.s08.003.10; the orchestrator
/// fail-closes ONLY on audit emit failure, not metric).
#[derive(Debug, Default)]
pub struct FailingQuotaCasMetrics;

impl FailingQuotaCasMetrics {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl QuotaCasMetricsObserver for FailingQuotaCasMetrics {
    fn record_check(
        &self,
        _label: QuotaCasResultLabel,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        Err(QuotaCasMetricsObserverError::Backend(
            "induced quota CAS metric failure (test fixture)".to_string(),
        ))
    }
    fn record_denial(
        &self,
        _t: Uuid,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        Err(QuotaCasMetricsObserverError::Backend(
            "induced".to_string(),
        ))
    }
    fn record_race_detected(
        &self,
        _t: Uuid,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        Err(QuotaCasMetricsObserverError::Backend(
            "induced".to_string(),
        ))
    }
    fn record_retry_after_secs(
        &self,
        _r: u64,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        Err(QuotaCasMetricsObserverError::Backend(
            "induced".to_string(),
        ))
    }
    fn record_check_duration_us(
        &self,
        _l: QuotaCasResultLabel,
        _d: u64,
    ) -> Result<(), QuotaCasMetricsObserverError> {
        Err(QuotaCasMetricsObserverError::Backend(
            "induced".to_string(),
        ))
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

    #[test]
    fn canonical_metric_names_count() {
        let names = canonical_metric_names();
        assert_eq!(names.len(), 5);
        for n in names {
            assert!(n.starts_with("corelink.quota.cas_"));
        }
    }

    #[test]
    fn result_label_strings_distinct() {
        let v = [
            QuotaCasResultLabel::Allow,
            QuotaCasResultLabel::Deny,
            QuotaCasResultLabel::Race,
        ];
        let mut set = std::collections::HashSet::new();
        for l in v {
            assert!(set.insert(l.as_str()));
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn in_memory_check_increments_aggregate_and_per_label() {
        let m = InMemoryQuotaCasMetrics::new();
        m.record_check(QuotaCasResultLabel::Allow).unwrap();
        m.record_check(QuotaCasResultLabel::Deny).unwrap();
        m.record_check(QuotaCasResultLabel::Allow).unwrap();
        assert_eq!(m.counter_total(QuotaCasMetricKind::CheckTotal), 3);
        assert_eq!(m.check_total_for_label(QuotaCasResultLabel::Allow), 2);
        assert_eq!(m.check_total_for_label(QuotaCasResultLabel::Deny), 1);
        assert_eq!(m.check_total_for_label(QuotaCasResultLabel::Race), 0);
    }

    #[test]
    fn denial_counter_per_tenant() {
        let m = InMemoryQuotaCasMetrics::new();
        let t = Uuid::from_u128(0xa);
        m.record_denial(t).unwrap();
        m.record_denial(t).unwrap();
        assert_eq!(
            m.counter_for_tenant(QuotaCasMetricKind::DenialsTotal, t),
            2
        );
    }

    #[test]
    fn race_detected_counter_per_tenant() {
        let m = InMemoryQuotaCasMetrics::new();
        let t = Uuid::from_u128(0xb);
        m.record_race_detected(t).unwrap();
        assert_eq!(
            m.counter_for_tenant(QuotaCasMetricKind::RaceDetectedTotal, t),
            1
        );
    }

    #[test]
    fn retry_after_observations_capture() {
        let m = InMemoryQuotaCasMetrics::new();
        m.record_retry_after_secs(60).unwrap();
        m.record_retry_after_secs(86_400).unwrap();
        let obs = m.retry_after_snapshot();
        assert_eq!(obs, vec![60, 86_400]);
    }

    #[test]
    fn duration_observations_capture_label() {
        let m = InMemoryQuotaCasMetrics::new();
        m.record_check_duration_us(QuotaCasResultLabel::Allow, 1500)
            .unwrap();
        m.record_check_duration_us(QuotaCasResultLabel::Deny, 800)
            .unwrap();
        let obs = m.duration_snapshot();
        assert_eq!(obs.len(), 2);
        assert_eq!(obs[0], (QuotaCasResultLabel::Allow, 1500));
        assert_eq!(obs[1], (QuotaCasResultLabel::Deny, 800));
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingQuotaCasMetrics::new();
        let err = m.record_check(QuotaCasResultLabel::Allow).unwrap_err();
        assert!(matches!(err, QuotaCasMetricsObserverError::Backend(_)));
    }
}
