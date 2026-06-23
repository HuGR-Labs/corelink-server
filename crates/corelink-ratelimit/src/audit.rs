//! Rate-limit-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-quota::audit` + `corelink-eviction::audit`: a
//! small rate-limit-flavoured sink trait the production wiring composes
//! on top of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S08-001 §6.1.8 freezes the canonical 3-event taxonomy:
//!
//! - `corelink.ratelimit.allowed` — emitted on every `Allow` decision
//!   (sampled at handler layer in production; in tests every decision
//!   emits one record).
//! - `corelink.ratelimit.denied_429` — emitted on the 429 deny arm
//!   (carries `cost`, `available_tokens`, `retry_after_secs`).
//! - `corelink.ratelimit.bucket_refilled` — emitted whenever the lazy
//!   refill step actually moves the `available_tokens` watermark
//!   (informational; helps DASH-RATE distinguish "bucket idle"
//!   vs "bucket actively refilling" in the per-tenant token gauge).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (WI-S08-002 per-IP
//! edge / WI-S08-003 quota-checker / WI-S08-004 abuse downgrade) can
//! extend the taxonomy additively without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::key::BucketKey;

/// Canonical rate-limit audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-08 follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum RateLimitEventType {
    /// `corelink.ratelimit.allowed` — `Allow` decision (cost consumed
    /// successfully).
    Allowed,
    /// `corelink.ratelimit.denied_429` — 429 + Retry-After arm fired.
    Denied429,
    /// `corelink.ratelimit.bucket_refilled` — lazy refill step moved
    /// the `available_tokens` watermark.
    BucketRefilled,
}

impl RateLimitEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "corelink.ratelimit.allowed",
            Self::Denied429 => "corelink.ratelimit.denied_429",
            Self::BucketRefilled => "corelink.ratelimit.bucket_refilled",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// `Denied429` is NOT SEV-1 by itself (single-tenant boundary
    /// breach is informational; the SEV-1 alert is on
    /// `corelink.ratelimit.cross_tenant_violation_total > 0` per
    /// WI §6.1.10 which is owned by the metric layer, not the audit
    /// layer).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        false
    }
}

impl core::fmt::Display for RateLimitEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 3] {
    &[
        "corelink.ratelimit.allowed",
        "corelink.ratelimit.denied_429",
        "corelink.ratelimit.bucket_refilled",
    ]
}

/// Typed rate-limit audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimitAuditRecord {
    /// Canonical event type.
    pub event_type: RateLimitEventType,
    /// Verified tenant id (extracted from the `AuthCtx` Tower
    /// middleware; never from the request body / headers per Lote
    /// 10.4bis lesson).
    pub tenant_id: Uuid,
    /// Bucket key (dimension + scope).
    pub bucket_key: BucketKey,
    /// Cost charged by THIS request (in tokens).
    pub cost: u32,
    /// Available tokens AFTER the decision was rendered (rounded down
    /// to integer for the audit trail; the wire-level decision carries
    /// the f64 precision).
    pub available_tokens_after: u64,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"sweeper"` for snapshot maintenance.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`RateLimitAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RateLimitAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("ratelimit audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the bucket UPDATE (S-01 audit_outbox table; fail-closed
///   envelope per Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out to direct SIEM in addition to the
///   outbox.
pub trait RateLimitAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: RateLimitAuditRecord) -> Result<(), RateLimitAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryRateLimitAuditSink {
    inner: std::sync::Arc<Mutex<Vec<RateLimitAuditRecord>>>,
}

impl InMemoryRateLimitAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<RateLimitAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: RateLimitEventType) -> Vec<RateLimitAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl RateLimitAuditSink for InMemoryRateLimitAuditSink {
    fn emit(&self, record: RateLimitAuditRecord) -> Result<(), RateLimitAuditSinkError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| RateLimitAuditSinkError::Store("audit sink mutex poisoned".to_string()))?;
        guard.push(record);
        Ok(())
    }
}

/// Production-safe **bounded** audit sink: accepts every record and
/// drops it (constant memory, zero heap growth per request).
///
/// ## Why this exists (F-022 closure)
///
/// [`InMemoryRateLimitAuditSink`] is a **test capture** sink — it pushes
/// every record onto an unbounded `Vec` that is never drained or capped.
/// Wiring it into the production data-plane layer
/// (`corelink-container::routes::ratelimit_layer`) leaks heap on ordinary
/// in-budget traffic (the limiter emits 1-2 records per request,
/// unconditionally) and can self-OOM a tenant's data-plane container
/// under honest sustained load. This sink is the bounded production
/// default: it satisfies the [`RateLimitAuditSink`] contract with
/// **O(1)** memory and **no allocation** on the hot path.
///
/// ## Forward path
///
/// The documented full prod composition is `OutboxAuditSink` (D1
/// `audit_outbox` INSERT in the bucket-UPDATE batch) + `MultiplexAuditSink`
/// (SIEM fan-out); those land with the live-DO/D1 wiring (WI-S08-006 PRR
/// ship gate). Until then this NoOp sink is the correct production
/// posture — rate-limit audit emit is informational (no SEV-1 arm; see
/// [`RateLimitEventType::is_sev1`]), so dropping it degrades observability
/// granularity, NOT correctness or the fail-closed money path. It is
/// strictly preferable to the unbounded test sink, which trades a leak
/// for the same (test-only) observability.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpRateLimitAuditSink;

impl NoOpRateLimitAuditSink {
    /// Construct a fresh bounded no-op sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RateLimitAuditSink for NoOpRateLimitAuditSink {
    #[inline]
    fn emit(&self, _record: RateLimitAuditRecord) -> Result<(), RateLimitAuditSinkError> {
        // Bounded by construction: accept-and-drop, O(1) memory, never
        // allocates. Always `Ok` so the fail-closed envelope never trips
        // on the informational rate-limit audit arm.
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 503 when the audit emit fires the
/// `Store` error).
#[derive(Debug, Default)]
pub struct FailingRateLimitAuditSink;

impl FailingRateLimitAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RateLimitAuditSink for FailingRateLimitAuditSink {
    fn emit(&self, _record: RateLimitAuditRecord) -> Result<(), RateLimitAuditSinkError> {
        Err(RateLimitAuditSinkError::Store(
            "induced ratelimit audit sink failure (test fixture)".to_string(),
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

    fn rec(t: RateLimitEventType) -> RateLimitAuditRecord {
        RateLimitAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            bucket_key: BucketKey::per_tenant(Uuid::nil()),
            cost: 1,
            available_tokens_after: 0,
            created_by_request_id: "test".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            RateLimitEventType::Allowed,
            RateLimitEventType::Denied429,
            RateLimitEventType::BucketRefilled,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.ratelimit."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 3);
        for s in canonical {
            assert!(s.starts_with("corelink.ratelimit."));
        }
    }

    #[test]
    fn no_arm_is_sev1_at_audit_layer() {
        // SEV-1 alerting on rate-limit lives at the metric layer
        // (cross_tenant_violation_total) per WI §6.1.10, not the audit
        // event taxonomy.
        for t in [
            RateLimitEventType::Allowed,
            RateLimitEventType::Denied429,
            RateLimitEventType::BucketRefilled,
        ] {
            assert!(!t.is_sev1());
        }
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryRateLimitAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(RateLimitEventType::Allowed)).unwrap();
        sink.emit(rec(RateLimitEventType::Denied429)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(RateLimitEventType::Allowed).len(), 1);
        assert_eq!(sink.snapshot_of(RateLimitEventType::Denied429).len(), 1);
    }

    #[test]
    fn noop_sink_accepts_and_drops_every_record() {
        // F-022 regression: the bounded prod sink must accept every
        // record (so the fail-closed envelope never trips on the
        // informational rate-limit arm) and retain NO state.
        let sink = NoOpRateLimitAuditSink::new();
        for _ in 0..10_000 {
            sink.emit(rec(RateLimitEventType::Allowed)).unwrap();
            sink.emit(rec(RateLimitEventType::BucketRefilled)).unwrap();
        }
        // NoOp is a zero-sized type — there is nothing to grow. Pin that
        // it carries no per-record state at the type level.
        assert_eq!(std::mem::size_of::<NoOpRateLimitAuditSink>(), 0);
        // Cloning is a no-op copy (Copy) — confirms no shared buffer.
        let _clone = sink;
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingRateLimitAuditSink::new();
        let err = sink.emit(rec(RateLimitEventType::Allowed)).unwrap_err();
        assert!(matches!(err, RateLimitAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryRateLimitAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(RateLimitEventType::Allowed)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", RateLimitEventType::Allowed),
            "corelink.ratelimit.allowed"
        );
    }
}
