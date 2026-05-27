//! `LogSink` trait + `InMemoryLogSink` orchestrator.
//!
//! ## Decision pipeline (per emit)
//!
//! For each `(LogRecord, request_id, now_ms)`:
//!
//! 1. **Cardinality guard**: register the per-event-type label tuple
//!    against the analytics `CardinalityValidator` (per WI §6.1.2;
//!    log emit fails-closed if the tuple would breach the budget;
//!    `tenant_id` is body data NOT a label so the cardinality budget
//!    bounds at the `LogEventType × Region × Tier` cartesian =
//!    4 × 22 × 5 = 440 tuples max).
//! 2. **Redaction**: walk the `data` value tree with the `PiiRedactor`
//!    substituting PII matches with canonical placeholders.
//! 3. **Audit emit BEFORE state mutation** per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`
//!    (Lote 10.6bis pattern + S-07 P1-1 fix). Audit failure aborts the
//!    sink push.
//! 4. **Sink push**: append the redacted, NDJSON-serialized line to the
//!    in-memory buffer (production wiring fans out to CF Logpush + R2 PUT
//!    + Loki push).
//!
//! ## F-001 closure
//!
//! The sink holds the in-memory NDJSON buffer under a per-instance
//! `Arc<Mutex<>>` (NOT a `static LazyLock`). Tests instantiate fresh
//! sinks per case so the orchestrator harness cannot accidentally
//! leak buffer state across cases.
//!
//! ## Fail-OPEN vs fail-CLOSED distinction (Lote 10.6bis lesson)
//!
//! Per WI §6.1.9: log emit fail-OPEN (best-effort observability;
//! missing log NOT compromise security). The fail-OPEN distinction
//! applies to the SINK transport (Logpush ingest unavailable / R2 PUT
//! failure / Loki push timeout) — production wiring records the
//! failure as `corelink.logpush.sink_failure` audit + SEV-3 alert and
//! continues. Audit fail-CLOSED applies to the AUDIT envelope
//! (separate WI-S09-004 concern). The cardinality + redaction guards
//! both fail-CLOSED at the orchestrator boundary because they protect
//! against runaway cost (cardinality blow-up) and CTRL-PRIV-001
//! bypass (raw PII leak).

use std::sync::{Arc, Mutex};

use corelink_analytics::{
    AnalyticsError, CardinalityValidator,
    InMemoryAnalyticsAuditSink, MetricLabelTuple, RedMetricKind,
    Region, Tier,
};

use crate::logpush::audit::{
    LogAuditEventType, LogAuditRecord, LogAuditSink,
};
use crate::logpush::error::{LogSinkError, LogpushError};
use crate::logpush::record::{LogEventType, LogRecord};
use crate::logpush::redaction::{PiiRedactor, RedactionOutcome};

/// Persisted log line (canonical NDJSON form + per-pattern hit
/// counts). Cloning is cheap so the production wiring can fan out to
/// multiple downstreams (R2 PUT + Loki push) without re-serializing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedLogLine {
    /// Canonical NDJSON line (no trailing newline).
    pub ndjson: String,
    /// Cumulative redaction hit counts for the originating record.
    pub redaction_hits: u32,
    /// Originating CloudEvents `type`.
    pub event_type: LogEventType,
    /// Originating `Region`.
    pub region: Region,
}

/// Log-sink trait. Production wiring composes:
///
/// - `LogpushR2LokiSink` — fan-out to CF Logpush ingest + R2 PUT +
///   Loki push (deferred to WI-S09-007 PRR ship gate per
///   `trait-abstraction-defer` charter pattern).
pub trait LogSink: Send + Sync + core::fmt::Debug {
    /// Push the redacted record. Production wiring fans out to CF
    /// Logpush + R2 + Loki.
    ///
    /// # Errors
    ///
    /// Returns [`LogSinkError::Backend`] on any backend failure.
    fn push(
        &self,
        line: PersistedLogLine,
    ) -> Result<(), LogSinkError>;
}

/// In-memory log sink + redaction + cardinality orchestrator. Cloning
/// shares the underlying buffers.
#[derive(Clone, Debug)]
pub struct InMemoryLogSink<R, A>
where
    R: PiiRedactor,
    A: LogAuditSink + 'static,
{
    redactor: Arc<R>,
    audit: Arc<A>,
    cardinality:
        Arc<CardinalityValidator<InMemoryAnalyticsAuditSink>>,
    state: Arc<Mutex<SinkState>>,
}

#[derive(Debug, Default)]
struct SinkState {
    buffer: Vec<PersistedLogLine>,
    sink_failure_count: u64,
}

impl<R, A> InMemoryLogSink<R, A>
where
    R: PiiRedactor,
    A: LogAuditSink + 'static,
{
    /// Construct with explicit redactor + audit sink + cardinality
    /// validator.
    pub fn new(
        redactor: Arc<R>,
        audit: Arc<A>,
        cardinality: Arc<
            CardinalityValidator<InMemoryAnalyticsAuditSink>,
        >,
    ) -> Self {
        Self {
            redactor,
            audit,
            cardinality,
            state: Arc::new(Mutex::new(SinkState::default())),
        }
    }

    /// Snapshot every persisted log line captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<PersistedLogLine> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.buffer.clone()
    }

    /// Number of records captured.
    #[must_use]
    pub fn len(&self) -> usize {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.buffer.len()
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cumulative count of sink-side transport failures (production
    /// wiring uses this as the SEV-3
    /// `corelink_logs_ingest_failures_total{reason}` alert source).
    #[must_use]
    pub fn sink_failure_count(&self) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.sink_failure_count
    }

    /// Emit a log record through the orchestrator pipeline.
    ///
    /// Decision pipeline:
    ///
    /// 1. Cardinality guard (per-event-type label tuple).
    /// 2. PII redaction (walk `data` tree).
    /// 3. Audit emit BEFORE buffer mutation (fail-closed envelope).
    /// 4. NDJSON serialize + buffer push.
    ///
    /// # Errors
    ///
    /// - [`LogpushError::CardinalityBudgetExceeded`] when the per-event-type
    ///   label tuple would breach the analytics cardinality budget.
    /// - [`LogpushError::Audit`] when the audit sink fails (fail-closed).
    /// - [`LogpushError::RedactionFailure`] when the NDJSON serialization
    ///   of the post-redaction record fails.
    /// - [`LogpushError::Internal`] when the per-instance mutex is
    ///   poisoned.
    pub fn emit(
        &self,
        mut record: LogRecord,
        tenant_tier: Tier,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<RedactionOutcome, LogpushError> {
        // 1. Cardinality guard.
        let labels = MetricLabelTuple {
            tenant_tier: Some(tenant_tier),
            region: Some(record.region),
            ..MetricLabelTuple::empty()
        };
        match self.cardinality.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            labels,
            created_by_request_id,
            now_ms,
        ) {
            Ok(_) => {}
            Err(AnalyticsError::CardinalityBudgetExceeded {
                observed,
                budget,
                scope,
                ..
            }) => {
                return Err(LogpushError::CardinalityBudgetExceeded {
                    observed,
                    budget,
                    scope,
                });
            }
            Err(other) => {
                return Err(LogpushError::Internal(format!(
                    "cardinality validator error: {other}"
                )));
            }
        }

        // 2. Redaction (walk `data` tree).
        let outcome = self.redactor.redact_json(&mut record.data);

        // 3. Audit emit BEFORE buffer mutation.
        self.audit.emit(LogAuditRecord {
            event_type: LogAuditEventType::RecordEmitted,
            log_event_type: record.event_type.as_str(),
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            redaction_count: outcome.total_hits(),
        })?;
        if outcome.total_hits() > 0 {
            self.audit.emit(LogAuditRecord {
                event_type: LogAuditEventType::RedactionApplied,
                log_event_type: record.event_type.as_str(),
                created_by_request_id: created_by_request_id
                    .to_string(),
                now_ms,
                redaction_count: outcome.total_hits(),
            })?;
        }

        // 4. NDJSON serialize + buffer push. Serialization failure
        // surfaces as `RedactionFailure` (the only failure mode here is
        // a non-finite float in the post-redaction `data` tree; the
        // production wiring drops the line + emits SEV-1).
        let ndjson = match record.to_ndjson_line() {
            Ok(s) => s,
            Err(e) => {
                self.audit.emit(LogAuditRecord {
                    event_type:
                        LogAuditEventType::RedactionFailure,
                    log_event_type: record.event_type.as_str(),
                    created_by_request_id: created_by_request_id
                        .to_string(),
                    now_ms,
                    redaction_count: outcome.total_hits(),
                })?;
                return Err(LogpushError::RedactionFailure(format!(
                    "NDJSON serialize failed: {e}"
                )));
            }
        };
        let event_type_for_line = record.event_type;
        let region_for_line = record.region;
        let line = PersistedLogLine {
            ndjson,
            redaction_hits: outcome.total_hits(),
            event_type: event_type_for_line,
            region: region_for_line,
        };
        let mut g = self.state.lock().map_err(|_| {
            LogpushError::Internal(
                "log sink mutex poisoned".to_string(),
            )
        })?;
        g.buffer.push(line);
        Ok(outcome)
    }

    /// Record a sink-side transport failure (production wiring calls
    /// this when the downstream Logpush ingest / R2 PUT / Loki push
    /// returns an error). Emits the canonical
    /// `corelink.logpush.sink_failure` audit (fail-OPEN per WI §6.1.9 —
    /// production wiring continues serving traffic + counter increments
    /// for the SEV-3 alert).
    ///
    /// # Errors
    ///
    /// - [`LogpushError::Audit`] when the audit emit fails.
    /// - [`LogpushError::Internal`] when the per-instance mutex is
    ///   poisoned.
    pub fn record_sink_failure(
        &self,
        event_type: LogEventType,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<(), LogpushError> {
        self.audit.emit(LogAuditRecord {
            event_type: LogAuditEventType::SinkFailure,
            log_event_type: event_type.as_str(),
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            redaction_count: 0,
        })?;
        let mut g = self.state.lock().map_err(|_| {
            LogpushError::Internal(
                "log sink mutex poisoned (sink_failure bump)"
                    .to_string(),
            )
        })?;
        g.sink_failure_count =
            g.sink_failure_count.saturating_add(1);
        Ok(())
    }
}

/// Always-failing log sink for adversarial tests.
#[derive(Debug, Default)]
pub struct FailingLogSink;

impl FailingLogSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl LogSink for FailingLogSink {
    fn push(
        &self,
        _line: PersistedLogLine,
    ) -> Result<(), LogSinkError> {
        Err(LogSinkError::Backend(
            "induced log sink failure (test fixture)".to_string(),
        ))
    }
}

/// In-memory `LogSink` impl that captures persisted lines without the
/// orchestrator's redaction + cardinality + audit envelope. Used by
/// the production wiring staging tests + multi-sink fan-out fixtures.
#[derive(Clone, Debug, Default)]
pub struct CapturedLogSink {
    inner: Arc<Mutex<Vec<PersistedLogLine>>>,
}

impl CapturedLogSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured line.
    #[must_use]
    pub fn snapshot(&self) -> Vec<PersistedLogLine> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl LogSink for CapturedLogSink {
    fn push(
        &self,
        line: PersistedLogLine,
    ) -> Result<(), LogSinkError> {
        let mut g = self.inner.lock().map_err(|_| {
            LogSinkError::Backend(
                "captured log sink mutex poisoned".to_string(),
            )
        })?;
        g.push(line);
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
    use crate::logpush::audit::{
        FailingLogAuditSink, InMemoryLogAuditSink,
    };
    use crate::logpush::redaction::InMemoryPiiRedactor;
    use serde_json::json;
    use uuid::Uuid;

    type Sink =
        InMemoryLogSink<InMemoryPiiRedactor, InMemoryLogAuditSink>;

    fn fresh_sink() -> (Sink, Arc<InMemoryLogAuditSink>) {
        let r = Arc::new(InMemoryPiiRedactor::new());
        let audit = Arc::new(InMemoryLogAuditSink::new());
        let cardinality_audit =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        let cardinality = Arc::new(
            CardinalityValidator::with_defaults(cardinality_audit),
        );
        let s = InMemoryLogSink::new(r, Arc::clone(&audit), cardinality);
        (s, audit)
    }

    fn fresh_record() -> LogRecord {
        LogRecord::new(
            LogEventType::RequestServed,
            "corelink-cas-iad",
            Uuid::now_v7(),
            1,
            Some(Uuid::now_v7()),
            Region::Iad,
            json!({
                "path": "/v1/cas/put",
                "user": "alice@example.com",
                "client_ip": "192.168.1.42"
            }),
        )
    }

    #[test]
    fn fresh_sink_is_empty() {
        let (s, _a) = fresh_sink();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert_eq!(s.sink_failure_count(), 0);
    }

    #[test]
    fn emit_redacts_data_and_persists_ndjson() {
        let (s, audit) = fresh_sink();
        let r = fresh_record();
        let outcome =
            s.emit(r, Tier::Team, "req-1", 1000).unwrap();
        assert_eq!(outcome.email_hits, 1);
        assert_eq!(outcome.ip_hits, 1);
        assert_eq!(s.len(), 1);
        let snap = s.snapshot();
        let line = snap.first().unwrap();
        assert!(!line.ndjson.contains("alice@example.com"));
        assert!(!line.ndjson.contains("192.168.1.42"));
        assert!(line.ndjson.contains("<EMAIL_REDACTED>"));
        assert!(line.ndjson.contains("<IP_REDACTED>"));
        assert_eq!(line.redaction_hits, 2);
        assert_eq!(line.event_type, LogEventType::RequestServed);
        // Audit: 1 RecordEmitted + 1 RedactionApplied.
        assert_eq!(
            audit
                .snapshot_of(LogAuditEventType::RecordEmitted)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(LogAuditEventType::RedactionApplied)
                .len(),
            1
        );
    }

    #[test]
    fn audit_failure_aborts_emit_no_buffer_mutation() {
        let r = Arc::new(InMemoryPiiRedactor::new());
        let audit = Arc::new(FailingLogAuditSink::new());
        let cardinality_audit =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        let cardinality = Arc::new(
            CardinalityValidator::with_defaults(cardinality_audit),
        );
        let s = InMemoryLogSink::new(r, audit, cardinality);
        let err = s.emit(fresh_record(), Tier::Team, "req-1", 1000);
        let err = err.unwrap_err();
        assert!(matches!(err, LogpushError::Audit(_)));
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn record_sink_failure_increments_counter_and_audits() {
        let (s, audit) = fresh_sink();
        s.record_sink_failure(
            LogEventType::RequestServed,
            "req-1",
            1000,
        )
        .unwrap();
        assert_eq!(s.sink_failure_count(), 1);
        assert_eq!(
            audit
                .snapshot_of(LogAuditEventType::SinkFailure)
                .len(),
            1
        );
    }

    #[test]
    fn emit_without_pii_emits_record_only_no_redaction_audit() {
        let (s, audit) = fresh_sink();
        let r = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-cas-iad",
            Uuid::now_v7(),
            1,
            None,
            Region::Iad,
            json!({"path": "/healthz", "status": 200}),
        );
        s.emit(r, Tier::Team, "req-1", 1000).unwrap();
        assert_eq!(
            audit
                .snapshot_of(LogAuditEventType::RecordEmitted)
                .len(),
            1
        );
        // No PII hits → no RedactionApplied audit.
        assert_eq!(
            audit
                .snapshot_of(LogAuditEventType::RedactionApplied)
                .len(),
            0
        );
    }

    #[test]
    fn cardinality_guard_rejects_over_budget_arm() {
        let r = Arc::new(InMemoryPiiRedactor::new());
        let audit = Arc::new(InMemoryLogAuditSink::new());
        let cardinality_audit =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        let cfg =
            corelink_analytics::AnalyticsConfig::with_budgets(
                1, 100,
            );
        let cardinality =
            Arc::new(CardinalityValidator::new(
                cardinality_audit,
                cfg,
            ));
        let s = InMemoryLogSink::new(r, audit, cardinality);

        // 1st emit (Team / Iad) → registered.
        let r1 = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-cas-iad",
            Uuid::now_v7(),
            1,
            None,
            Region::Iad,
            json!({}),
        );
        s.emit(r1, Tier::Team, "req-1", 1000).unwrap();
        // 2nd emit (Free / Iad) → NEW unique tuple → over budget.
        let r2 = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-cas-iad",
            Uuid::now_v7(),
            2,
            None,
            Region::Iad,
            json!({}),
        );
        let err = s.emit(r2, Tier::Free, "req-2", 2000);
        let err = err.unwrap_err();
        assert!(matches!(
            err,
            LogpushError::CardinalityBudgetExceeded { .. }
        ));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let (s, _a) = fresh_sink();
        let s2 = s.clone();
        s.emit(fresh_record(), Tier::Team, "req-1", 1000).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn captured_log_sink_persists_line() {
        let captured = CapturedLogSink::new();
        let line = PersistedLogLine {
            ndjson: "{\"x\":1}".to_string(),
            redaction_hits: 0,
            event_type: LogEventType::RequestServed,
            region: Region::Iad,
        };
        captured.push(line.clone()).unwrap();
        let snap = captured.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap.first().unwrap(), &line);
    }

    #[test]
    fn failing_log_sink_returns_backend_error() {
        let s = FailingLogSink::new();
        let err = s
            .push(PersistedLogLine {
                ndjson: "{}".to_string(),
                redaction_hits: 0,
                event_type: LogEventType::RequestServed,
                region: Region::Iad,
            })
            .unwrap_err();
        assert!(matches!(err, LogSinkError::Backend(_)));
    }
}
