//! `OtlpExporter` trait + `InMemoryOtlpExporter` capture sink.
//!
//! Production wiring composes:
//!
//! - `OtlpHttpExporter` — fan-out to Grafana Tempo `/v1/traces` per
//!   region (deferred to WI-S09-007 PRR ship gate per the
//!   `trait-abstraction-defer` charter pattern; production binding
//!   uses `worker::send_future` fire-and-forget per Lote 10.7bis R5
//!   P0-3 — NEVER `tokio::spawn`).
//! - `MultiplexOtlpExporter` — fan-out to multiple OTLP endpoints
//!   (deferred to S-14 BYOK for customer-side Tempo tenants).
//!
//! The in-memory fake here covers the algorithmic invariants the
//! production binding relies on: cardinality-bounded buffer push,
//! audit fail-closed envelope, fail-OPEN sink semantics on transport
//! failure (per WI §6.1.6 — the SINK fails-OPEN; the AUDIT envelope
//! fails-CLOSED at the orchestrator boundary).

use std::sync::{Arc, Mutex};

use crate::error::OtlpExporterError;
use crate::span::SpanRecord;

/// OTLP exporter trait. Production wiring uses
/// `worker::send_future` fire-and-forget POST to `/v1/traces` per
/// the OTLP HTTP/JSON spec.
pub trait OtlpExporter: Send + Sync + core::fmt::Debug {
    /// Export a batch of canonical span records. Caller is
    /// responsible for sampling decision (only sampled spans land
    /// here per OpenTelemetry SDK SpanProcessor canonical contract).
    ///
    /// # Errors
    ///
    /// Returns [`OtlpExporterError::Backend`] on any backend failure.
    fn export(
        &self,
        spans: Vec<SpanRecord>,
    ) -> Result<(), OtlpExporterError>;
}

/// In-memory exporter that captures every batch for assertion. Cloning
/// shares the underlying buffer.
#[derive(Clone, Debug, Default)]
pub struct InMemoryOtlpExporter {
    inner: Arc<Mutex<Vec<SpanRecord>>>,
}

impl InMemoryOtlpExporter {
    /// Construct a fresh exporter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every span captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<SpanRecord> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of spans captured.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl OtlpExporter for InMemoryOtlpExporter {
    fn export(
        &self,
        spans: Vec<SpanRecord>,
    ) -> Result<(), OtlpExporterError> {
        let mut g = self.inner.lock().map_err(|_| {
            OtlpExporterError::Backend(
                "in-memory OTLP exporter mutex poisoned".to_string(),
            )
        })?;
        g.extend(spans);
        Ok(())
    }
}

/// Always-failing exporter for adversarial tests of the fail-OPEN
/// sink semantics.
#[derive(Debug, Default)]
pub struct FailingOtlpExporter;

impl FailingOtlpExporter {
    /// Construct a fresh always-failing exporter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl OtlpExporter for FailingOtlpExporter {
    fn export(
        &self,
        _spans: Vec<SpanRecord>,
    ) -> Result<(), OtlpExporterError> {
        Err(OtlpExporterError::Backend(
            "induced OTLP exporter failure (test fixture)".to_string(),
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
    use crate::span::{SpanKind, SpanRecord, SpanStatus};

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
    fn fresh_exporter_is_empty() {
        let e = InMemoryOtlpExporter::new();
        assert!(e.is_empty());
        assert_eq!(e.len(), 0);
    }

    #[test]
    fn export_appends_to_buffer() {
        let e = InMemoryOtlpExporter::new();
        e.export(vec![sample_span()]).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e.snapshot().len(), 1);
    }

    #[test]
    fn export_multiple_batches_accumulates() {
        let e = InMemoryOtlpExporter::new();
        e.export(vec![sample_span()]).unwrap();
        e.export(vec![sample_span(), sample_span()]).unwrap();
        assert_eq!(e.len(), 3);
    }

    #[test]
    fn cloned_exporter_shares_buffer() {
        let e = InMemoryOtlpExporter::new();
        let e2 = e.clone();
        e.export(vec![sample_span()]).unwrap();
        assert_eq!(e2.len(), 1);
    }

    #[test]
    fn failing_exporter_returns_backend_error() {
        let e = FailingOtlpExporter::new();
        let err = e.export(vec![sample_span()]).unwrap_err();
        assert!(matches!(err, OtlpExporterError::Backend(_)));
    }
}
