//! Audit event sink. The orchestrator fires events BEFORE state
//! mutation per INV-AUDIT-APPEND-ONLY (consent decisions are
//! regulatory-grade — silent loss is unacceptable).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::DpaAcceptanceError;
use crate::schema::{LocaleBcp47, SignupId, TenantId};

/// Canonical audit event names emitted by the DPA acceptance service.
#[must_use]
pub fn canonical_dpa_audit_event_names() -> &'static [&'static str] {
    &[
        "dpa.accepted",
        "dpa.locale_mismatch",
        "dpa.hash_mismatch",
        "dpa.idempotency_conflict",
        "dpa.replay_detected",
    ]
}

/// Audit event payload (closed; new arms added with `#[non_exhaustive]`
/// safety upstream).
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum DpaAuditEvent {
    /// Successful acceptance.
    #[serde(rename = "dpa.accepted")]
    Accepted {
        /// Tenant.
        tenant_id: TenantId,
        /// Signup id.
        signup_id: SignupId,
        /// JWT `jti`.
        jti: String,
        /// Locale captured.
        locale: LocaleBcp47,
    },
    /// Locale on payload disagreed with the server-resolved locale
    /// (Lote 10.16 violation; CTRL-PRIV-CONSENT-005).
    #[serde(rename = "dpa.locale_mismatch")]
    LocaleMismatch {
        /// Tenant.
        tenant_id: TenantId,
        /// Signup id.
        signup_id: SignupId,
        /// Server-resolved BCP-47.
        server: String,
        /// Payload BCP-47.
        payload: String,
    },
    /// Client-supplied `notice_text_hash` did not match the server
    /// recompute (CTRL-PRIV-CONSENT-001 violation).
    #[serde(rename = "dpa.hash_mismatch")]
    HashMismatch {
        /// Tenant.
        tenant_id: TenantId,
        /// Signup id.
        signup_id: SignupId,
    },
    /// `signup_id` reused with a diverging payload.
    #[serde(rename = "dpa.idempotency_conflict")]
    IdempotencyConflict {
        /// Tenant.
        tenant_id: TenantId,
        /// Signup id.
        signup_id: SignupId,
    },
    /// Replay attempt detected on the verify path (already-issued `jti`
    /// re-presented with diverging claims).
    #[serde(rename = "dpa.replay_detected")]
    ReplayDetected {
        /// JWT `jti`.
        jti: String,
    },
}

/// Audit emit sink.
pub trait DpaAuditSink: Send + Sync + std::fmt::Debug {
    /// Emit one audit event.
    ///
    /// # Errors
    ///
    /// Returns [`DpaAcceptanceError::AuditEmit`] on sink failure;
    /// orchestrator fails CLOSED before mutating state.
    fn emit(&self, event: DpaAuditEvent) -> Result<(), DpaAcceptanceError>;
}

/// In-memory capture sink for tests.
#[derive(Debug, Default)]
pub struct InMemoryDpaAuditSink {
    events: Mutex<Vec<DpaAuditEvent>>,
}

impl InMemoryDpaAuditSink {
    /// Build an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the captured events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<DpaAuditEvent> {
        self.events.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// `true` if the sink has captured at least one event matching
    /// `predicate`.
    pub fn contains<P: Fn(&DpaAuditEvent) -> bool>(&self, predicate: P) -> bool {
        self.events
            .lock()
            .map(|g| g.iter().any(predicate))
            .unwrap_or(false)
    }
}

impl DpaAuditSink for InMemoryDpaAuditSink {
    fn emit(&self, event: DpaAuditEvent) -> Result<(), DpaAcceptanceError> {
        let mut guard = self
            .events
            .lock()
            .map_err(|e| DpaAcceptanceError::AuditEmit(format!("poisoned: {}", e)))?;
        guard.push(event);
        Ok(())
    }
}
