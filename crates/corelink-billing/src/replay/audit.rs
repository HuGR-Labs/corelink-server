//! Audit-of-audit (meta-audit) trait + InMemory test sink for the
//! `corelink-billing-replay` orchestrator.
//!
//! Mirrors the `corelink-billing-reconcile::audit` discipline byte-for-byte:
//! every replay decision arm fires its own audit event BEFORE state
//! mutation per the canonical Lote 10.6bis pattern + S-07 P1-1 fix. The
//! replay forensic engine is THE audit-grade dispute-resolution
//! primitive; a corrupted decision arm without a meta-audit row would
//! mean the SOC 2 CC1.4 + GAAP ASC 606 + GDPR Art. 22 evidence trail
//! loses the canonical replay request moment.
//!
//! WI-S10-006 §6.1.5 freezes the canonical 5-event taxonomy:
//!
//! - `corelink.billing_replay.request_authorized` — orchestrator
//!   accepted the canonical `billing_forensics_admin` role check; the
//!   replay decision arm fires next.
//! - `corelink.billing_replay.request_denied` — orchestrator rejected
//!   the request: the presented role is not `billing_forensics_admin`.
//!   HTTP 403 surface.
//! - `corelink.billing_replay.dry_run_planned` — orchestrator computed
//!   the dry-run plan (no state mutation past this audit row); the
//!   canonical [`super::event::ReplayDecision::DryRunPlan`] arm.
//! - `corelink.billing_replay.executed` — orchestrator ran the canonical
//!   replay pipeline end-to-end. Idempotent re-submission of the same
//!   `request_id` short-circuits to `request_authorized` with
//!   `idempotent_replay = true`; only the FIRST submission lands an
//!   `executed` row.
//! - `corelink.billing_replay.layer_diverged` — the reconstructed
//!   layers diverged from the production reference values. Fires
//!   ALONGSIDE the `executed` row (NOT instead of it) so the auditor
//!   evidence trail captures both the execution event + the divergence
//!   anomaly as separate rows.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs can extend the
//! taxonomy additively without breaking downstream sinks.

use std::sync::Mutex;

use uuid::Uuid;

use super::error::ReplayAuditSinkError as ReplayAuditEmitErrorInner;
use super::event::{LayerDriftSummary, ReplayDecision, ReplayReason};

/// Canonical billing-replay meta-audit taxonomy. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs (e.g. `legal_discovery_request` for WI-S10-007 PRR ship gate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ReplayAuditEventType {
    /// `corelink.billing_replay.request_authorized` — role check
    /// passed.
    RequestAuthorized,
    /// `corelink.billing_replay.request_denied` — role check
    /// rejected; HTTP 403 surface.
    RequestDenied,
    /// `corelink.billing_replay.dry_run_planned` — dry-run-arm plan
    /// computed; no state mutation past this audit row.
    DryRunPlanned,
    /// `corelink.billing_replay.executed` — canonical 3-layer replay
    /// pipeline ran end-to-end.
    Executed,
    /// `corelink.billing_replay.layer_diverged` — reconstructed
    /// layers diverged from the production reference values; SEV-2
    /// forensic anomaly (the divergence itself is informational
    /// because the canonical reconciliation worker WI-S10-004 already
    /// SEV-1-pages on `layer3_diverged`; replay-side divergence is
    /// supplementary forensic evidence).
    LayerDiverged,
}

impl ReplayAuditEventType {
    /// Canonical CloudEvents `type` attribute string. Pinned for D1
    /// CHECK constraints + dashboard widget grouping +
    /// cross-component regression tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestAuthorized => "corelink.billing_replay.request_authorized",
            Self::RequestDenied => "corelink.billing_replay.request_denied",
            Self::DryRunPlanned => "corelink.billing_replay.dry_run_planned",
            Self::Executed => "corelink.billing_replay.executed",
            Self::LayerDiverged => "corelink.billing_replay.layer_diverged",
        }
    }

    /// Whether this variant represents a SEV-1 alert. Replay-side
    /// audits are NEVER SEV-1: the canonical SEV-1 surface is
    /// reconciliation worker WI-S10-004 (`stripe_paused`). Replay is
    /// the forensic follow-up; informational only.
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        false
    }

    /// Whether this variant represents a SEV-2 alert. Only
    /// `layer_diverged` lands a SEV-2 (forensic anomaly:
    /// reconstruction differs from production reference; root-cause
    /// analysis required).
    #[must_use]
    pub const fn is_sev2(self) -> bool {
        matches!(self, Self::LayerDiverged)
    }

    /// Whether this variant represents a SEV-3 monitor. The denied
    /// arm is SEV-3 (informational — repeated denials may indicate
    /// brute-force or operator misconfiguration; the canonical
    /// PagerDuty `corelink-security` service consumes the SEV-3 fan-
    /// out).
    #[must_use]
    pub const fn is_sev3(self) -> bool {
        matches!(self, Self::RequestDenied)
    }
}

impl core::fmt::Display for ReplayAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Map a [`ReplayDecision`] arm to its canonical primary audit event
/// type. A decision arm may emit MULTIPLE audit events (e.g.
/// `Executed { layer_diverged: true }` emits both `executed` AND
/// `layer_diverged` per the canonical orchestrator pipeline); this
/// helper returns the PRIMARY event type for the arm.
#[must_use]
pub const fn audit_event_for_decision(decision: &ReplayDecision) -> ReplayAuditEventType {
    match decision {
        ReplayDecision::Authorized { .. } => ReplayAuditEventType::RequestAuthorized,
        ReplayDecision::Denied403 { .. } => ReplayAuditEventType::RequestDenied,
        ReplayDecision::DryRunPlan { .. } => ReplayAuditEventType::DryRunPlanned,
        ReplayDecision::Executed { .. } => ReplayAuditEventType::Executed,
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_replay_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.billing_replay.request_authorized",
        "corelink.billing_replay.request_denied",
        "corelink.billing_replay.dry_run_planned",
        "corelink.billing_replay.executed",
        "corelink.billing_replay.layer_diverged",
    ]
}

/// Typed billing-replay meta-audit record. Production wiring serializes
/// via a CloudEvents 1.0 envelope (mirrors the audit-chain S-09
/// inheritance + the WI-S10-001 emit surface); the trait surface
/// accepts the typed shape so the sink + the envelope serializer share
/// an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayAuditRecord {
    /// Canonical event type.
    pub event_type: ReplayAuditEventType,
    /// Canonical UUIDv7 request id (the idempotency ledger key).
    pub request_id: Uuid,
    /// Tenant id of the canonical (tenant, billing_period) replay
    /// scope. Per Lote 10.4bis: tenant id MUST come from the
    /// authenticated middleware (NOT the request body).
    pub tenant_id: Uuid,
    /// Admin pubkey of the requesting principal (the canonical
    /// `requested_by` slot from the [`super::event::ReplayRequest`]).
    pub requested_by: Uuid,
    /// Canonical replay reason taxonomy (the canonical reason slot
    /// from the [`super::event::ReplayRequest`]).
    pub reason: ReplayReason,
    /// Optional layer-drift summary (populated only on the
    /// `LayerDiverged` arm; `None` on the other arms).
    pub layer_drift_summary: Option<LayerDriftSummary>,
    /// Canonical billing period (`YYYY-MM`).
    pub billing_period: String,
    /// Producer-side wall-clock instant (Unix epoch ms; canonical no
    /// `_ms` suffix per Lote 10.7bis P0-3).
    pub now_ms: u64,
    /// Free-form context (e.g. presented_role, idempotent_replay
    /// flag, layers_planned). Reserved for adversarial debug +
    /// dashboard widget grouping.
    pub context: String,
}

/// Audit-of-audit emit error alias (lifted from
/// [`super::error::ReplayAuditSinkError`]).
pub use super::error::ReplayAuditSinkError as ReplayAuditEmitError;

/// Audit-of-audit sink trait. Production wiring composes:
///
/// - `S09ChainReplayAuditSink` — canonical S-09 audit-chain INSERT
///   into the append-only ledger; tamper-detectable per S-09
///   INV-OBS-AUDIT-CHAIN-INTEGRITY.
/// - `MultiplexReplayAuditSink` — fan-out to direct SIEM in addition
///   to the chain.
pub trait ReplayAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// abort the replay fail-CLOSED at THIS trait surface (per
    /// WI-S10-006 §1 invariant 7 — the replay forensic engine is
    /// fail-CLOSED at the integrity layer; no fail-OPEN policy
    /// override).
    ///
    /// # Errors
    ///
    /// Returns [`ReplayAuditEmitError::Store`] on any backend failure.
    fn emit(&self, record: ReplayAuditRecord) -> Result<(), ReplayAuditEmitErrorInner>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryReplayAuditSink {
    inner: std::sync::Arc<Mutex<Vec<ReplayAuditRecord>>>,
}

impl InMemoryReplayAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ReplayAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: ReplayAuditEventType) -> Vec<ReplayAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }

    /// Filter snapshot down to records for a single tenant.
    #[must_use]
    pub fn snapshot_for_tenant(&self, tenant_id: Uuid) -> Vec<ReplayAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.tenant_id == tenant_id)
            .collect()
    }
}

impl ReplayAuditSink for InMemoryReplayAuditSink {
    fn emit(&self, record: ReplayAuditRecord) -> Result<(), ReplayAuditEmitErrorInner> {
        let mut guard = self.inner.lock().map_err(|_| {
            ReplayAuditEmitErrorInner::Store("billing-replay audit sink mutex poisoned".to_string())
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingReplayAuditSink;

impl FailingReplayAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ReplayAuditSink for FailingReplayAuditSink {
    fn emit(&self, _record: ReplayAuditRecord) -> Result<(), ReplayAuditEmitErrorInner> {
        Err(ReplayAuditEmitErrorInner::Store(
            "induced billing-replay audit sink failure (test fixture)".to_string(),
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

    fn rec(t: ReplayAuditEventType) -> ReplayAuditRecord {
        ReplayAuditRecord {
            event_type: t,
            request_id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            requested_by: Uuid::now_v7(),
            reason: ReplayReason::CustomerDispute,
            layer_drift_summary: None,
            billing_period: "2026-05".to_string(),
            now_ms: 1,
            context: "test".to_string(),
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            ReplayAuditEventType::RequestAuthorized,
            ReplayAuditEventType::RequestDenied,
            ReplayAuditEventType::DryRunPlanned,
            ReplayAuditEventType::Executed,
            ReplayAuditEventType::LayerDiverged,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.billing_replay."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_replay_audit_event_strings();
        assert_eq!(canonical.len(), 5);
        for s in canonical {
            assert!(s.starts_with("corelink.billing_replay."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(ReplayAuditEventType::LayerDiverged.is_sev2());
        assert!(ReplayAuditEventType::RequestDenied.is_sev3());
        assert!(!ReplayAuditEventType::RequestAuthorized.is_sev1());
        assert!(!ReplayAuditEventType::Executed.is_sev1());
        assert!(!ReplayAuditEventType::DryRunPlanned.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryReplayAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(ReplayAuditEventType::RequestAuthorized))
            .unwrap();
        sink.emit(rec(ReplayAuditEventType::Executed)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(ReplayAuditEventType::RequestAuthorized)
                .len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingReplayAuditSink::new();
        let err = sink
            .emit(rec(ReplayAuditEventType::RequestAuthorized))
            .unwrap_err();
        assert!(matches!(err, ReplayAuditEmitErrorInner::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryReplayAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(ReplayAuditEventType::RequestAuthorized))
            .unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", ReplayAuditEventType::Executed),
            "corelink.billing_replay.executed"
        );
    }

    #[test]
    fn audit_event_for_decision_pins_arm_mapping() {
        assert_eq!(
            audit_event_for_decision(&ReplayDecision::Authorized {
                idempotent_replay: false
            }),
            ReplayAuditEventType::RequestAuthorized
        );
        assert_eq!(
            audit_event_for_decision(&ReplayDecision::Denied403 {
                presented_role: "viewer".to_string()
            }),
            ReplayAuditEventType::RequestDenied
        );
        assert_eq!(
            audit_event_for_decision(&ReplayDecision::DryRunPlan { layers_planned: 3 }),
            ReplayAuditEventType::DryRunPlanned
        );
        assert_eq!(
            audit_event_for_decision(&ReplayDecision::Executed {
                layer_diverged: true
            }),
            ReplayAuditEventType::Executed
        );
    }

    #[test]
    fn snapshot_for_tenant_filters() {
        let sink = InMemoryReplayAuditSink::new();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        let mut r1 = rec(ReplayAuditEventType::Executed);
        r1.tenant_id = t1;
        let mut r2 = rec(ReplayAuditEventType::Executed);
        r2.tenant_id = t2;
        sink.emit(r1).unwrap();
        sink.emit(r2).unwrap();
        assert_eq!(sink.snapshot_for_tenant(t1).len(), 1);
        assert_eq!(sink.snapshot_for_tenant(t2).len(), 1);
    }
}
