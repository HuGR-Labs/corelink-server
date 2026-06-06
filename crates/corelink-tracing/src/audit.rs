//! Tracing-flavoured audit trait + InMemory test sink.
//!
//! Mirrors `corelink-logpush::audit` discipline: a small
//! tracing-flavoured sink trait the production wiring composes on top
//! of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor (WI-S09-004) lifts these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S09-003 §6.1 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.tracing.span_started` — emitted on every span start
//!   accepted by the orchestrator (after sampler decision).
//! - `corelink.tracing.span_ended` — emitted on every span end (tail
//!   decision deferred to WI-S09-007 PRR ship gate).
//! - `corelink.tracing.sampler_decision` — emitted on every sampler
//!   evaluation (HeadSampled / Drop). Tail-sampling deferred.
//! - `corelink.tracing.exporter_failure` — emitted on every OTLP
//!   exporter transport failure (Tempo OTLP HTTP `/v1/traces`
//!   unavailable). Production wiring fail-OPEN per WI §6.1.6.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (WI-S09-004 audit
//! chain / WI-S09-005 dashboards) can extend the taxonomy additively
//! without breaking downstream sinks.

use std::sync::Mutex;

/// Canonical tracing audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-09 follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TracingAuditEventType {
    /// `corelink.tracing.span_started` — span start accepted by the
    /// orchestrator after the sampler decision.
    SpanStarted,
    /// `corelink.tracing.span_ended` — span end accepted by the
    /// orchestrator (the spans buffer received the SpanRecord).
    SpanEnded,
    /// `corelink.tracing.sampler_decision` — sampler evaluation
    /// surfaced (informational; pinned for cardinality budget against
    /// `corelink_tracing_spans_exported_total{sampled_decision}`
    /// counter).
    SamplerDecision,
    /// `corelink.tracing.exporter_failure` — OTLP exporter transport
    /// failure (production wiring fail-OPEN per WI §6.1.6).
    ExporterFailure,
}

impl TracingAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SpanStarted => "corelink.tracing.span_started",
            Self::SpanEnded => "corelink.tracing.span_ended",
            Self::SamplerDecision => "corelink.tracing.sampler_decision",
            Self::ExporterFailure => "corelink.tracing.exporter_failure",
        }
    }

    /// Whether this variant fires SEV-3 alerting per WI §6.1.10.
    #[must_use]
    pub const fn is_sev3(self) -> bool {
        matches!(self, Self::ExporterFailure)
    }
}

impl core::fmt::Display for TracingAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.tracing.span_started",
        "corelink.tracing.span_ended",
        "corelink.tracing.sampler_decision",
        "corelink.tracing.exporter_failure",
    ]
}

/// Typed tracing audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TracingAuditRecord {
    /// Canonical event type.
    pub event_type: TracingAuditEventType,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"sweeper"` for snapshot maintenance.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
    /// Hex-encoded trace_id (32 lowercase hex chars; W3C canonical).
    /// Empty string for events with no associated trace (e.g.
    /// `ExporterFailure` without a single batch span).
    pub trace_id_hex: String,
    /// Hex-encoded span_id (16 lowercase hex chars; W3C canonical).
    /// Empty string for events with no associated span.
    pub span_id_hex: String,
}

/// Errors surfaced by [`TracingAuditSink::emit`].
pub use crate::error::TracingAuditSinkError as TracingAuditEmitError;

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxTracingAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the OTLP exporter emit (S-01 audit_outbox table;
///   fail-closed envelope per Lote 10.6bis pattern + S-07 P1-1 fix).
/// - `MultiplexTracingAuditSink` — fan-out to direct SIEM in addition
///   to the outbox.
pub trait TracingAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// span-emit fail-closed per Lote 10.6bis pattern.
    ///
    /// # Errors
    ///
    /// Returns [`TracingAuditEmitError::Store`] on any backend failure.
    fn emit(&self, record: TracingAuditRecord) -> Result<(), TracingAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryTracingAuditSink {
    inner: std::sync::Arc<Mutex<Vec<TracingAuditRecord>>>,
}

impl InMemoryTracingAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<TracingAuditRecord> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of records captured.
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

    /// Filter snapshot down to records of a single event type.
    #[must_use]
    pub fn snapshot_of(&self, event_type: TracingAuditEventType) -> Vec<TracingAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl TracingAuditSink for InMemoryTracingAuditSink {
    fn emit(&self, record: TracingAuditRecord) -> Result<(), TracingAuditEmitError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| TracingAuditEmitError::Store("audit sink mutex poisoned".to_string()))?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope.
#[derive(Debug, Default)]
pub struct FailingTracingAuditSink;

impl FailingTracingAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TracingAuditSink for FailingTracingAuditSink {
    fn emit(&self, _record: TracingAuditRecord) -> Result<(), TracingAuditEmitError> {
        Err(TracingAuditEmitError::Store(
            "induced tracing audit sink failure (test fixture)".to_string(),
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

    fn rec(t: TracingAuditEventType) -> TracingAuditRecord {
        TracingAuditRecord {
            event_type: t,
            created_by_request_id: "test".to_string(),
            now_ms: 1,
            trace_id_hex: String::new(),
            span_id_hex: String::new(),
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            TracingAuditEventType::SpanStarted,
            TracingAuditEventType::SpanEnded,
            TracingAuditEventType::SamplerDecision,
            TracingAuditEventType::ExporterFailure,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.tracing."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 4);
        for s in canonical {
            assert!(s.starts_with("corelink.tracing."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(TracingAuditEventType::ExporterFailure.is_sev3());
        assert!(!TracingAuditEventType::SpanStarted.is_sev3());
        assert!(!TracingAuditEventType::SpanEnded.is_sev3());
        assert!(!TracingAuditEventType::SamplerDecision.is_sev3());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryTracingAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(TracingAuditEventType::SpanStarted)).unwrap();
        sink.emit(rec(TracingAuditEventType::SpanEnded)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(TracingAuditEventType::SpanStarted).len(),
            1
        );
        assert_eq!(sink.snapshot_of(TracingAuditEventType::SpanEnded).len(), 1);
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingTracingAuditSink::new();
        let err = sink
            .emit(rec(TracingAuditEventType::SpanStarted))
            .unwrap_err();
        assert!(matches!(err, TracingAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryTracingAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(TracingAuditEventType::SpanStarted)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", TracingAuditEventType::SpanStarted),
            "corelink.tracing.span_started"
        );
    }
}
