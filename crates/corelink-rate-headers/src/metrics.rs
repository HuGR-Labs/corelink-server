//! Circuit-breaker + RFC 9331 metrics emission trait + InMemory test
//! sink.
//!
//! ## Canonical 9-metric ladder (mirror of WI-S08-005 §6.1.13)
//!
//! - `corelink.rate_limited_within_quota_total{type, region}` — counter;
//!   SLI failure denominator source (within-plan 429 + global circuit
//!   open). The SLI distinction critical to sprint contract §7.10.s08.1
//!   error-budget correctness.
//! - `corelink.rate_limited_over_quota_total{type, region}` — counter;
//!   legitimate over-plan path (per_ip / per_pat / over_quota); NOT in
//!   the SLI denominator.
//! - `corelink.global_circuit.state{region}` — gauge (0 = closed,
//!   1 = half_open, 2 = open).
//! - `corelink.global_circuit.trips_total{region, reason}` — counter;
//!   **alert SEV-1 if > 0** (CAP-RATE-004 oncall pager wake-up).
//! - `corelink.global_circuit.recoveries_total{region}` — counter;
//!   informational (HalfOpen → Closed transition).
//! - `corelink.global_circuit.single_signal_alarm_total{region, signal}`
//!   — counter; SEV-3 alert source (one signal breached but multi-signal
//!   not met; investigation trigger).
//! - `corelink.global_circuit.half_open_duration_ms{region}` —
//!   histogram (Open → HalfOpen → Closed dwell).
//! - `corelink.global_circuit.manual_override_total{region, target_state}`
//!   — counter; **alert SEV-2 if > 0** (LGPD audit trail).
//! - `corelink.rate_limit_headers.rfc9331_compliance_total{type, version}`
//!   — counter; informational (header-emit success counter).

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

/// Canonical metric name list (mirrors WI-S08-005 §6.1.13).
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 9] {
    &[
        "corelink.rate_limited_within_quota_total",
        "corelink.rate_limited_over_quota_total",
        "corelink.global_circuit.state",
        "corelink.global_circuit.trips_total",
        "corelink.global_circuit.recoveries_total",
        "corelink.global_circuit.single_signal_alarm_total",
        "corelink.global_circuit.half_open_duration_ms",
        "corelink.global_circuit.manual_override_total",
        "corelink.rate_limit_headers.rfc9331_compliance_total",
    ]
}

/// Canonical metric kind discriminator for the in-memory test sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CircuitMetricKind {
    /// `rate_limited_within_quota_total` — SLI failure denominator.
    WithinQuotaTotal,
    /// `rate_limited_over_quota_total` — legitimate; NOT in SLI.
    OverQuotaTotal,
    /// `global_circuit.trips_total` — SEV-1 oncall pager.
    TripsTotal,
    /// `global_circuit.recoveries_total` — informational.
    RecoveriesTotal,
    /// `global_circuit.single_signal_alarm_total` — SEV-3.
    SingleSignalAlarmTotal,
    /// `global_circuit.manual_override_total` — SEV-2 LGPD.
    ManualOverrideTotal,
    /// `rate_limit_headers.rfc9331_compliance_total` — informational.
    Rfc9331ComplianceTotal,
}

/// Errors surfaced by [`CircuitMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CircuitMetricsObserverError {
    /// Backend transport failure.
    #[error("circuit metrics observer error: {0}")]
    Backend(String),
}

/// Trait surface for the metrics observer. Production wiring composes
/// a Prometheus / OTLP shim; tests use [`InMemoryCircuitMetrics`] for
/// deterministic assertions.
pub trait CircuitMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Record a `rate_limited_within_quota_total{type=…, region=…}`
    /// increment. The `kind_label` is the canonical `XRateLimitTypeKind`
    /// string (`tenant_quota` / `global_circuit_open`); the `region`
    /// label disambiguates per-region trip events.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_within_quota(
        &self,
        kind_label: &'static str,
        region: &str,
    ) -> Result<(), CircuitMetricsObserverError>;

    /// Record a `rate_limited_over_quota_total{type=…, region=…}`
    /// increment.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_over_quota(
        &self,
        kind_label: &'static str,
        region: &str,
    ) -> Result<(), CircuitMetricsObserverError>;

    /// Record a `global_circuit.trips_total{region, reason}` increment.
    /// SEV-1 alert source.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_trip(
        &self,
        region: &str,
        reason_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError>;

    /// Record a `global_circuit.recoveries_total{region}` increment.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_recovery(
        &self,
        region: &str,
    ) -> Result<(), CircuitMetricsObserverError>;

    /// Record a `global_circuit.single_signal_alarm_total{region,
    /// signal}` increment. SEV-3 alert source.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_single_signal_alarm(
        &self,
        region: &str,
        signal_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError>;

    /// Record a `global_circuit.manual_override_total{region,
    /// target_state}` increment. SEV-2 LGPD audit trail.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_manual_override(
        &self,
        region: &str,
        target_state_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError>;

    /// Record a `rate_limit_headers.rfc9331_compliance_total{type,
    /// version}` increment.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn record_rfc9331_compliance(
        &self,
        kind_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError>;
}

type CounterKey = (CircuitMetricKind, String, String);
type CounterMap = HashMap<CounterKey, u64>;

/// In-memory test metrics observer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryCircuitMetrics {
    counters: std::sync::Arc<Mutex<CounterMap>>,
}

impl InMemoryCircuitMetrics {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total counter (across labels) for a kind.
    #[must_use]
    pub fn counter_total(&self, kind: CircuitMetricKind) -> u64 {
        let g = match self.counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.iter()
            .filter(|((k, _, _), _)| *k == kind)
            .map(|(_, v)| v)
            .sum()
    }

    /// Counter for a (kind, label_a, label_b) tuple.
    #[must_use]
    pub fn counter_for_labels(
        &self,
        kind: CircuitMetricKind,
        label_a: &str,
        label_b: &str,
    ) -> u64 {
        let g = match self.counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(kind, label_a.to_string(), label_b.to_string()))
            .copied()
            .unwrap_or(0)
    }

    fn bump(
        &self,
        kind: CircuitMetricKind,
        label_a: String,
        label_b: String,
    ) -> Result<(), CircuitMetricsObserverError> {
        let mut g = self.counters.lock().map_err(|_| {
            CircuitMetricsObserverError::Backend(
                "metrics counter mutex poisoned".to_string(),
            )
        })?;
        *g.entry((kind, label_a, label_b)).or_insert(0) += 1;
        Ok(())
    }
}

impl CircuitMetricsObserver for InMemoryCircuitMetrics {
    fn record_within_quota(
        &self,
        kind_label: &'static str,
        region: &str,
    ) -> Result<(), CircuitMetricsObserverError> {
        self.bump(
            CircuitMetricKind::WithinQuotaTotal,
            kind_label.to_string(),
            region.to_string(),
        )
    }

    fn record_over_quota(
        &self,
        kind_label: &'static str,
        region: &str,
    ) -> Result<(), CircuitMetricsObserverError> {
        self.bump(
            CircuitMetricKind::OverQuotaTotal,
            kind_label.to_string(),
            region.to_string(),
        )
    }

    fn record_trip(
        &self,
        region: &str,
        reason_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        self.bump(
            CircuitMetricKind::TripsTotal,
            region.to_string(),
            reason_label.to_string(),
        )
    }

    fn record_recovery(
        &self,
        region: &str,
    ) -> Result<(), CircuitMetricsObserverError> {
        self.bump(
            CircuitMetricKind::RecoveriesTotal,
            region.to_string(),
            String::new(),
        )
    }

    fn record_single_signal_alarm(
        &self,
        region: &str,
        signal_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        self.bump(
            CircuitMetricKind::SingleSignalAlarmTotal,
            region.to_string(),
            signal_label.to_string(),
        )
    }

    fn record_manual_override(
        &self,
        region: &str,
        target_state_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        self.bump(
            CircuitMetricKind::ManualOverrideTotal,
            region.to_string(),
            target_state_label.to_string(),
        )
    }

    fn record_rfc9331_compliance(
        &self,
        kind_label: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        self.bump(
            CircuitMetricKind::Rfc9331ComplianceTotal,
            kind_label.to_string(),
            String::new(),
        )
    }
}

/// Always-failing observer for adversarial tests of fail-soft metric
/// emission (production wiring may downgrade metric failures to
/// log-and-continue; the orchestrator fail-closes ONLY on audit emit
/// failure, not metric).
#[derive(Debug, Default)]
pub struct FailingCircuitMetrics;

impl FailingCircuitMetrics {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CircuitMetricsObserver for FailingCircuitMetrics {
    fn record_within_quota(
        &self,
        _k: &'static str,
        _r: &str,
    ) -> Result<(), CircuitMetricsObserverError> {
        Err(CircuitMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_over_quota(
        &self,
        _k: &'static str,
        _r: &str,
    ) -> Result<(), CircuitMetricsObserverError> {
        Err(CircuitMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_trip(
        &self,
        _r: &str,
        _x: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        Err(CircuitMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_recovery(
        &self,
        _r: &str,
    ) -> Result<(), CircuitMetricsObserverError> {
        Err(CircuitMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_single_signal_alarm(
        &self,
        _r: &str,
        _s: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        Err(CircuitMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_manual_override(
        &self,
        _r: &str,
        _t: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        Err(CircuitMetricsObserverError::Backend("induced".to_string()))
    }
    fn record_rfc9331_compliance(
        &self,
        _k: &'static str,
    ) -> Result<(), CircuitMetricsObserverError> {
        Err(CircuitMetricsObserverError::Backend("induced".to_string()))
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
        assert_eq!(names.len(), 9);
        for n in names {
            assert!(
                n.starts_with("corelink.rate_limited_")
                    || n.starts_with("corelink.global_circuit.")
                    || n.starts_with("corelink.rate_limit_headers.")
            );
        }
    }

    #[test]
    fn within_quota_counter_per_region_and_type() {
        let m = InMemoryCircuitMetrics::new();
        m.record_within_quota("tenant_quota", "iad").unwrap();
        m.record_within_quota("tenant_quota", "iad").unwrap();
        m.record_within_quota("global_circuit_open", "iad").unwrap();
        assert_eq!(
            m.counter_for_labels(
                CircuitMetricKind::WithinQuotaTotal,
                "tenant_quota",
                "iad"
            ),
            2
        );
        assert_eq!(
            m.counter_for_labels(
                CircuitMetricKind::WithinQuotaTotal,
                "global_circuit_open",
                "iad"
            ),
            1
        );
        assert_eq!(
            m.counter_total(CircuitMetricKind::WithinQuotaTotal),
            3
        );
    }

    #[test]
    fn over_quota_counter_per_region_and_type() {
        let m = InMemoryCircuitMetrics::new();
        m.record_over_quota("per_ip", "iad").unwrap();
        m.record_over_quota("per_pat", "iad").unwrap();
        m.record_over_quota("over_quota", "sam").unwrap();
        assert_eq!(
            m.counter_total(CircuitMetricKind::OverQuotaTotal),
            3
        );
    }

    #[test]
    fn trip_counter_per_region_and_reason() {
        let m = InMemoryCircuitMetrics::new();
        m.record_trip("iad", "MultiSignalCombined").unwrap();
        m.record_trip("iad", "MultiSignalCombined").unwrap();
        m.record_trip("sam", "ManualOverride").unwrap();
        assert_eq!(
            m.counter_for_labels(
                CircuitMetricKind::TripsTotal,
                "iad",
                "MultiSignalCombined"
            ),
            2
        );
        assert_eq!(
            m.counter_for_labels(
                CircuitMetricKind::TripsTotal,
                "sam",
                "ManualOverride"
            ),
            1
        );
    }

    #[test]
    fn recovery_counter_per_region() {
        let m = InMemoryCircuitMetrics::new();
        m.record_recovery("iad").unwrap();
        m.record_recovery("iad").unwrap();
        assert_eq!(
            m.counter_total(CircuitMetricKind::RecoveriesTotal),
            2
        );
    }

    #[test]
    fn single_signal_alarm_per_region_and_signal() {
        let m = InMemoryCircuitMetrics::new();
        m.record_single_signal_alarm("iad", "error_5xx").unwrap();
        m.record_single_signal_alarm("iad", "p99_latency").unwrap();
        assert_eq!(
            m.counter_total(CircuitMetricKind::SingleSignalAlarmTotal),
            2
        );
    }

    #[test]
    fn manual_override_per_region_and_target() {
        let m = InMemoryCircuitMetrics::new();
        m.record_manual_override("iad", "open").unwrap();
        m.record_manual_override("iad", "closed").unwrap();
        assert_eq!(
            m.counter_total(CircuitMetricKind::ManualOverrideTotal),
            2
        );
    }

    #[test]
    fn rfc9331_compliance_per_kind() {
        let m = InMemoryCircuitMetrics::new();
        m.record_rfc9331_compliance("tenant_quota").unwrap();
        m.record_rfc9331_compliance("over_quota").unwrap();
        assert_eq!(
            m.counter_total(CircuitMetricKind::Rfc9331ComplianceTotal),
            2
        );
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingCircuitMetrics::new();
        let err = m.record_trip("iad", "x").unwrap_err();
        assert!(matches!(err, CircuitMetricsObserverError::Backend(_)));
    }
}
