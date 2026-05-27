//! Canonical [`MetricPoint`] envelope + [`TraceSpan`] alias.
//!
//! Per `INV-OBS-NO-PII`, every metric forwarded to a third-party
//! observability vendor is keyed by a canonical
//! [`corelink_analytics::RedMetricKind`] (15 enum variants, snake_case
//! `corelink_*` slugs). The [`MetricPoint::labels`] field carries the canonical
//! pseudonymous-only label set documented in
//! `specs/03_architecture/observability_model.md §10` — the free-form
//! customer-side `tenant_id` is pseudonymized at the boundary BEFORE
//! export.

use std::collections::BTreeMap;

use corelink_analytics::RedMetricKind;

/// Canonical trace span envelope (aliased to
/// [`corelink_tracing::SpanRecord`] for forward compatibility).
pub type TraceSpan = corelink_tracing::SpanRecord;

/// Canonical metric-point envelope.
///
/// Forwarded to a third-party vendor (Datadog v2 series; OTLP /v1/metrics
/// protobuf; Prom remote-write protobuf + Snappy). The envelope is
/// vendor-agnostic; per-vendor wire encoding is the adapter's job.
///
/// **Wire encoding note.** This envelope intentionally does NOT derive
/// `serde::{Serialize, Deserialize}` because [`RedMetricKind`] is
/// `corelink-analytics`-internal and not part of the serde surface. The
/// per-vendor adapter is responsible for encoding to Datadog v2 series
/// JSON / OTLP protobuf / Prom remote-write protobuf using
/// [`MetricPoint::metric_name`] as the canonical name slug.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MetricPoint {
    /// Canonical metric kind (enforces the closed 15-variant taxonomy
    /// per `corelink-analytics::canonical`).
    pub kind: RedMetricKind,
    /// Pseudonymous-only canonical labels (sorted by key for stable
    /// serialization; `tenant_tier`, `region`, `result`, `op_type`,
    /// `bucket`, `reason`, etc.). Keys + values MUST be PII-free per
    /// `INV-OBS-NO-PII`.
    pub labels: BTreeMap<String, String>,
    /// Observation value.
    pub value: MetricValue,
    /// Unix-epoch nanoseconds at which the observation was taken.
    pub timestamp_ns: u128,
}

impl MetricPoint {
    /// Construct a new metric point.
    #[must_use]
    pub fn new(
        kind: RedMetricKind,
        labels: BTreeMap<String, String>,
        value: MetricValue,
        timestamp_ns: u128,
    ) -> Self {
        Self {
            kind,
            labels,
            value,
            timestamp_ns,
        }
    }

    /// Canonical Prom-compatible metric-name slug.
    #[must_use]
    pub fn metric_name(&self) -> &'static str {
        self.kind.as_str()
    }
}

/// Canonical metric value variant (counter / gauge / histogram bucket).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MetricValue {
    /// Monotonic counter sample (`requests_total`, `bytes_total`, ...).
    Counter(u64),
    /// Gauge sample (`dedup_ratio`, `quota_used`, ...).
    Gauge(f64),
    /// Histogram observation (single sample value; bucketization is
    /// adapter-side).
    Histogram(f64),
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
    fn metric_point_carries_canonical_kind() {
        let p = MetricPoint::new(
            RedMetricKind::CasPutRequestsTotal,
            BTreeMap::from([
                ("tenant_tier".to_string(), "free".to_string()),
                ("region".to_string(), "us-east-1".to_string()),
                ("result".to_string(), "ok".to_string()),
            ]),
            MetricValue::Counter(1),
            1_715_000_000_000_000_000,
        );
        assert_eq!(p.metric_name(), "corelink_cas_put_requests_total");
        assert_eq!(p.labels.len(), 3);
    }

    #[test]
    fn metric_value_gauge_round_trip() {
        let v = MetricValue::Gauge(0.42);
        match v {
            MetricValue::Gauge(g) => assert!((g - 0.42).abs() < f64::EPSILON),
            _ => panic!("expected gauge"),
        }
    }
}
