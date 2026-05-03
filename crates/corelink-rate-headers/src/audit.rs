//! Circuit-breaker audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-quota::audit` (S-07 PROVISIONAL) +
//! `corelink-ratelimit::audit` (S-08 token-bucket) +
//! `corelink-quota-cas::audit` (S-08 atomic CAS) +
//! `corelink-abuse::audit` (S-08 heuristic abuse detection): a small
//! circuit-flavoured sink trait the production wiring composes on top
//! of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit chain
//! processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S08-005 §6.1.11 freezes the canonical 5-event taxonomy:
//!
//! - `corelink.circuit.tripped` — emitted on every Closed → Open
//!   transition (real failure detection; multi-signal canonical OR
//!   manual-override admin force-open). SEV-1 alert source.
//! - `corelink.circuit.half_open_probe` — emitted on Open → HalfOpen
//!   transition (hysteresis recovery dwell met; entering 10% sample
//!   probe). Informational lineage record.
//! - `corelink.circuit.closed_recovery` — emitted on HalfOpen → Closed
//!   transition (signal floor + ≥ 90% sample success met; full
//!   recovery). SEV-2 informational so post-mortem can correlate.
//! - `corelink.circuit.request_rejected` — emitted on every 429
//!   GlobalCircuitOpen rendered to a request (the camada-0 reject path).
//!   Informational; the SLI-distinction within-quota counter sources
//!   this audit when the trip reason is NOT a ManualOverride.
//! - `corelink.circuit.manual_override` — emitted on every admin S-13
//!   force-open / force-closed action; SEV-2 alert source. LGPD audit
//!   trail per sprint contract §15 R-S08-004 absorbed.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (DASH-RATE widget
//! at WI-S08-006; admin-plane S-13) can extend the taxonomy additively
//! without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;

/// Canonical circuit-breaker audit taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for S-08 follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CircuitEventType {
    /// `corelink.circuit.tripped` — Closed → Open transition.
    Tripped,
    /// `corelink.circuit.half_open_probe` — Open → HalfOpen transition.
    HalfOpenProbe,
    /// `corelink.circuit.closed_recovery` — HalfOpen → Closed transition.
    ClosedRecovery,
    /// `corelink.circuit.request_rejected` — 429 GlobalCircuitOpen
    /// rendered to a request.
    RequestRejected,
    /// `corelink.circuit.manual_override` — admin S-13 force open /
    /// force closed action.
    ManualOverride,
}

impl CircuitEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tripped => "corelink.circuit.tripped",
            Self::HalfOpenProbe => "corelink.circuit.half_open_probe",
            Self::ClosedRecovery => "corelink.circuit.closed_recovery",
            Self::RequestRejected => "corelink.circuit.request_rejected",
            Self::ManualOverride => "corelink.circuit.manual_override",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// `Tripped` is SEV-1 (oncall pager wake-up; CAP-RATE-004 alert).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::Tripped)
    }
}

impl core::fmt::Display for CircuitEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.circuit.tripped",
        "corelink.circuit.half_open_probe",
        "corelink.circuit.closed_recovery",
        "corelink.circuit.request_rejected",
        "corelink.circuit.manual_override",
    ]
}

/// Typed circuit audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope; the trait surface accepts the typed shape
/// so the sink + the envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq)]
pub struct CircuitAuditRecord {
    /// Canonical event type.
    pub event_type: CircuitEventType,
    /// Region scope (canonical CF colocode, e.g. "iad" / "sam").
    pub region: String,
    /// Reason serialised as text (`Error5xxRateExceeded` /
    /// `LatencyP99Exceeded` / `DoErrorRateExceeded` /
    /// `MultiSignalCombined` / `ManualOverride`); `None` when irrelevant
    /// (HalfOpen → Closed recovery has no reason; the previous Open
    /// event carries it).
    pub trip_reason: Option<String>,
    /// Producer-side wall-clock instant (Unix epoch ms; canonical no
    /// `_ms` suffix per Lote 10.7bis P0-3).
    pub now_ms: u64,
    /// Source attribution: the request id / cron-tick batch id.
    pub created_by_request_id: String,
    /// Manual-override admin attribution (`Some` only when event_type
    /// is `ManualOverride`; LGPD audit trail per sprint contract §15
    /// R-S08-004 absorbed).
    pub manual_override_admin_id: Option<String>,
}

/// Errors surfaced by [`CircuitAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CircuitAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("circuit audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the `global_circuit_state` mutation (S-01 audit_outbox
///   table; fail-closed envelope per Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (`Tripped` IS SEV-1).
pub trait CircuitAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 5xx
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`CircuitAuditSinkError::Store`] on any backend failure.
    fn emit(
        &self,
        record: CircuitAuditRecord,
    ) -> Result<(), CircuitAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryCircuitAuditSink {
    inner: std::sync::Arc<Mutex<Vec<CircuitAuditRecord>>>,
}

impl InMemoryCircuitAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<CircuitAuditRecord> {
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
    pub fn snapshot_of(
        &self,
        event_type: CircuitEventType,
    ) -> Vec<CircuitAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl CircuitAuditSink for InMemoryCircuitAuditSink {
    fn emit(
        &self,
        record: CircuitAuditRecord,
    ) -> Result<(), CircuitAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            CircuitAuditSinkError::Store(
                "audit sink mutex poisoned".to_string(),
            )
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 5xx when the audit emit fires the
/// `Store` error).
#[derive(Debug, Default)]
pub struct FailingCircuitAuditSink;

impl FailingCircuitAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CircuitAuditSink for FailingCircuitAuditSink {
    fn emit(
        &self,
        _record: CircuitAuditRecord,
    ) -> Result<(), CircuitAuditSinkError> {
        Err(CircuitAuditSinkError::Store(
            "induced circuit audit sink failure (test fixture)".to_string(),
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

    fn rec(t: CircuitEventType) -> CircuitAuditRecord {
        CircuitAuditRecord {
            event_type: t,
            region: "iad".to_string(),
            trip_reason: None,
            now_ms: 1,
            created_by_request_id: "test".to_string(),
            manual_override_admin_id: None,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            CircuitEventType::Tripped,
            CircuitEventType::HalfOpenProbe,
            CircuitEventType::ClosedRecovery,
            CircuitEventType::RequestRejected,
            CircuitEventType::ManualOverride,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.circuit."));
            assert!(set.insert(t.as_str()), "duplicate: {t}");
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 5);
        for s in canonical {
            assert!(s.starts_with("corelink.circuit."));
        }
    }

    #[test]
    fn sev1_subset_only_tripped() {
        assert!(CircuitEventType::Tripped.is_sev1());
        assert!(!CircuitEventType::HalfOpenProbe.is_sev1());
        assert!(!CircuitEventType::ClosedRecovery.is_sev1());
        assert!(!CircuitEventType::RequestRejected.is_sev1());
        assert!(!CircuitEventType::ManualOverride.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryCircuitAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(CircuitEventType::Tripped)).unwrap();
        sink.emit(rec(CircuitEventType::HalfOpenProbe)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(CircuitEventType::Tripped).len(), 1);
        assert_eq!(
            sink.snapshot_of(CircuitEventType::HalfOpenProbe).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingCircuitAuditSink::new();
        let err = sink.emit(rec(CircuitEventType::Tripped)).unwrap_err();
        assert!(matches!(err, CircuitAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryCircuitAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(CircuitEventType::Tripped)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", CircuitEventType::Tripped),
            "corelink.circuit.tripped"
        );
    }
}
