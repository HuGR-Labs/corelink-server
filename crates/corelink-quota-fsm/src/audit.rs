//! Audit-of-audit (meta-audit) trait + InMemory test sink for the
//! `corelink-quota-fsm` orchestrator.
//!
//! Mirrors the `corelink-billing-reconcile::audit` discipline: every
//! state-mutating transition fires its own audit event BEFORE the state
//! mutation per the canonical Lote 10.6bis pattern + S-07 P1-1 fix. The
//! quota state machine is THE customer-cost-protection contract that
//! drives the hot-path 429 hard-block + the operator-driven suspension
//! arm; a corrupted transition without a meta-audit row would mean the
//! SOC 2 CC1.4 + GAAP ASC 606 evidence trail loses the canonical
//! transition moment + the customer dispute trail is broken.
//!
//! WI-S10-005 §6.1 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.billing_quota.state_changed` — utilization-derived
//!   transition between any two of `WithinPlan / SoftWarning80pct /
//!   SoftWarning95pct / OverQuota100pct` (NOT the suspension/reinstate
//!   arms — those have their own typed audit kinds).
//! - `corelink.billing_quota.overage_telemetry_recorded` — fired AT
//!   THE 80pct AND 95pct entries; the canonical signal that S-13
//!   admin/notifications consumer reads to dispatch email + in-app
//!   notification (per the WI brief: actual email send is REJECTED in
//!   S-10 and DEFERRED to S-13; THIS WI emits the telemetry contract
//!   only). Mirrors WI-S10-001 hot-path-emit pattern (event lands in
//!   the audit chain; consumer reads + acts).
//! - `corelink.billing_quota.suspended` — terminal-arm transition into
//!   `SuspendedForNonPayment` after the per-tenant invoice-failure
//!   counter reaches the canonical threshold (default 3 per WI brief +
//!   sprint contract §15 R-009 abuse detection inheritance).
//! - `corelink.billing_quota.reinstated` — operator-driven reinstatement
//!   from `SuspendedForNonPayment` back to the utilization-derived
//!   state. Authorization gate (the `billing_admin` role check) lives
//!   at the production wiring's Tower middleware, NOT here — this
//!   crate ships the state-machine contract; the role enforcement is
//!   the caller's responsibility per the trait surface contract.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs can extend the
//! taxonomy additively without breaking downstream sinks.

use std::sync::Mutex;

use uuid::Uuid;

use crate::error::QuotaFsmAuditSinkError;
use crate::event::{QuotaState, QuotaTransition};

/// Canonical quota-fsm meta-audit taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum QuotaAuditEventType {
    /// `corelink.billing_quota.state_changed` — utilization-derived
    /// transition between any two utilization-bucket states.
    StateChanged,
    /// `corelink.billing_quota.overage_telemetry_recorded` — fired at
    /// 80pct + 95pct entries. The S-13 admin/notifications consumer
    /// reads this and dispatches the customer-facing email + in-app
    /// notification (email send DEFERRED per ADR-0020 FROZEN +
    /// orchestrator brief).
    OverageTelemetryRecorded,
    /// `corelink.billing_quota.suspended` — transition into terminal
    /// `SuspendedForNonPayment` arm.
    Suspended,
    /// `corelink.billing_quota.reinstated` — operator-driven
    /// reinstatement from `SuspendedForNonPayment`. The authorization
    /// gate is enforced upstream at the production wiring's Tower
    /// middleware (`billing_admin` role per CTRL-AUTHZ-001 +
    /// CTRL-AUTHZ-002).
    Reinstated,
}

impl QuotaAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StateChanged => "corelink.billing_quota.state_changed",
            Self::OverageTelemetryRecorded => {
                "corelink.billing_quota.overage_telemetry_recorded"
            }
            Self::Suspended => "corelink.billing_quota.suspended",
            Self::Reinstated => "corelink.billing_quota.reinstated",
        }
    }

    /// Whether this variant represents a SEV-1 alert per WI-S10-005
    /// §6.1: the canonical SEV-1 arm is `Suspended` (terminal customer
    /// service degradation = legal exposure if wrong tier hit + Finance
    /// + Legal page).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::Suspended)
    }
}

impl core::fmt::Display for QuotaAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_quota_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.billing_quota.state_changed",
        "corelink.billing_quota.overage_telemetry_recorded",
        "corelink.billing_quota.suspended",
        "corelink.billing_quota.reinstated",
    ]
}

/// Typed quota-fsm meta-audit record. Production wiring serializes via
/// a CloudEvents 1.0 envelope (mirrors the audit-chain S-09 inheritance);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
///
/// Why typed transition payload (NOT `serde_json::Value`): per Lote
/// 10.9-quinquies NEW-P0-2 — the audit envelope written to the S-09 7y
/// archive must be byte-deterministic; an untyped value defeats
/// compile-time taxonomy enforcement + risks PII leak through the
/// permissive payload (e.g. customer email surfacing through a generic
/// JSON field). The typed [`crate::event::QuotaTransition`] enum is the
/// canonical absorption pattern (mirrors WI-S10-004
/// `ReconcileDecision`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaAuditRecord {
    /// Canonical event type.
    pub event_type: QuotaAuditEventType,
    /// Tenant id of the quota transition (every event in this crate is
    /// per-tenant scoped; the production wiring's TenantCtx middleware
    /// pins the id at request admission per S-03 inheritance).
    pub tenant_id: Uuid,
    /// State observed BEFORE the transition (when applicable; `None`
    /// for the genesis `state_changed` arm where the per-tenant row
    /// is being created).
    pub from_state: Option<QuotaState>,
    /// State observed AFTER the transition (or, for telemetry/suspension
    /// arms, the state being entered).
    pub to_state: QuotaState,
    /// Producer-side wall-clock instant (Unix epoch ms; canonical
    /// `_at_ms` semantics per Lote 10.7bis P0-3 — note that the column
    /// itself uses no `_ms` suffix at the storage layer).
    pub now_ms: u64,
    /// Free-form context (e.g. utilization percentage value, invoice
    /// failure count). Reserved for adversarial debug + dashboard
    /// widget grouping.
    pub context: String,
}

/// Map a [`QuotaTransition`] arm to its canonical audit event type.
/// Pinned by the `prop_audit_emit_per_decision_arm` property test
/// (every state-mutating arm fires exactly its canonical audit kind;
/// the `NoChange` arm fires NO audit by construction at the orchestrator
/// boundary).
///
/// Returns `None` for the `NoChange` arm (no audit row by design — the
/// idempotent rerun is silent at the audit-of-audit layer).
///
/// Note that the orchestrator additionally emits the canonical
/// `OverageTelemetryRecorded` audit on entries into 80pct + 95pct (the
/// telemetry contract that S-13 admin/notifications consumer subscribes
/// to). This canonical mapping handles the state-mutation audit only;
/// the orchestrator wires both audits at the entry into the warning
/// bands.
#[must_use]
pub const fn audit_event_for_transition(
    transition: QuotaTransition,
) -> Option<QuotaAuditEventType> {
    match transition {
        QuotaTransition::NoChange { .. } => None,
        QuotaTransition::TransitionedTo80pct { .. }
        | QuotaTransition::TransitionedTo95pct { .. }
        | QuotaTransition::TransitionedTo100pct { .. } => {
            Some(QuotaAuditEventType::StateChanged)
        }
        QuotaTransition::Suspended { .. } => Some(QuotaAuditEventType::Suspended),
        QuotaTransition::Reinstated { .. } => Some(QuotaAuditEventType::Reinstated),
    }
}

/// Whether a given transition fires the canonical
/// `OverageTelemetryRecorded` audit (entries into 80pct + 95pct
/// warning bands).
#[must_use]
pub const fn transition_emits_overage_telemetry(transition: QuotaTransition) -> bool {
    matches!(
        transition,
        QuotaTransition::TransitionedTo80pct { .. }
            | QuotaTransition::TransitionedTo95pct { .. }
    )
}

/// Audit-of-audit sink trait. Production wiring composes:
///
/// - `OutboxQuotaAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the state mutation; fail-CLOSED envelope per
///   Lote 10.6bis pattern + S-07 P1-1 fix.
/// - `MultiplexQuotaAuditSink` — fan-out to the canonical S-09
///   audit-chain CloudEvents v1.0 sink in addition to the outbox.
pub trait QuotaAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to abort
    /// the transition fail-CLOSED at THIS trait surface (per WI-S10-005
    /// §1 invariant 8 — the transition surface is fail-CLOSED at the
    /// integrity layer; the hot-path is_hard_blocked() query is the
    /// counter-pattern fail-OPEN + lives at the Tower-layer boundary
    /// at WI-S10-007).
    ///
    /// # Errors
    ///
    /// Returns [`QuotaFsmAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: QuotaAuditRecord) -> Result<(), QuotaFsmAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryQuotaAuditSink {
    inner: std::sync::Arc<Mutex<Vec<QuotaAuditRecord>>>,
}

impl InMemoryQuotaAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<QuotaAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: QuotaAuditEventType) -> Vec<QuotaAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl QuotaAuditSink for InMemoryQuotaAuditSink {
    fn emit(&self, record: QuotaAuditRecord) -> Result<(), QuotaFsmAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            QuotaFsmAuditSinkError::Store(
                "quota-fsm audit sink mutex poisoned".to_string(),
            )
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingQuotaAuditSink;

impl FailingQuotaAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl QuotaAuditSink for FailingQuotaAuditSink {
    fn emit(&self, _record: QuotaAuditRecord) -> Result<(), QuotaFsmAuditSinkError> {
        Err(QuotaFsmAuditSinkError::Store(
            "induced quota-fsm audit sink failure (test fixture)".to_string(),
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

    fn rec(t: QuotaAuditEventType) -> QuotaAuditRecord {
        QuotaAuditRecord {
            event_type: t,
            tenant_id: Uuid::now_v7(),
            from_state: None,
            to_state: QuotaState::WithinPlan,
            now_ms: 1,
            context: "test".to_string(),
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            QuotaAuditEventType::StateChanged,
            QuotaAuditEventType::OverageTelemetryRecorded,
            QuotaAuditEventType::Suspended,
            QuotaAuditEventType::Reinstated,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.billing_quota."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_quota_audit_event_strings();
        assert_eq!(canonical.len(), 4);
        for s in canonical {
            assert!(s.starts_with("corelink.billing_quota."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(QuotaAuditEventType::Suspended.is_sev1());
        assert!(!QuotaAuditEventType::StateChanged.is_sev1());
        assert!(!QuotaAuditEventType::OverageTelemetryRecorded.is_sev1());
        assert!(!QuotaAuditEventType::Reinstated.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryQuotaAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(QuotaAuditEventType::StateChanged)).unwrap();
        sink.emit(rec(QuotaAuditEventType::OverageTelemetryRecorded))
            .unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(QuotaAuditEventType::StateChanged).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingQuotaAuditSink::new();
        let err = sink
            .emit(rec(QuotaAuditEventType::StateChanged))
            .unwrap_err();
        let store = matches!(err, QuotaFsmAuditSinkError::Store(_));
        assert!(store);
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryQuotaAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(QuotaAuditEventType::StateChanged)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", QuotaAuditEventType::Suspended),
            "corelink.billing_quota.suspended"
        );
    }

    #[test]
    fn audit_event_for_transition_pins_arm_mapping() {
        assert_eq!(
            audit_event_for_transition(QuotaTransition::NoChange {
                state: QuotaState::WithinPlan
            }),
            None
        );
        assert_eq!(
            audit_event_for_transition(QuotaTransition::TransitionedTo80pct {
                from: QuotaState::WithinPlan
            }),
            Some(QuotaAuditEventType::StateChanged)
        );
        assert_eq!(
            audit_event_for_transition(QuotaTransition::TransitionedTo95pct {
                from: QuotaState::SoftWarning80pct
            }),
            Some(QuotaAuditEventType::StateChanged)
        );
        assert_eq!(
            audit_event_for_transition(QuotaTransition::TransitionedTo100pct {
                from: QuotaState::SoftWarning95pct
            }),
            Some(QuotaAuditEventType::StateChanged)
        );
        assert_eq!(
            audit_event_for_transition(QuotaTransition::Suspended {
                invoice_failures: 3
            }),
            Some(QuotaAuditEventType::Suspended)
        );
        assert_eq!(
            audit_event_for_transition(QuotaTransition::Reinstated {
                new_state: QuotaState::WithinPlan
            }),
            Some(QuotaAuditEventType::Reinstated)
        );
    }

    #[test]
    fn transition_emits_overage_telemetry_on_80_and_95_only() {
        assert!(transition_emits_overage_telemetry(
            QuotaTransition::TransitionedTo80pct {
                from: QuotaState::WithinPlan
            }
        ));
        assert!(transition_emits_overage_telemetry(
            QuotaTransition::TransitionedTo95pct {
                from: QuotaState::SoftWarning80pct
            }
        ));
        assert!(!transition_emits_overage_telemetry(
            QuotaTransition::TransitionedTo100pct {
                from: QuotaState::SoftWarning95pct
            }
        ));
        assert!(!transition_emits_overage_telemetry(
            QuotaTransition::NoChange {
                state: QuotaState::WithinPlan
            }
        ));
        assert!(!transition_emits_overage_telemetry(
            QuotaTransition::Suspended {
                invoice_failures: 3
            }
        ));
    }
}
