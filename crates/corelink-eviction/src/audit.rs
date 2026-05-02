//! Eviction-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-gc::audit` + `corelink-dedup::audit`: a small
//! eviction-flavoured sink trait the production wiring composes on top
//! of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit chain
//! processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S07-002 §6.1.10 freezes the canonical 5-event taxonomy:
//!
//! - `corelink.evict.evicted` — emitted ONCE per soft-deleted blob
//!   (the [`EvictionDecision::Evict`](crate::EvictionDecision::Evict)
//!   arm). Carries `digest_hex8`, `bytes_reclaimed`, `tier`, `reason`
//!   ("ttl_expired" | "lru_below_quota").
//! - `corelink.evict.skipped_reachable` — emitted when the race-aware
//!   reachable check found `active_refcount > 0` (the
//!   [`EvictionDecision::SkipReachable`](crate::EvictionDecision::SkipReachable)
//!   arm). Carries the offending `ac_meta.created_at_ms` for forensics.
//!   Sustained spike alert — signals INV-GC-001 inheritance is
//!   actively blocking deletes (which is the load-bearing protection;
//!   sustained 0 over a week could indicate the reachable check is
//!   short-circuited).
//! - `corelink.evict.skipped_ttl` — emitted when a candidate row's
//!   `last_accessed_at_ms` is within the per-tier TTL window (the
//!   [`EvictionDecision::SkipTtlNotExpired`](crate::EvictionDecision::SkipTtlNotExpired)
//!   arm). Sampled emission (low cardinality for forensic spot-check;
//!   production wiring SHOULD downsample at handler layer).
//! - `corelink.evict.skipped_quota_ok` — emitted when the per-region
//!   eviction pass observes `bytes_used / bytes_quota < 95%` and the
//!   ad-hoc trigger does NOT fire (the
//!   [`EvictionDecision::SkipQuotaOk`](crate::EvictionDecision::SkipQuotaOk)
//!   arm).
//! - `corelink.evict.quota_trigger_fired` — emitted ONCE when the 95%
//!   trigger arm fires, `target_bytes_to_reclaim` carried for
//!   correlation with subsequent `evicted` records.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (DASH-EVICT
//! widget, S-13 admin override audit) can extend the taxonomy
//! additively without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::region::EvictionRegion;
use crate::tier::Tier;

/// Canonical Eviction audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-07 follow-on WIs (DASH-EVICT widget,
/// S-13 admin override audit).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum EvictionEventType {
    /// `corelink.evict.evicted` — soft-delete fired for a confirmed
    /// orphan blob (INV-EVICT-SOFT-DELETE-FIRST).
    Evicted,
    /// `corelink.evict.skipped_reachable` — race-aware reachable check
    /// returned `active_refcount > 0` (INV-GC-001 inheritance fires).
    SkippedReachable,
    /// `corelink.evict.skipped_ttl` — candidate is within the per-tier
    /// TTL window; not yet eligible.
    SkippedTtl,
    /// `corelink.evict.skipped_quota_ok` — per-region pass observed
    /// `bytes_used < 95% bytes_quota`; ad-hoc eviction does NOT fire.
    SkippedQuotaOk,
    /// `corelink.evict.quota_trigger_fired` — emitted ONCE at the
    /// boundary of the 95% threshold; correlates with subsequent
    /// `Evicted` records via the `quota_trigger_request_id`.
    QuotaTriggerFired,
}

impl EvictionEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Evicted => "corelink.evict.evicted",
            Self::SkippedReachable => "corelink.evict.skipped_reachable",
            Self::SkippedTtl => "corelink.evict.skipped_ttl",
            Self::SkippedQuotaOk => "corelink.evict.skipped_quota_ok",
            Self::QuotaTriggerFired => "corelink.evict.quota_trigger_fired",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// None of the eviction events are SEV-1 — alerts fire at the
    /// metric layer (sustained spike on `cascade_prevented_total` =
    /// SEV-2; INV-GC-001 violation on
    /// `gc_invariant_violation_total` = SEV-0 per WI §6.1.10).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::QuotaTriggerFired)
    }
}

impl core::fmt::Display for EvictionEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.evict.evicted",
        "corelink.evict.skipped_reachable",
        "corelink.evict.skipped_ttl",
        "corelink.evict.skipped_quota_ok",
        "corelink.evict.quota_trigger_fired",
    ]
}

/// Reason mnemonic for the `Evicted` arm (carried in
/// [`EvictionAuditRecord::reason`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EvictionReason {
    /// Blob's `last_accessed_at_ms` is older than the per-tier TTL
    /// window — TTL expired.
    TtlExpired,
    /// Blob is the cold-end of the LRU and the tenant is at >= 95%
    /// quota — LRU + quota pressure.
    LruQuotaPressure,
    /// Skipped — reachable check fired (INV-GC-001 inheritance).
    SkippedReachable,
    /// Skipped — within TTL window.
    SkippedTtl,
    /// Skipped — tenant below 95% quota.
    SkippedQuotaOk,
    /// Quota trigger boundary fired.
    QuotaTriggerFired,
}

impl EvictionReason {
    /// Canonical lower-snake-case reason mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TtlExpired => "ttl_expired",
            Self::LruQuotaPressure => "lru_quota_pressure",
            Self::SkippedReachable => "skipped_reachable",
            Self::SkippedTtl => "skipped_ttl",
            Self::SkippedQuotaOk => "skipped_quota_ok",
            Self::QuotaTriggerFired => "quota_trigger_fired",
        }
    }
}

/// Typed Eviction audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvictionAuditRecord {
    /// Canonical event type.
    pub event_type: EvictionEventType,
    /// Verified tenant id (extracted from the `TenantCtx` Tower
    /// middleware; never from the request body / headers).
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: EvictionRegion,
    /// Tenant tier (drives metric labels for downstream rollup).
    pub tier: Tier,
    /// First-8-hex prefix of the candidate digest (privacy-friendly
    /// forensic trail). Empty for `QuotaTriggerFired` (no per-blob
    /// scope).
    pub digest_hex8: String,
    /// Bytes reclaimed by THIS decision — `Some(n)` for `Evicted`;
    /// `None` for skip arms; `Some(target)` for `QuotaTriggerFired`.
    pub bytes: Option<u64>,
    /// Reason mnemonic.
    pub reason: EvictionReason,
    /// Source attribution: the request id (`x-request-id` header or
    /// `"cron"` for the daily scheduled tick).
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`EvictionAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EvictionAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("eviction audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the `blob_meta.deleted_at` UPDATE (S-01 audit_outbox
///   table; fail-closed envelope per Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (forward; `QuotaTriggerFired` IS SEV-1 per WI
///   §6.1.10 alert taxonomy).
pub trait EvictionAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`EvictionAuditSinkError::Store`] on any backend
    /// failure.
    fn emit(&self, record: EvictionAuditRecord)
        -> Result<(), EvictionAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryEvictionAuditSink {
    inner: std::sync::Arc<Mutex<Vec<EvictionAuditRecord>>>,
}

impl InMemoryEvictionAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<EvictionAuditRecord> {
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
        event_type: EvictionEventType,
    ) -> Vec<EvictionAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl EvictionAuditSink for InMemoryEvictionAuditSink {
    fn emit(
        &self,
        record: EvictionAuditRecord,
    ) -> Result<(), EvictionAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            EvictionAuditSinkError::Store(
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
pub struct FailingEvictionAuditSink;

impl FailingEvictionAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl EvictionAuditSink for FailingEvictionAuditSink {
    fn emit(
        &self,
        _record: EvictionAuditRecord,
    ) -> Result<(), EvictionAuditSinkError> {
        Err(EvictionAuditSinkError::Store(
            "induced audit sink failure (test fixture)".to_string(),
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

    fn rec(t: EvictionEventType) -> EvictionAuditRecord {
        EvictionAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            region: EvictionRegion::Sam,
            tier: Tier::Free,
            digest_hex8: String::new(),
            bytes: None,
            reason: EvictionReason::TtlExpired,
            created_by_request_id: "cron".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            EvictionEventType::Evicted,
            EvictionEventType::SkippedReachable,
            EvictionEventType::SkippedTtl,
            EvictionEventType::SkippedQuotaOk,
            EvictionEventType::QuotaTriggerFired,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.evict."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 5);
        for s in canonical {
            assert!(s.starts_with("corelink.evict."));
        }
    }

    #[test]
    fn sev1_subset_only_quota_trigger_fired() {
        // QuotaTriggerFired is SEV-1 (operator pager wake-up; tenant
        // breach signal). Other arms NOT SEV-1.
        assert!(EvictionEventType::QuotaTriggerFired.is_sev1());
        assert!(!EvictionEventType::Evicted.is_sev1());
        assert!(!EvictionEventType::SkippedReachable.is_sev1());
        assert!(!EvictionEventType::SkippedTtl.is_sev1());
        assert!(!EvictionEventType::SkippedQuotaOk.is_sev1());
    }

    #[test]
    fn reason_canonical_strings() {
        assert_eq!(EvictionReason::TtlExpired.as_str(), "ttl_expired");
        assert_eq!(
            EvictionReason::LruQuotaPressure.as_str(),
            "lru_quota_pressure"
        );
        assert_eq!(
            EvictionReason::SkippedReachable.as_str(),
            "skipped_reachable"
        );
        assert_eq!(EvictionReason::SkippedTtl.as_str(), "skipped_ttl");
        assert_eq!(
            EvictionReason::SkippedQuotaOk.as_str(),
            "skipped_quota_ok"
        );
        assert_eq!(
            EvictionReason::QuotaTriggerFired.as_str(),
            "quota_trigger_fired"
        );
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryEvictionAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(EvictionEventType::Evicted)).unwrap();
        sink.emit(rec(EvictionEventType::SkippedReachable)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(EvictionEventType::Evicted).len(),
            1
        );
        assert_eq!(
            sink.snapshot_of(EvictionEventType::SkippedReachable).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingEvictionAuditSink::new();
        let err = sink.emit(rec(EvictionEventType::Evicted)).unwrap_err();
        assert!(matches!(err, EvictionAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryEvictionAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(EvictionEventType::Evicted)).unwrap();
        assert_eq!(s2.len(), 1);
    }
}
