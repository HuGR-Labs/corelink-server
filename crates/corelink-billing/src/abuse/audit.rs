//! Abuse-detection-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-quota::audit` (S-07 PROVISIONAL) +
//! `corelink-ratelimit::audit` (S-08 token-bucket) +
//! `corelink-quota-cas::audit` (S-08 atomic CAS): a small
//! abuse-flavoured sink trait the production wiring composes on top of
//! the `audit_outbox` (WI-S01-004) row insert. The S-09 audit chain
//! processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S08-004 §6.1.10 freezes the canonical 7-event taxonomy:
//!
//! - `corelink.abuse.score_computed` — emitted on every score
//!   computation (5min cron-tick canonical aggregation per WI §1
//!   invariant 4); informational lineage record of the 4-feature
//!   weighted-sum result regardless of decision arm.
//! - `corelink.abuse.decision_benign` — score < SUSPICIOUS_THRESHOLD;
//!   tier Noop; no automated action applied.
//! - `corelink.abuse.decision_suspicious` — SUSPICIOUS_THRESHOLD ≤
//!   score < MALICIOUS_THRESHOLD; silent downgrade applied via
//!   `tenant_rate_override` (WI §6.1.4 CI-2).
//! - `corelink.abuse.decision_malicious` — score ≥ MALICIOUS_THRESHOLD;
//!   admin review trigger SEV-2; suspend candidate tier.
//! - `corelink.abuse.downgrade_applied` — silent-downgrade applied
//!   side-effect record (informational lineage to `tenant_rate_override`
//!   table; production wiring writes the override mid-decision).
//! - `corelink.abuse.admin_review_triggered` — SEV-2 PagerDuty admin
//!   notification fired on the AdminReviewTriggerSev2 tier.
//! - `corelink.abuse.suspend_applied` — admin manual suspend executed
//!   AFTER human review (per LGPD Art. 20 humane response: NEVER
//!   auto-applied; this audit row is emitted ONLY when an admin
//!   approves a suspend; programmatic attempts hit
//!   [`super::error::AbuseError::AutoSuspendForbidden`] without ever
//!   reaching the audit emit).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (DASH-RATE widget;
//! WI-S08-006 PRR ship gate; admin-plane S-13) can extend the taxonomy
//! additively without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

/// Canonical abuse-detection audit taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for S-08 follow-on WIs (DASH-RATE
/// widget, WI-S08-006 PRR ship gate, admin-plane S-13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AbuseEventType {
    /// `corelink.abuse.score_computed` — informational lineage record
    /// of the 4-feature weighted-sum result regardless of decision arm.
    ScoreComputed,
    /// `corelink.abuse.decision_benign` — score below SUSPICIOUS
    /// threshold; tier Noop.
    DecisionBenign,
    /// `corelink.abuse.decision_suspicious` — score in [SUSPICIOUS,
    /// MALICIOUS); silent-downgrade applied via tenant_rate_override.
    DecisionSuspicious,
    /// `corelink.abuse.decision_malicious` — score ≥ MALICIOUS; admin
    /// review trigger.
    DecisionMalicious,
    /// `corelink.abuse.downgrade_applied` — silent-downgrade
    /// side-effect record.
    DowngradeApplied,
    /// `corelink.abuse.admin_review_triggered` — SEV-2 PagerDuty
    /// notification fired.
    AdminReviewTriggered,
    /// `corelink.abuse.suspend_applied` — admin manual suspend
    /// executed AFTER human review (NEVER auto-applied per LGPD
    /// Art. 20 + GDPR Art. 22).
    SuspendApplied,
}

impl AbuseEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScoreComputed => "corelink.abuse.score_computed",
            Self::DecisionBenign => "corelink.abuse.decision_benign",
            Self::DecisionSuspicious => "corelink.abuse.decision_suspicious",
            Self::DecisionMalicious => "corelink.abuse.decision_malicious",
            Self::DowngradeApplied => "corelink.abuse.downgrade_applied",
            Self::AdminReviewTriggered => {
                "corelink.abuse.admin_review_triggered"
            }
            Self::SuspendApplied => "corelink.abuse.suspend_applied",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// `SuspendApplied` is SEV-1 (operator pager wake-up; LGPD
    /// compliance trail).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::SuspendApplied)
    }
}

impl core::fmt::Display for AbuseEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 7] {
    &[
        "corelink.abuse.score_computed",
        "corelink.abuse.decision_benign",
        "corelink.abuse.decision_suspicious",
        "corelink.abuse.decision_malicious",
        "corelink.abuse.downgrade_applied",
        "corelink.abuse.admin_review_triggered",
        "corelink.abuse.suspend_applied",
    ]
}

/// Typed abuse audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope; the trait surface accepts the typed shape
/// so the sink + the envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq)]
pub struct AbuseAuditRecord {
    /// Canonical event type.
    pub event_type: AbuseEventType,
    /// Verified tenant id (extracted from `AuthCtx`; never from
    /// request body / headers per Lote 10.4bis lesson).
    pub tenant_id: Uuid,
    /// Computed abuse score [0.0, 1.0] — `Some(s)` for ScoreComputed +
    /// every Decision* arm; `None` for side-effect records that follow
    /// the decision (DowngradeApplied / AdminReviewTriggered /
    /// SuspendApplied where the score has already been audited).
    pub score: Option<f64>,
    /// 5min aggregation window start (Unix epoch ms; canonical no
    /// `_ms` suffix per Lote 10.7bis P0-3). `Some(t)` for ScoreComputed
    /// + Decision* arms; `None` for side-effect records.
    pub window_start: Option<u64>,
    /// 5min aggregation window end (Unix epoch ms).
    pub window_end: Option<u64>,
    /// Source attribution: the request id / cron-tick batch id.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`AbuseAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AbuseAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("abuse audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the abuse_score_history mutation (S-01 audit_outbox
///   table; fail-closed envelope per Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (`SuspendApplied` IS SEV-1).
pub trait AbuseAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 5xx
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`AbuseAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: AbuseAuditRecord) -> Result<(), AbuseAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryAbuseAuditSink {
    inner: std::sync::Arc<Mutex<Vec<AbuseAuditRecord>>>,
}

impl InMemoryAbuseAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AbuseAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: AbuseEventType) -> Vec<AbuseAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl AbuseAuditSink for InMemoryAbuseAuditSink {
    fn emit(&self, record: AbuseAuditRecord) -> Result<(), AbuseAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            AbuseAuditSinkError::Store("audit sink mutex poisoned".to_string())
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 5xx when the audit emit fires the
/// `Store` error).
#[derive(Debug, Default)]
pub struct FailingAbuseAuditSink;

impl FailingAbuseAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AbuseAuditSink for FailingAbuseAuditSink {
    fn emit(&self, _record: AbuseAuditRecord) -> Result<(), AbuseAuditSinkError> {
        Err(AbuseAuditSinkError::Store(
            "induced abuse audit sink failure (test fixture)".to_string(),
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

    fn rec(t: AbuseEventType) -> AbuseAuditRecord {
        AbuseAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            score: None,
            window_start: None,
            window_end: None,
            created_by_request_id: "test".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            AbuseEventType::ScoreComputed,
            AbuseEventType::DecisionBenign,
            AbuseEventType::DecisionSuspicious,
            AbuseEventType::DecisionMalicious,
            AbuseEventType::DowngradeApplied,
            AbuseEventType::AdminReviewTriggered,
            AbuseEventType::SuspendApplied,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.abuse."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 7);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 7);
        for s in canonical {
            assert!(s.starts_with("corelink.abuse."));
        }
    }

    #[test]
    fn sev1_subset_only_suspend_applied() {
        assert!(AbuseEventType::SuspendApplied.is_sev1());
        assert!(!AbuseEventType::ScoreComputed.is_sev1());
        assert!(!AbuseEventType::DecisionBenign.is_sev1());
        assert!(!AbuseEventType::DecisionSuspicious.is_sev1());
        assert!(!AbuseEventType::DecisionMalicious.is_sev1());
        assert!(!AbuseEventType::DowngradeApplied.is_sev1());
        assert!(!AbuseEventType::AdminReviewTriggered.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryAbuseAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(AbuseEventType::ScoreComputed)).unwrap();
        sink.emit(rec(AbuseEventType::DecisionBenign)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(AbuseEventType::ScoreComputed).len(), 1);
        assert_eq!(sink.snapshot_of(AbuseEventType::DecisionBenign).len(), 1);
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingAbuseAuditSink::new();
        let err = sink.emit(rec(AbuseEventType::ScoreComputed)).unwrap_err();
        assert!(matches!(err, AbuseAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryAbuseAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(AbuseEventType::ScoreComputed)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", AbuseEventType::ScoreComputed),
            "corelink.abuse.score_computed"
        );
    }
}
