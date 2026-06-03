//! Log-emit-flavoured audit trait + InMemory test sink.
//!
//! Mirrors `corelink-analytics::audit` discipline: a small
//! logpush-flavoured sink trait the production wiring composes on top
//! of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor (WI-S09-004) lifts these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S09-002 §6.1 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.logpush.record_emitted` — emitted on every successful
//!   `LogRecord` accepted by the sink (after redaction + cardinality
//!   guard).
//! - `corelink.logpush.redaction_applied` — emitted on every redaction
//!   substitution (one record per emit; counts the number of pattern
//!   hits in the canonical taxonomy).
//! - `corelink.logpush.redaction_failure` — emitted on every redaction
//!   step that surfaces an error (rare; production wiring fails-closed
//!   to drop the log line + SEV-3 alert per CTRL-PRIV-001 bypass risk).
//! - `corelink.logpush.sink_failure` — emitted on every sink-side
//!   transport failure (Logpush ingest unavailable / R2 PUT failure /
//!   Loki push timeout). Production wiring fail-OPEN (log loss
//!   tolerable; vs audit fail-closed in WI-S09-004).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (WI-S09-003 traces
//! / WI-S09-004 audit chain / WI-S09-005 dashboards) can extend the
//! taxonomy additively without breaking downstream sinks.

use std::sync::Mutex;

/// Canonical logpush audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-09 follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum LogAuditEventType {
    /// `corelink.logpush.record_emitted` — successful emit (the sink
    /// accepted the redacted `LogRecord` + the cardinality guard
    /// allowed the unique-tuple addition).
    RecordEmitted,
    /// `corelink.logpush.redaction_applied` — redaction substitution
    /// fired (one or more PII patterns matched in the input data).
    RedactionApplied,
    /// `corelink.logpush.redaction_failure` — redaction step surfaced
    /// an error (CTRL-PRIV-001 bypass risk; SEV-3 alert source).
    RedactionFailure,
    /// `corelink.logpush.sink_failure` — sink-side transport failure
    /// (production wiring fail-OPEN per WI §6.1.9).
    SinkFailure,
}

impl LogAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecordEmitted => "corelink.logpush.record_emitted",
            Self::RedactionApplied => "corelink.logpush.redaction_applied",
            Self::RedactionFailure => "corelink.logpush.redaction_failure",
            Self::SinkFailure => "corelink.logpush.sink_failure",
        }
    }

    /// Whether this variant is informational (vs SEV-3 alerting). Per
    /// WI §6.1.10: `RedactionFailure` is SEV-1 (CTRL-PRIV-001 leak
    /// risk). `SinkFailure` is SEV-3. Others informational.
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::RedactionFailure)
    }

    /// Whether this variant fires SEV-3 alerting per WI §6.1.10.
    #[must_use]
    pub const fn is_sev3(self) -> bool {
        matches!(self, Self::SinkFailure)
    }
}

impl core::fmt::Display for LogAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.logpush.record_emitted",
        "corelink.logpush.redaction_applied",
        "corelink.logpush.redaction_failure",
        "corelink.logpush.sink_failure",
    ]
}

/// Typed logpush audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogAuditRecord {
    /// Canonical event type.
    pub event_type: LogAuditEventType,
    /// Canonical CloudEvents `type` of the originating `LogRecord`
    /// (e.g. `request_served`); allows downstream filtering by the
    /// log's domain category.
    pub log_event_type: &'static str,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"sweeper"` for snapshot maintenance.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
    /// Number of redaction substitutions performed on the originating
    /// `LogRecord` (used as a SEV-3 trigger source if a sustained
    /// non-zero stream of `RedactionFailure` is observed).
    pub redaction_count: u32,
}

/// Errors surfaced by [`LogAuditSink::emit`].
pub use crate::logpush::error::LogAuditSinkError as LogAuditEmitError;

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxLogAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the log-record sink emit (S-01 audit_outbox table;
///   fail-closed envelope per Lote 10.6bis pattern + S-07 P1-1 fix).
/// - `MultiplexLogAuditSink` — fan-out to direct SIEM in addition to
///   the outbox.
pub trait LogAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// log-emit fail-OPEN per WI §6.1.9 (logpush log loss is tolerable
    /// degradation; audit fail-closed is the WI-S09-004 separate
    /// concern).
    ///
    /// # Errors
    ///
    /// Returns [`LogAuditEmitError::Store`] on any backend failure.
    fn emit(&self, record: LogAuditRecord) -> Result<(), LogAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryLogAuditSink {
    inner: std::sync::Arc<Mutex<Vec<LogAuditRecord>>>,
}

impl InMemoryLogAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<LogAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: LogAuditEventType) -> Vec<LogAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl LogAuditSink for InMemoryLogAuditSink {
    fn emit(&self, record: LogAuditRecord) -> Result<(), LogAuditEmitError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LogAuditEmitError::Store("audit sink mutex poisoned".to_string()))?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope.
#[derive(Debug, Default)]
pub struct FailingLogAuditSink;

impl FailingLogAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl LogAuditSink for FailingLogAuditSink {
    fn emit(&self, _record: LogAuditRecord) -> Result<(), LogAuditEmitError> {
        Err(LogAuditEmitError::Store(
            "induced logpush audit sink failure (test fixture)".to_string(),
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

    fn rec(t: LogAuditEventType) -> LogAuditRecord {
        LogAuditRecord {
            event_type: t,
            log_event_type: "request_served",
            created_by_request_id: "test".to_string(),
            now_ms: 1,
            redaction_count: 0,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            LogAuditEventType::RecordEmitted,
            LogAuditEventType::RedactionApplied,
            LogAuditEventType::RedactionFailure,
            LogAuditEventType::SinkFailure,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.logpush."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 4);
        for s in canonical {
            assert!(s.starts_with("corelink.logpush."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(LogAuditEventType::RedactionFailure.is_sev1());
        assert!(!LogAuditEventType::RedactionFailure.is_sev3());
        assert!(LogAuditEventType::SinkFailure.is_sev3());
        assert!(!LogAuditEventType::SinkFailure.is_sev1());
        assert!(!LogAuditEventType::RecordEmitted.is_sev1());
        assert!(!LogAuditEventType::RecordEmitted.is_sev3());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryLogAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(LogAuditEventType::RecordEmitted)).unwrap();
        sink.emit(rec(LogAuditEventType::RedactionApplied)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(LogAuditEventType::RecordEmitted).len(), 1);
        assert_eq!(
            sink.snapshot_of(LogAuditEventType::RedactionApplied).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingLogAuditSink::new();
        let err = sink
            .emit(rec(LogAuditEventType::RecordEmitted))
            .unwrap_err();
        assert!(matches!(err, LogAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryLogAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(LogAuditEventType::RecordEmitted)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", LogAuditEventType::RecordEmitted),
            "corelink.logpush.record_emitted"
        );
    }
}
