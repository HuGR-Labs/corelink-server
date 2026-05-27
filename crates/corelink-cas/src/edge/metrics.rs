//! Edge metrics observer trait + canonical metric name list +
//! InMemory capture sink.
//!
//! WI-S08-002 §6.1.10 freezes the canonical 5 metrics every emission
//! path uses (CloudEvent dotted naming spec → underscored Prometheus
//! exposed name per `prometheus.io/docs/practices/naming/`):
//!
//! - `corelink.edge.decision_total{result=allowed|denied_blocklist|denied_abuse}` (counter)
//! - `corelink.edge.cidr_blocklist_size{family=4|6}` (gauge; sampled
//!   from the in-memory mirror after every add/remove)
//! - `corelink.edge.cidr_blocklist_added_total{reason}` (counter)
//! - `corelink.edge.cidr_blocklist_removed_total` (counter)
//! - `corelink.edge.cf_api_error_total{operation=add|remove|reconcile}`
//!   (counter; alert SEV-2 if > 10/5min — production CF binding gate)

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

use crate::edge::cidr::CidrFamily;

/// Canonical metric names per WI §6.1.10 — pinned for cross-component
/// regression tests + dashboard widget configuration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 5] {
    &[
        "corelink.edge.decision_total",
        "corelink.edge.cidr_blocklist_size",
        "corelink.edge.cidr_blocklist_added_total",
        "corelink.edge.cidr_blocklist_removed_total",
        "corelink.edge.cf_api_error_total",
    ]
}

/// Errors surfaced by [`EdgeMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EdgeMetricsObserverError {
    /// Metric backend transport failure (e.g. counter sink down).
    #[error("edge metrics observer backend error: {0}")]
    Backend(String),
}

/// Coarse metric kind tag — drives dispatch in the in-memory sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EdgeMetricKind {
    /// `corelink.edge.decision_total`.
    DecisionTotal,
    /// `corelink.edge.cidr_blocklist_size` — gauge.
    CidrBlocklistSize,
    /// `corelink.edge.cidr_blocklist_added_total`.
    CidrBlocklistAddedTotal,
    /// `corelink.edge.cidr_blocklist_removed_total`.
    CidrBlocklistRemovedTotal,
    /// `corelink.edge.cf_api_error_total`.
    CfApiErrorTotal,
}

impl EdgeMetricKind {
    /// Canonical metric name (matches [`canonical_metric_names`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DecisionTotal => "corelink.edge.decision_total",
            Self::CidrBlocklistSize => "corelink.edge.cidr_blocklist_size",
            Self::CidrBlocklistAddedTotal => "corelink.edge.cidr_blocklist_added_total",
            Self::CidrBlocklistRemovedTotal => "corelink.edge.cidr_blocklist_removed_total",
            Self::CfApiErrorTotal => "corelink.edge.cf_api_error_total",
        }
    }
}

/// Result label for `decision_total`. Closed canonical 3-set per WI
/// §6.1.10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EdgeResultLabel {
    /// `result=allowed`.
    Allowed,
    /// `result=denied_blocklist`.
    DeniedBlocklist,
    /// `result=denied_abuse`.
    DeniedAbuse,
}

impl EdgeResultLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::DeniedBlocklist => "denied_blocklist",
            Self::DeniedAbuse => "denied_abuse",
        }
    }
}

/// Edge metrics observer trait every backend (statsd / prometheus /
/// CloudWatch / in-memory) implements.
pub trait EdgeMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment `corelink.edge.decision_total{result=...}` by 1.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeMetricsObserverError::Backend`] on backend failure.
    fn record_decision(
        &self,
        result: EdgeResultLabel,
    ) -> Result<(), EdgeMetricsObserverError>;

    /// Set the gauge `corelink.edge.cidr_blocklist_size{family=...}`
    /// to `count`.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeMetricsObserverError::Backend`] on backend failure.
    fn observe_blocklist_size(
        &self,
        family: CidrFamily,
        count: u64,
    ) -> Result<(), EdgeMetricsObserverError>;

    /// Increment `corelink.edge.cidr_blocklist_added_total{reason=...}`
    /// by 1.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeMetricsObserverError::Backend`] on backend failure.
    fn record_blocklist_add(
        &self,
        reason: &str,
    ) -> Result<(), EdgeMetricsObserverError>;

    /// Increment `corelink.edge.cidr_blocklist_removed_total` by 1.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeMetricsObserverError::Backend`] on backend failure.
    fn record_blocklist_remove(&self) -> Result<(), EdgeMetricsObserverError>;

    /// Increment `corelink.edge.cf_api_error_total{operation=...}` by 1.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeMetricsObserverError::Backend`] on backend failure.
    fn record_cf_api_error(
        &self,
        operation: &str,
    ) -> Result<(), EdgeMetricsObserverError>;
}

/// In-memory metrics observer. Captures every recorded metric for
/// property test assertion. F-001 closure preserved (per-instance
/// `Mutex`; no process-global state).
#[derive(Debug, Default)]
pub struct InMemoryEdgeMetrics {
    inner: Mutex<MetricsState>,
}

#[derive(Debug, Default)]
struct MetricsState {
    counters: HashMap<String, u64>,
    gauges: HashMap<String, u64>,
}

impl InMemoryEdgeMetrics {
    /// Construct a fresh observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counter snapshot for a fully-qualified label string.
    #[must_use]
    pub fn counter(&self, label: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.counters.get(label).copied().unwrap_or_default(),
            Err(p) => p
                .into_inner()
                .counters
                .get(label)
                .copied()
                .unwrap_or_default(),
        }
    }

    /// Per-metric aggregate counter snapshot summing every label
    /// fragment under the canonical metric name.
    #[must_use]
    pub fn counter_total(&self, kind: EdgeMetricKind) -> u64 {
        let prefix = kind.as_str();
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .counters
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(_, v)| *v)
            .sum()
    }

    /// Gauge snapshot.
    #[must_use]
    pub fn gauge(&self, label: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.gauges.get(label).copied().unwrap_or_default(),
            Err(p) => p
                .into_inner()
                .gauges
                .get(label)
                .copied()
                .unwrap_or_default(),
        }
    }

    fn bump_counter(
        &self,
        label: String,
        by: u64,
    ) -> Result<(), EdgeMetricsObserverError> {
        let mut guard = self.inner.lock().map_err(|_| {
            EdgeMetricsObserverError::Backend(
                "metrics observer mutex poisoned".to_string(),
            )
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(by);
        Ok(())
    }
}

impl EdgeMetricsObserver for InMemoryEdgeMetrics {
    fn record_decision(
        &self,
        result: EdgeResultLabel,
    ) -> Result<(), EdgeMetricsObserverError> {
        let label = format!(
            "{}{{result={}}}",
            EdgeMetricKind::DecisionTotal.as_str(),
            result.as_str()
        );
        self.bump_counter(label, 1)
    }

    fn observe_blocklist_size(
        &self,
        family: CidrFamily,
        count: u64,
    ) -> Result<(), EdgeMetricsObserverError> {
        let label = format!(
            "{}{{family={}}}",
            EdgeMetricKind::CidrBlocklistSize.as_str(),
            family.as_int()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            EdgeMetricsObserverError::Backend(
                "metrics observer mutex poisoned".to_string(),
            )
        })?;
        guard.gauges.insert(label, count);
        Ok(())
    }

    fn record_blocklist_add(
        &self,
        reason: &str,
    ) -> Result<(), EdgeMetricsObserverError> {
        let label = format!(
            "{}{{reason={reason}}}",
            EdgeMetricKind::CidrBlocklistAddedTotal.as_str(),
        );
        self.bump_counter(label, 1)
    }

    fn record_blocklist_remove(&self) -> Result<(), EdgeMetricsObserverError> {
        let label = EdgeMetricKind::CidrBlocklistRemovedTotal.as_str().to_string();
        self.bump_counter(label, 1)
    }

    fn record_cf_api_error(
        &self,
        operation: &str,
    ) -> Result<(), EdgeMetricsObserverError> {
        let label = format!(
            "{}{{operation={operation}}}",
            EdgeMetricKind::CfApiErrorTotal.as_str(),
        );
        self.bump_counter(label, 1)
    }
}

/// Always-failing observer for adversarial tests of the
/// log-and-continue downgrade path.
#[derive(Debug, Default)]
pub struct FailingEdgeMetrics;

impl FailingEdgeMetrics {
    /// Construct a fresh always-failing observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl EdgeMetricsObserver for FailingEdgeMetrics {
    fn record_decision(
        &self,
        _result: EdgeResultLabel,
    ) -> Result<(), EdgeMetricsObserverError> {
        Err(EdgeMetricsObserverError::Backend(
            "induced edge metrics failure (test fixture)".to_string(),
        ))
    }

    fn observe_blocklist_size(
        &self,
        _family: CidrFamily,
        _count: u64,
    ) -> Result<(), EdgeMetricsObserverError> {
        Err(EdgeMetricsObserverError::Backend(
            "induced edge metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_blocklist_add(
        &self,
        _reason: &str,
    ) -> Result<(), EdgeMetricsObserverError> {
        Err(EdgeMetricsObserverError::Backend(
            "induced edge metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_blocklist_remove(&self) -> Result<(), EdgeMetricsObserverError> {
        Err(EdgeMetricsObserverError::Backend(
            "induced edge metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_cf_api_error(
        &self,
        _operation: &str,
    ) -> Result<(), EdgeMetricsObserverError> {
        Err(EdgeMetricsObserverError::Backend(
            "induced edge metrics failure (test fixture)".to_string(),
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
    fn canonical_metric_names_align_with_enum() {
        let names = canonical_metric_names();
        assert_eq!(names.len(), 5);
        assert!(names.contains(&EdgeMetricKind::DecisionTotal.as_str()));
        assert!(names.contains(&EdgeMetricKind::CidrBlocklistSize.as_str()));
        assert!(names.contains(&EdgeMetricKind::CidrBlocklistAddedTotal.as_str()));
        assert!(names.contains(&EdgeMetricKind::CidrBlocklistRemovedTotal.as_str()));
        assert!(names.contains(&EdgeMetricKind::CfApiErrorTotal.as_str()));
    }

    #[test]
    fn metric_kinds_unique_canonical_strings() {
        let v = [
            EdgeMetricKind::DecisionTotal,
            EdgeMetricKind::CidrBlocklistSize,
            EdgeMetricKind::CidrBlocklistAddedTotal,
            EdgeMetricKind::CidrBlocklistRemovedTotal,
            EdgeMetricKind::CfApiErrorTotal,
        ];
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(k.as_str().starts_with("corelink.edge."));
            assert!(set.insert(k.as_str()), "duplicate metric: {}", k.as_str());
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn decision_total_increments_per_label() {
        let m = InMemoryEdgeMetrics::new();
        m.record_decision(EdgeResultLabel::Allowed).unwrap();
        m.record_decision(EdgeResultLabel::Allowed).unwrap();
        m.record_decision(EdgeResultLabel::DeniedBlocklist).unwrap();
        m.record_decision(EdgeResultLabel::DeniedAbuse).unwrap();
        assert_eq!(m.counter_total(EdgeMetricKind::DecisionTotal), 4);
    }

    #[test]
    fn blocklist_size_overwrites_gauge() {
        let m = InMemoryEdgeMetrics::new();
        m.observe_blocklist_size(CidrFamily::V4, 10).unwrap();
        let label = format!(
            "{}{{family=4}}",
            EdgeMetricKind::CidrBlocklistSize.as_str()
        );
        assert_eq!(m.gauge(&label), 10);
        m.observe_blocklist_size(CidrFamily::V4, 20).unwrap();
        assert_eq!(m.gauge(&label), 20);
    }

    #[test]
    fn add_total_increments_per_reason() {
        let m = InMemoryEdgeMetrics::new();
        m.record_blocklist_add("ManualAdmin").unwrap();
        m.record_blocklist_add("Sustained4xx").unwrap();
        assert_eq!(m.counter_total(EdgeMetricKind::CidrBlocklistAddedTotal), 2);
    }

    #[test]
    fn remove_total_increments() {
        let m = InMemoryEdgeMetrics::new();
        m.record_blocklist_remove().unwrap();
        m.record_blocklist_remove().unwrap();
        assert_eq!(
            m.counter_total(EdgeMetricKind::CidrBlocklistRemovedTotal),
            2
        );
    }

    #[test]
    fn cf_api_error_increments_per_operation() {
        let m = InMemoryEdgeMetrics::new();
        m.record_cf_api_error("add").unwrap();
        m.record_cf_api_error("add").unwrap();
        m.record_cf_api_error("reconcile").unwrap();
        assert_eq!(m.counter_total(EdgeMetricKind::CfApiErrorTotal), 3);
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingEdgeMetrics::new();
        assert!(matches!(
            m.record_decision(EdgeResultLabel::Allowed).unwrap_err(),
            EdgeMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.observe_blocklist_size(CidrFamily::V4, 0).unwrap_err(),
            EdgeMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_blocklist_add("ManualAdmin").unwrap_err(),
            EdgeMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_blocklist_remove().unwrap_err(),
            EdgeMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_cf_api_error("add").unwrap_err(),
            EdgeMetricsObserverError::Backend(_)
        ));
    }

    #[test]
    fn result_label_canonical_strings() {
        assert_eq!(EdgeResultLabel::Allowed.as_str(), "allowed");
        assert_eq!(
            EdgeResultLabel::DeniedBlocklist.as_str(),
            "denied_blocklist"
        );
        assert_eq!(EdgeResultLabel::DeniedAbuse.as_str(), "denied_abuse");
    }
}
