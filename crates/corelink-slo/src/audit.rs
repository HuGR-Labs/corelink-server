//! Audit-of-alert (meta-audit) trait + InMemory test sink.
//!
//! Mirrors `corelink-audit-chain::audit` discipline at the alert
//! orchestrator boundary — every alert decision arm fires its own
//! audit event BEFORE state mutation per
//! `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`. The alert path is itself a
//! compliance primitive (SOC 2 CC7.2 + LGPD Art. 20 humane response
//! traceability); a missed alert decision audit event would mean the
//! verifier cannot retroactively diagnose why an outage was not
//! escalated.
//!
//! WI-S09-006 §6.1.4 freezes the canonical 5-event taxonomy:
//!
//! - `corelink.slo.burn_rate_evaluated` — emitted on every burn-rate
//!   sample evaluated (informational; surfaces the calculator
//!   decision input + the chosen `AlertDecision` arm).
//! - `corelink.slo.alert_fired` — emitted on every non-`Quiet`
//!   decision (page or ticket).
//! - `corelink.slo.alert_quiet` — emitted on every `Quiet` decision
//!   (informational; SLO healthy path).
//! - `corelink.slo.page_dispatched` — emitted after a successful
//!   PagerDuty dispatch (page arm only).
//! - `corelink.slo.ticket_filed` — emitted after a ticket arm is
//!   recorded (no PagerDuty dispatch; business-hours review queue).

use std::sync::{Arc, Mutex};

use crate::error::SloError;

/// Canonical SLO meta-audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for follow-on WIs (e.g. S-13 admin custom
/// SEV-tier forward / S-19 Stripe billing-impact alerts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SloAuditEventType {
    /// `corelink.slo.burn_rate_evaluated` — emitted for every burn-
    /// rate sample evaluated (informational; surfaces calculator
    /// inputs + chosen decision arm).
    BurnRateEvaluated,
    /// `corelink.slo.alert_fired` — emitted on every non-`Quiet`
    /// decision (page or ticket).
    AlertFired,
    /// `corelink.slo.alert_quiet` — emitted on every `Quiet` decision
    /// (informational; SLO healthy path).
    AlertQuiet,
    /// `corelink.slo.page_dispatched` — emitted after a successful
    /// PagerDuty dispatch.
    PageDispatched,
    /// `corelink.slo.ticket_filed` — emitted after a ticket arm is
    /// recorded.
    TicketFiled,
}

impl SloAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BurnRateEvaluated => "corelink.slo.burn_rate_evaluated",
            Self::AlertFired => "corelink.slo.alert_fired",
            Self::AlertQuiet => "corelink.slo.alert_quiet",
            Self::PageDispatched => "corelink.slo.page_dispatched",
            Self::TicketFiled => "corelink.slo.ticket_filed",
        }
    }
}

impl core::fmt::Display for SloAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 5-element audit event-string list — pinned at the type
/// system layer for surface-stability regression tests + dashboard
/// widget configuration.
#[must_use]
pub const fn canonical_slo_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.slo.burn_rate_evaluated",
        "corelink.slo.alert_fired",
        "corelink.slo.alert_quiet",
        "corelink.slo.page_dispatched",
        "corelink.slo.ticket_filed",
    ]
}

/// Typed SLO meta-audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors the on-the-wire `AuditEvent`
/// shape from `corelink-audit-chain`); the trait surface accepts the
/// typed shape so the sink + the envelope serializer share an
/// unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SloAuditRecord {
    /// Canonical event type.
    pub event_type: SloAuditEventType,
    /// Canonical SLI slug (`sli.slug()`; e.g. `SLI-AVAIL-CAS-GET`).
    pub sli_slug: &'static str,
    /// Canonical burn-rate window slug (`window.slug()`;
    /// e.g. `fast_1h`).
    pub window_slug: &'static str,
    /// Canonical alert decision slug (`decision.slug()`;
    /// e.g. `page_sev0`).
    pub decision_slug: &'static str,
    /// Tenant identifier the alert pertains to (per-tenant ledger
    /// partition per WI §1 invariant 6); `"global"` for non-tenant-
    /// scoped SLIs (e.g. control plane).
    pub tenant_id: String,
    /// Canonical PagerDuty dedup key (`{sli_slug}:{window_slug}:{tenant_id}`)
    /// used for idempotent dispatch; empty string on quiet arms.
    pub pagerduty_dedup_key: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`SloAuditSink::emit`].
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SloAuditEmitError {
    /// Backend transport failure (e.g. D1 INSERT rejected /
    /// in-memory mutex poisoned).
    #[error("SLO audit sink store failed: {0}")]
    Store(String),
}

impl From<SloAuditEmitError> for SloError {
    fn from(value: SloAuditEmitError) -> Self {
        Self::Audit(value.to_string())
    }
}

/// Audit-of-alert sink trait. Production wiring composes:
///
/// - `OutboxSloAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the PagerDuty dispatch (S-01 audit_outbox table; fail-
///   closed envelope per Lote 10.6bis pattern + S-07 P1-1 fix).
/// - `MultiplexSloAuditSink` — fan-out to direct SIEM in addition to
///   the outbox.
pub trait SloAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// alert emit fail-CLOSED per WI §6.1.10 (audit data integrity >
    /// alert availability; missing audit event = SOC 2 CC7.2 +
    /// LGPD Art. 20 traceability gap).
    ///
    /// # Errors
    ///
    /// Returns [`SloAuditEmitError::Store`] on any backend failure.
    fn emit(&self, record: SloAuditRecord) -> Result<(), SloAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemorySloAuditSink {
    inner: Arc<Mutex<Vec<SloAuditRecord>>>,
}

impl InMemorySloAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<SloAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: SloAuditEventType) -> Vec<SloAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl SloAuditSink for InMemorySloAuditSink {
    fn emit(&self, record: SloAuditRecord) -> Result<(), SloAuditEmitError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| SloAuditEmitError::Store("audit sink mutex poisoned".to_string()))?;
        g.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope.
#[derive(Debug, Default)]
pub struct FailingSloAuditSink;

impl FailingSloAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SloAuditSink for FailingSloAuditSink {
    fn emit(&self, _record: SloAuditRecord) -> Result<(), SloAuditEmitError> {
        Err(SloAuditEmitError::Store(
            "induced SLO audit sink failure (test fixture)".to_string(),
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

    fn rec(t: SloAuditEventType) -> SloAuditRecord {
        SloAuditRecord {
            event_type: t,
            sli_slug: "SLI-AVAIL-CAS-GET",
            window_slug: "fast_1h",
            decision_slug: "page_sev0",
            tenant_id: "global".to_string(),
            pagerduty_dedup_key: "SLI-AVAIL-CAS-GET:fast_1h:global".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            SloAuditEventType::BurnRateEvaluated,
            SloAuditEventType::AlertFired,
            SloAuditEventType::AlertQuiet,
            SloAuditEventType::PageDispatched,
            SloAuditEventType::TicketFiled,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.slo."));
            assert!(set.insert(t.as_str()));
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_slo_audit_event_strings();
        assert_eq!(canonical.len(), 5);
        for s in canonical {
            assert!(s.starts_with("corelink.slo."));
        }
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let s = InMemorySloAuditSink::new();
        assert!(s.is_empty());
        s.emit(rec(SloAuditEventType::BurnRateEvaluated)).unwrap();
        s.emit(rec(SloAuditEventType::AlertFired)).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s.snapshot_of(SloAuditEventType::AlertFired).len(), 1);
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let s = FailingSloAuditSink::new();
        let err = s.emit(rec(SloAuditEventType::AlertFired)).unwrap_err();
        assert!(matches!(err, SloAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemorySloAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(SloAuditEventType::AlertFired)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn audit_emit_error_lifts_to_slo_audit_arm() {
        let inner = SloAuditEmitError::Store("x".to_string());
        let outer: SloError = inner.into();
        assert!(matches!(outer, SloError::Audit(_)));
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", SloAuditEventType::BurnRateEvaluated),
            "corelink.slo.burn_rate_evaluated"
        );
    }
}
