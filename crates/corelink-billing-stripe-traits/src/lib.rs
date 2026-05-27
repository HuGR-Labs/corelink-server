//! Wave-36 Trigger A — port traits + identity/outcome/error types
//! for the Stripe webhook materializer pipeline.
//!
//! # Why this crate exists
//!
//! Wave 36 Stage 2.C closure (`specs/_audits/2026-05-26-w36-stage2c-closure.md`
//! §5.1) identified a dep-graph cycle hazard: under the canonical
//! Stage 3 cargo-deny lockdown (direct `corelink-stripe-real` deps
//! denied for non-umbrella consumers), the materializer crate
//! (`corelink-billing-stripe-materializer`) — which depends on
//! `corelink-stripe-real` solely for the four port traits + identity
//! / outcome / error types defined in
//! `corelink-stripe-real::webhook_dispatch` — would need to migrate
//! through the umbrella `corelink-billing`, creating
//! `billing → materializer → billing` (cycle).
//!
//! Resolution (Option 4 of §5.1, user-authorized 2026-05-27): extract
//! the four trait definitions + their typed wire surface to this new
//! leaf crate. Both `corelink-stripe-real` (which still carries the
//! concrete HTTPS adapter / `WebhookDispatcher` runtime) and
//! `corelink-billing-stripe-materializer` (which carries the
//! D1-backed materializer / audit / idempotency adapters) depend on
//! this crate. Because the crate has **zero** `corelink-*` deps,
//! there is no path from here back to billing — the cycle is broken
//! by construction.
//!
//! # Layering invariants
//!
//! - **Zero `corelink-*` deps.** Enforced by `Cargo.toml` (and any
//!   future dep addition MUST keep this invariant — it is the entire
//!   purpose of the crate).
//! - **`#![forbid(unsafe_code)]`.** Same charter as every other
//!   billing-side crate.
//! - **All public enums + structs are `#[non_exhaustive]`** so future
//!   variants / fields do not break consumers.
//! - **No tokio.** The traits are sync; async runtime concerns live
//!   in the concrete adapter crates.
//! - **No `unwrap`/`expect`/`panic`/`indexing_slicing` in lib code.**
//!
//! # Symbol catalogue (extracted from
//! `corelink-stripe-real::webhook_dispatch`)
//!
//! Traits (4):
//!
//! - [`AuditEmitter`] — pluggable audit sink (fail-CLOSED contract).
//! - [`IdempotencyStore`] — dedup attempt outcome.
//! - [`StateMaterializer`] — per-event-type state mutation seam.
//! - [`SliRecorder`] — per-dispatch SLI observation sink.
//!
//! Identity / wire / outcome / error types:
//!
//! - [`CanonicalWebhookEventType`]
//! - [`StripeWebhookEnvelope`]
//! - [`IdempotencyToken`]
//! - [`IdempotencyOutcome`]
//! - [`MaterializerError`]
//! - [`AuditRecord`]
//! - [`AuditOutcome`]
//! - [`SliObservation`]
//! - [`DispatchResponse`]
//! - [`SLI_BILLING_STRIPE_EVENT_SECONDS`] constant.
//!
//! Concrete adapters (HTTPS, test fakes, in-memory stores) stay in
//! `corelink-stripe-real`; this crate is trait-surface-only.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use core::fmt;

use serde::Deserialize;

// =========================================================================
// Canonical 10-element event taxonomy.
// =========================================================================

/// The canonical Stripe event taxonomy this dispatcher recognises.
///
/// `Unknown` is the forward-compat sink for any Stripe event-type
/// string outside the 10 enumerated arms; the dispatcher acks with
/// 200 + emits an audit row so we can observe the unknown rate without
/// breaking when Stripe ships new event types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CanonicalWebhookEventType {
    /// `customer.subscription.deleted`        (state mutator).
    SubscriptionDeleted,
    /// `customer.subscription.updated`        (state mutator).
    SubscriptionUpdated,
    /// `invoice.paid`                         (state mutator).
    InvoicePaid,
    /// `invoice.payment_failed`               (state mutator).
    InvoicePaymentFailed,
    /// `charge.dispute.created`               (state mutator).
    ChargeDisputeCreated,
    /// `customer.subscription.created`        (observability echo).
    SubscriptionCreated,
    /// `customer.subscription.trial_will_end` (observability echo).
    SubscriptionTrialWillEnd,
    /// `charge.refunded`                      (observability echo).
    ChargeRefunded,
    /// `customer.created`                     (observability echo).
    CustomerCreated,
    /// `invoice.created`                      (observability echo).
    InvoiceCreated,
    /// Any event type outside the 10-element canonical set.
    Unknown,
}

impl CanonicalWebhookEventType {
    /// Map a Stripe event type string into a canonical variant.
    ///
    /// Returns [`Self::Unknown`] for any type outside the SLA-required
    /// 10-element set; never errors.
    #[must_use]
    pub fn classify(raw: &str) -> Self {
        match raw {
            "customer.subscription.deleted" => Self::SubscriptionDeleted,
            "customer.subscription.updated" => Self::SubscriptionUpdated,
            "invoice.paid" => Self::InvoicePaid,
            "invoice.payment_failed" => Self::InvoicePaymentFailed,
            "charge.dispute.created" => Self::ChargeDisputeCreated,
            "customer.subscription.created" => Self::SubscriptionCreated,
            "customer.subscription.trial_will_end" => Self::SubscriptionTrialWillEnd,
            "charge.refunded" => Self::ChargeRefunded,
            "customer.created" => Self::CustomerCreated,
            "invoice.created" => Self::InvoiceCreated,
            _ => Self::Unknown,
        }
    }

    /// Wire string for SLI label / audit `event_type` field.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SubscriptionDeleted => "customer.subscription.deleted",
            Self::SubscriptionUpdated => "customer.subscription.updated",
            Self::InvoicePaid => "invoice.paid",
            Self::InvoicePaymentFailed => "invoice.payment_failed",
            Self::ChargeDisputeCreated => "charge.dispute.created",
            Self::SubscriptionCreated => "customer.subscription.created",
            Self::SubscriptionTrialWillEnd => "customer.subscription.trial_will_end",
            Self::ChargeRefunded => "charge.refunded",
            Self::CustomerCreated => "customer.created",
            Self::InvoiceCreated => "invoice.created",
            Self::Unknown => "unknown",
        }
    }

    /// True iff the dispatcher should materialize per-event state. The
    /// observability-only echoes (`*_created` etc.) return false.
    #[must_use]
    pub const fn is_state_mutator(self) -> bool {
        matches!(
            self,
            Self::SubscriptionDeleted
                | Self::SubscriptionUpdated
                | Self::InvoicePaid
                | Self::InvoicePaymentFailed
                | Self::ChargeDisputeCreated
        )
    }

    /// All ten canonical SLA-required event types (excludes `Unknown`).
    #[must_use]
    pub const fn sla_event_types() -> [Self; 10] {
        [
            Self::SubscriptionDeleted,
            Self::SubscriptionUpdated,
            Self::InvoicePaid,
            Self::InvoicePaymentFailed,
            Self::ChargeDisputeCreated,
            Self::SubscriptionCreated,
            Self::SubscriptionTrialWillEnd,
            Self::ChargeRefunded,
            Self::CustomerCreated,
            Self::InvoiceCreated,
        ]
    }
}

// =========================================================================
// Wire envelope (deserialized AFTER signature verify).
// =========================================================================

/// Top-level Stripe event envelope. Only the canonical addressing
/// fields are typed; the payload sub-object stays loose
/// (`serde_json::Value`) because every event-type has a different
/// shape and the materializer extracts what it needs.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct StripeWebhookEnvelope {
    /// Stripe-assigned event id, e.g. `evt_1Nf2k3Xyz...`.
    pub id: String,
    /// Stripe event-type string, e.g. `customer.subscription.deleted`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Inner `data.object` (loose).
    #[serde(default)]
    pub data: serde_json::Value,
    /// Stripe `created` timestamp (unix seconds). Absent on some events.
    #[serde(default)]
    pub created: u64,
}

// =========================================================================
// BLAKE3 idempotency token.
// =========================================================================

/// 32-byte BLAKE3 digest derived from the Stripe event id. Equal
/// digests imply equal event ids (collision probability < 2^-128).
///
/// Used as the dedup primary-key in the `stripe_event_log` D1 table.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdempotencyToken([u8; 32]);

impl IdempotencyToken {
    /// Derive a token from a Stripe event id by hashing
    /// `b"stripe-event-id:" || event_id` with BLAKE3. The domain prefix
    /// pins the hash family to this use-case so future reuses (e.g.
    /// hashing aggregate counters) cannot collide by construction.
    #[must_use]
    pub fn from_event_id(event_id: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"stripe-event-id:");
        hasher.update(event_id.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    /// Raw 32-byte digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// 64-char lower-hex representation; stable for DB rows / logs.
    #[must_use]
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Debug for IdempotencyToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 8-char prefix; never dump full token for log-correlation
        // hygiene (the token is derived from a non-secret event id, but
        // mirroring CAS hashes keeps logs uniformly short).
        write!(f, "IdempotencyToken({}...)", &self.to_hex()[..8])
    }
}

// =========================================================================
// Idempotency store trait.
// =========================================================================

/// Outcome of an idempotency dedup attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdempotencyOutcome {
    /// Token was new; dispatch should proceed.
    FirstSight,
    /// Token already present; dispatch must be skipped (Stripe retry).
    AlreadyProcessed,
}

/// Pluggable dedup store. Production wires this to a D1
/// `INSERT OR IGNORE INTO stripe_event_log` statement; tests use the
/// in-memory fake.
pub trait IdempotencyStore: fmt::Debug + Send + Sync {
    /// Attempt to insert `token`. Returns
    /// [`IdempotencyOutcome::FirstSight`] iff the row was created, or
    /// [`IdempotencyOutcome::AlreadyProcessed`] iff the row already
    /// existed. Returns `Err(String)` on transient backend failure
    /// (caller propagates as HTTP 500 → Stripe retries).
    fn try_insert(
        &self,
        token: IdempotencyToken,
        event_type: CanonicalWebhookEventType,
        now_ms: u64,
    ) -> Result<IdempotencyOutcome, String>;
}

// =========================================================================
// State materializer trait.
// =========================================================================

/// Errors a materializer can surface to the dispatcher.
#[derive(Debug)]
#[non_exhaustive]
pub enum MaterializerError {
    /// Transient backend error (D1 unavailable, write conflict, etc.).
    /// Dispatcher returns HTTP 500 → Stripe retries.
    Transient(String),
    /// Permanent input error (envelope shape unexpected, required
    /// field missing, etc.). Dispatcher returns HTTP 422 → Stripe
    /// stops retrying.
    InvalidPayload(String),
}

impl fmt::Display for MaterializerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(s) => write!(f, "transient backend error: {s}"),
            Self::InvalidPayload(s) => write!(f, "invalid payload: {s}"),
        }
    }
}

impl std::error::Error for MaterializerError {}

/// Trait that owns per-event-type state mutation. Production binds
/// this to the canonical D1 writers (`customers`, `subscriptions`,
/// `invoices`, `disputes`) inside `corelink-tier-selection` /
/// `corelink-billing-*`. Tests use the `RecordingStateMaterializer`
/// fake in `corelink-stripe-real`.
///
/// All methods receive the verified envelope. Implementations MUST
/// be idempotent at the row level (a second call with the same event
/// id is impossible past the dispatcher's dedup gate, but downstream
/// rows MUST still tolerate it for the rollback-replay edge case).
pub trait StateMaterializer: fmt::Debug + Send + Sync {
    /// `customer.subscription.deleted` → downgrade tenant to Free.
    fn on_subscription_deleted(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError>;
    /// `customer.subscription.updated` → refresh tier + status.
    fn on_subscription_updated(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError>;
    /// `invoice.paid` → extend access expiry + mark invoice paid.
    fn on_invoice_paid(&self, env: &StripeWebhookEnvelope) -> Result<(), MaterializerError>;
    /// `invoice.payment_failed` → set grace-period flag.
    fn on_invoice_payment_failed(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError>;
    /// `charge.dispute.created` → freeze charges + Finance alert.
    fn on_charge_dispute_created(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError>;
}

// =========================================================================
// Audit emission (canonical `corelink.billing.stripe_event_processed.v1`).
// =========================================================================

/// One audit row emitted per dispatched event. The mapping to a
/// CloudEvents-style envelope (or to the canonical
/// `corelink-audit-chain` builder) is the production binder's
/// responsibility; this struct carries the typed fields.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct AuditRecord {
    /// Always `"corelink.billing.stripe_event_processed.v1"`.
    pub event_name: &'static str,
    /// Stripe event id (e.g. `evt_1Nf2k3...`).
    pub stripe_event_id: String,
    /// Canonical (post-classification) event type.
    pub canonical_event_type: CanonicalWebhookEventType,
    /// Outcome — `dispatched`, `duplicate`, `signature_invalid`,
    /// `envelope_invalid`, `materializer_failed`, `materializer_invalid`,
    /// `unknown_event_type`.
    pub outcome: AuditOutcome,
    /// Idempotency token hex (None for early-exit paths before token
    /// derivation — e.g. signature_invalid).
    pub idempotency_token_hex: Option<String>,
    /// Wall-clock ms when the audit record was assembled.
    pub ts_ms: u64,
    /// Optional error detail (free-form, NEVER contains body/secret).
    pub error_detail: Option<String>,
}

impl AuditRecord {
    /// Construct an audit record from its canonical column set.
    ///
    /// The struct is `#[non_exhaustive]`, so out-of-crate brace-init
    /// is unavailable; this constructor is the canonical seam used
    /// by the `corelink-stripe-real::webhook_dispatch::WebhookDispatcher`
    /// and any future binders. Eight parameters reflect the canonical
    /// audit row shape — every field is load-bearing.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        event_name: &'static str,
        stripe_event_id: String,
        canonical_event_type: CanonicalWebhookEventType,
        outcome: AuditOutcome,
        idempotency_token_hex: Option<String>,
        ts_ms: u64,
        error_detail: Option<String>,
    ) -> Self {
        Self {
            event_name,
            stripe_event_id,
            canonical_event_type,
            outcome,
            idempotency_token_hex,
            ts_ms,
            error_detail,
        }
    }
}

/// Outcome enumeration; bound to the audit record. Driven by the
/// dispatcher; emitters never set this themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuditOutcome {
    /// Event dispatched to the materializer and accepted.
    Dispatched,
    /// Duplicate event id; ack 200 with no dispatch.
    Duplicate,
    /// Signature verify failed (replay / mismatch / future-dated /
    /// missing header).
    SignatureInvalid,
    /// JSON envelope did not parse.
    EnvelopeInvalid,
    /// Materializer reported a transient error (500 → Stripe retries).
    MaterializerFailed,
    /// Materializer reported an invalid payload (422; Stripe stops).
    MaterializerInvalid,
    /// Unknown event type acked for forward-compat.
    UnknownEventType,
}

impl AuditOutcome {
    /// Wire string used in SLI labels / audit JSON.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dispatched => "dispatched",
            Self::Duplicate => "duplicate",
            Self::SignatureInvalid => "signature_invalid",
            Self::EnvelopeInvalid => "envelope_invalid",
            Self::MaterializerFailed => "materializer_failed",
            Self::MaterializerInvalid => "materializer_invalid",
            Self::UnknownEventType => "unknown_event_type",
        }
    }
}

/// Pluggable audit sink. Production binds to `corelink-audit-chain`;
/// tests use the in-memory fake. Audit emission is **fail-CLOSED**:
/// a returned `Err` aborts the dispatcher with HTTP 500.
pub trait AuditEmitter: fmt::Debug + Send + Sync {
    /// Emit one audit row. Errors propagate as HTTP 500 (Stripe retries
    /// → next delivery hits the dedup row → resolved without re-dispatch).
    fn emit(&self, record: &AuditRecord) -> Result<(), String>;
}

// =========================================================================
// SLI recorder (`corelink_billing_stripe_event_seconds`).
// =========================================================================

/// Canonical SLI histogram name emitted per dispatched webhook.
/// Underscore-separated per Prom canonical naming (Lote 10.9bis P0-E).
pub const SLI_BILLING_STRIPE_EVENT_SECONDS: &str = "corelink_billing_stripe_event_seconds";

/// One SLI observation. Labels are explicit (no map) so the production
/// binder can wire them into any registry without runtime label-map
/// coercion. The wire shape matches the canonical Prom histogram.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct SliObservation {
    /// Always `"corelink_billing_stripe_event_seconds"`.
    pub metric_name: &'static str,
    /// Latency seconds (wall-clock; observed by the caller around the
    /// whole pipeline). `f64` so the histogram bucketing is uniform.
    pub seconds: f64,
    /// Canonical event-type label.
    pub event_type: CanonicalWebhookEventType,
    /// Outcome label.
    pub outcome: AuditOutcome,
}

impl SliObservation {
    /// Construct an SLI observation. Same `#[non_exhaustive]` seam
    /// rationale as [`AuditRecord::new`]; used by every binder that
    /// emits observations against the canonical histogram.
    #[must_use]
    pub const fn new(
        metric_name: &'static str,
        seconds: f64,
        event_type: CanonicalWebhookEventType,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            metric_name,
            seconds,
            event_type,
            outcome,
        }
    }
}

/// Pluggable SLI sink.
pub trait SliRecorder: fmt::Debug + Send + Sync {
    /// Record one observation. Implementations MUST NOT block.
    fn observe(&self, obs: SliObservation);
}

// =========================================================================
// Dispatcher response (HTTP-status-coded outcome).
// =========================================================================

/// HTTP-status-coded outcome of one dispatch pipeline call.
/// Mapped 1:1 to PCI DSS SAQ-A error envelope per `compliance_matrix.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DispatchResponse {
    /// 200 OK — dispatched, duplicate-acked, or unknown-event-type-acked.
    Ok200,
    /// 400 Bad Request — missing/non-ascii `Stripe-Signature` header.
    BadRequest400,
    /// 401 Unauthorized — signature verify failed (HMAC mismatch / replay).
    Unauthorized401,
    /// 422 Unprocessable Entity — envelope JSON malformed OR
    /// materializer reported permanent input error.
    Unprocessable422,
    /// 500 Internal Server Error — transient backend / audit / materializer
    /// failure. Stripe retries.
    InternalError500,
}

impl DispatchResponse {
    /// Bare HTTP status code (u16).
    #[must_use]
    pub const fn status_code(self) -> u16 {
        match self {
            Self::Ok200 => 200,
            Self::BadRequest400 => 400,
            Self::Unauthorized401 => 401,
            Self::Unprocessable422 => 422,
            Self::InternalError500 => 500,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn classify_matches_all_ten_sla_event_types() {
        for canon in CanonicalWebhookEventType::sla_event_types() {
            assert_eq!(CanonicalWebhookEventType::classify(canon.label()), canon);
        }
    }

    #[test]
    fn state_mutator_flag_marks_canonical_five() {
        let mut mutators = 0;
        for canon in CanonicalWebhookEventType::sla_event_types() {
            if canon.is_state_mutator() {
                mutators += 1;
            }
        }
        assert_eq!(mutators, 5);
    }

    #[test]
    fn classify_unknown_for_outside_set() {
        assert_eq!(
            CanonicalWebhookEventType::classify("stripe.future.type"),
            CanonicalWebhookEventType::Unknown
        );
    }

    #[test]
    fn blake3_idempotency_token_stable_per_event_id() {
        let t1 = IdempotencyToken::from_event_id("evt_001");
        let t2 = IdempotencyToken::from_event_id("evt_001");
        let t3 = IdempotencyToken::from_event_id("evt_002");
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
        assert_eq!(t1.to_hex().len(), 64);
    }

    #[test]
    fn dispatch_response_status_codes() {
        assert_eq!(DispatchResponse::Ok200.status_code(), 200);
        assert_eq!(DispatchResponse::BadRequest400.status_code(), 400);
        assert_eq!(DispatchResponse::Unauthorized401.status_code(), 401);
        assert_eq!(DispatchResponse::Unprocessable422.status_code(), 422);
        assert_eq!(DispatchResponse::InternalError500.status_code(), 500);
    }

    #[test]
    fn audit_outcome_labels_stable() {
        assert_eq!(AuditOutcome::Dispatched.label(), "dispatched");
        assert_eq!(AuditOutcome::Duplicate.label(), "duplicate");
        assert_eq!(AuditOutcome::SignatureInvalid.label(), "signature_invalid");
    }

    #[test]
    fn idempotency_token_debug_short_prefix() {
        let t = IdempotencyToken::from_event_id("evt_dbg");
        let s = format!("{t:?}");
        assert!(s.starts_with("IdempotencyToken("));
        assert!(s.contains("..."));
    }
}
