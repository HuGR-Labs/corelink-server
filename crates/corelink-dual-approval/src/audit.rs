//! Admin op CloudEvent audit emission (CAP-ADMIN-006, WI-S13-002).
//!
//! Emits `corelink.admin.op.executed` (success) or
//! `corelink.admin.op.denied` (any failure variant) as CloudEvents v1.0.2
//! envelopes, with rich payload: actor + mfa_ts + dual_approver +
//! op_payload_hash + prev_state_hash + signature chain integrity.
//!
//! Fail-CLOSED: audit emit fires BEFORE op execution; failure = 503.
//! Atomic with `admin_op_log` INSERT (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{ActorIdentity, AdminAuditEventData, AdminOpType, ApprovalOutcome};

/// Canonical CloudEvent type strings for admin op audit.
pub const AUDIT_TYPE_EXECUTED: &str = "corelink.admin.op.executed";
/// Denied event type.
pub const AUDIT_TYPE_DENIED: &str = "corelink.admin.op.denied";

/// Builder for [`AdminOpCloudEvent`] (avoids >7-argument constructor).
#[derive(Debug)]
pub struct AdminOpCloudEventBuilder {
    /// Op UUID (from Uuid::now_v7).
    pub op_id: Uuid,
    /// Region string.
    pub region: String,
    /// Outcome (approved / denied_*).
    pub outcome: ApprovalOutcome,
    /// Caller identity.
    pub caller: ActorIdentity,
    /// Approver identity.
    pub approver: ActorIdentity,
    /// MFA assertion timestamp ms.
    pub mfa_ts_ms: u64,
    /// Op type.
    pub op_type: AdminOpType,
    /// SHA-256 of op_payload.
    pub op_payload_hash: [u8; 32],
    /// SHA-256 of prior state.
    pub prev_state_hash: [u8; 32],
    /// Replay nonce.
    pub nonce: [u8; 16],
    /// HMAC chain integrity signature.
    pub hmac_chain_sig: [u8; 32],
    /// Server timestamp ms.
    pub now_ms: u64,
}

impl AdminOpCloudEventBuilder {
    /// Build into [`AdminOpCloudEvent`].
    pub fn build(self) -> AdminOpCloudEvent {
        let event_type = if self.outcome == ApprovalOutcome::Approved {
            AUDIT_TYPE_EXECUTED.to_owned()
        } else {
            AUDIT_TYPE_DENIED.to_owned()
        };
        let data = AdminAuditEventData {
            actor: self.caller,
            mfa_ts_ms: self.mfa_ts_ms,
            dual_approver: self.approver,
            op_type: format!("{:?}", self.op_type),
            op_payload_hash: hex::encode(self.op_payload_hash),
            prev_state_hash: hex::encode(self.prev_state_hash),
            nonce: hex::encode(self.nonce),
            outcome: self.outcome.as_str().to_owned(),
            signature: hex::encode(self.hmac_chain_sig),
        };
        AdminOpCloudEvent {
            specversion: "1.0",
            id: self.op_id,
            source: format!("/corelink/admin/{}", self.region),
            event_type,
            time: ms_to_rfc3339(self.now_ms),
            data,
        }
    }
}

/// CloudEvents v1.0.2 envelope for admin op audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminOpCloudEvent {
    /// CloudEvents specversion.
    pub specversion: &'static str,
    /// Event ID (UUIDv7).
    pub id: Uuid,
    /// Source path.
    pub source: String,
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: String,
    /// RFC 3339 timestamp.
    pub time: String,
    /// Rich data payload.
    pub data: AdminAuditEventData,
}

/// Convert ms-since-epoch to a basic RFC 3339 string (UTC).
fn ms_to_rfc3339(ms: u64) -> String {
    let secs = ms / 1000;
    let millis = ms % 1000;
    format!("{secs}.{millis:03}Z")
}

/// Trait for admin op audit sinks.
///
/// Production: Cloudflare audit_outbox D1 table (atomic batch per
/// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). In-memory sink for CI.
pub trait AdminOpAuditSink: Send + Sync + std::fmt::Debug {
    /// Emit an admin op CloudEvent. Returns `Err` on failure (fail-CLOSED).
    fn emit(&self, event: AdminOpCloudEvent) -> Result<(), AdminAuditSinkError>;
    /// Return all captured events (for testing).
    fn captured(&self) -> Vec<AdminOpCloudEvent>;
}

/// Error from audit sink emission.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AdminAuditSinkError {
    /// Sink refused the event (D1 insert failure, etc.).
    #[error("audit sink failure: {0}")]
    SinkFailure(String),
}

/// In-memory audit sink for CI and property tests (F-001: per-instance
/// `Arc<Mutex<>>`).
#[derive(Debug, Clone)]
pub struct InMemoryAdminOpAuditSink {
    captured: Arc<Mutex<Vec<AdminOpCloudEvent>>>,
}

impl InMemoryAdminOpAuditSink {
    /// Construct a fresh empty sink.
    pub fn new() -> Self {
        Self {
            captured: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for InMemoryAdminOpAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AdminOpAuditSink for InMemoryAdminOpAuditSink {
    fn emit(&self, event: AdminOpCloudEvent) -> Result<(), AdminAuditSinkError> {
        let mut guard = self
            .captured
            .lock()
            .map_err(|e| AdminAuditSinkError::SinkFailure(e.to_string()))?;
        guard.push(event);
        Ok(())
    }

    fn captured(&self) -> Vec<AdminOpCloudEvent> {
        self.captured.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

/// Always-failing audit sink — used to test fail-CLOSED behaviour.
#[derive(Debug, Clone)]
pub struct FailingAdminOpAuditSink;

impl AdminOpAuditSink for FailingAdminOpAuditSink {
    fn emit(&self, _event: AdminOpCloudEvent) -> Result<(), AdminAuditSinkError> {
        Err(AdminAuditSinkError::SinkFailure(
            "injected sink failure".to_owned(),
        ))
    }

    fn captured(&self) -> Vec<AdminOpCloudEvent> {
        Vec::new()
    }
}
