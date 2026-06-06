//! Analytics-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-quota::audit` + `corelink-ratelimit::audit`: a
//! small analytics-flavoured sink trait the production wiring composes
//! on top of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S09-001 §11 freezes the canonical 3-event taxonomy:
//!
//! - `corelink.analytics.metric_emitted` — emitted on every successful
//!   counter / gauge / histogram observation (sampled at handler layer
//!   in production; in tests every decision emits one record).
//! - `corelink.analytics.cardinality_rejected` — emitted on the
//!   per-metric budget rejection arm (carries `metric_name`,
//!   `observed`, `budget`).
//! - `corelink.analytics.budget_exceeded` — emitted on the global
//!   budget rejection arm (carries `observed_global`, `budget_global`).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (WI-S09-002 logs /
//! WI-S09-003 traces / WI-S09-004 audit chain) can extend the taxonomy
//! additively without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;

/// Canonical analytics audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-09 follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AnalyticsEventType {
    /// `corelink.analytics.metric_emitted` — successful emit (the
    /// validator allowed the unique-tuple addition + the observer
    /// dispatch returned Ok).
    MetricEmitted,
    /// `corelink.analytics.cardinality_rejected` — per-metric budget
    /// rejected (the validator fail-closed because the unique tuple
    /// count would exceed the metric's budget).
    CardinalityRejected,
    /// `corelink.analytics.budget_exceeded` — global budget rejected
    /// (the validator fail-closed because the sum across all metrics
    /// would exceed the global budget).
    BudgetExceeded,
}

impl AnalyticsEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MetricEmitted => "corelink.analytics.metric_emitted",
            Self::CardinalityRejected => "corelink.analytics.cardinality_rejected",
            Self::BudgetExceeded => "corelink.analytics.budget_exceeded",
        }
    }

    /// Whether this variant requires SEV-2 alerting at the production
    /// observability layer. `CardinalityRejected` + `BudgetExceeded`
    /// are both SEV-2 (degraded observability — some series dropped
    /// at the validator boundary; INV-OBS-CARDINALITY-BUDGET canary).
    /// `MetricEmitted` is informational — high-volume; sampled in
    /// production.
    #[must_use]
    pub const fn is_sev2(self) -> bool {
        matches!(self, Self::CardinalityRejected | Self::BudgetExceeded)
    }
}

impl core::fmt::Display for AnalyticsEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 3] {
    &[
        "corelink.analytics.metric_emitted",
        "corelink.analytics.cardinality_rejected",
        "corelink.analytics.budget_exceeded",
    ]
}

/// Typed analytics audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyticsAuditRecord {
    /// Canonical event type.
    pub event_type: AnalyticsEventType,
    /// Canonical metric name slug (matches `RedMetricKind::as_str()`).
    pub metric_name: &'static str,
    /// Currently-observed unique tuple count for the metric (or
    /// global aggregate when the event is `BudgetExceeded`).
    pub observed: u64,
    /// Configured cardinality budget for the metric (or global) at
    /// emit time.
    pub budget: u64,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"sweeper"` for snapshot maintenance.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`AnalyticsAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AnalyticsAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("analytics audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the cardinality ledger UPDATE (S-01 audit_outbox table;
///   fail-closed envelope per Lote 10.6bis pattern + S-07 P1-1 fix).
/// - `MultiplexAuditSink` — fan-out to direct SIEM in addition to the
///   outbox.
pub trait AnalyticsAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`AnalyticsAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: AnalyticsAuditRecord) -> Result<(), AnalyticsAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryAnalyticsAuditSink {
    inner: std::sync::Arc<Mutex<Vec<AnalyticsAuditRecord>>>,
}

impl InMemoryAnalyticsAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AnalyticsAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: AnalyticsEventType) -> Vec<AnalyticsAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl AnalyticsAuditSink for InMemoryAnalyticsAuditSink {
    fn emit(&self, record: AnalyticsAuditRecord) -> Result<(), AnalyticsAuditSinkError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| AnalyticsAuditSinkError::Store("audit sink mutex poisoned".to_string()))?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 503 when the audit emit fires the
/// `Store` error per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
#[derive(Debug, Default)]
pub struct FailingAnalyticsAuditSink;

impl FailingAnalyticsAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AnalyticsAuditSink for FailingAnalyticsAuditSink {
    fn emit(&self, _record: AnalyticsAuditRecord) -> Result<(), AnalyticsAuditSinkError> {
        Err(AnalyticsAuditSinkError::Store(
            "induced analytics audit sink failure (test fixture)".to_string(),
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

    fn rec(t: AnalyticsEventType) -> AnalyticsAuditRecord {
        AnalyticsAuditRecord {
            event_type: t,
            metric_name: "corelink_cas_put_requests_total",
            observed: 1,
            budget: 20_000,
            created_by_request_id: "test".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            AnalyticsEventType::MetricEmitted,
            AnalyticsEventType::CardinalityRejected,
            AnalyticsEventType::BudgetExceeded,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.analytics."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 3);
        for s in canonical {
            assert!(s.starts_with("corelink.analytics."));
        }
    }

    #[test]
    fn sev2_classification_only_for_rejection_arms() {
        assert!(!AnalyticsEventType::MetricEmitted.is_sev2());
        assert!(AnalyticsEventType::CardinalityRejected.is_sev2());
        assert!(AnalyticsEventType::BudgetExceeded.is_sev2());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryAnalyticsAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(AnalyticsEventType::MetricEmitted)).unwrap();
        sink.emit(rec(AnalyticsEventType::CardinalityRejected))
            .unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(AnalyticsEventType::MetricEmitted).len(), 1);
        assert_eq!(
            sink.snapshot_of(AnalyticsEventType::CardinalityRejected)
                .len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingAnalyticsAuditSink::new();
        let err = sink
            .emit(rec(AnalyticsEventType::MetricEmitted))
            .unwrap_err();
        assert!(matches!(err, AnalyticsAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryAnalyticsAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(AnalyticsEventType::MetricEmitted)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", AnalyticsEventType::MetricEmitted),
            "corelink.analytics.metric_emitted"
        );
    }
}
