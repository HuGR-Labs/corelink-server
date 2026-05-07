//! Erasure audit sink trait + InMemory test sink.
//!
//! Mirrors the `corelink-dsr::audit` discipline byte-for-byte: every
//! erasure decision arm fires its own audit event BEFORE state
//! mutation per the canonical Lote 10.6bis pattern + S-07 P1-1 fix +
//! ADR-S11-002 split-tier (DSR is regulatory-grade fail-CLOSED;
//! distinct from `corelink-billing-emit` fail-OPEN at the customer
//! hot path). A corrupted decision arm without an audit row would
//! mean the SOC 2 P5.1..5.2 + LGPD Art. 18 IV + GDPR Art. 17 +
//! CCPA §1798.105 evidence trail loses the canonical erasure step
//! moment — which IS the regulatory finding (multa LGPD 2% revenue /
//! GDPR 4% global).
//!
//! WI-S11-002 §1 freezes the canonical 8-event taxonomy:
//!
//! - `corelink.erasure.plan_generated` — orchestrator emitted a
//!   canonical 12-arm plan; audit fires BEFORE the per-backend
//!   fanout dispatch.
//! - `corelink.erasure.fanout_initiated` — orchestrator dispatched
//!   the 12 per-backend erase calls; audit fires BEFORE the first
//!   backend mutation.
//! - `corelink.erasure.backend_completed` — a per-backend erasure
//!   step completed (Erased / Pseudonymized / PartialFailure /
//!   Failed / NotApplicable); audit fires BEFORE the canonical
//!   D1 `dsr_erasure_log` tombstone insert.
//! - `corelink.erasure.verification_started` — 24h verification cron
//!   sweep started; audit fires BEFORE the per-backend
//!   `verification_hash()` query.
//! - `corelink.erasure.verified_complete` — verification sweep
//!   confirmed every backend reports a successful outcome; audit
//!   fires BEFORE the canonical `dsr.completed.v1` event +
//!   R2 evidence-dsr signed URL binding.
//! - `corelink.erasure.verified_partial` — verification sweep
//!   detected at least one non-successful backend outcome; audit
//!   fires BEFORE the SEV-1 alert + RB-DSR-ERASURE-INCOMPLETE
//!   runbook activation.
//! - `corelink.erasure.sla_breached` — > 24h elapsed since plan
//!   generation and at least one backend not yet verified; audit
//!   fires BEFORE the SEV-1 alert + RB-DSR-ERASURE-INCOMPLETE
//!   runbook activation.
//! - `corelink.erasure.report_generated` — BLAKE3-signed JCS-canonical
//!   erasure-report.json produced; audit fires BEFORE the canonical
//!   R2 evidence-dsr upload + signed URL 24h TTL binding.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs can extend the
//! taxonomy additively without breaking downstream sinks (e.g. S-14
//! BYOK may add `corelink.erasure.crypto_erase_initiated`).

use std::sync::Mutex;

use uuid::Uuid;

use crate::error::ErasureAuditSinkError;
use crate::event::{BackendKind, BackendOutcome};

/// Canonical erasure audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErasureAuditEventType {
    /// `corelink.erasure.plan_generated` — orchestrator emitted a
    /// canonical 12-arm plan.
    PlanGenerated,
    /// `corelink.erasure.fanout_initiated` — orchestrator dispatched
    /// the 12 per-backend erase calls.
    FanoutInitiated,
    /// `corelink.erasure.backend_completed` — a per-backend erasure
    /// step completed.
    BackendCompleted,
    /// `corelink.erasure.verification_started` — 24h verification
    /// cron sweep started.
    VerificationStarted,
    /// `corelink.erasure.verified_complete` — verification sweep
    /// confirmed every backend reports a successful outcome.
    VerifiedComplete,
    /// `corelink.erasure.verified_partial` — verification sweep
    /// detected at least one non-successful backend outcome.
    VerifiedPartial,
    /// `corelink.erasure.sla_breached` — > 24h elapsed since plan
    /// generation and at least one backend not yet verified.
    SlaBreached,
    /// `corelink.erasure.report_generated` — BLAKE3-signed JCS-
    /// canonical erasure-report.json produced.
    ReportGenerated,
}

impl ErasureAuditEventType {
    /// Canonical CloudEvents `type` attribute string. Pinned for
    /// D1 CHECK constraints + dashboard widget grouping +
    /// cross-component regression tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlanGenerated => "corelink.erasure.plan_generated",
            Self::FanoutInitiated => "corelink.erasure.fanout_initiated",
            Self::BackendCompleted => "corelink.erasure.backend_completed",
            Self::VerificationStarted => "corelink.erasure.verification_started",
            Self::VerifiedComplete => "corelink.erasure.verified_complete",
            Self::VerifiedPartial => "corelink.erasure.verified_partial",
            Self::SlaBreached => "corelink.erasure.sla_breached",
            Self::ReportGenerated => "corelink.erasure.report_generated",
        }
    }

    /// Whether this variant represents a SEV-1 alert. The canonical
    /// SEV-1 surface for erasure is the
    /// `verified_partial` + `sla_breached` arms (FM-450
    /// erasure-incomplete cross-backend P0 S=5 → upgrade FF-HR-010).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::VerifiedPartial | Self::SlaBreached)
    }

    /// Whether this variant represents a SEV-2 alert. No erasure
    /// audit arm is SEV-2 (per-backend retry exhaustion lands in
    /// FM-061 mapping at the backend taxonomy, not the audit
    /// taxonomy).
    #[must_use]
    pub const fn is_sev2(self) -> bool {
        false
    }

    /// Whether this variant represents a SEV-3 monitor. The
    /// `verification_started` arm is SEV-3 (informational —
    /// 24h cron sweep tick).
    #[must_use]
    pub const fn is_sev3(self) -> bool {
        matches!(self, Self::VerificationStarted)
    }
}

impl core::fmt::Display for ErasureAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_erasure_audit_event_strings() -> &'static [&'static str; 8] {
    &[
        "corelink.erasure.plan_generated",
        "corelink.erasure.fanout_initiated",
        "corelink.erasure.backend_completed",
        "corelink.erasure.verification_started",
        "corelink.erasure.verified_complete",
        "corelink.erasure.verified_partial",
        "corelink.erasure.sla_breached",
        "corelink.erasure.report_generated",
    ]
}

/// Typed erasure audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors the audit-chain S-09 inheritance
/// alongside the WI-S11-001 DSR + WI-S10-001 billing emit surfaces);
/// the trait surface accepts the typed shape so the sink + the
/// envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErasureAuditRecord {
    /// Canonical event type.
    pub event_type: ErasureAuditEventType,
    /// Canonical UUIDv7 DSR id (matches the request).
    pub dsr_id: Uuid,
    /// Tenant id of the canonical (tenant, dsr_id) erasure scope.
    /// Per Lote 10.4bis: tenant id MUST come from the authenticated
    /// middleware (NOT the request body).
    pub tenant_id: Uuid,
    /// Data subject id (canonical UUIDv7; PAT principal post-authn).
    /// Per CTRL-PRIV-014: the PRODUCTION audit envelope serializer
    /// pseudonymizes this to `sha256(subject_id || tenant_salt)` so
    /// the durable audit trail never carries the raw id; the trait
    /// surface here ships the raw UUID for in-memory test pinning.
    pub subject_id: Uuid,
    /// Optional per-backend kind (populated only on
    /// `BackendCompleted` arm; `None` on the other arms).
    pub backend: Option<BackendKind>,
    /// Optional per-backend outcome (populated only on
    /// `BackendCompleted` arm; `None` on the other arms).
    pub outcome: Option<BackendOutcome>,
    /// Producer-side wall-clock instant (Unix epoch ms; canonical
    /// no `_ms` suffix per Lote 10.7bis P0-3 except where the type +
    /// peer fields disambiguate; the suffix is intentional here).
    pub now_ms: u64,
    /// Free-form context (e.g. plan_size echo, dispatched_count echo,
    /// failed_count echo, elapsed_ms echo, retry_count echo).
    /// Reserved for adversarial debug + dashboard widget grouping.
    pub context: String,
}

/// Audit-of-erasure sink trait. Production wiring composes:
///
/// - `S09ChainErasureAuditSink` — canonical S-09 audit-chain APPEND
///   into the immutable ledger; tamper-detectable per S-09
///   INV-OBS-AUDIT-CHAIN-INTEGRITY.
/// - `MultiplexErasureAuditSink` — fan-out to direct SIEM in addition
///   to the chain.
pub trait ErasureAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// abort the erasure pipeline fail-CLOSED at THIS trait surface
    /// (per ADR-S11-002 — DSR is regulatory-grade fail-CLOSED; no
    /// fail-OPEN policy override).
    ///
    /// # Errors
    ///
    /// Returns [`ErasureAuditSinkError::Store`] on any backend
    /// failure.
    fn emit(&self, record: ErasureAuditRecord) -> Result<(), ErasureAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryErasureAuditSink {
    inner: std::sync::Arc<Mutex<Vec<ErasureAuditRecord>>>,
}

impl InMemoryErasureAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ErasureAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: ErasureAuditEventType) -> Vec<ErasureAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }

    /// Filter snapshot down to records for a single tenant.
    #[must_use]
    pub fn snapshot_for_tenant(&self, tenant_id: Uuid) -> Vec<ErasureAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.tenant_id == tenant_id)
            .collect()
    }
}

impl ErasureAuditSink for InMemoryErasureAuditSink {
    fn emit(&self, record: ErasureAuditRecord) -> Result<(), ErasureAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            ErasureAuditSinkError::Store("erasure audit sink mutex poisoned".to_string())
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingErasureAuditSink;

impl FailingErasureAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ErasureAuditSink for FailingErasureAuditSink {
    fn emit(&self, _record: ErasureAuditRecord) -> Result<(), ErasureAuditSinkError> {
        Err(ErasureAuditSinkError::Store(
            "induced erasure audit sink failure (test fixture)".to_string(),
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

    fn rec(t: ErasureAuditEventType) -> ErasureAuditRecord {
        ErasureAuditRecord {
            event_type: t,
            dsr_id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            subject_id: Uuid::now_v7(),
            backend: None,
            outcome: None,
            now_ms: 1,
            context: "test".to_string(),
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            ErasureAuditEventType::PlanGenerated,
            ErasureAuditEventType::FanoutInitiated,
            ErasureAuditEventType::BackendCompleted,
            ErasureAuditEventType::VerificationStarted,
            ErasureAuditEventType::VerifiedComplete,
            ErasureAuditEventType::VerifiedPartial,
            ErasureAuditEventType::SlaBreached,
            ErasureAuditEventType::ReportGenerated,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.erasure."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 8);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_erasure_audit_event_strings();
        assert_eq!(canonical.len(), 8);
        for s in canonical {
            assert!(s.starts_with("corelink.erasure."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(ErasureAuditEventType::VerifiedPartial.is_sev1());
        assert!(ErasureAuditEventType::SlaBreached.is_sev1());
        assert!(!ErasureAuditEventType::PlanGenerated.is_sev1());
        assert!(ErasureAuditEventType::VerificationStarted.is_sev3());
        assert!(!ErasureAuditEventType::VerifiedComplete.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryErasureAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(ErasureAuditEventType::PlanGenerated)).unwrap();
        sink.emit(rec(ErasureAuditEventType::FanoutInitiated))
            .unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(ErasureAuditEventType::PlanGenerated).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingErasureAuditSink::new();
        let err = sink
            .emit(rec(ErasureAuditEventType::PlanGenerated))
            .unwrap_err();
        assert!(matches!(err, ErasureAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryErasureAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(ErasureAuditEventType::PlanGenerated)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", ErasureAuditEventType::ReportGenerated),
            "corelink.erasure.report_generated"
        );
    }

    #[test]
    fn snapshot_for_tenant_filters() {
        let sink = InMemoryErasureAuditSink::new();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        let mut r1 = rec(ErasureAuditEventType::PlanGenerated);
        r1.tenant_id = t1;
        let mut r2 = rec(ErasureAuditEventType::PlanGenerated);
        r2.tenant_id = t2;
        sink.emit(r1).unwrap();
        sink.emit(r2).unwrap();
        assert_eq!(sink.snapshot_for_tenant(t1).len(), 1);
        assert_eq!(sink.snapshot_for_tenant(t2).len(), 1);
    }
}
