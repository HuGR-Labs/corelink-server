//! Signup audit-of-audit taxonomy.
//!
//! Per Lote 10.6bis pattern + INV-AUDIT-APPEND-ONLY: every orchestrator
//! state mutation emits a `corelink.signup.*` audit record BEFORE the
//! mutation. Audit failure returns a typed error and the mutation is
//! aborted (fail-CLOSED).
//!
//! Canonical 4-event taxonomy:
//!
//! - `corelink.signup.started` — pre-mutation lookup completed; the
//!   orchestrator is about to open the atomic D1 tx.
//! - `corelink.signup.completed` — atomic D1 tx committed AND Stripe
//!   customer linked synchronously.
//! - `corelink.signup.failed` — orchestrator aborted (input rejection,
//!   D1 tx rollback, or non-retryable billing error mapping to rejection).
//! - `corelink.signup.deferred` — atomic D1 tx committed; Stripe link
//!   pending via saga compensation (chaos Stripe outage path).

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::correlation::CorrelationId;
use crate::outcome::OrchestrationStep;
use crate::region::PrimaryRegion;

/// Canonical 4-element audit event taxonomy per WI §16 + spec contract
/// §5.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SignupAuditEventType {
    /// `corelink.signup.started` — pre-mutation lookup OK; opening
    /// atomic D1 tx.
    Started,
    /// `corelink.signup.completed` — atomic D1 tx committed AND Stripe
    /// linked synchronously.
    Completed,
    /// `corelink.signup.failed` — orchestrator aborted (rejection or
    /// rollback).
    Failed,
    /// `corelink.signup.deferred` — atomic D1 tx committed; billing
    /// pending via saga compensation.
    Deferred,
}

impl SignupAuditEventType {
    /// Canonical CloudEvents type string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "corelink.signup.started",
            Self::Completed => "corelink.signup.completed",
            Self::Failed => "corelink.signup.failed",
            Self::Deferred => "corelink.signup.deferred",
        }
    }
}

impl core::fmt::Display for SignupAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 4-element list of [`SignupAuditEventType`] strings for
/// surface-stability regression tests.
#[must_use]
pub const fn canonical_signup_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.signup.started",
        "corelink.signup.completed",
        "corelink.signup.failed",
        "corelink.signup.deferred",
    ]
}

/// Audit record emitted on every orchestrator decision arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SignupAuditRecord {
    /// CloudEvents type.
    pub event_type: SignupAuditEventType,
    /// Correlation id (PAT-CORRELATION-ID-001).
    pub correlation_id: CorrelationId,
    /// Idempotency key (carried so audit chain consumers can join with
    /// the signup_orchestration table).
    pub idempotency_key: String,
    /// Region the tenant was pinned to (populated for Started /
    /// Completed / Deferred; None for failed-pre-tx Rejected arm).
    pub primary_region: Option<PrimaryRegion>,
    /// Step at which the event was emitted (populated for `Failed`;
    /// None otherwise).
    pub step: Option<OrchestrationStep>,
    /// Reason label (populated for `Failed` and `Deferred`).
    pub reason: Option<String>,
}

impl SignupAuditRecord {
    /// Construct a new audit record. Optional fields default to `None`
    /// and may be filled via the builder-style setters.
    #[must_use]
    pub fn new(
        event_type: SignupAuditEventType,
        correlation_id: CorrelationId,
        idempotency_key: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            correlation_id,
            idempotency_key: idempotency_key.into(),
            primary_region: None,
            step: None,
            reason: None,
        }
    }

    /// Attach a [`PrimaryRegion`] (used by Started / Completed /
    /// Deferred arms).
    #[must_use]
    pub fn with_region(mut self, region: PrimaryRegion) -> Self {
        self.primary_region = Some(region);
        self
    }

    /// Attach an [`OrchestrationStep`] (used by `Failed` arm).
    #[must_use]
    pub fn with_step(mut self, step: OrchestrationStep) -> Self {
        self.step = Some(step);
        self
    }

    /// Attach a reason label (used by `Failed` and `Deferred` arms).
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// Audit-of-audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SignupAuditEmitError {
    /// Downstream audit sink rejected the record (e.g. R2 write
    /// failure / chain-link advance failure).
    #[error("signup audit sink rejected emit: {0}")]
    Rejected(String),
}

/// Trait every signup audit-of-audit sink satisfies.
pub trait SignupAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single audit record. MUST be invoked BEFORE the
    /// orchestrator mutation; a returned `Err` aborts the mutation per
    /// INV-AUDIT-APPEND-ONLY + Lote 10.6bis pattern.
    fn emit(&self, record: &SignupAuditRecord) -> Result<(), SignupAuditEmitError>;
}

/// In-memory test sink — accumulates emitted records for property
/// inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemorySignupAuditSink {
    records: Arc<Mutex<Vec<SignupAuditRecord>>>,
}

impl InMemorySignupAuditSink {
    /// Construct an empty in-memory sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<SignupAuditRecord> {
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

impl SignupAuditSink for InMemorySignupAuditSink {
    fn emit(&self, record: &SignupAuditRecord) -> Result<(), SignupAuditEmitError> {
        let mut g = self
            .records
            .lock()
            .map_err(|e| SignupAuditEmitError::Rejected(format!("mutex poisoned: {e}")))?;
        g.push(record.clone());
        Ok(())
    }
}

/// Adversarial fixture sink that always rejects (forces orchestrator
/// fail-CLOSED behaviour in tests).
#[derive(Clone, Debug, Default)]
pub struct FailingSignupAuditSink;

impl SignupAuditSink for FailingSignupAuditSink {
    fn emit(&self, _record: &SignupAuditRecord) -> Result<(), SignupAuditEmitError> {
        Err(SignupAuditEmitError::Rejected(
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
    fn event_strings_canonical() {
        let expected = canonical_signup_audit_event_strings();
        let actual: Vec<&str> = [
            SignupAuditEventType::Started,
            SignupAuditEventType::Completed,
            SignupAuditEventType::Failed,
            SignupAuditEventType::Deferred,
        ]
        .iter()
        .map(|e| e.as_str())
        .collect();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn in_memory_sink_records_emit() {
        let sink = InMemorySignupAuditSink::new();
        assert!(sink.is_empty());
        let rec = SignupAuditRecord::new(
            SignupAuditEventType::Started,
            CorrelationId::new("cid-1"),
            "idem-1",
        );
        sink.emit(&rec).unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0], rec);
    }

    #[test]
    fn failing_sink_always_rejects() {
        let sink = FailingSignupAuditSink;
        let rec = SignupAuditRecord::new(
            SignupAuditEventType::Started,
            CorrelationId::new("cid-1"),
            "idem-1",
        );
        assert!(sink.emit(&rec).is_err());
    }

    #[test]
    fn record_builder_round_trip() {
        let rec = SignupAuditRecord::new(
            SignupAuditEventType::Failed,
            CorrelationId::new("cid-2"),
            "idem-2",
        )
        .with_region(PrimaryRegion::Sam)
        .with_step(OrchestrationStep::InsertTenant)
        .with_reason("FK violation");
        assert_eq!(rec.primary_region, Some(PrimaryRegion::Sam));
        assert_eq!(rec.step, Some(OrchestrationStep::InsertTenant));
        assert_eq!(rec.reason.as_deref(), Some("FK violation"));
    }
}
