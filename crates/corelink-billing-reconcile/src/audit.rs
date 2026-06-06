//! Audit-of-audit (meta-audit) trait + InMemory test sink for the
//! `corelink-billing-reconcile` orchestrator.
//!
//! Mirrors the `corelink-billing-stripe::audit` discipline: every
//! reconciliation decision arm fires its own audit event BEFORE state
//! mutation per the canonical Lote 10.6bis pattern + S-07 P1-1 fix. The
//! reconciliation worker is THE financial-integrity gate that detects
//! drift between WI-S10-001 raw events, WI-S10-002 aggregated counters,
//! and WI-S10-003 Stripe usage_records; a corrupted decision arm without
//! a meta-audit row would mean the SOC 2 CC1.4 + GAAP ASC 606 evidence
//! trail loses the canonical drift detection moment.
//!
//! WI-S10-004 §6.1 freezes the canonical 6-event taxonomy:
//!
//! - `corelink.billing_reconcile.run_started` — orchestrator pinned the
//!   run-start watermark BEFORE any layer query (mirrors WI-S10-002
//!   aggregator `run_started` discipline).
//! - `corelink.billing_reconcile.no_drift` — three layers within the
//!   canonical Quiet tolerance (`drift_pct < 0.01%`); forensic trail.
//! - `corelink.billing_reconcile.auto_fixed` — drift detected AND
//!   within the dual-condition gate (`record_count ≤ 5 AND drift_pct
//!   ≤ 0.01%`); auto-fix arm fired (Lote 10.6bis P0-6 inheritance).
//! - `corelink.billing_reconcile.ticket_filed` — `0.01% ≤ drift_pct <
//!   0.1%` SEV-3 monitor; drift-history ticket queued for Finance
//!   triage.
//! - `corelink.billing_reconcile.page_dispatched` — `0.1% ≤ drift_pct
//!   < 1%` SEV-2 page; PagerDuty `corelink-finance` + `corelink-sre`
//!   services dispatched.
//! - `corelink.billing_reconcile.stripe_paused` — `drift_pct ≥ 1%`
//!   SEV-1 page + Stripe-submission auto-pause (Layer 3 drift =
//!   customer-facing invoice may be wrong = legal exposure).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs can extend the
//! taxonomy additively without breaking downstream sinks.

use std::sync::Mutex;

use uuid::Uuid;

use crate::error::ReconcileAuditSinkError as ReconcileAuditEmitErrorInner;
use crate::event::{ReconcileDecision, ReconcileLayerKind};

/// Canonical billing-reconcile meta-audit taxonomy. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ReconcileAuditEventType {
    /// `corelink.billing_reconcile.run_started` — orchestrator pinned
    /// the run-start watermark BEFORE any layer query.
    RunStarted,
    /// `corelink.billing_reconcile.no_drift` — three layers within
    /// the canonical Quiet tolerance.
    NoDrift,
    /// `corelink.billing_reconcile.auto_fixed` — auto-fix dual-condition
    /// gate fired.
    AutoFixed,
    /// `corelink.billing_reconcile.ticket_filed` — SEV-3 monitor.
    TicketFiled,
    /// `corelink.billing_reconcile.page_dispatched` — SEV-2 page.
    PageDispatched,
    /// `corelink.billing_reconcile.stripe_paused` — SEV-1 page +
    /// Stripe auto-pause.
    StripePaused,
}

impl ReconcileAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStarted => "corelink.billing_reconcile.run_started",
            Self::NoDrift => "corelink.billing_reconcile.no_drift",
            Self::AutoFixed => "corelink.billing_reconcile.auto_fixed",
            Self::TicketFiled => "corelink.billing_reconcile.ticket_filed",
            Self::PageDispatched => "corelink.billing_reconcile.page_dispatched",
            Self::StripePaused => "corelink.billing_reconcile.stripe_paused",
        }
    }

    /// Whether this variant represents a SEV-1 alert per WI-S10-004
    /// §6.1.18 (Layer 3 drift OR hash-chain violation OR Layer 1/2
    /// query failure).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::StripePaused)
    }

    /// Whether this variant represents a SEV-2 alert.
    #[must_use]
    pub const fn is_sev2(self) -> bool {
        matches!(self, Self::PageDispatched)
    }

    /// Whether this variant represents a SEV-3 monitor.
    #[must_use]
    pub const fn is_sev3(self) -> bool {
        matches!(self, Self::TicketFiled)
    }
}

impl core::fmt::Display for ReconcileAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Map a [`ReconcileDecision`] arm to its canonical audit event type.
/// Pinned by the `prop_audit_emit_per_decision_arm` property test (every
/// decision arm fires exactly its canonical audit kind).
#[must_use]
pub const fn audit_event_for_decision(decision: ReconcileDecision) -> ReconcileAuditEventType {
    match decision {
        ReconcileDecision::NoDrift { .. } => ReconcileAuditEventType::NoDrift,
        ReconcileDecision::AutoFixed { .. } => ReconcileAuditEventType::AutoFixed,
        ReconcileDecision::TicketSev3 { .. } => ReconcileAuditEventType::TicketFiled,
        ReconcileDecision::PageSev2 { .. } => ReconcileAuditEventType::PageDispatched,
        ReconcileDecision::PageSev1AutoPaused { .. } => ReconcileAuditEventType::StripePaused,
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_reconcile_audit_event_strings() -> &'static [&'static str; 6] {
    &[
        "corelink.billing_reconcile.run_started",
        "corelink.billing_reconcile.no_drift",
        "corelink.billing_reconcile.auto_fixed",
        "corelink.billing_reconcile.ticket_filed",
        "corelink.billing_reconcile.page_dispatched",
        "corelink.billing_reconcile.stripe_paused",
    ]
}

/// Typed billing-reconcile meta-audit record. Production wiring
/// serializes via a CloudEvents 1.0 envelope (mirrors the audit-chain
/// S-09 inheritance); the trait surface accepts the typed shape so the
/// sink + the envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileAuditRecord {
    /// Canonical event type.
    pub event_type: ReconcileAuditEventType,
    /// Tenant id of the reconciliation run (`None` only for global
    /// run-started arms in production wiring; the orchestrator here
    /// always populates it).
    pub tenant_id: Option<Uuid>,
    /// Canonical billing period (`YYYY-MM`).
    pub billing_period: String,
    /// Layer where the maximum drift was observed (the "primary"
    /// drift signal). `None` for `RunStarted` + `NoDrift` arms.
    pub primary_layer: Option<ReconcileLayerKind>,
    /// Producer-side wall-clock instant (Unix epoch ms; canonical
    /// `_ms` suffix per Lote 10.7bis P0-3).
    pub now_ms: u64,
    /// Free-form context (e.g. drift_pct value, Stripe-pause ack
    /// status). Reserved for adversarial debug + dashboard widget
    /// grouping.
    pub context: String,
}

/// Audit-of-audit emit error alias (lifted from
/// [`crate::error::ReconcileAuditSinkError`]).
pub use crate::error::ReconcileAuditSinkError as ReconcileAuditEmitError;

/// Audit-of-audit sink trait. Production wiring composes:
///
/// - `OutboxReconcileAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the drift-history INSERT;  fail-CLOSED envelope per
///   Lote 10.6bis pattern + S-07 P1-1 fix.
/// - `MultiplexReconcileAuditSink` — fan-out to direct SIEM in
///   addition to the outbox.
pub trait ReconcileAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// abort the run fail-CLOSED at THIS trait surface (per WI-S10-004
    /// §1 invariant 7 — the reconciliation worker is fail-CLOSED at
    /// the integrity layer; no fail-OPEN policy override).
    ///
    /// # Errors
    ///
    /// Returns [`ReconcileAuditEmitError::Store`] on any backend
    /// failure.
    fn emit(&self, record: ReconcileAuditRecord) -> Result<(), ReconcileAuditEmitErrorInner>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryReconcileAuditSink {
    inner: std::sync::Arc<Mutex<Vec<ReconcileAuditRecord>>>,
}

impl InMemoryReconcileAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ReconcileAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: ReconcileAuditEventType) -> Vec<ReconcileAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl ReconcileAuditSink for InMemoryReconcileAuditSink {
    fn emit(&self, record: ReconcileAuditRecord) -> Result<(), ReconcileAuditEmitErrorInner> {
        let mut guard = self.inner.lock().map_err(|_| {
            ReconcileAuditEmitErrorInner::Store(
                "billing-reconcile audit sink mutex poisoned".to_string(),
            )
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingReconcileAuditSink;

impl FailingReconcileAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ReconcileAuditSink for FailingReconcileAuditSink {
    fn emit(&self, _record: ReconcileAuditRecord) -> Result<(), ReconcileAuditEmitErrorInner> {
        Err(ReconcileAuditEmitErrorInner::Store(
            "induced billing-reconcile audit sink failure (test fixture)".to_string(),
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

    fn rec(t: ReconcileAuditEventType) -> ReconcileAuditRecord {
        ReconcileAuditRecord {
            event_type: t,
            tenant_id: Some(Uuid::now_v7()),
            billing_period: "2026-05".to_string(),
            primary_layer: None,
            now_ms: 1,
            context: "test".to_string(),
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            ReconcileAuditEventType::RunStarted,
            ReconcileAuditEventType::NoDrift,
            ReconcileAuditEventType::AutoFixed,
            ReconcileAuditEventType::TicketFiled,
            ReconcileAuditEventType::PageDispatched,
            ReconcileAuditEventType::StripePaused,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.billing_reconcile."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_reconcile_audit_event_strings();
        assert_eq!(canonical.len(), 6);
        for s in canonical {
            assert!(s.starts_with("corelink.billing_reconcile."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(ReconcileAuditEventType::StripePaused.is_sev1());
        assert!(ReconcileAuditEventType::PageDispatched.is_sev2());
        assert!(ReconcileAuditEventType::TicketFiled.is_sev3());
        assert!(!ReconcileAuditEventType::NoDrift.is_sev1());
        assert!(!ReconcileAuditEventType::NoDrift.is_sev2());
        assert!(!ReconcileAuditEventType::NoDrift.is_sev3());
        assert!(!ReconcileAuditEventType::AutoFixed.is_sev1());
        assert!(!ReconcileAuditEventType::AutoFixed.is_sev2());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryReconcileAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(ReconcileAuditEventType::RunStarted)).unwrap();
        sink.emit(rec(ReconcileAuditEventType::NoDrift)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(ReconcileAuditEventType::RunStarted).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingReconcileAuditSink::new();
        let err = sink
            .emit(rec(ReconcileAuditEventType::RunStarted))
            .unwrap_err();
        let store = matches!(err, ReconcileAuditEmitErrorInner::Store(_));
        assert!(store);
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryReconcileAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(ReconcileAuditEventType::RunStarted)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", ReconcileAuditEventType::StripePaused),
            "corelink.billing_reconcile.stripe_paused"
        );
    }

    #[test]
    fn audit_event_for_decision_pins_arm_mapping() {
        assert_eq!(
            audit_event_for_decision(ReconcileDecision::NoDrift { max_drift_pct: 0.0 }),
            ReconcileAuditEventType::NoDrift
        );
        assert_eq!(
            audit_event_for_decision(ReconcileDecision::AutoFixed {
                max_drift_pct: 0.0,
                drift_record_count: 1
            }),
            ReconcileAuditEventType::AutoFixed
        );
        assert_eq!(
            audit_event_for_decision(ReconcileDecision::TicketSev3 {
                max_drift_pct: 0.0005,
                primary_layer: ReconcileLayerKind::Layer1Emit,
            }),
            ReconcileAuditEventType::TicketFiled
        );
        assert_eq!(
            audit_event_for_decision(ReconcileDecision::PageSev2 {
                max_drift_pct: 0.005,
                primary_layer: ReconcileLayerKind::Layer2Aggregate,
            }),
            ReconcileAuditEventType::PageDispatched
        );
        assert_eq!(
            audit_event_for_decision(ReconcileDecision::PageSev1AutoPaused {
                max_drift_pct: 0.05,
                primary_layer: ReconcileLayerKind::Layer3Stripe,
                pause_acked: true,
            }),
            ReconcileAuditEventType::StripePaused
        );
    }
}
