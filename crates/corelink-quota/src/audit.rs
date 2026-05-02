//! Quota-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-eviction::audit` + `corelink-dedup::audit`: a
//! small quota-flavoured sink trait the production wiring composes on
//! top of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S07-003 §6.1 freezes the canonical 5-event taxonomy:
//!
//! - `corelink.quota.check_passed` — emitted on every `Allow` decision
//!   (sampled at handler layer in production; in tests every decision
//!   emits one record).
//! - `corelink.quota.denied_429` — emitted on the PROVISIONAL
//!   transitional 429 + Retry-After arm (per ADR-0020 FROZEN; canonical
//!   S-08 hard-block).
//! - `corelink.quota.reserved` — emitted on every `Reserve` decision
//!   (write requests `WriteBlob` / `BatchUpdateBlobs` / `WriteAction`
//!   that succeed at the boundary check; carries `reservation_id` +
//!   `requested_bytes` + size-proportional TTL `expires_at_ms`).
//! - `corelink.quota.reservation_expired` — emitted by the TTL sweep
//!   when a reservation auto-releases (no quota leak per
//!   INV-QUOTA-RESERVATION-TTL).
//! - `corelink.quota.reservation_rolled_in` — emitted on
//!   `commit_reservation` when the reserved bytes roll into
//!   `tenant_storage_state.bytes_used`.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (DASH-EVICT widget,
//! S-08 canonical rate-limit DO) can extend the taxonomy additively
//! without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use corelink_eviction::EvictionRegion;

use crate::reservation::ReservationId;

/// Canonical Quota audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-07 follow-on WIs (DASH-EVICT widget,
/// S-08 canonical rate-limit DO).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum QuotaEventType {
    /// `corelink.quota.check_passed` — `Allow` decision (read path or
    /// write under boundary).
    CheckPassed,
    /// `corelink.quota.denied_429` — PROVISIONAL transitional 429 +
    /// Retry-After arm fired (per ADR-0020 FROZEN; canonical S-08
    /// hard-block).
    Denied429,
    /// `corelink.quota.reserved` — write request reserved bytes;
    /// carries `reservation_id` + `requested_bytes` + size-proportional
    /// TTL `expires_at_ms`.
    Reserved,
    /// `corelink.quota.reservation_expired` — TTL sweep auto-released
    /// the reservation (no quota leak per INV-QUOTA-RESERVATION-TTL).
    ReservationExpired,
    /// `corelink.quota.reservation_rolled_in` — commit_reservation
    /// rolled the reserved bytes into `tenant_storage_state.bytes_used`.
    ReservationRolledIn,
}

impl QuotaEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CheckPassed => "corelink.quota.check_passed",
            Self::Denied429 => "corelink.quota.denied_429",
            Self::Reserved => "corelink.quota.reserved",
            Self::ReservationExpired => "corelink.quota.reservation_expired",
            Self::ReservationRolledIn => "corelink.quota.reservation_rolled_in",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// `Denied429` is SEV-1 (operator pager wake-up; tenant breach
    /// signal aligning with WI §6.1.10 alert taxonomy
    /// `corelink_quota_denials_total`).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::Denied429)
    }
}

impl core::fmt::Display for QuotaEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.quota.check_passed",
        "corelink.quota.denied_429",
        "corelink.quota.reserved",
        "corelink.quota.reservation_expired",
        "corelink.quota.reservation_rolled_in",
    ]
}

/// Typed Quota audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaAuditRecord {
    /// Canonical event type.
    pub event_type: QuotaEventType,
    /// Verified tenant id (extracted from the `AuthCtx` Tower
    /// middleware; never from the request body / headers per Lote
    /// 10.4bis lesson).
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: EvictionRegion,
    /// Bytes touched by THIS decision — `Some(n)` for `Reserved` /
    /// `ReservationRolledIn` / `ReservationExpired`; `Some(would_use)`
    /// for `Denied429`; `None` for `CheckPassed` (read path).
    pub bytes: Option<u64>,
    /// Reservation id — `Some(_)` for `Reserved` /
    /// `ReservationRolledIn` / `ReservationExpired`; `None` otherwise.
    pub reservation_id: Option<ReservationId>,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"sweeper"` for the TTL alarm cleanup.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`QuotaAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("quota audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the reservation INSERT (S-01 audit_outbox table;
///   fail-closed envelope per Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (forward; `Denied429` IS SEV-1).
pub trait QuotaAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`QuotaAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: QuotaAuditRecord) -> Result<(), QuotaAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
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
    pub fn snapshot_of(
        &self,
        event_type: QuotaEventType,
    ) -> Vec<QuotaAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl QuotaAuditSink for InMemoryQuotaAuditSink {
    fn emit(&self, record: QuotaAuditRecord) -> Result<(), QuotaAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            QuotaAuditSinkError::Store("audit sink mutex poisoned".to_string())
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 503 when the audit emit fires the
/// `Store` error).
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
    fn emit(&self, _record: QuotaAuditRecord) -> Result<(), QuotaAuditSinkError> {
        Err(QuotaAuditSinkError::Store(
            "induced quota audit sink failure (test fixture)".to_string(),
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

    fn rec(t: QuotaEventType) -> QuotaAuditRecord {
        QuotaAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            region: EvictionRegion::Sam,
            bytes: None,
            reservation_id: None,
            created_by_request_id: "test".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            QuotaEventType::CheckPassed,
            QuotaEventType::Denied429,
            QuotaEventType::Reserved,
            QuotaEventType::ReservationExpired,
            QuotaEventType::ReservationRolledIn,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.quota."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 5);
        for s in canonical {
            assert!(s.starts_with("corelink.quota."));
        }
    }

    #[test]
    fn sev1_subset_only_denied_429() {
        // Denied429 IS SEV-1 (operator pager wake-up).
        assert!(QuotaEventType::Denied429.is_sev1());
        // The other arms are NOT SEV-1.
        assert!(!QuotaEventType::CheckPassed.is_sev1());
        assert!(!QuotaEventType::Reserved.is_sev1());
        assert!(!QuotaEventType::ReservationExpired.is_sev1());
        assert!(!QuotaEventType::ReservationRolledIn.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryQuotaAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(QuotaEventType::Reserved)).unwrap();
        sink.emit(rec(QuotaEventType::Denied429)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(QuotaEventType::Reserved).len(), 1);
        assert_eq!(sink.snapshot_of(QuotaEventType::Denied429).len(), 1);
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingQuotaAuditSink::new();
        let err = sink.emit(rec(QuotaEventType::Reserved)).unwrap_err();
        assert!(matches!(err, QuotaAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryQuotaAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(QuotaEventType::CheckPassed)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", QuotaEventType::CheckPassed),
            "corelink.quota.check_passed"
        );
    }
}
