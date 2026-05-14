//! 2 CloudEvents canonical types for residency enforcement.
//!
//! Canonical prefix `dev.hugr.corelink.residency.<verb>.v1` per Lote 10.9bis P0-G.
//! Emitted to `audit-<region>` R2 bucket (Object Lock 7y) fail-CLOSED.
//!
//! Audit fail-CLOSED ordering: `lookup → emit_audit → mutate_state`
//! per S-06 P0-2 / S-07 P1-1 lesson. State NEVER mutated if audit emit fails.

use crate::{BackendKind, Region};
use serde::{Deserialize, Serialize};

/// CloudEvents `type` field — 2 canonical types per Lote 10.9bis P0-G prefix.
///
/// `dev.hugr.corelink.residency.{request_routed,write_rejected_cross_region}.v1`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ResidencyAuditEventType {
    /// Emitted on every routed request (outcome = `accepted` or `rejected`).
    ///
    /// CloudEvents type: `dev.hugr.corelink.residency.request_routed.v1`
    RequestRouted,
    /// Emitted when a cross-region write is rejected.
    ///
    /// CloudEvents type: `dev.hugr.corelink.residency.write_rejected_cross_region.v1`
    WriteRejectedCrossRegion,
}

impl ResidencyAuditEventType {
    /// Returns the canonical CloudEvents `type` string per Lote 10.9bis P0-G prefix.
    pub fn as_cloudevents_type(self) -> &'static str {
        match self {
            ResidencyAuditEventType::RequestRouted => {
                "dev.hugr.corelink.residency.request_routed.v1"
            }
            ResidencyAuditEventType::WriteRejectedCrossRegion => {
                "dev.hugr.corelink.residency.write_rejected_cross_region.v1"
            }
        }
    }
}

/// Outcome of a routed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum RoutingOutcome {
    /// Request region matches tenant.primary_region — routed to correct backend.
    Accepted,
    /// Request region mismatches tenant.primary_region — rejected 451.
    Rejected,
}

/// Payload for `dev.hugr.corelink.residency.request_routed.v1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRoutedPayload {
    /// Tenant whose request was routed.
    pub tenant_id: String,
    /// Region the request arrived at.
    pub requested_region: Region,
    /// Canonical region for this tenant.
    pub expected_region: Region,
    /// Routing outcome.
    pub outcome: RoutingOutcome,
}

/// Payload for `dev.hugr.corelink.residency.write_rejected_cross_region.v1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteRejectedPayload {
    /// Tenant whose write was rejected.
    pub tenant_id: String,
    /// Region the write targeted.
    pub attempted_region: Region,
    /// Canonical region for this tenant.
    pub expected_region: Region,
    /// Backend that would have received the cross-region write.
    pub backend: BackendKind,
}

/// Canonical audit record emitted to `audit-<region>` R2 bucket (Object Lock 7y).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidencyAuditRecord {
    /// CloudEvents spec version (`"1.0"`).
    pub spec_version: String,
    /// CloudEvents type — canonical prefix per Lote 10.9bis P0-G.
    pub event_type: String,
    /// Source URI for this service.
    pub source: String,
    /// Unique event ID (UUIDv4 deterministic in tests).
    pub id: String,
    /// ISO 8601 UTC timestamp.
    pub time: String,
    /// JSON-encoded CloudEvents data payload.
    pub data: serde_json::Value,
}

impl ResidencyAuditRecord {
    /// Construct a `request_routed` record.
    pub fn request_routed(id: impl Into<String>, time: impl Into<String>, payload: RequestRoutedPayload) -> Self {
        let data = serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null);
        Self {
            spec_version: "1.0".to_string(),
            event_type: ResidencyAuditEventType::RequestRouted.as_cloudevents_type().to_string(),
            source: "https://corelink.dev/residency-enforcement".to_string(),
            id: id.into(),
            time: time.into(),
            data,
        }
    }

    /// Construct a `write_rejected_cross_region` record.
    pub fn write_rejected(id: impl Into<String>, time: impl Into<String>, payload: WriteRejectedPayload) -> Self {
        let data = serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null);
        Self {
            spec_version: "1.0".to_string(),
            event_type: ResidencyAuditEventType::WriteRejectedCrossRegion
                .as_cloudevents_type()
                .to_string(),
            source: "https://corelink.dev/residency-enforcement".to_string(),
            id: id.into(),
            time: time.into(),
            data,
        }
    }
}

/// Audit sink trait — emit fail-CLOSED.
///
/// # Fail-CLOSED contract (INV-AUDIT-APPEND-ONLY + AC-007)
///
/// State is NEVER mutated if `emit` returns `Err`. The ordering is:
/// `lookup → emit_audit → mutate_state`.
pub trait ResidencyAuditSink: Send + Sync {
    /// Emit one audit record to `audit-<region>` R2 Object Lock 7y.
    ///
    /// Returns `Err` if emit fails; caller MUST NOT mutate state.
    fn emit(&self, record: ResidencyAuditRecord) -> Result<(), String>;
}

/// In-memory capture sink for tests.
#[derive(Debug, Default)]
pub struct InMemoryResidencyAuditSink {
    records: std::sync::Mutex<Vec<ResidencyAuditRecord>>,
}

impl InMemoryResidencyAuditSink {
    /// Create a new empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot all emitted records (clone).
    pub fn records(&self) -> Vec<ResidencyAuditRecord> {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl ResidencyAuditSink for InMemoryResidencyAuditSink {
    fn emit(&self, record: ResidencyAuditRecord) -> Result<(), String> {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(record);
        Ok(())
    }
}

/// Failing audit sink — always returns Err; used to verify fail-CLOSED behavior.
#[derive(Debug, Default)]
pub struct FailingResidencyAuditSink;

impl ResidencyAuditSink for FailingResidencyAuditSink {
    fn emit(&self, _record: ResidencyAuditRecord) -> Result<(), String> {
        Err("audit-infrastructure-unavailable: simulated failure".to_string())
    }
}
