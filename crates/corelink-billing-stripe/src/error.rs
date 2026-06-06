//! Canonical error taxonomy for the `corelink-billing-stripe` adapter.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline).
//!
//! ## Fail policy boundary
//!
//! Per WI-S10-003 §1 + sprint contract §5.3 R-S10-6, the Stripe adapter
//! surface is **fail-CLOSED at the billing layer** mirroring the
//! `corelink-billing-aggregator` discipline: a partial Stripe charge
//! state would cause Layer 3 reconciliation drift > 0.1% = customer
//! dispute. Therefore every decision arm halts on ANY error; SEV-1 / SEV-2
//! alert fires; manual replay via the canonical retry queue protocol
//! after operator triage.
//!
//! See `corelink-billing-emit::error` (fail-OPEN at hot path) +
//! `corelink-billing-aggregator::error` (fail-CLOSED at aggregation
//! layer) for the canonical split-tier policy lattice per Lote 10.6bis.

use thiserror::Error;

/// Audit-of-audit sink failure surface (lifted into [`StripeError::Audit`]).
///
/// The audit-of-audit envelope (`corelink.billing_stripe.{usage_recorded,
/// duplicate_rejected, webhook_received, signature_rejected,
/// signature_verified, signature_skew_rejected}`) emits BEFORE state
/// mutation per the canonical S-07 P1-1 fix + Lote 10.6bis pattern: audit
/// failure aborts the run; the Stripe usage ledger + the webhook event
/// log remain at their pre-call state.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StripeAuditSinkError {
    /// Backend transport failure (D1 audit_outbox INSERT rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`StripeError::Audit`] and returns to the caller; no
    /// Stripe API call, no webhook event dispatch, no state mutation.
    #[error("billing-stripe audit sink store error: {0}")]
    Store(String),
}

/// Stripe usage-record ledger failure surface (lifted into
/// [`StripeError::Ledger`]).
///
/// Production wiring binds this to the Stripe API
/// `POST /v1/subscription_items/{id}/usage_records` endpoint with the
/// `Idempotency-Key` header set to the canonical 64-char hex digest from
/// [`crate::idempotency::derive_idempotency_key`]; the in-memory fake
/// here pins the same `(tenant_id, subscription_item_id, idempotency_key)`
/// UNIQUE PK at the trait surface so adversarial tests can falsify
/// INV-BILLING-NO-DUP independently of the production HTTP client
/// (deferred to WI-S10-007 PRR ship gate per `trait-abstraction-defer`
/// charter pattern).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StripeUsageLedgerError {
    /// Backend transport failure (Stripe API 5xx / network partition /
    /// rate-limit 429 over retry budget). Production wiring at WI-S10-007
    /// surfaces a SEV-2 alert + halts the per-tenant invoicing pipeline;
    /// the canonical PAT-QUEUE-EVENTS-001 retry queue retains the
    /// pending usage records.
    #[error("billing-stripe usage ledger backend error: {0}")]
    Backend(String),

    /// Idempotency-Key reuse with diverged canonical bytes: a usage
    /// record was previously recorded for the same canonical
    /// `Idempotency-Key` but the recomputed canonical aggregate bytes
    /// diverge from the stored bytes. Per WI-S10-003 §10 it is
    /// structurally impossible (same aggregate → same canonical bytes →
    /// same BLAKE3 idempotency key) but the defensive arm is required
    /// because Stripe's 24h idempotency window can outlast a code-bug
    /// regression that changed the canonicalization rule.
    #[error(
        "billing-stripe idempotency-key reuse with diverged canonical bytes (CRITICAL replay corruption signal): key={idempotency_key}"
    )]
    IdempotencyKeyReuse {
        /// The canonical 64-char hex idempotency key.
        idempotency_key: String,
    },
}

/// Stripe webhook event log failure surface (lifted into
/// [`StripeError::WebhookLog`]).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StripeWebhookLogError {
    /// Backend transport failure (D1 INSERT rejected / network
    /// partition).
    #[error("billing-stripe webhook log backend error: {0}")]
    Backend(String),
}

/// Canonical error surface returned by the [`crate::adapter`] +
/// [`crate::webhook`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StripeError {
    /// JCS canonicalization (RFC 8785) failed when deriving the
    /// idempotency key from the aggregate bytes. Fail-CLOSED at the
    /// trait surface — without canonical bytes the idempotency key
    /// cannot be derived; a same-aggregate retry would no longer
    /// reproduce the same key, so retrying would create a duplicate
    /// Stripe charge. The production wiring queues the usage record for
    /// retry only after the underlying serialization issue is resolved.
    #[error("billing-stripe JCS canonicalization failed: {0}")]
    Canonicalization(String),

    /// Audit envelope rejection (audit sink unavailable). Fail-CLOSED
    /// per Lote 10.6bis pattern: no Stripe API call, no webhook
    /// dispatch, no state mutation.
    #[error("billing-stripe audit envelope failure (state unchanged): {0}")]
    Audit(#[from] StripeAuditSinkError),

    /// Underlying Stripe usage-record ledger failure (Stripe API rejected
    /// the request or the in-memory fake observed an idempotency-key
    /// reuse with diverged bytes).
    #[error("billing-stripe usage ledger failure: {0}")]
    Ledger(#[from] StripeUsageLedgerError),

    /// Webhook event log failure (D1 INSERT rejected).
    #[error("billing-stripe webhook event log failure: {0}")]
    WebhookLog(#[from] StripeWebhookLogError),

    /// Webhook signature rejected at the canonical verification surface
    /// (HMAC-SHA256 mismatch, malformed `Stripe-Signature` header,
    /// unknown scheme version, or hex decode failure). Per WI-S10-003
    /// §6.1 the rejection is the canonical fail-CLOSED arm: SEV-2
    /// alert fires; the request is rejected; the audit
    /// `corelink.billing_stripe.signature_rejected` arm fires BEFORE
    /// the rejection so observability is complete.
    #[error("billing-stripe webhook signature rejected: {0}")]
    SignatureRejected(String),

    /// Webhook replay-window rejected at the canonical 5-minute skew
    /// boundary (`(now - t) > 300s`). Per Stripe webhook signature spec
    /// the canonical 5-min tolerance window guards against signature
    /// capture + delayed replay attacks. Fail-CLOSED per WI-S10-003
    /// §6.1.5 R-007 mitigation; SEV-2 alert.
    #[error(
        "billing-stripe webhook timestamp skew exceeds 5-min tolerance: now_ms={now_ms} signature_ts_ms={signature_ts_ms} delta_ms={delta_ms}"
    )]
    SignatureSkewRejected {
        /// Receiver-side wall-clock instant (Unix epoch ms).
        now_ms: u64,
        /// Stripe-side wall-clock instant from the canonical
        /// `Stripe-Signature: t=<unix_ts>` field, converted to ms.
        signature_ts_ms: u64,
        /// Absolute skew in milliseconds.
        delta_ms: u64,
    },

    /// Internal invariant violation (e.g. mutex poisoned by a panicking
    /// run; usize → u64 truncation on a 32-bit target which is
    /// structurally unreachable for the wasm32 worker; etc.). Production
    /// wiring maps this to fail-CLOSED at the trait surface — the run
    /// aborts rather than risk corrupt state.
    #[error("billing-stripe internal state invariant violated: {0}")]
    Internal(String),
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

    #[test]
    fn canonicalization_displays_diagnostic() {
        let e = StripeError::Canonicalization("non-finite float".into());
        let s = format!("{e}");
        assert!(s.contains("JCS canonicalization"));
    }

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = StripeAuditSinkError::Store("induced".to_string());
        let e: StripeError = inner.into();
        assert!(matches!(e, StripeError::Audit(_)));
    }

    #[test]
    fn from_ledger_error_lifts_cleanly() {
        let inner = StripeUsageLedgerError::Backend("induced".to_string());
        let e: StripeError = inner.into();
        assert!(matches!(e, StripeError::Ledger(_)));
    }

    #[test]
    fn from_webhook_log_error_lifts_cleanly() {
        let inner = StripeWebhookLogError::Backend("induced".to_string());
        let e: StripeError = inner.into();
        assert!(matches!(e, StripeError::WebhookLog(_)));
    }

    #[test]
    fn signature_rejected_displays_diagnostic() {
        let e = StripeError::SignatureRejected("HMAC-SHA256 mismatch".to_string());
        let s = format!("{e}");
        assert!(s.contains("signature rejected"));
    }

    #[test]
    fn signature_skew_rejected_displays_diagnostic() {
        let e = StripeError::SignatureSkewRejected {
            now_ms: 1_000_000_000_000,
            signature_ts_ms: 999_999_400_000,
            delta_ms: 600_000,
        };
        let s = format!("{e}");
        assert!(s.contains("5-min tolerance"));
        assert!(s.contains("delta_ms=600000"));
    }

    #[test]
    fn idempotency_key_reuse_displays_diagnostic() {
        let e = StripeUsageLedgerError::IdempotencyKeyReuse {
            idempotency_key: "ab".repeat(32),
        };
        let s = format!("{e}");
        assert!(s.contains("CRITICAL replay corruption"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = StripeError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }
}
