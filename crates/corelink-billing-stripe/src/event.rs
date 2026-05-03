//! Canonical Stripe-adapter types: [`IdempotencyKey`] newtype +
//! [`SubscriptionItemId`] newtype + [`UsageRecordRequest`] +
//! [`StripeAdapterDecision`] + [`WebhookEventKind`] +
//! [`WebhookEvent`].
//!
//! ## Why a typed [`IdempotencyKey`] (not raw `String`)
//!
//! Per WI-S10-003 §10 + sprint contract §5.3 R-S10-6 the
//! `Idempotency-Key` HTTP header MUST be the canonical 64-char hex
//! BLAKE3-256 of the JCS-canonical aggregate bytes. Wrapping the value
//! in a newtype lets the trait surface refuse arbitrary strings
//! (defense-in-depth against accidental mis-derivation at the call
//! site).
//!
//! ## Why typed [`WebhookEventKind`] enum (not `serde_json::Value`)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 (absorbed via WI-S10-003 §1
//! invariant 6): Stripe webhook payloads contain customer PII (email,
//! name, billing address). An untyped `serde_json::Value` defeats
//! compile-time PII redaction enforcement; a typed enum + per-variant
//! payload struct lets downstream PII wrappers (`CustomerEmail`,
//! `CustomerName` per WI-S10-003 §6.1) interpose at the serialization
//! boundary so the audit log NEVER persists raw email / name.
//!
//! The typed enum is `#[non_exhaustive]` so follow-on WIs (S-13 admin /
//! S-19 enterprise onboarding) can extend the taxonomy additively
//! without breaking downstream `match` sites.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical 32-byte BLAKE3-256 idempotency-key newtype. Always
/// hex-rendered when serialized to JSON for the on-the-wire shape (RFC
/// 4648 §8 hex-lowercase canonical form).
///
/// Stripe's `Idempotency-Key` header accepts up to 255 chars; a 64-char
/// hex-lowercase digest fits well below the limit + matches the 32-byte
/// BLAKE3-256 output (mirrors `corelink_billing_aggregator::ChainHash`
/// + `corelink_billing_emit::IdemKey` discipline).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdempotencyKey(pub [u8; 32]);

impl IdempotencyKey {
    /// Construct directly from the 32-byte BLAKE3 digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 32-byte array.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Render as a 64-char lowercase hex string (the canonical
    /// `Idempotency-Key` header value).
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Length of the hex-rendered form (always 64).
    #[must_use]
    pub const fn hex_len() -> usize {
        64
    }
}

impl Serialize for IdempotencyKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for IdempotencyKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let bytes = hex::decode(&s).map_err(|e| {
            serde::de::Error::custom(format!("IdempotencyKey hex decode failed: {e}"))
        })?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "IdempotencyKey expected 32 bytes; got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(Self(out))
    }
}

impl core::fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Canonical Stripe `subscription_item_id` newtype (e.g. `si_1A2B3C…`).
/// Wrapped in a newtype so the trait surface refuses raw strings —
/// production wiring fetches the per-tenant subscription_item_id from
/// the Neon Postgres `subscription` table at usage-record emit time.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionItemId(String);

impl SubscriptionItemId {
    /// Construct from a raw Stripe `si_*` string. Per WI-S10-003 §6.1
    /// the production wiring populates this from the Neon
    /// `subscription` table; the trait surface accepts the canonical
    /// shape.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for SubscriptionItemId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Canonical Stripe usage-record request shape, as accepted by the
/// adapter trait. Production wiring serializes this to the canonical
/// Stripe API form
/// `POST /v1/subscription_items/{id}/usage_records` with body
/// `quantity={qty}&timestamp={ts}&action=increment` + the
/// `Idempotency-Key` header set to [`UsageRecordRequest::idempotency_key`].
///
/// `quantity` is `u128` because the upstream `AggregatedCounter::data
/// .total_qty` is `u128`; the production wiring saturating-coerces to
/// `u64` at the Stripe API surface (Stripe usage records accept up to
/// 2^53 per the API docs; CoreLink billing volume is 10^9 events/yr ×
/// max qty bytes ≈ 10^28 < 2^96 < u128::MAX so the saturation is
/// defensive).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecordRequest {
    /// Tenant id (per-tenant subscription scoping).
    pub tenant_id: Uuid,
    /// Canonical billing period (`YYYY-MM`).
    pub billing_period: String,
    /// Canonical event kind discriminant string (mirrors
    /// `UsageEventKind::as_str()`).
    pub event_kind: String,
    /// Total billable quantity (sum of contributing event qty's per
    /// the upstream `AggregatedCounter::data.total_qty`).
    pub total_qty: u128,
    /// Stripe `subscription_item_id` (e.g. `si_1A2B3C…`).
    pub subscription_item_id: SubscriptionItemId,
    /// Canonical `Idempotency-Key` derived from BLAKE3-256 of the
    /// JCS-canonical aggregate bytes.
    pub idempotency_key: IdempotencyKey,
    /// Wall-clock instant of the request emit (Unix epoch ms;
    /// canonical `_ms` suffix).
    pub now_ms: u64,
}

/// Canonical Stripe webhook event kind taxonomy. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs (S-13 admin / S-19 enterprise onboarding).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WebhookEventKind {
    /// `invoice.created` — Stripe created a draft invoice.
    InvoiceCreated,
    /// `invoice.paid` — Stripe processed payment successfully.
    InvoicePaid,
    /// `invoice.payment_failed` — Stripe failed to process payment.
    InvoiceFailed,
    /// `customer.subscription.updated` — subscription state changed
    /// (e.g. tier upgrade, cancel, pause).
    SubscriptionUpdated,
    /// `customer.created` — Stripe minted a new customer object.
    CustomerCreated,
}

impl WebhookEventKind {
    /// Canonical Stripe event-kind string (mirrors the on-the-wire
    /// `type` attribute of the Stripe webhook payload).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvoiceCreated => "invoice.created",
            Self::InvoicePaid => "invoice.paid",
            Self::InvoiceFailed => "invoice.payment_failed",
            Self::SubscriptionUpdated => "customer.subscription.updated",
            Self::CustomerCreated => "customer.created",
        }
    }
}

impl core::fmt::Display for WebhookEventKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical Stripe webhook event shape. Production wiring deserializes
/// the canonical Stripe webhook JSON into this typed shape so the
/// downstream PII wrappers (per WI-S10-003 §6.1) can interpose at
/// serialization. The `payload_redacted` field is the JSON bytes of
/// the Stripe payload AFTER PII redaction (production wiring uses the
/// `CustomerEmail` / `CustomerName` Serialize-impl wrappers; the
/// in-memory fake here treats the field as the canonical bytes for
/// tamper-detection tests).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// Canonical Stripe `evt_*` event id (UNIQUE per delivery; used as
    /// the dedup key in the webhook event log).
    pub stripe_event_id: String,
    /// Canonical event kind (the typed taxonomy).
    pub kind: WebhookEventKind,
    /// Stripe-side wall-clock instant of the event emit (Unix epoch
    /// ms; canonical `_ms` suffix).
    pub event_ts_ms: u64,
    /// PII-redacted JSON bytes of the Stripe payload (per
    /// WI-S10-003 §6.1 the redaction lives at the
    /// `CustomerEmail`/`CustomerName` Serialize-impl wrappers in the
    /// production wiring; the in-memory fake here treats the bytes as
    /// canonical input for tamper-detection tests).
    pub payload_redacted: Vec<u8>,
}

/// Decision returned by [`crate::adapter::StripeBillingAdapter::record_usage`]
/// + [`crate::webhook::StripeWebhookHandler::handle`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StripeAdapterDecision {
    /// Usage record landed at Stripe: the canonical
    /// `Idempotency-Key` was unseen + the production HTTP client (or
    /// in-memory fake) recorded the line item + the
    /// `corelink.billing_stripe.usage_recorded` audit fired.
    UsageRecorded {
        /// Canonical idempotency key of the recorded line.
        idempotency_key: IdempotencyKey,
        /// Stripe-side `subscription_item_id` of the recorded line.
        subscription_item_id: SubscriptionItemId,
        /// Tenant id of the recorded line.
        tenant_id: Uuid,
        /// Total billable quantity recorded.
        total_qty: u128,
    },
    /// Usage record was a duplicate: the canonical
    /// `Idempotency-Key` was already accepted within the canonical 24h
    /// Stripe idempotency window (the in-memory fake stores the same
    /// set membership). The chain head + Stripe ledger are unchanged;
    /// the `corelink.billing_stripe.duplicate_rejected` audit fires
    /// for visibility.
    DuplicateRejected {
        /// Canonical idempotency key of the duplicate.
        idempotency_key: IdempotencyKey,
        /// Tenant id of the duplicate.
        tenant_id: Uuid,
    },
    /// Webhook event landed: the signature verified + the timestamp
    /// fell inside the canonical 5-min replay window + the typed
    /// payload deserialized + the
    /// `corelink.billing_stripe.webhook_processed` audit fired.
    WebhookProcessed {
        /// Canonical Stripe event id.
        stripe_event_id: String,
        /// Canonical event kind.
        kind: WebhookEventKind,
    },
    /// Webhook signature rejected: the HMAC-SHA256 verify diverged
    /// from the claimed `v1=` field, OR the `Stripe-Signature` header
    /// was malformed, OR the scheme version is unknown. The
    /// `corelink.billing_stripe.signature_rejected` audit fires BEFORE
    /// the rejection so observability is complete (the request is
    /// rejected with a `StripeError::SignatureRejected`; this
    /// decision arm is exposed for tests + dashboards).
    SignatureRejected {
        /// Free-form rejection reason (mirrors the
        /// `StripeError::SignatureRejected` payload).
        reason: String,
    },
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
    fn idempotency_key_hex_len_canonical() {
        assert_eq!(IdempotencyKey::hex_len(), 64);
    }

    #[test]
    fn idempotency_key_serde_round_trip() {
        let k = IdempotencyKey([0xCD; 32]);
        let s = serde_json::to_string(&k).unwrap();
        // 64 hex chars + 2 quotes = 66.
        assert_eq!(s.len(), 66);
        let back: IdempotencyKey = serde_json::from_str(&s).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn idempotency_key_invalid_hex_rejected() {
        let err = serde_json::from_str::<IdempotencyKey>("\"zz\"").unwrap_err();
        assert!(format!("{err}").contains("hex decode"));
    }

    #[test]
    fn idempotency_key_wrong_length_rejected() {
        let err = serde_json::from_str::<IdempotencyKey>("\"ab\"").unwrap_err();
        assert!(format!("{err}").contains("expected 32 bytes"));
    }

    #[test]
    fn subscription_item_id_round_trips() {
        let s = SubscriptionItemId::new("si_1A2B3C");
        assert_eq!(s.as_str(), "si_1A2B3C");
        let json = serde_json::to_string(&s).unwrap();
        let back: SubscriptionItemId = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn webhook_event_kind_strings_canonical() {
        assert_eq!(WebhookEventKind::InvoiceCreated.as_str(), "invoice.created");
        assert_eq!(WebhookEventKind::InvoicePaid.as_str(), "invoice.paid");
        assert_eq!(
            WebhookEventKind::InvoiceFailed.as_str(),
            "invoice.payment_failed"
        );
        assert_eq!(
            WebhookEventKind::SubscriptionUpdated.as_str(),
            "customer.subscription.updated"
        );
        assert_eq!(WebhookEventKind::CustomerCreated.as_str(), "customer.created");
    }

    #[test]
    fn webhook_event_kind_variants_unique_strings() {
        let v = [
            WebhookEventKind::InvoiceCreated,
            WebhookEventKind::InvoicePaid,
            WebhookEventKind::InvoiceFailed,
            WebhookEventKind::SubscriptionUpdated,
            WebhookEventKind::CustomerCreated,
        ];
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(set.insert(k.as_str()), "duplicate canonical: {k}");
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn webhook_event_kind_display_matches_str() {
        assert_eq!(
            format!("{}", WebhookEventKind::InvoicePaid),
            "invoice.paid"
        );
    }

    #[test]
    fn usage_record_request_round_trips() {
        let tenant = Uuid::now_v7();
        let req = UsageRecordRequest {
            tenant_id: tenant,
            billing_period: "2026-05".to_string(),
            event_kind: "cas_put".to_string(),
            total_qty: 100,
            subscription_item_id: SubscriptionItemId::new("si_1A2B3C"),
            idempotency_key: IdempotencyKey([0xAB; 32]),
            now_ms: 1_700_000_000_000,
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: UsageRecordRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn webhook_event_round_trips() {
        let e = WebhookEvent {
            stripe_event_id: "evt_abc123".to_string(),
            kind: WebhookEventKind::InvoicePaid,
            event_ts_ms: 1_700_000_000_000,
            payload_redacted: b"{\"redacted\":true}".to_vec(),
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: WebhookEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn stripe_adapter_decision_variants_distinct() {
        let tenant = Uuid::now_v7();
        let recorded = StripeAdapterDecision::UsageRecorded {
            idempotency_key: IdempotencyKey([0xAB; 32]),
            subscription_item_id: SubscriptionItemId::new("si_x"),
            tenant_id: tenant,
            total_qty: 1,
        };
        let dup = StripeAdapterDecision::DuplicateRejected {
            idempotency_key: IdempotencyKey([0xAB; 32]),
            tenant_id: tenant,
        };
        let processed = StripeAdapterDecision::WebhookProcessed {
            stripe_event_id: "evt_x".to_string(),
            kind: WebhookEventKind::InvoicePaid,
        };
        let rejected = StripeAdapterDecision::SignatureRejected {
            reason: "HMAC mismatch".to_string(),
        };
        assert_ne!(format!("{recorded:?}"), format!("{dup:?}"));
        assert_ne!(format!("{processed:?}"), format!("{rejected:?}"));
    }
}
