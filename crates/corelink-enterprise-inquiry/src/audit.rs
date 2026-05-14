//! Enterprise inquiry audit-of-audit taxonomy.
//!
//! Per Lote 10.6bis pattern + S-07 P1-1 fix: every ledger state
//! mutation emits an audit record BEFORE the mutation. Audit failure
//! returns a typed error and the mutation is aborted (fail-CLOSED).

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::form::{InquiryId, InquiryStatus};

/// Canonical 7-element audit event taxonomy per WI §6.1 + §15.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum InquiryAuditEventType {
    /// `corelink.onboarding.enterprise_inquiry_received` — form
    /// validated + persisted.
    Received,
    /// `corelink.onboarding.enterprise_inquiry_atomic_ok` — saga
    /// committed (Slack + CRM both confirmed).
    AtomicOk,
    /// `corelink.onboarding.enterprise_inquiry_atomic_rollback` —
    /// saga rolled back (Slack or CRM failed; compensating action
    /// dispatched).
    AtomicRollback,
    /// `corelink.onboarding.enterprise_inquiry_auto_reply_sent` —
    /// auto-reply email handed off to SES (or fake mailer).
    AutoReplySent,
    /// `corelink.onboarding.enterprise_inquiry_sla_breached` — 24h
    /// SLA breach detected (Sev2).
    SlaBreached,
    /// `corelink.onboarding.enterprise_inquiry_spam_blocked` —
    /// reCAPTCHA / rate-limit / bot-detection rejected the submission.
    SpamBlocked,
    /// `corelink.onboarding.enterprise_inquiry_partial_escalated` —
    /// saga sat in partial state > 5 min ceiling; RB invoked.
    PartialEscalated,
}

impl InquiryAuditEventType {
    /// Canonical CloudEvents type string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Received => "corelink.onboarding.enterprise_inquiry_received",
            Self::AtomicOk => "corelink.onboarding.enterprise_inquiry_atomic_ok",
            Self::AtomicRollback => "corelink.onboarding.enterprise_inquiry_atomic_rollback",
            Self::AutoReplySent => "corelink.onboarding.enterprise_inquiry_auto_reply_sent",
            Self::SlaBreached => "corelink.onboarding.enterprise_inquiry_sla_breached",
            Self::SpamBlocked => "corelink.onboarding.enterprise_inquiry_spam_blocked",
            Self::PartialEscalated => "corelink.onboarding.enterprise_inquiry_partial_escalated",
        }
    }
}

impl core::fmt::Display for InquiryAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 7-element list for surface-stability regression tests.
#[must_use]
pub const fn canonical_inquiry_audit_event_strings() -> &'static [&'static str; 7] {
    &[
        "corelink.onboarding.enterprise_inquiry_received",
        "corelink.onboarding.enterprise_inquiry_atomic_ok",
        "corelink.onboarding.enterprise_inquiry_atomic_rollback",
        "corelink.onboarding.enterprise_inquiry_auto_reply_sent",
        "corelink.onboarding.enterprise_inquiry_sla_breached",
        "corelink.onboarding.enterprise_inquiry_spam_blocked",
        "corelink.onboarding.enterprise_inquiry_partial_escalated",
    ]
}

/// Audit record emitted on every ledger mutation arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InquiryAuditRecord {
    /// CloudEvents type.
    pub event_type: InquiryAuditEventType,
    /// Inquiry the record pertains to.
    pub inquiry_id: InquiryId,
    /// Timestamp (ms since epoch).
    pub ts_ms: u64,
    /// Correlation id (PAT-CORRELATION-ID-001).
    pub correlation_id: String,
    /// Status the inquiry transitioned INTO (where applicable).
    pub new_status: Option<InquiryStatus>,
}

impl InquiryAuditRecord {
    /// Construct a minimal record without a status transition.
    #[must_use]
    pub fn new(
        event_type: InquiryAuditEventType,
        inquiry_id: InquiryId,
        ts_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            inquiry_id,
            ts_ms,
            correlation_id: correlation_id.into(),
            new_status: None,
        }
    }

    /// Attach a target status transition.
    #[must_use]
    pub fn with_status(mut self, status: InquiryStatus) -> Self {
        self.new_status = Some(status);
        self
    }
}

/// Audit-of-audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum InquiryAuditEmitError {
    /// Downstream audit sink rejected the record (e.g. R2 write
    /// failure / chain-link advance failure).
    #[error("inquiry audit sink rejected emit: {0}")]
    Rejected(String),
}

/// Trait every inquiry audit-of-audit sink satisfies.
pub trait InquiryAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single audit record. MUST be invoked BEFORE the ledger
    /// mutation; a returned `Err` aborts the mutation per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
    ///
    /// # Errors
    ///
    /// Returns [`InquiryAuditEmitError::Rejected`] when the downstream
    /// audit sink rejects the record. Callers MUST abort the in-
    /// flight ledger mutation per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
    fn emit(&self, record: &InquiryAuditRecord) -> Result<(), InquiryAuditEmitError>;
}

/// In-memory test sink — accumulates emitted records for property
/// inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemoryInquiryAuditSink {
    records: Arc<Mutex<Vec<InquiryAuditRecord>>>,
}

impl InMemoryInquiryAuditSink {
    /// Construct an empty in-memory sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<InquiryAuditRecord> {
        match self.records.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.records.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True if no events have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl InquiryAuditSink for InMemoryInquiryAuditSink {
    fn emit(&self, record: &InquiryAuditRecord) -> Result<(), InquiryAuditEmitError> {
        let mut g = self
            .records
            .lock()
            .map_err(|e| InquiryAuditEmitError::Rejected(format!("mutex poisoned: {e}")))?;
        g.push(record.clone());
        Ok(())
    }
}

/// Adversarial fixture sink that always rejects (forces ledger
/// fail-CLOSED behaviour in tests).
#[derive(Clone, Debug, Default)]
pub struct FailingInquiryAuditSink;

impl InquiryAuditSink for FailingInquiryAuditSink {
    fn emit(&self, _record: &InquiryAuditRecord) -> Result<(), InquiryAuditEmitError> {
        Err(InquiryAuditEmitError::Rejected(
            "adversarial fixture: always rejects".to_string(),
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

    #[test]
    fn audit_event_strings_canonical() {
        let expected = canonical_inquiry_audit_event_strings();
        let actual: Vec<&str> = [
            InquiryAuditEventType::Received,
            InquiryAuditEventType::AtomicOk,
            InquiryAuditEventType::AtomicRollback,
            InquiryAuditEventType::AutoReplySent,
            InquiryAuditEventType::SlaBreached,
            InquiryAuditEventType::SpamBlocked,
            InquiryAuditEventType::PartialEscalated,
        ]
        .iter()
        .map(|e| e.as_str())
        .collect();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn in_memory_sink_records_emit() {
        let sink = InMemoryInquiryAuditSink::new();
        assert!(sink.is_empty());
        let rec = InquiryAuditRecord::new(
            InquiryAuditEventType::Received,
            InquiryId::new("inq-1"),
            100,
            "cid-1",
        );
        sink.emit(&rec).unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0], rec);
    }

    #[test]
    fn failing_sink_always_rejects() {
        let sink = FailingInquiryAuditSink;
        let rec = InquiryAuditRecord::new(
            InquiryAuditEventType::Received,
            InquiryId::new("inq-1"),
            100,
            "cid-1",
        );
        assert!(sink.emit(&rec).is_err());
    }
}
