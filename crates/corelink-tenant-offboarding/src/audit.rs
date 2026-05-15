//! Tenant-offboarding audit sink trait + InMemory test sink.
//!
//! Mirrors the `corelink-dsr::audit` discipline: every state
//! transition fires its own audit row BEFORE the durable store
//! mutation per ADR-S11-002 split-tier fail-CLOSED pattern.
//!
//! Canonical 6-event taxonomy:
//!
//! - `corelink.tenant.offboarding.cancel_requested` — ACTIVE →
//!   CANCEL_REQUESTED (customer click + anti-fraud verified).
//! - `corelink.tenant.offboarding.grace_started` — CANCEL_REQUESTED
//!   → GRACE_PERIOD (T+1 timer).
//! - `corelink.tenant.offboarding.read_only_entered` —
//!   GRACE_PERIOD → READ_ONLY (T+30 timer).
//! - `corelink.tenant.offboarding.suspended_entered` — READ_ONLY →
//!   SUSPENDED (T+45 timer).
//! - `corelink.tenant.offboarding.restored` — any of
//!   CANCEL_REQUESTED / GRACE_PERIOD / READ_ONLY → ACTIVE
//!   (customer self-service revert OR ops force-revert).
//! - `corelink.tenant.offboarding.erased` — SUSPENDED → ERASED
//!   (admin commit; cryptographic erasure + R2/D1/KV cleanup).

use std::sync::Mutex;

use crate::error::TenantOffboardingAuditSinkError;
use crate::state::{TenantOffboardingState, TransitionTrigger};

/// Canonical 6-event tenant-offboarding audit taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TenantOffboardingAuditEventType {
    /// `corelink.tenant.offboarding.cancel_requested`.
    CancelRequested,
    /// `corelink.tenant.offboarding.grace_started`.
    GraceStarted,
    /// `corelink.tenant.offboarding.read_only_entered`.
    ReadOnlyEntered,
    /// `corelink.tenant.offboarding.suspended_entered`.
    SuspendedEntered,
    /// `corelink.tenant.offboarding.restored`.
    Restored,
    /// `corelink.tenant.offboarding.erased`.
    Erased,
}

impl TenantOffboardingAuditEventType {
    /// Canonical CloudEvents `type` attribute string. Pinned for D1
    /// CHECK constraints + audit-chain extension.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CancelRequested => "corelink.tenant.offboarding.cancel_requested",
            Self::GraceStarted => "corelink.tenant.offboarding.grace_started",
            Self::ReadOnlyEntered => "corelink.tenant.offboarding.read_only_entered",
            Self::SuspendedEntered => "corelink.tenant.offboarding.suspended_entered",
            Self::Restored => "corelink.tenant.offboarding.restored",
            Self::Erased => "corelink.tenant.offboarding.erased",
        }
    }

    /// Resolve the canonical audit event type for a destination
    /// state landed via the canonical state machine. Used by the
    /// orchestrator to emit the correct row.
    #[must_use]
    pub const fn for_destination(
        to: TenantOffboardingState,
    ) -> Option<TenantOffboardingAuditEventType> {
        match to {
            TenantOffboardingState::CancelRequested => Some(Self::CancelRequested),
            TenantOffboardingState::GracePeriod => Some(Self::GraceStarted),
            TenantOffboardingState::ReadOnly => Some(Self::ReadOnlyEntered),
            TenantOffboardingState::Suspended => Some(Self::SuspendedEntered),
            TenantOffboardingState::Erased => Some(Self::Erased),
            // ACTIVE as a destination implies a Restored transition.
            TenantOffboardingState::Active => Some(Self::Restored),
        }
    }
}

/// Canonical audit record persisted by the sink. Mirrors the
/// `DsrAuditRecord` discipline (event_type + tenant_id +
/// occurred_at_ms + payload).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantOffboardingAuditRecord {
    /// The canonical event type.
    pub event_type: TenantOffboardingAuditEventType,
    /// Tenant id (opaque string; production wiring uses ULID).
    pub tenant_id: String,
    /// Source state at the moment of the transition.
    pub from: TenantOffboardingState,
    /// Destination state at the moment of the transition.
    pub to: TenantOffboardingState,
    /// Trigger that drove the transition.
    pub trigger: TransitionTrigger,
    /// Wall-clock occurred-at timestamp (ms since Unix epoch).
    pub occurred_at_ms: i64,
    /// Optional operator identifier (set when `trigger` is
    /// `OpsForced` or `AdminCommitErasure`; production wiring
    /// validates RBAC at the route layer).
    pub operator_id: Option<String>,
}

/// Audit sink trait. Production wiring at PRR ship gate binds this
/// to the canonical S-09 audit-chain extension (every transition is
/// a separate `ChainEvent` per `INV-OBS-AUDIT-CHAIN-INTEGRITY`).
pub trait TenantOffboardingAuditSink: Send + Sync + core::fmt::Debug {
    /// Emit a single audit record. MUST be called BEFORE the
    /// durable store mutation per the fail-CLOSED envelope
    /// (ADR-S11-002 split-tier). Returning `Err` aborts the
    /// transition; the durable store remains at its pre-call state.
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingAuditSinkError`] if the underlying
    /// backend (audit chain / SIEM / outbox) rejects the append.
    fn emit(
        &self,
        record: TenantOffboardingAuditRecord,
    ) -> Result<(), TenantOffboardingAuditSinkError>;
}

/// In-memory audit sink for tests. Captures every emitted record in
/// a `Mutex<Vec<>>` so tests can assert audit-completeness.
#[derive(Debug, Default)]
pub struct InMemoryTenantOffboardingAuditSink {
    captured: Mutex<Vec<TenantOffboardingAuditRecord>>,
}

impl InMemoryTenantOffboardingAuditSink {
    /// Fresh empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of every record captured so far (in emit order).
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingAuditSinkError::Store`] if the
    /// per-instance mutex is poisoned (panicking emit elsewhere).
    pub fn snapshot(
        &self,
    ) -> Result<Vec<TenantOffboardingAuditRecord>, TenantOffboardingAuditSinkError> {
        let guard = self.captured.lock().map_err(|_| {
            TenantOffboardingAuditSinkError::Store("audit sink mutex poisoned".to_string())
        })?;
        Ok(guard.clone())
    }

    /// Number of records captured so far.
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingAuditSinkError::Store`] if the
    /// per-instance mutex is poisoned.
    pub fn len(&self) -> Result<usize, TenantOffboardingAuditSinkError> {
        let guard = self.captured.lock().map_err(|_| {
            TenantOffboardingAuditSinkError::Store("audit sink mutex poisoned".to_string())
        })?;
        Ok(guard.len())
    }

    /// Whether no records have been captured.
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingAuditSinkError::Store`] if the
    /// per-instance mutex is poisoned.
    pub fn is_empty(&self) -> Result<bool, TenantOffboardingAuditSinkError> {
        Ok(self.len()? == 0)
    }
}

impl TenantOffboardingAuditSink for InMemoryTenantOffboardingAuditSink {
    fn emit(
        &self,
        record: TenantOffboardingAuditRecord,
    ) -> Result<(), TenantOffboardingAuditSinkError> {
        let mut guard = self.captured.lock().map_err(|_| {
            TenantOffboardingAuditSinkError::Store("audit sink mutex poisoned".to_string())
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing audit sink. Used by tests to force the
/// fail-CLOSED envelope.
#[derive(Debug, Default)]
pub struct FailingTenantOffboardingAuditSink;

impl FailingTenantOffboardingAuditSink {
    /// Fresh sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TenantOffboardingAuditSink for FailingTenantOffboardingAuditSink {
    fn emit(
        &self,
        _record: TenantOffboardingAuditRecord,
    ) -> Result<(), TenantOffboardingAuditSinkError> {
        Err(TenantOffboardingAuditSinkError::Store(
            "induced audit failure (fail-CLOSED test scaffold)".to_string(),
        ))
    }
}

/// Canonical CloudEvents `type` strings for every variant. Pinned
/// for cross-component regression tests.
#[must_use]
pub const fn canonical_tenant_offboarding_audit_event_strings() -> [&'static str; 6] {
    [
        "corelink.tenant.offboarding.cancel_requested",
        "corelink.tenant.offboarding.grace_started",
        "corelink.tenant.offboarding.read_only_entered",
        "corelink.tenant.offboarding.suspended_entered",
        "corelink.tenant.offboarding.restored",
        "corelink.tenant.offboarding.erased",
    ]
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

    fn rec() -> TenantOffboardingAuditRecord {
        TenantOffboardingAuditRecord {
            event_type: TenantOffboardingAuditEventType::CancelRequested,
            tenant_id: "tenant-a".to_string(),
            from: TenantOffboardingState::Active,
            to: TenantOffboardingState::CancelRequested,
            trigger: TransitionTrigger::CustomerInitiated,
            occurred_at_ms: 1,
            operator_id: None,
        }
    }

    #[test]
    fn in_memory_sink_captures_emit() {
        let sink = InMemoryTenantOffboardingAuditSink::new();
        sink.emit(rec()).unwrap();
        let snap = sink.snapshot().unwrap();
        assert_eq!(snap.len(), 1);
        assert_eq!(
            snap[0].event_type,
            TenantOffboardingAuditEventType::CancelRequested
        );
    }

    #[test]
    fn failing_sink_errors() {
        let sink = FailingTenantOffboardingAuditSink::new();
        let r = sink.emit(rec());
        assert!(r.is_err());
    }

    #[test]
    fn event_type_strings_are_distinct() {
        let s = canonical_tenant_offboarding_audit_event_strings();
        let unique: std::collections::HashSet<&str> = s.iter().copied().collect();
        assert_eq!(unique.len(), s.len());
    }

    #[test]
    fn for_destination_covers_every_state() {
        // Every reachable destination state has a canonical audit
        // event; this is a structural invariant the orchestrator
        // relies on.
        use TenantOffboardingState as S;
        for s in [
            S::Active,
            S::CancelRequested,
            S::GracePeriod,
            S::ReadOnly,
            S::Suspended,
            S::Erased,
        ] {
            assert!(
                TenantOffboardingAuditEventType::for_destination(s).is_some(),
                "state {s} must have a canonical audit event"
            );
        }
    }
}
