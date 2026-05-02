//! Quota-CAS-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-quota::audit` (S-07 PROVISIONAL) +
//! `corelink-ratelimit::audit` (S-08 token-bucket): a small
//! quota-CAS-flavoured sink trait the production wiring composes on
//! top of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S08-003 §6.1.9 freezes the canonical 6-event taxonomy:
//!
//! - `corelink.quota.cas_check_passed` — emitted on every `Allow`
//!   decision (CAS predicate held; bytes_used updated atomically).
//! - `corelink.quota.cas_denied_429_hard_block` — emitted on the
//!   canonical hard-block 429 + Retry-After arm (per ADR-0020 FROZEN
//!   100% boundary; supersedes S-07 PROVISIONAL).
//! - `corelink.quota.cas_race_detected` — emitted when a CAS attempt
//!   observes a stale `cas_version` (mid-flight bump from concurrent
//!   commit_reservation / eviction_reclaim). The orchestrator retries
//!   up to `max_cas_attempts`; this event records the retry signal
//!   for SEV-3 race-detection observability.
//! - `corelink.quota.cas_commit_succeeded` — emitted when a successful
//!   acquire's bytes_used delta commits to the AtomicCasState.
//! - `corelink.quota.cas_release_idempotent` — emitted when an idempotent
//!   release fires (no-op release of a TTL-expired or never-reserved
//!   slot). The audit row preserves the audit-trail signal for forensics.
//! - `corelink.quota.cas_retry_after_emitted` — informational record
//!   of the canonical Retry-After value emitted on the 429 arm
//!   (lineage signal for cross-component drift detection — if S-07
//!   PROVISIONAL drift starts emitting different Retry-After values,
//!   the cross-comp regression test catches it).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (DASH-RATE
//! widget; WI-S08-006 PRR ship gate; admin-plane S-13) can extend the
//! taxonomy additively without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use corelink_eviction::EvictionRegion;

/// Canonical Quota-CAS audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-08 follow-on WIs (DASH-RATE widget,
/// WI-S08-006 PRR ship gate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum QuotaCasEventType {
    /// `corelink.quota.cas_check_passed` — `Allow` decision; CAS
    /// predicate held + bytes_used updated atomically.
    CasCheckPassed,
    /// `corelink.quota.cas_denied_429_hard_block` — canonical 100%
    /// hard-block 429 + Retry-After arm fired (per ADR-0020 FROZEN;
    /// supersedes S-07 PROVISIONAL transitional).
    CasDenied429HardBlock,
    /// `corelink.quota.cas_race_detected` — stale CAS version
    /// observed at write time; orchestrator retried (or exhausted).
    CasRaceDetected,
    /// `corelink.quota.cas_commit_succeeded` — a successful acquire's
    /// bytes_used delta committed atomically to AtomicCasState.
    CasCommitSucceeded,
    /// `corelink.quota.cas_release_idempotent` — idempotent release
    /// fired (no-op).
    CasReleaseIdempotent,
    /// `corelink.quota.cas_retry_after_emitted` — informational
    /// record of the canonical Retry-After value emitted on the 429
    /// arm (lineage signal for S-07 PROVISIONAL drift detection).
    CasRetryAfterEmitted,
}

impl QuotaCasEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CasCheckPassed => "corelink.quota.cas_check_passed",
            Self::CasDenied429HardBlock => {
                "corelink.quota.cas_denied_429_hard_block"
            }
            Self::CasRaceDetected => "corelink.quota.cas_race_detected",
            Self::CasCommitSucceeded => "corelink.quota.cas_commit_succeeded",
            Self::CasReleaseIdempotent => {
                "corelink.quota.cas_release_idempotent"
            }
            Self::CasRetryAfterEmitted => {
                "corelink.quota.cas_retry_after_emitted"
            }
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// `CasDenied429HardBlock` is SEV-1 (operator pager wake-up; tenant
    /// breach signal).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::CasDenied429HardBlock)
    }
}

impl core::fmt::Display for QuotaCasEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 6] {
    &[
        "corelink.quota.cas_check_passed",
        "corelink.quota.cas_denied_429_hard_block",
        "corelink.quota.cas_race_detected",
        "corelink.quota.cas_commit_succeeded",
        "corelink.quota.cas_release_idempotent",
        "corelink.quota.cas_retry_after_emitted",
    ]
}

/// Typed Quota-CAS audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope; the trait surface accepts the typed shape
/// so the sink + the envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaCasAuditRecord {
    /// Canonical event type.
    pub event_type: QuotaCasEventType,
    /// Verified tenant id (extracted from `AuthCtx`; never from
    /// request body / headers per Lote 10.4bis lesson).
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: EvictionRegion,
    /// Bytes touched by THIS decision — `Some(n)` for
    /// `CasCheckPassed` / `CasCommitSucceeded` /
    /// `CasReleaseIdempotent`; `Some(would_use)` for
    /// `CasDenied429HardBlock`; `None` for
    /// `CasRaceDetected` / `CasRetryAfterEmitted`.
    pub bytes: Option<u64>,
    /// CAS version observed at decision time — `Some(v)` for the
    /// arms that read CAS state; `None` for the
    /// `CasRetryAfterEmitted` informational record.
    pub cas_version: Option<u64>,
    /// CAS attempt count (1-indexed) — `Some(n)` for race-detection
    /// observability; tracks how many retries were needed before the
    /// successful write.
    pub cas_attempt: Option<u32>,
    /// Retry-After value emitted (seconds; canonical
    /// days-until-month-reset semantic per ADR-0020 FROZEN). `Some(s)`
    /// only on `CasDenied429HardBlock` + `CasRetryAfterEmitted`.
    pub retry_after_secs: Option<u64>,
    /// Source attribution: the request id (`x-request-id` header).
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`QuotaCasAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaCasAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("quota CAS audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the CAS state mutation (S-01 audit_outbox table;
///   fail-closed envelope per Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (`CasDenied429HardBlock` IS SEV-1).
pub trait QuotaCasAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`QuotaCasAuditSinkError::Store`] on any backend failure.
    fn emit(
        &self,
        record: QuotaCasAuditRecord,
    ) -> Result<(), QuotaCasAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryQuotaCasAuditSink {
    inner: std::sync::Arc<Mutex<Vec<QuotaCasAuditRecord>>>,
}

impl InMemoryQuotaCasAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<QuotaCasAuditRecord> {
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
    pub fn snapshot_of(
        &self,
        event_type: QuotaCasEventType,
    ) -> Vec<QuotaCasAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl QuotaCasAuditSink for InMemoryQuotaCasAuditSink {
    fn emit(
        &self,
        record: QuotaCasAuditRecord,
    ) -> Result<(), QuotaCasAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            QuotaCasAuditSinkError::Store(
                "audit sink mutex poisoned".to_string(),
            )
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 503 when the audit emit fires the
/// `Store` error).
#[derive(Debug, Default)]
pub struct FailingQuotaCasAuditSink;

impl FailingQuotaCasAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl QuotaCasAuditSink for FailingQuotaCasAuditSink {
    fn emit(
        &self,
        _record: QuotaCasAuditRecord,
    ) -> Result<(), QuotaCasAuditSinkError> {
        Err(QuotaCasAuditSinkError::Store(
            "induced quota CAS audit sink failure (test fixture)".to_string(),
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

    fn rec(t: QuotaCasEventType) -> QuotaCasAuditRecord {
        QuotaCasAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            region: EvictionRegion::Sam,
            bytes: None,
            cas_version: None,
            cas_attempt: None,
            retry_after_secs: None,
            created_by_request_id: "test".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            QuotaCasEventType::CasCheckPassed,
            QuotaCasEventType::CasDenied429HardBlock,
            QuotaCasEventType::CasRaceDetected,
            QuotaCasEventType::CasCommitSucceeded,
            QuotaCasEventType::CasReleaseIdempotent,
            QuotaCasEventType::CasRetryAfterEmitted,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.quota.cas_"));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 6);
        for s in canonical {
            assert!(s.starts_with("corelink.quota.cas_"));
        }
    }

    #[test]
    fn sev1_subset_only_denied_429_hard_block() {
        // CasDenied429HardBlock IS SEV-1 (operator pager wake-up).
        assert!(QuotaCasEventType::CasDenied429HardBlock.is_sev1());
        // The other arms are NOT SEV-1.
        assert!(!QuotaCasEventType::CasCheckPassed.is_sev1());
        assert!(!QuotaCasEventType::CasRaceDetected.is_sev1());
        assert!(!QuotaCasEventType::CasCommitSucceeded.is_sev1());
        assert!(!QuotaCasEventType::CasReleaseIdempotent.is_sev1());
        assert!(!QuotaCasEventType::CasRetryAfterEmitted.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryQuotaCasAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(QuotaCasEventType::CasCheckPassed)).unwrap();
        sink.emit(rec(QuotaCasEventType::CasDenied429HardBlock))
            .unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(QuotaCasEventType::CasCheckPassed).len(),
            1
        );
        assert_eq!(
            sink.snapshot_of(QuotaCasEventType::CasDenied429HardBlock)
                .len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingQuotaCasAuditSink::new();
        let err = sink.emit(rec(QuotaCasEventType::CasCheckPassed)).unwrap_err();
        assert!(matches!(err, QuotaCasAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryQuotaCasAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(QuotaCasEventType::CasCheckPassed)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", QuotaCasEventType::CasCheckPassed),
            "corelink.quota.cas_check_passed"
        );
    }
}
