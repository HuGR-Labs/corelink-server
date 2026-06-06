//! `MetricsExporter` trait + canonical [`ExporterVariant`] enum +
//! [`InMemoryFake`] (deterministic test fake) + one deferred-real stub
//! per vendor (`DatadogExporter` / `OtelCollectorExporter` /
//! `GrafanaCloudExporter`).
//!
//! Per the `trait-abstraction-defer` charter pattern, each vendor stub
//! returns `Ok(())` after a `tracing::debug!` so the end-to-end behavior
//! of the orchestrator (fail-OPEN audit envelope discipline, dispatch
//! by variant) is testable now, and the real HTTP/gRPC client lands in
//! a follow-up WI behind its own chaos test (induced 5xx, induced
//! network partition, induced auth reject).

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::otel::audit::{
    AuditEmitError, ExportFailedAuditSink, ExportFailedEvent, ExportFailedPath,
};
use crate::otel::config::{DatadogConfig, GrafanaCloudConfig, OtelCollectorConfig};
use crate::otel::error::ExporterError;
use crate::otel::metric::{MetricPoint, TraceSpan};

/// Canonical exporter variant enum (`#[non_exhaustive]`).
///
/// Adding a new vendor (Honeycomb, New Relic, Splunk Observability, AWS
/// CloudWatch) lands here additively without a breaking change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExporterVariant {
    /// Datadog HTTP API exporter (v2 series + APM intake).
    Datadog,
    /// OpenTelemetry Collector exporter (OTLP gRPC/HTTP).
    OtelCollector,
    /// Grafana Cloud exporter (Prometheus remote-write v1 + Tempo OTLP).
    GrafanaCloud,
    /// No-op exporter (customer opted out of forwarding entirely).
    Disabled,
}

impl ExporterVariant {
    /// Canonical slug for the audit envelope.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Datadog => "datadog",
            Self::OtelCollector => "otel_collector",
            Self::GrafanaCloud => "grafana_cloud",
            Self::Disabled => "disabled",
        }
    }
}

/// Canonical exporter trait every vendor adapter satisfies.
///
/// **Fail-OPEN contract.** Both methods return `Result<(), ExporterError>`,
/// but the orchestrator boundary
/// (see [`MetricsExporter::export_batch_fail_open`] +
/// [`MetricsExporter::export_trace_fail_open`]) catches the error, emits
/// the canonical `corelink.observability.export_failed` audit event, and
/// returns `Ok(())` to the caller. The raw error path exists so
/// adversarial tests can pin transport failure semantics.
pub trait MetricsExporter: Send + Sync + core::fmt::Debug {
    /// Variant slug (audit envelope + label cardinality).
    fn variant(&self) -> ExporterVariant;

    /// Forward a batch of canonical metric points.
    ///
    /// # Errors
    ///
    /// Vendor transport / auth / rate-limit / payload errors (per
    /// [`ExporterError`]). Callers MUST go through
    /// [`MetricsExporter::export_batch_fail_open`] in the request
    /// path.
    fn export_batch(&self, metrics: &[MetricPoint]) -> Result<(), ExporterError>;

    /// Forward a single trace span.
    ///
    /// # Errors
    ///
    /// Vendor transport / auth / rate-limit / payload errors (per
    /// [`ExporterError`]). Callers MUST go through
    /// [`MetricsExporter::export_trace_fail_open`] in the request
    /// path.
    fn export_trace(&self, span: &TraceSpan) -> Result<(), ExporterError>;

    /// Fail-OPEN orchestration boundary for [`Self::export_batch`].
    ///
    /// Emits a canonical `corelink.observability.export_failed` audit
    /// event into `audit_sink` on any [`ExporterError`] and returns
    /// `Ok(())`. The only `Err` propagated is an
    /// [`AuditEmitError`] — i.e., the audit chain itself failed (a
    /// CRITICAL P0 condition; the orchestrator must NOT silently lose
    /// audit evidence).
    ///
    /// # Errors
    ///
    /// Returns [`AuditEmitError`] only if the audit sink itself failed
    /// (internal mutex / IO). All vendor failures are absorbed.
    fn export_batch_fail_open(
        &self,
        metrics: &[MetricPoint],
        audit_sink: &dyn ExportFailedAuditSink,
        now_unix_ms: u64,
    ) -> Result<(), AuditEmitError> {
        if let Err(e) = self.export_batch(metrics) {
            // NOTE: per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER, audit emit
            // happens BEFORE the orchestrator decides to fail-OPEN.
            // The audit sink's own error is itself fatal — we do NOT
            // swallow it.
            let len = u32::try_from(metrics.len()).unwrap_or(u32::MAX);
            let ev = ExportFailedEvent::from_error(
                self.variant(),
                ExportFailedPath::Metric,
                &e,
                len,
                now_unix_ms,
            );
            audit_sink.record(&ev)?;
            // Customer's vendor down ≠ our system down. Return Ok(()).
            tracing::debug!(
                target: "corelink_otel_export",
                variant = self.variant().as_str(),
                error = %e,
                "metric export failed; fail-OPEN with audit emit"
            );
        }
        Ok(())
    }

    /// Fail-OPEN orchestration boundary for [`Self::export_trace`].
    ///
    /// Mirror of [`Self::export_batch_fail_open`] for the trace path.
    ///
    /// # Errors
    ///
    /// Returns [`AuditEmitError`] only if the audit sink itself failed.
    fn export_trace_fail_open(
        &self,
        span: &TraceSpan,
        audit_sink: &dyn ExportFailedAuditSink,
        now_unix_ms: u64,
    ) -> Result<(), AuditEmitError> {
        if let Err(e) = self.export_trace(span) {
            let ev = ExportFailedEvent::from_error(
                self.variant(),
                ExportFailedPath::Trace,
                &e,
                1,
                now_unix_ms,
            );
            audit_sink.record(&ev)?;
            tracing::debug!(
                target: "corelink_otel_export",
                variant = self.variant().as_str(),
                error = %e,
                "trace export failed; fail-OPEN with audit emit"
            );
        }
        Ok(())
    }
}

// ----------------------------------------------------------------------
// In-memory deterministic fake
// ----------------------------------------------------------------------

/// Internal buffer state for the in-memory fake.
#[derive(Debug, Default)]
struct InMemoryState {
    metrics: Vec<MetricPoint>,
    spans: Vec<TraceSpan>,
    /// Programmable failure mode for the next call.
    next_metric_failure: Option<ExporterError>,
    next_trace_failure: Option<ExporterError>,
}

/// Deterministic in-memory fake. Used by every unit + property test in
/// this crate AND by downstream crates that depend on the trait surface.
///
/// Cloning shares the underlying buffer (Arc<Mutex<...>>).
#[derive(Clone, Debug, Default)]
pub struct InMemoryFake {
    variant: ExporterVariantBacker,
    inner: Arc<Mutex<InMemoryState>>,
}

#[derive(Clone, Copy, Debug, Default)]
enum ExporterVariantBacker {
    #[default]
    Datadog,
    OtelCollector,
    GrafanaCloud,
    Disabled,
}

impl From<ExporterVariantBacker> for ExporterVariant {
    fn from(b: ExporterVariantBacker) -> Self {
        match b {
            ExporterVariantBacker::Datadog => Self::Datadog,
            ExporterVariantBacker::OtelCollector => Self::OtelCollector,
            ExporterVariantBacker::GrafanaCloud => Self::GrafanaCloud,
            ExporterVariantBacker::Disabled => Self::Disabled,
        }
    }
}

impl InMemoryFake {
    /// Construct a fresh fake masquerading as `variant`.
    #[must_use]
    pub fn new(variant: ExporterVariant) -> Self {
        // The match below is exhaustive against the currently-known
        // canonical variants. `ExporterVariant` is `#[non_exhaustive]`
        // so future variants would need a fall-through arm; today the
        // 4 known variants cover the surface.
        let v = match variant {
            ExporterVariant::Datadog => ExporterVariantBacker::Datadog,
            ExporterVariant::OtelCollector => ExporterVariantBacker::OtelCollector,
            ExporterVariant::GrafanaCloud => ExporterVariantBacker::GrafanaCloud,
            ExporterVariant::Disabled => ExporterVariantBacker::Disabled,
        };
        Self {
            variant: v,
            inner: Arc::new(Mutex::new(InMemoryState::default())),
        }
    }

    /// Snapshot every metric point recorded so far.
    #[must_use]
    pub fn snapshot_metrics(&self) -> Vec<MetricPoint> {
        match self.inner.lock() {
            Ok(g) => g.metrics.clone(),
            Err(p) => p.into_inner().metrics.clone(),
        }
    }

    /// Snapshot every trace span recorded so far.
    #[must_use]
    pub fn snapshot_spans(&self) -> Vec<TraceSpan> {
        match self.inner.lock() {
            Ok(g) => g.spans.clone(),
            Err(p) => p.into_inner().spans.clone(),
        }
    }

    /// Program the next [`MetricsExporter::export_batch`] call to fail
    /// with `err`.
    pub fn inject_metric_failure(&self, err: ExporterError) {
        if let Ok(mut g) = self.inner.lock() {
            g.next_metric_failure = Some(err);
        }
    }

    /// Program the next [`MetricsExporter::export_trace`] call to fail
    /// with `err`.
    pub fn inject_trace_failure(&self, err: ExporterError) {
        if let Ok(mut g) = self.inner.lock() {
            g.next_trace_failure = Some(err);
        }
    }
}

impl MetricsExporter for InMemoryFake {
    fn variant(&self) -> ExporterVariant {
        self.variant.into()
    }

    fn export_batch(&self, metrics: &[MetricPoint]) -> Result<(), ExporterError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| ExporterError::Internal("fake mutex poisoned".into()))?;
        if let Some(err) = g.next_metric_failure.take() {
            return Err(err);
        }
        g.metrics.extend_from_slice(metrics);
        Ok(())
    }

    fn export_trace(&self, span: &TraceSpan) -> Result<(), ExporterError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| ExporterError::Internal("fake mutex poisoned".into()))?;
        if let Some(err) = g.next_trace_failure.take() {
            return Err(err);
        }
        g.spans.push(span.clone());
        Ok(())
    }
}

// ----------------------------------------------------------------------
// Deferred-real per-vendor stubs (trait-abstraction-defer charter pattern)
// ----------------------------------------------------------------------

/// Datadog HTTP API exporter (deferred-real stub).
///
/// Returns `Ok(())` after a `tracing::debug!` so end-to-end behavior
/// is testable. Real wiring lands in a follow-up WI behind its own
/// chaos test (induced 5xx from `api.datadoghq.com`, induced 401/403
/// from rotated key, induced 429 rate-limit).
#[derive(Clone, Debug)]
pub struct DatadogExporter {
    config: DatadogConfig,
}

impl DatadogExporter {
    /// Construct from a validated [`DatadogConfig`].
    #[must_use]
    pub fn new(config: DatadogConfig) -> Self {
        Self { config }
    }

    /// Read-only handle to the underlying config.
    #[must_use]
    pub fn config(&self) -> &DatadogConfig {
        &self.config
    }
}

impl MetricsExporter for DatadogExporter {
    fn variant(&self) -> ExporterVariant {
        ExporterVariant::Datadog
    }

    fn export_batch(&self, metrics: &[MetricPoint]) -> Result<(), ExporterError> {
        tracing::debug!(
            target: "corelink_otel_export",
            site = ?self.config.site,
            host = self.config.site.ingest_host(),
            n = metrics.len(),
            "stub DatadogExporter::export_batch (real HTTP wiring deferred)"
        );
        Ok(())
    }

    fn export_trace(&self, span: &TraceSpan) -> Result<(), ExporterError> {
        tracing::debug!(
            target: "corelink_otel_export",
            site = ?self.config.site,
            span_name = %span.name,
            "stub DatadogExporter::export_trace (real APM wiring deferred)"
        );
        Ok(())
    }
}

/// OpenTelemetry Collector exporter (deferred-real stub).
#[derive(Clone, Debug)]
pub struct OtelCollectorExporter {
    config: OtelCollectorConfig,
}

impl OtelCollectorExporter {
    /// Construct from a validated [`OtelCollectorConfig`].
    #[must_use]
    pub fn new(config: OtelCollectorConfig) -> Self {
        Self { config }
    }

    /// Read-only handle to the underlying config.
    #[must_use]
    pub fn config(&self) -> &OtelCollectorConfig {
        &self.config
    }
}

impl MetricsExporter for OtelCollectorExporter {
    fn variant(&self) -> ExporterVariant {
        ExporterVariant::OtelCollector
    }

    fn export_batch(&self, metrics: &[MetricPoint]) -> Result<(), ExporterError> {
        tracing::debug!(
            target: "corelink_otel_export",
            endpoint = %self.config.endpoint,
            protocol = ?self.config.protocol,
            n = metrics.len(),
            "stub OtelCollectorExporter::export_batch (real OTLP wiring deferred)"
        );
        Ok(())
    }

    fn export_trace(&self, span: &TraceSpan) -> Result<(), ExporterError> {
        tracing::debug!(
            target: "corelink_otel_export",
            endpoint = %self.config.endpoint,
            protocol = ?self.config.protocol,
            span_name = %span.name,
            "stub OtelCollectorExporter::export_trace (real OTLP wiring deferred)"
        );
        Ok(())
    }
}

/// Grafana Cloud exporter (deferred-real stub).
///
/// Real wiring will fan-out metrics via Prometheus remote-write v1 and
/// traces via Tempo OTLP HTTP.
#[derive(Clone, Debug)]
pub struct GrafanaCloudExporter {
    config: GrafanaCloudConfig,
}

impl GrafanaCloudExporter {
    /// Construct from a validated [`GrafanaCloudConfig`].
    #[must_use]
    pub fn new(config: GrafanaCloudConfig) -> Self {
        Self { config }
    }

    /// Read-only handle to the underlying config.
    #[must_use]
    pub fn config(&self) -> &GrafanaCloudConfig {
        &self.config
    }
}

impl MetricsExporter for GrafanaCloudExporter {
    fn variant(&self) -> ExporterVariant {
        ExporterVariant::GrafanaCloud
    }

    fn export_batch(&self, metrics: &[MetricPoint]) -> Result<(), ExporterError> {
        tracing::debug!(
            target: "corelink_otel_export",
            prom_url = %self.config.prom_remote_write_url,
            instance_id = %self.config.instance_id,
            n = metrics.len(),
            "stub GrafanaCloudExporter::export_batch (real Prom remote-write deferred)"
        );
        Ok(())
    }

    fn export_trace(&self, span: &TraceSpan) -> Result<(), ExporterError> {
        tracing::debug!(
            target: "corelink_otel_export",
            tempo_url = %self.config.tempo_otlp_url,
            instance_id = %self.config.instance_id,
            span_name = %span.name,
            "stub GrafanaCloudExporter::export_trace (real Tempo OTLP deferred)"
        );
        Ok(())
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
    use std::collections::BTreeMap;

    use corelink_analytics::RedMetricKind;
    use corelink_tracing::{SpanKind, SpanRecord, SpanStatus};

    use super::*;
    use crate::otel::audit::InMemoryExportFailedAuditSink;
    use crate::otel::config::{DatadogSite, OtlpProtocol};
    use crate::otel::metric::MetricValue;

    fn sample_metric() -> MetricPoint {
        MetricPoint::new(
            RedMetricKind::CasPutRequestsTotal,
            BTreeMap::from([
                ("tenant_tier".to_string(), "free".to_string()),
                ("region".to_string(), "us-east-1".to_string()),
                ("result".to_string(), "ok".to_string()),
            ]),
            MetricValue::Counter(1),
            1_715_000_000_000_000_000,
        )
    }

    fn sample_span() -> SpanRecord {
        let mut s = SpanRecord::new(
            [0xAA; 16],
            [0xBB; 8],
            None,
            "cas.put",
            SpanKind::Server,
            100,
        );
        s.end(SpanStatus::Ok, 200);
        s
    }

    #[test]
    fn variant_str_canonical_slugs() {
        assert_eq!(ExporterVariant::Datadog.as_str(), "datadog");
        assert_eq!(ExporterVariant::OtelCollector.as_str(), "otel_collector");
        assert_eq!(ExporterVariant::GrafanaCloud.as_str(), "grafana_cloud");
        assert_eq!(ExporterVariant::Disabled.as_str(), "disabled");
    }

    #[test]
    fn in_memory_fake_captures_metrics() {
        let f = InMemoryFake::new(ExporterVariant::Datadog);
        f.export_batch(&[sample_metric(), sample_metric()]).unwrap();
        assert_eq!(f.snapshot_metrics().len(), 2);
    }

    #[test]
    fn in_memory_fake_captures_spans() {
        let f = InMemoryFake::new(ExporterVariant::OtelCollector);
        f.export_trace(&sample_span()).unwrap();
        assert_eq!(f.snapshot_spans().len(), 1);
    }

    #[test]
    fn in_memory_fake_injects_metric_failure() {
        let f = InMemoryFake::new(ExporterVariant::GrafanaCloud);
        f.inject_metric_failure(ExporterError::Transport("conn reset".into()));
        let err = f.export_batch(&[sample_metric()]).unwrap_err();
        assert!(matches!(err, ExporterError::Transport(_)));
        // failure was one-shot; next call succeeds
        f.export_batch(&[sample_metric()]).unwrap();
        assert_eq!(f.snapshot_metrics().len(), 1);
    }

    #[test]
    fn fail_open_emits_audit_on_metric_transport_failure() {
        let f = InMemoryFake::new(ExporterVariant::Datadog);
        let sink = InMemoryExportFailedAuditSink::new();
        f.inject_metric_failure(ExporterError::Transport("conn reset".into()));
        // Caller sees Ok(()) even though transport failed
        f.export_batch_fail_open(&[sample_metric()], &sink, 1_715_000_000_000)
            .unwrap();
        assert_eq!(sink.len(), 1);
        let ev = &sink.snapshot()[0];
        assert_eq!(ev.variant, ExporterVariant::Datadog);
        assert_eq!(ev.path, ExportFailedPath::Metric);
        assert_eq!(ev.batch_size, 1);
    }

    #[test]
    fn fail_open_emits_audit_on_trace_transport_failure() {
        let f = InMemoryFake::new(ExporterVariant::OtelCollector);
        let sink = InMemoryExportFailedAuditSink::new();
        f.inject_trace_failure(ExporterError::RateLimited("429".into()));
        f.export_trace_fail_open(&sample_span(), &sink, 0).unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0].path, ExportFailedPath::Trace);
    }

    #[test]
    fn fail_open_passthrough_when_export_succeeds() {
        let f = InMemoryFake::new(ExporterVariant::Datadog);
        let sink = InMemoryExportFailedAuditSink::new();
        f.export_batch_fail_open(&[sample_metric()], &sink, 0)
            .unwrap();
        assert!(sink.is_empty());
        assert_eq!(f.snapshot_metrics().len(), 1);
    }

    #[test]
    fn datadog_stub_returns_ok_for_batch_and_trace() {
        let cfg = DatadogConfig::new("a".repeat(32), DatadogSite::Us1).unwrap();
        let e = DatadogExporter::new(cfg);
        e.export_batch(&[sample_metric()]).unwrap();
        e.export_trace(&sample_span()).unwrap();
        assert_eq!(e.variant(), ExporterVariant::Datadog);
        assert_eq!(e.config().site, DatadogSite::Us1);
    }

    #[test]
    fn otel_collector_stub_returns_ok() {
        let cfg =
            OtelCollectorConfig::new("https://otel.example.com:4318", OtlpProtocol::Http, None)
                .unwrap();
        let e = OtelCollectorExporter::new(cfg);
        e.export_batch(&[sample_metric()]).unwrap();
        e.export_trace(&sample_span()).unwrap();
        assert_eq!(e.variant(), ExporterVariant::OtelCollector);
    }

    #[test]
    fn grafana_cloud_stub_returns_ok() {
        let cfg = GrafanaCloudConfig::new(
            "https://prometheus-prod-13.grafana.net/api/prom/push",
            "https://tempo-prod-04.grafana.net/tempo/api/push",
            "12345",
            "g".repeat(32),
        )
        .unwrap();
        let e = GrafanaCloudExporter::new(cfg);
        e.export_batch(&[sample_metric()]).unwrap();
        e.export_trace(&sample_span()).unwrap();
        assert_eq!(e.variant(), ExporterVariant::GrafanaCloud);
    }

    #[test]
    fn disabled_fake_still_emits_audit_on_injected_failure() {
        let f = InMemoryFake::new(ExporterVariant::Disabled);
        let sink = InMemoryExportFailedAuditSink::new();
        f.inject_metric_failure(ExporterError::PayloadRejected("400".into()));
        f.export_batch_fail_open(&[sample_metric()], &sink, 0)
            .unwrap();
        let ev = &sink.snapshot()[0];
        assert_eq!(ev.variant, ExporterVariant::Disabled);
    }
}
