//! Audit-of-audit (meta-audit) trait + InMemory test sink for the
//! `corelink-billing-stripe` adapter surface.
//!
//! Mirrors the `corelink-billing-aggregator::audit` discipline: every
//! adapter / webhook decision arm fires its own audit event BEFORE state
//! mutation per the canonical Lote 10.6bis pattern + S-07 P1-1 fix. The
//! Stripe adapter is THE financial-integrity boundary between CoreLink
//! aggregates (WI-S10-002) and Stripe customer-facing invoices; a
//! corrupted decision arm without a meta-audit row would mean the daily
//! reconciliation worker (WI-S10-004) cannot retroactively diagnose the
//! root cause of a chargeback / dispute / drift event.
//!
//! WI-S10-003 §6.1 freezes the canonical 6-event taxonomy:
//!
//! - `corelink.billing_stripe.usage_recorded` — usage record landed at
//!   Stripe (production HTTP client returned 2xx; in-memory fake
//!   accepted the canonical idempotency key).
//! - `corelink.billing_stripe.duplicate_rejected` — usage record was a
//!   duplicate within the canonical Stripe 24h idempotency window; the
//!   Stripe ledger is unchanged; the audit fires for visibility.
//! - `corelink.billing_stripe.webhook_received` — webhook payload
//!   reached the handler (BEFORE any signature verification). Pins the
//!   wall-clock arrival watermark for forensic reconstruction; SEV-3
//!   monitor for unexpected spikes.
//! - `corelink.billing_stripe.signature_rejected` — webhook
//!   `Stripe-Signature` HMAC-SHA256 verify diverged from the claimed
//!   `v1=` field, OR the header was malformed, OR the scheme version is
//!   unknown. SEV-2 alert (potential attack signal).
//! - `corelink.billing_stripe.signature_verified` — signature verified +
//!   timestamp inside the canonical 5-min replay window. Pins the
//!   accepted-event watermark for forensic reconstruction.
//! - `corelink.billing_stripe.signature_skew_rejected` — signature
//!   verified BUT the timestamp delta exceeds the canonical 5-min
//!   tolerance window (`(now - t) > 300s`); SEV-2 alert (potential
//!   replay attack).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs can extend the
//! taxonomy additively without breaking downstream sinks.

use std::sync::Mutex;

/// Canonical billing-stripe meta-audit taxonomy. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum StripeAuditEventType {
    /// `corelink.billing_stripe.usage_recorded` — usage record landed
    /// at Stripe.
    UsageRecorded,
    /// `corelink.billing_stripe.duplicate_rejected` — duplicate usage
    /// record short-circuited within the Stripe 24h idempotency
    /// window.
    DuplicateRejected,
    /// `corelink.billing_stripe.webhook_received` — webhook payload
    /// reached the handler (BEFORE any signature verification).
    WebhookReceived,
    /// `corelink.billing_stripe.signature_rejected` — webhook
    /// `Stripe-Signature` HMAC-SHA256 verify diverged.
    SignatureRejected,
    /// `corelink.billing_stripe.signature_verified` — webhook
    /// signature verified + timestamp inside 5-min replay window.
    SignatureVerified,
    /// `corelink.billing_stripe.signature_skew_rejected` — webhook
    /// timestamp delta exceeds canonical 5-min tolerance.
    SignatureSkewRejected,
}

impl StripeAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsageRecorded => "corelink.billing_stripe.usage_recorded",
            Self::DuplicateRejected => "corelink.billing_stripe.duplicate_rejected",
            Self::WebhookReceived => "corelink.billing_stripe.webhook_received",
            Self::SignatureRejected => "corelink.billing_stripe.signature_rejected",
            Self::SignatureVerified => "corelink.billing_stripe.signature_verified",
            Self::SignatureSkewRejected => "corelink.billing_stripe.signature_skew_rejected",
        }
    }

    /// Whether this variant fires SEV-2 alerting per WI-S10-003 §6.1
    /// risk register R-003 / R-004 (signature forgery / replay
    /// attempt).
    #[must_use]
    pub const fn is_sev2(self) -> bool {
        matches!(
            self,
            Self::SignatureRejected | Self::SignatureSkewRejected
        )
    }
}

impl core::fmt::Display for StripeAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_stripe_audit_event_strings() -> &'static [&'static str; 6] {
    &[
        "corelink.billing_stripe.usage_recorded",
        "corelink.billing_stripe.duplicate_rejected",
        "corelink.billing_stripe.webhook_received",
        "corelink.billing_stripe.signature_rejected",
        "corelink.billing_stripe.signature_verified",
        "corelink.billing_stripe.signature_skew_rejected",
    ]
}

/// Typed billing-stripe meta-audit record. Production wiring serializes
/// via a CloudEvents 1.0 envelope (mirrors the audit-chain S-09
/// inheritance); the trait surface accepts the typed shape so the sink +
/// the envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripeAuditRecord {
    /// Canonical event type.
    pub event_type: StripeAuditEventType,
    /// Tenant id of the originating decision (`None` if the audit row
    /// is webhook-side and the tenant id has not yet been resolved
    /// from the Stripe customer id).
    pub tenant_id: Option<uuid::Uuid>,
    /// Producer-side wall-clock instant (Unix epoch ms; canonical
    /// `_ms` suffix).
    pub now_ms: u64,
    /// Free-form context (e.g. canonical idempotency key hex,
    /// signature rejection reason, Stripe event id). Reserved for
    /// adversarial debug + dashboard widget grouping.
    pub context: String,
}

/// Audit-of-audit emit error alias (lifted from
/// [`crate::error::StripeAuditSinkError`]).
pub use crate::error::StripeAuditSinkError as StripeAuditEmitError;

/// Audit-of-audit sink trait. Production wiring composes:
///
/// - `OutboxStripeAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the Stripe usage-record write;  fail-CLOSED envelope
///   per Lote 10.6bis pattern + S-07 P1-1 fix.
/// - `MultiplexStripeAuditSink` — fan-out to direct SIEM in addition
///   to the outbox.
pub trait StripeAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to abort
    /// the run fail-CLOSED at THIS trait surface (per WI-S10-003 §1
    /// invariant 12: the Stripe adapter is fail-CLOSED at the billing
    /// layer; no fail-OPEN policy override).
    ///
    /// # Errors
    ///
    /// Returns [`StripeAuditEmitError::Store`] on any backend failure.
    fn emit(&self, record: StripeAuditRecord) -> Result<(), StripeAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryStripeAuditSink {
    inner: std::sync::Arc<Mutex<Vec<StripeAuditRecord>>>,
}

impl InMemoryStripeAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<StripeAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: StripeAuditEventType) -> Vec<StripeAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl StripeAuditSink for InMemoryStripeAuditSink {
    fn emit(&self, record: StripeAuditRecord) -> Result<(), StripeAuditEmitError> {
        let mut guard = self.inner.lock().map_err(|_| {
            StripeAuditEmitError::Store(
                "billing-stripe audit sink mutex poisoned".to_string(),
            )
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingStripeAuditSink;

impl FailingStripeAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl StripeAuditSink for FailingStripeAuditSink {
    fn emit(&self, _record: StripeAuditRecord) -> Result<(), StripeAuditEmitError> {
        Err(StripeAuditEmitError::Store(
            "induced billing-stripe audit sink failure (test fixture)".to_string(),
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
    use uuid::Uuid;

    fn rec(t: StripeAuditEventType) -> StripeAuditRecord {
        StripeAuditRecord {
            event_type: t,
            tenant_id: Some(Uuid::now_v7()),
            now_ms: 1,
            context: "test".to_string(),
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            StripeAuditEventType::UsageRecorded,
            StripeAuditEventType::DuplicateRejected,
            StripeAuditEventType::WebhookReceived,
            StripeAuditEventType::SignatureRejected,
            StripeAuditEventType::SignatureVerified,
            StripeAuditEventType::SignatureSkewRejected,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.billing_stripe."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_stripe_audit_event_strings();
        assert_eq!(canonical.len(), 6);
        for s in canonical {
            assert!(s.starts_with("corelink.billing_stripe."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(StripeAuditEventType::SignatureRejected.is_sev2());
        assert!(StripeAuditEventType::SignatureSkewRejected.is_sev2());
        assert!(!StripeAuditEventType::UsageRecorded.is_sev2());
        assert!(!StripeAuditEventType::DuplicateRejected.is_sev2());
        assert!(!StripeAuditEventType::WebhookReceived.is_sev2());
        assert!(!StripeAuditEventType::SignatureVerified.is_sev2());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryStripeAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(StripeAuditEventType::UsageRecorded)).unwrap();
        sink.emit(rec(StripeAuditEventType::WebhookReceived)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(StripeAuditEventType::UsageRecorded).len(),
            1
        );
        assert_eq!(
            sink.snapshot_of(StripeAuditEventType::WebhookReceived).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingStripeAuditSink::new();
        let err = sink
            .emit(rec(StripeAuditEventType::UsageRecorded))
            .unwrap_err();
        assert!(matches!(err, StripeAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryStripeAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(StripeAuditEventType::UsageRecorded)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", StripeAuditEventType::SignatureSkewRejected),
            "corelink.billing_stripe.signature_skew_rejected"
        );
    }
}
