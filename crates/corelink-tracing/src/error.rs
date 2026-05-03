//! Canonical error taxonomy for the `corelink-tracing` emit surface.
//!
//! All variants `#[non_exhaustive]` so additive growth lands without
//! breaking downstream `match` sites (Lote 10.6bis discipline).

use thiserror::Error;

/// Audit-sink failure surface (lifted into [`TracingError::Audit`]).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TracingAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("tracing audit sink store error: {0}")]
    Store(String),
}

/// Exporter-side transport failure surface (lifted into
/// [`TracingError::Exporter`]).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OtlpExporterError {
    /// Backend transport failure (Tempo OTLP HTTP `/v1/traces` ingest
    /// unavailable / TLS handshake failure / 5xx). Production wiring
    /// records this as `corelink_tracing_export_failures_total{reason}`
    /// SEV-3 alert per WI §6.1.10.
    #[error("OTLP exporter backend error: {0}")]
    Backend(String),
}

/// Canonical error surface returned by the [`crate::sampler`] +
/// [`crate::exporter`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TracingError {
    /// W3C `traceparent` parse error. Per W3C Trace Context §3.2
    /// the recipient MUST reject malformed inputs (invalid version /
    /// invalid hex / wrong segment lengths / all-zero trace_id /
    /// all-zero span_id).
    #[error("W3C traceparent parse error: {0}")]
    TraceContextParseError(String),

    /// Audit emit failure aborts the span emit (fail-closed envelope
    /// per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    #[error("tracing audit emit failure: {0}")]
    Audit(#[from] TracingAuditSinkError),

    /// Underlying OTLP exporter transport failure (production Tempo
    /// HTTP binding rejected; the in-memory fake never returns this
    /// outside of the `FailingOtlpExporter` adversarial fixture).
    #[error("OTLP exporter backend failure: {0}")]
    Exporter(#[from] OtlpExporterError),

    /// Sampler decision exceeded its rate budget (e.g. invalid rate
    /// > 1.0 surfaced from a misconfigured runtime override).
    #[error("sampler rate out of bounds: {observed_rate}")]
    SamplerRateOutOfBounds {
        /// Observed rate value (must be in `[0.0, 1.0]`).
        observed_rate: f64,
    },

    /// Internal invariant violation (e.g. mutex poisoned by a panicking
    /// emit). Production wiring maps this to fail-OPEN per WI §6.1.9
    /// (tracing emit fail-OPEN distinction vs audit fail-closed).
    #[error("tracing internal state invariant violated: {0}")]
    Internal(String),
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
    fn trace_context_parse_error_displays_diagnostic() {
        let e = TracingError::TraceContextParseError(
            "invalid hex".into(),
        );
        let s = format!("{e}");
        assert!(s.contains("traceparent"));
    }

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner =
            TracingAuditSinkError::Store("induced".to_string());
        let e: TracingError = inner.into();
        assert!(matches!(e, TracingError::Audit(_)));
    }

    #[test]
    fn from_exporter_error_lifts_cleanly() {
        let inner =
            OtlpExporterError::Backend("induced".to_string());
        let e: TracingError = inner.into();
        assert!(matches!(e, TracingError::Exporter(_)));
    }

    #[test]
    fn sampler_rate_out_of_bounds_displays_diagnostic() {
        let e = TracingError::SamplerRateOutOfBounds {
            observed_rate: 1.5,
        };
        let s = format!("{e}");
        assert!(s.contains("1.5"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = TracingError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }
}
