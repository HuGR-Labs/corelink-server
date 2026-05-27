//! Consent audit emit — fail-CLOSED envelope (ADR-S11-002 split-tier).
//!
//! Consent is regulatory-grade; audit MUST fire BEFORE state mutation.
//! Audit failure → request aborts fail-CLOSED (no silent loss).
//! Distinct from billing fail-OPEN at Lote 10.6bis split-tier.
//!
//! # CloudEvents canonical types (Lote 10.9bis P0-G prefix)
//!
//! - `dev.hugr.corelink.consent.granted.v1`
//! - `dev.hugr.corelink.consent.revoked.v1`
//!
//! Emitted to `audit-<region>` R2 Object Lock 7y (CTRL-PRIV-CONSENT-003).

use std::sync::{Arc, Mutex};

use super::error::ConsentLedgerError;

/// Canonical consent audit event type (CloudEvents `type` field).
///
/// `#[non_exhaustive]` — callers must handle future variants.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConsentAuditEventType {
    /// `dev.hugr.corelink.consent.granted.v1`
    Granted,
    /// `dev.hugr.corelink.consent.revoked.v1`
    Revoked,
}

impl ConsentAuditEventType {
    /// CloudEvents `type` string per Lote 10.9bis P0-G prefix.
    #[must_use]
    pub fn as_cloudevent_type(&self) -> &'static str {
        match self {
            Self::Granted => "dev.hugr.corelink.consent.granted.v1",
            Self::Revoked => "dev.hugr.corelink.consent.revoked.v1",
        }
    }
}

impl std::fmt::Display for ConsentAuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_cloudevent_type())
    }
}

/// Returns all canonical CloudEvents type strings.
#[must_use]
pub fn canonical_consent_audit_event_strings() -> Vec<&'static str> {
    vec![
        "dev.hugr.corelink.consent.granted.v1",
        "dev.hugr.corelink.consent.revoked.v1",
    ]
}

/// Canonical audit record emitted for every consent decision arm.
#[derive(Debug, Clone)]
pub struct ConsentAuditRecord {
    /// CloudEvents `type`.
    pub event_type: ConsentAuditEventType,
    /// Consent or revocation record ID.
    pub record_id: String,
    /// Tenant identifier.
    pub tenant_id: String,
    /// SHA-256 hash of the subject ID (CTRL-PRIV-014 — raw subject_id
    /// MUST NOT appear in audit logs).
    pub subject_id_hash: String,
    /// Purpose string.
    pub purpose: String,
    /// ISO 8601 UTC timestamp.
    pub submission_ts: String,
}

/// Trait for emitting consent audit events.
///
/// Implementations MUST be fail-CLOSED: if emit fails, the caller
/// MUST abort the request without mutating state.
pub trait ConsentAuditSink: Send + Sync {
    /// Emit a consent audit record.
    ///
    /// # Fail-CLOSED contract
    ///
    /// Returns `Err` if the audit could not be durably recorded.
    /// The caller MUST rollback any in-progress state mutation and
    /// return a 503 with retry-after.
    fn emit(
        &self,
        record: ConsentAuditRecord,
    ) -> Result<(), ConsentLedgerError>;
}

/// In-memory audit sink that captures all emitted records.
///
/// Uses `Arc<Mutex<Vec<_>>>` F-001 closure (per-instance).
#[derive(Debug, Clone)]
pub struct InMemoryConsentAuditSink {
    records: Arc<Mutex<Vec<ConsentAuditRecord>>>,
}

impl InMemoryConsentAuditSink {
    /// Create a new empty capture sink.
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return a snapshot of all captured records.
    pub fn captured(&self) -> Vec<ConsentAuditRecord> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl Default for InMemoryConsentAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsentAuditSink for InMemoryConsentAuditSink {
    fn emit(&self, record: ConsentAuditRecord) -> Result<(), ConsentLedgerError> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(record);
        Ok(())
    }
}

/// Failing audit sink — always returns an error.
///
/// Used in chaos tests to verify fail-CLOSED behavior: state MUST remain
/// unchanged when audit emit fails (S-06 P0-2 / S-07 P1-1 lesson applied).
#[derive(Debug)]
pub struct FailingConsentAuditSink;

impl ConsentAuditSink for FailingConsentAuditSink {
    fn emit(&self, _record: ConsentAuditRecord) -> Result<(), ConsentLedgerError> {
        Err(ConsentLedgerError::Audit(
            "FailingConsentAuditSink: injected failure".to_owned(),
        ))
    }
}
