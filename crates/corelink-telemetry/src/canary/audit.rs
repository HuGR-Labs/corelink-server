//! Audit-of-canary (meta-audit) trait + InMemory test sink.
//!
//! Mirrors `corelink-audit-chain::audit` discipline at the canary
//! orchestrator boundary — every canary loop decision arm fires its
//! own audit event BEFORE state mutation per
//! `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`. The canary path is itself a
//! ship-gate primitive (sprint contract §6 DoD; 12_960 sustained loops
//! / 72h is the GA evidence row); a missed canary decision audit
//! event would mean the verifier cannot retroactively reconstruct
//! whether the 72h sustained criterion was actually met.
//!
//! WI-S09-007 §6.1.10 freezes the canonical 5-event taxonomy:
//!
//! - `corelink.canary.loop_executed` — emitted on every successful
//!   canary loop (informational; surfaces region + decision arm +
//!   latencies + dispatch lag).
//! - `corelink.canary.assertion_failed` — emitted on every
//!   `CanaryAssertion` ceiling breach (latency miss; SEV-2 alert
//!   source after 3 consecutive in same region).
//! - `corelink.canary.digest_mismatch` — emitted on BLAKE3 digest
//!   mismatch (SEV-1 alert source; data integrity issue).
//! - `corelink.canary.observability_unhealthy` — emitted on
//!   observability stack component unhealthy (SEV-3 alert source).
//! - `corelink.canary.dispatch_lag_exceeded` — emitted on canary cron
//!   dispatch lag > 90s sustained 5min (SEV-3 alert source).

use std::sync::{Arc, Mutex};

use crate::canary::assertion::CanaryAssertion;
use crate::canary::error::CanaryError;
use crate::canary::region::CanaryRegion;
use crate::canary::result::{CanaryDecision, HealthComponent};

/// Canonical canary meta-audit taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on sprints (e.g. S-13
/// admin manual override / S-14 enterprise tier custom assertion arm).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CanaryAuditEventType {
    /// `corelink.canary.loop_executed` — emitted for every successful
    /// canary loop (informational; surfaces region + decision arm +
    /// latencies + dispatch lag).
    LoopExecuted,
    /// `corelink.canary.assertion_failed` — emitted on every
    /// `CanaryAssertion` ceiling breach.
    AssertionFailed,
    /// `corelink.canary.digest_mismatch` — emitted on BLAKE3 digest
    /// mismatch (SEV-1 alert source).
    DigestMismatch,
    /// `corelink.canary.observability_unhealthy` — emitted on
    /// observability stack component unhealthy (SEV-3 alert source).
    ObservabilityUnhealthy,
    /// `corelink.canary.dispatch_lag_exceeded` — emitted on canary
    /// cron dispatch lag > 90s sustained.
    DispatchLagExceeded,
}

impl CanaryAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LoopExecuted => "corelink.canary.loop_executed",
            Self::AssertionFailed => "corelink.canary.assertion_failed",
            Self::DigestMismatch => "corelink.canary.digest_mismatch",
            Self::ObservabilityUnhealthy => "corelink.canary.observability_unhealthy",
            Self::DispatchLagExceeded => "corelink.canary.dispatch_lag_exceeded",
        }
    }
}

impl core::fmt::Display for CanaryAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 5-element audit event-string list — pinned at the type
/// system layer for surface-stability regression tests + dashboard
/// widget configuration.
#[must_use]
pub const fn canonical_canary_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.canary.loop_executed",
        "corelink.canary.assertion_failed",
        "corelink.canary.digest_mismatch",
        "corelink.canary.observability_unhealthy",
        "corelink.canary.dispatch_lag_exceeded",
    ]
}

/// Typed canary meta-audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors the on-the-wire `AuditEvent`
/// shape from `corelink-audit-chain`); the trait surface accepts the
/// typed shape so the sink + the envelope serializer share an
/// unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanaryAuditRecord {
    /// Canonical event type.
    pub event_type: CanaryAuditEventType,
    /// Canary region the loop ran in.
    pub region: CanaryRegion,
    /// Canonical decision slug computed for the loop (e.g. `pass` /
    /// `degraded` / `failed_region`); `""` empty when the audit row
    /// is emitted before the decision is computed (e.g. early
    /// dispatch lag exceeded arm).
    pub decision_slug: &'static str,
    /// Canary tenant identifier (always
    /// [`crate::canary::config::SYNTHETIC_CANARY_TENANT_ID`] per WI-S09-007
    /// §1 invariant 6); excluded from real-tenant SLI denominator
    /// computation via WI-S09-006 recording rule filter.
    pub tenant_id: String,
    /// Canonical assertion slug for `AssertionFailed` arms (e.g.
    /// `cas_get_p99_ms`); `""` empty for non-assertion arms.
    pub assertion_slug: &'static str,
    /// Canonical health component slug for `ObservabilityUnhealthy`
    /// arms (e.g. `mimir`); `""` empty for non-health arms.
    pub component_slug: &'static str,
    /// Observed dispatch lag in ms (informational; > 90_000 = SEV-3).
    pub dispatch_lag_ms: u64,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

impl CanaryAuditRecord {
    /// Constructor for `LoopExecuted` arm.
    #[must_use]
    pub fn loop_executed(
        region: CanaryRegion,
        decision: CanaryDecision,
        tenant_id: String,
        dispatch_lag_ms: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            event_type: CanaryAuditEventType::LoopExecuted,
            region,
            decision_slug: decision.slug(),
            tenant_id,
            assertion_slug: "",
            component_slug: "",
            dispatch_lag_ms,
            now_ms,
        }
    }

    /// Constructor for `AssertionFailed` arm.
    #[must_use]
    pub fn assertion_failed(
        region: CanaryRegion,
        decision: CanaryDecision,
        assertion: CanaryAssertion,
        tenant_id: String,
        dispatch_lag_ms: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            event_type: CanaryAuditEventType::AssertionFailed,
            region,
            decision_slug: decision.slug(),
            tenant_id,
            assertion_slug: assertion.slug(),
            component_slug: "",
            dispatch_lag_ms,
            now_ms,
        }
    }

    /// Constructor for `DigestMismatch` arm.
    #[must_use]
    pub fn digest_mismatch(
        region: CanaryRegion,
        tenant_id: String,
        dispatch_lag_ms: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            event_type: CanaryAuditEventType::DigestMismatch,
            region,
            decision_slug: CanaryDecision::FailedRegion.slug(),
            tenant_id,
            assertion_slug: CanaryAssertion::DigestMatch.slug(),
            component_slug: "",
            dispatch_lag_ms,
            now_ms,
        }
    }

    /// Constructor for `ObservabilityUnhealthy` arm.
    #[must_use]
    pub fn observability_unhealthy(
        region: CanaryRegion,
        component: HealthComponent,
        tenant_id: String,
        dispatch_lag_ms: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            event_type: CanaryAuditEventType::ObservabilityUnhealthy,
            region,
            decision_slug: CanaryDecision::FailedRegion.slug(),
            tenant_id,
            assertion_slug: "",
            component_slug: component.slug(),
            dispatch_lag_ms,
            now_ms,
        }
    }

    /// Constructor for `DispatchLagExceeded` arm.
    #[must_use]
    pub fn dispatch_lag_exceeded(
        region: CanaryRegion,
        tenant_id: String,
        dispatch_lag_ms: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            event_type: CanaryAuditEventType::DispatchLagExceeded,
            region,
            decision_slug: CanaryDecision::FailedRegion.slug(),
            tenant_id,
            assertion_slug: "",
            component_slug: "",
            dispatch_lag_ms,
            now_ms,
        }
    }
}

/// Errors surfaced by [`CanaryAuditSink::emit`].
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CanaryAuditEmitError {
    /// Backend transport failure (e.g. D1 INSERT rejected /
    /// in-memory mutex poisoned).
    #[error("canary audit sink store failed: {0}")]
    Store(String),
}

impl From<CanaryAuditEmitError> for CanaryError {
    fn from(value: CanaryAuditEmitError) -> Self {
        Self::Audit(value.to_string())
    }
}

/// Audit-of-canary sink trait. Production wiring composes:
///
/// - `OutboxCanaryAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the canary observability emit (S-01 audit_outbox
///   table; fail-CLOSED envelope per Lote 10.6bis pattern + S-07
///   P1-1 fix).
/// - `MultiplexCanaryAuditSink` — fan-out to direct SIEM in addition
///   to the outbox.
pub trait CanaryAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// canary emit fail-CLOSED per WI §6.1.10 (audit data integrity >
    /// canary availability; missing audit event = ship-gate evidence
    /// gap).
    ///
    /// # Errors
    ///
    /// Returns [`CanaryAuditEmitError::Store`] on any backend failure.
    fn emit(&self, record: CanaryAuditRecord) -> Result<(), CanaryAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryCanaryAuditSink {
    inner: Arc<Mutex<Vec<CanaryAuditRecord>>>,
}

impl InMemoryCanaryAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<CanaryAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: CanaryAuditEventType) -> Vec<CanaryAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl CanaryAuditSink for InMemoryCanaryAuditSink {
    fn emit(&self, record: CanaryAuditRecord) -> Result<(), CanaryAuditEmitError> {
        let mut g = self.inner.lock().map_err(|_| {
            CanaryAuditEmitError::Store("canary audit sink mutex poisoned".to_string())
        })?;
        g.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingCanaryAuditSink;

impl FailingCanaryAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CanaryAuditSink for FailingCanaryAuditSink {
    fn emit(&self, _record: CanaryAuditRecord) -> Result<(), CanaryAuditEmitError> {
        Err(CanaryAuditEmitError::Store(
            "induced canary audit sink failure (test fixture)".to_string(),
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
    use crate::canary::config::SYNTHETIC_CANARY_TENANT_ID;

    fn canary_tenant_id() -> String {
        SYNTHETIC_CANARY_TENANT_ID.to_string()
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            CanaryAuditEventType::LoopExecuted,
            CanaryAuditEventType::AssertionFailed,
            CanaryAuditEventType::DigestMismatch,
            CanaryAuditEventType::ObservabilityUnhealthy,
            CanaryAuditEventType::DispatchLagExceeded,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.canary."));
            assert!(set.insert(t.as_str()));
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_canary_audit_event_strings();
        assert_eq!(canonical.len(), 5);
        for s in canonical {
            assert!(s.starts_with("corelink.canary."));
        }
    }

    #[test]
    fn loop_executed_constructor_canonical() {
        let r = CanaryAuditRecord::loop_executed(
            CanaryRegion::Enam,
            CanaryDecision::Pass,
            canary_tenant_id(),
            0,
            1,
        );
        assert_eq!(r.event_type, CanaryAuditEventType::LoopExecuted);
        assert_eq!(r.region, CanaryRegion::Enam);
        assert_eq!(r.decision_slug, "pass");
        assert_eq!(r.assertion_slug, "");
        assert_eq!(r.component_slug, "");
    }

    #[test]
    fn assertion_failed_constructor_carries_assertion_slug() {
        let r = CanaryAuditRecord::assertion_failed(
            CanaryRegion::Weur,
            CanaryDecision::Degraded,
            CanaryAssertion::CasGetP99,
            canary_tenant_id(),
            0,
            1,
        );
        assert_eq!(r.event_type, CanaryAuditEventType::AssertionFailed);
        assert_eq!(r.assertion_slug, "cas_get_p99_ms");
    }

    #[test]
    fn digest_mismatch_constructor_canonical_failed_region() {
        let r = CanaryAuditRecord::digest_mismatch(CanaryRegion::Apac, canary_tenant_id(), 0, 1);
        assert_eq!(r.decision_slug, "failed_region");
        assert_eq!(r.assertion_slug, "digest_match");
    }

    #[test]
    fn observability_unhealthy_constructor_carries_component() {
        let r = CanaryAuditRecord::observability_unhealthy(
            CanaryRegion::Enam,
            HealthComponent::Mimir,
            canary_tenant_id(),
            0,
            1,
        );
        assert_eq!(r.component_slug, "mimir");
        assert_eq!(r.decision_slug, "failed_region");
    }

    #[test]
    fn dispatch_lag_constructor_carries_lag_ms() {
        let r = CanaryAuditRecord::dispatch_lag_exceeded(
            CanaryRegion::Enam,
            canary_tenant_id(),
            95_000,
            1,
        );
        assert_eq!(r.dispatch_lag_ms, 95_000);
        assert_eq!(r.decision_slug, "failed_region");
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let s = InMemoryCanaryAuditSink::new();
        assert!(s.is_empty());
        s.emit(CanaryAuditRecord::loop_executed(
            CanaryRegion::Enam,
            CanaryDecision::Pass,
            canary_tenant_id(),
            0,
            1,
        ))
        .unwrap();
        s.emit(CanaryAuditRecord::dispatch_lag_exceeded(
            CanaryRegion::Weur,
            canary_tenant_id(),
            95_000,
            2,
        ))
        .unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            s.snapshot_of(CanaryAuditEventType::DispatchLagExceeded).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let s = FailingCanaryAuditSink::new();
        let err = s
            .emit(CanaryAuditRecord::loop_executed(
                CanaryRegion::Enam,
                CanaryDecision::Pass,
                canary_tenant_id(),
                0,
                1,
            ))
            .unwrap_err();
        assert!(matches!(err, CanaryAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryCanaryAuditSink::new();
        let s2 = s1.clone();
        s1.emit(CanaryAuditRecord::loop_executed(
            CanaryRegion::Enam,
            CanaryDecision::Pass,
            canary_tenant_id(),
            0,
            1,
        ))
        .unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn audit_emit_error_lifts_to_canary_audit_arm() {
        let inner = CanaryAuditEmitError::Store("x".to_string());
        let outer: CanaryError = inner.into();
        assert!(matches!(outer, CanaryError::Audit(_)));
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", CanaryAuditEventType::LoopExecuted),
            "corelink.canary.loop_executed"
        );
    }
}
