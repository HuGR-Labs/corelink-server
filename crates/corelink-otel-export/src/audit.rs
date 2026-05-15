//! `ExportFailedEvent` envelope + audit sink trait.
//!
//! Per `INV-OBS-EXPORT-FAIL-OPEN`, every transport failure on a
//! customer-export path emits a `corelink.observability.export_failed`
//! audit event BEFORE the orchestrator returns `Ok(())` to the caller.
//! The audit chain is the load-bearing falsifiability target — a
//! customer who claims "your metrics never reached our Datadog"
//! must be able to look at the audit log and see whether we tried +
//! how the vendor responded.
//!
//! The CloudEvents-aligned envelope is consumed by
//! `corelink-audit-chain` in production (deferred wiring); the
//! in-memory sink here covers the algorithmic invariants the chain
//! relies on.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::ExporterError;
use crate::exporter::ExporterVariant;

/// Canonical audit event type emitted by every failing export path.
pub const EXPORT_FAILED_EVENT_TYPE: &str = "corelink.observability.export_failed";

/// CloudEvents-aligned payload for the `corelink.observability.export_failed`
/// audit event.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct ExportFailedEvent {
    /// Canonical event type slug.
    pub event_type: String,
    /// Exporter variant that failed.
    pub variant: ExporterVariant,
    /// Whether the failure was on the metric path
    /// (`export_batch`) or the trace path (`export_trace`).
    pub path: ExportFailedPath,
    /// Vendor-supplied reason (truncated to 256 bytes per
    /// `observability_model.md §10.2`).
    pub reason: String,
    /// Count of metric points / spans in the failed batch (telemetry
    /// for "how many samples did we lose").
    pub batch_size: u32,
    /// Unix-epoch milliseconds at which the failure was observed.
    pub observed_at_unix_ms: u64,
}

impl ExportFailedEvent {
    /// Construct a new export-failed audit event from an
    /// [`ExporterError`] and a path marker.
    #[must_use]
    pub fn from_error(
        variant: ExporterVariant,
        path: ExportFailedPath,
        err: &ExporterError,
        batch_size: u32,
        observed_at_unix_ms: u64,
    ) -> Self {
        let raw = err.to_string();
        let reason = if raw.len() > 256 {
            // safe truncation at char boundary (ASCII-clean reasons)
            let mut end = 256;
            while !raw.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            raw.get(..end).unwrap_or("").to_string()
        } else {
            raw
        };
        Self {
            event_type: EXPORT_FAILED_EVENT_TYPE.to_string(),
            variant,
            path,
            reason,
            batch_size,
            observed_at_unix_ms,
        }
    }
}

/// Which side of [`crate::MetricsExporter`] raised the failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExportFailedPath {
    /// Metric batch path (`export_batch`).
    Metric,
    /// Trace path (`export_trace`).
    Trace,
}

/// Trait every concrete audit sink implements (production sink is the
/// `corelink-audit-chain` writer; deferred).
pub trait ExportFailedAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one audit event. The orchestrator calls this BEFORE
    /// converting an exporter failure into `Ok(())`, so audit-emit
    /// failure is itself fatal — the orchestrator returns `Err(...)`
    /// from the audit emit branch (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns `Err` only on internal failure (mutex poisoning, IO).
    /// Customer-side observability vendor failures NEVER bubble through
    /// this method.
    fn record(&self, event: &ExportFailedEvent) -> Result<(), AuditEmitError>;
}

/// Errors raised by the audit sink itself (internal-only; never
/// reflects customer vendor state).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuditEmitError {
    /// Internal sink failure (mutex poisoned, IO).
    #[error("audit sink internal failure: {0}")]
    Internal(String),
}

/// In-memory deterministic audit sink (clone-shared buffer).
#[derive(Clone, Debug, Default)]
pub struct InMemoryExportFailedAuditSink {
    inner: Arc<Mutex<Vec<ExportFailedEvent>>>,
}

impl InMemoryExportFailedAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every event recorded so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ExportFailedEvent> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of events recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ExportFailedAuditSink for InMemoryExportFailedAuditSink {
    fn record(&self, event: &ExportFailedEvent) -> Result<(), AuditEmitError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AuditEmitError::Internal("audit sink mutex poisoned".into()))?;
        g.push(event.clone());
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
    use super::*;

    #[test]
    fn export_failed_event_carries_canonical_slug() {
        let err = ExporterError::Transport("conn reset".into());
        let ev = ExportFailedEvent::from_error(
            ExporterVariant::Datadog,
            ExportFailedPath::Metric,
            &err,
            42,
            1_715_000_000_000,
        );
        assert_eq!(ev.event_type, "corelink.observability.export_failed");
        assert_eq!(ev.variant, ExporterVariant::Datadog);
        assert_eq!(ev.path, ExportFailedPath::Metric);
        assert_eq!(ev.batch_size, 42);
    }

    #[test]
    fn long_reason_is_truncated_to_256_bytes() {
        let big = "x".repeat(1000);
        let err = ExporterError::Transport(big);
        let ev = ExportFailedEvent::from_error(
            ExporterVariant::OtelCollector,
            ExportFailedPath::Trace,
            &err,
            1,
            0,
        );
        assert!(ev.reason.len() <= 256);
    }

    #[test]
    fn in_memory_sink_records_events() {
        let sink = InMemoryExportFailedAuditSink::new();
        assert!(sink.is_empty());
        let ev = ExportFailedEvent::from_error(
            ExporterVariant::GrafanaCloud,
            ExportFailedPath::Metric,
            &ExporterError::RateLimited("429".into()),
            10,
            123,
        );
        sink.record(&ev).unwrap();
        assert_eq!(sink.len(), 1);
        let snap = sink.snapshot();
        assert_eq!(snap[0].variant, ExporterVariant::GrafanaCloud);
    }
}
