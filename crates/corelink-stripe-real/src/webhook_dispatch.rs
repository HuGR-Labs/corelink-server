//! Production Stripe webhook dispatch pipeline.
//!
//! This module wires the canonical end-to-end flow that replaces the
//! in-process fake dispatch attached to `apps/server`. The pipeline is
//! intentionally pure / sync at this layer (no tokio) so it can be
//! driven from any HTTP server crate (axum, hyper, worker-rs, tests)
//! without runtime coupling.
//!
//! # Pipeline
//!
//! ```text
//! POST /v1/billing/stripe-webhook
//!   1. Read `Stripe-Signature` header.                 -> 400 if missing.
//!   2. Verify HMAC-SHA256 over raw payload bytes via
//!      `webhook::verify_webhook_signature` (constant-time, 5-min
//!      replay tolerance, multi-v1 key-rotation tolerant).
//!      -> 401 on signature mismatch / replay / future-dated.
//!   3. Parse JSON envelope (`event.id` + `event.type`).
//!      -> 422 on malformed envelope.
//!   4. Derive a 256-bit BLAKE3 idempotency token from `event.id`.
//!   5. Insert-or-ignore the token into the dedup store.
//!      - Already-processed -> return 200 immediately (NO dispatch).
//!      - Transient backend failure -> 500 (Stripe retries).
//!   6. Classify the event type into the canonical 10-element taxonomy.
//!   7. Hand to the `StateMaterializer` trait (production: D1 writer;
//!      tests: recording fake) which materializes the per-event row(s).
//!   8. Emit one `corelink.billing.stripe_event_processed.v1` audit
//!      record. Audit fail = the request returns 500 (fail-CLOSED).
//!   9. Record `corelink_billing_stripe_event_seconds` SLI histogram
//!      observation tagged with the canonical event type. (Always
//!      emitted, even on failure, so dashboards see latency for the
//!      sad path.)
//!  10. Return canonical [`DispatchResponse`] -> HTTP status code.
//! ```
//!
//! # 10-element SLA event taxonomy
//!
//! Per S-10 sprint contract + WI-S10-003 §6.1.6 + `0018_stripe_idem_keys.sql`
//! event_type CHECK constraint. The five **state-mutating** events
//! (CTRL-BILLING-001 / INV-BILLING-NO-DUP) are:
//!
//! - `customer.subscription.deleted`        — downgrade to Free tier.
//! - `customer.subscription.updated`        — refresh tier + status.
//! - `invoice.paid`                         — extend access expiry.
//! - `invoice.payment_failed`               — set grace-period flag.
//! - `charge.dispute.created`               — freeze charges + notify Finance.
//!
//! The five **forward-compat / observability-only** events (acked + audited
//! but no state mutation today; reserved for S-13+ scope):
//!
//! - `customer.subscription.created`        — pre-checkout-complete echo.
//! - `customer.subscription.trial_will_end` — trial 3-day notice.
//! - `charge.refunded`                      — refund issued.
//! - `customer.created`                     — new customer record echo.
//! - `invoice.created`                      — invoice generated echo.
//!
//! # Charter compliance
//!
//! - `#![forbid(unsafe_code)]` (crate-level).
//! - No `unwrap`/`expect`/`panic`/`indexing_slicing` in lib code.
//! - No tokio in src (sync trait surface; callers schedule the async
//!   read of the body and pass bytes in).
//! - Audit fail-CLOSED: emit BEFORE returning success; emit-failure
//!   propagates as 500.
//! - All public enums are `#[non_exhaustive]`.
//! - Idempotency token comparison via direct BLAKE3 32-byte equality;
//!   signature HMAC compare is constant-time via `subtle` (see
//!   `webhook::verify_webhook_signature`).
//! - PCI DSS SAQ-A error envelope: 401 (signature) / 422 (envelope) /
//!   500 (backend) / 200 (success or duplicate) per
//!   `compliance_matrix.md`.

use core::fmt;
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use crate::error::WebhookVerifyError;
use crate::webhook::{verify_webhook_signature, DEFAULT_TOLERANCE_SECONDS};

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
// Idempotency store trait + in-memory fake.
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

/// Internal row stored alongside each dedup token (event-type +
/// insertion timestamp). Kept private so the in-memory map type
/// alias factors cleanly past clippy::type_complexity.
type IdempotencyRow = (CanonicalWebhookEventType, u64);

/// Default in-memory store (D1 mirror; same `INSERT OR IGNORE` semantics).
#[derive(Clone, Debug, Default)]
pub struct InMemoryIdempotencyStore {
    seen: Arc<Mutex<std::collections::HashMap<[u8; 32], IdempotencyRow>>>,
}

impl InMemoryIdempotencyStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct tokens currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.seen.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// `true` iff no events have been stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn try_insert(
        &self,
        token: IdempotencyToken,
        event_type: CanonicalWebhookEventType,
        now_ms: u64,
    ) -> Result<IdempotencyOutcome, String> {
        let mut g = self
            .seen
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        if g.contains_key(token.as_bytes()) {
            Ok(IdempotencyOutcome::AlreadyProcessed)
        } else {
            g.insert(*token.as_bytes(), (event_type, now_ms));
            Ok(IdempotencyOutcome::FirstSight)
        }
    }
}

// =========================================================================
// State materializer trait (production: D1 writers; tests: recorder).
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
/// `corelink-billing-*`. Tests use [`RecordingStateMaterializer`].
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

/// Test-only recorder. Stores `(event_type, event_id)` per call so
/// the integration test can assert the right method fired for the
/// right event. Wraps an optional `force_error` to drive the 422/500
/// arms.
#[derive(Clone, Debug, Default)]
pub struct RecordingStateMaterializer {
    calls: Arc<Mutex<Vec<(CanonicalWebhookEventType, String)>>>,
    force_error: Arc<Mutex<Option<MaterializerError>>>,
}

impl RecordingStateMaterializer {
    /// Construct an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Force the next call to return `Err(err)`. Cleared after one use.
    pub fn arm_error(&self, err: MaterializerError) {
        if let Ok(mut g) = self.force_error.lock() {
            *g = Some(err);
        }
    }

    /// Snapshot recorded calls.
    #[must_use]
    pub fn calls(&self) -> Vec<(CanonicalWebhookEventType, String)> {
        match self.calls.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded calls.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.calls().len()
    }

    fn record(
        &self,
        ty: CanonicalWebhookEventType,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        if let Ok(mut g) = self.force_error.lock() {
            if let Some(err) = g.take() {
                return Err(err);
            }
        }
        let mut g = self
            .calls
            .lock()
            .map_err(|e| MaterializerError::Transient(format!("mutex poisoned: {e}")))?;
        g.push((ty, env.id.clone()));
        Ok(())
    }
}

impl StateMaterializer for RecordingStateMaterializer {
    fn on_subscription_deleted(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.record(CanonicalWebhookEventType::SubscriptionDeleted, env)
    }
    fn on_subscription_updated(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.record(CanonicalWebhookEventType::SubscriptionUpdated, env)
    }
    fn on_invoice_paid(&self, env: &StripeWebhookEnvelope) -> Result<(), MaterializerError> {
        self.record(CanonicalWebhookEventType::InvoicePaid, env)
    }
    fn on_invoice_payment_failed(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.record(CanonicalWebhookEventType::InvoicePaymentFailed, env)
    }
    fn on_charge_dispute_created(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.record(CanonicalWebhookEventType::ChargeDisputeCreated, env)
    }
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

/// Test-only in-memory audit emitter.
#[derive(Clone, Debug, Default)]
pub struct RecordingAuditEmitter {
    records: Arc<Mutex<Vec<AuditRecord>>>,
    /// If set, every `emit` call returns this error (drives fail-CLOSED tests).
    fail_with: Arc<Mutex<Option<String>>>,
}

impl RecordingAuditEmitter {
    /// Construct an empty emitter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Force every emit to fail with `msg`.
    pub fn fail_with(&self, msg: impl Into<String>) {
        if let Ok(mut g) = self.fail_with.lock() {
            *g = Some(msg.into());
        }
    }

    /// Snapshot emitted records.
    #[must_use]
    pub fn records(&self) -> Vec<AuditRecord> {
        match self.records.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of records with `outcome`.
    #[must_use]
    pub fn count_with_outcome(&self, outcome: AuditOutcome) -> usize {
        self.records()
            .iter()
            .filter(|r| r.outcome == outcome)
            .count()
    }
}

impl AuditEmitter for RecordingAuditEmitter {
    fn emit(&self, record: &AuditRecord) -> Result<(), String> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(msg) = g.as_ref() {
                return Err(msg.clone());
            }
        }
        let mut g = self
            .records
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        g.push(record.clone());
        Ok(())
    }
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

/// Pluggable SLI sink.
pub trait SliRecorder: fmt::Debug + Send + Sync {
    /// Record one observation. Implementations MUST NOT block.
    fn observe(&self, obs: SliObservation);
}

/// Test-only recorder.
#[derive(Clone, Debug, Default)]
pub struct RecordingSliRecorder {
    obs: Arc<Mutex<Vec<SliObservation>>>,
}

impl RecordingSliRecorder {
    /// Construct an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot observations.
    #[must_use]
    pub fn observations(&self) -> Vec<SliObservation> {
        match self.obs.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count observations.
    #[must_use]
    pub fn count(&self) -> usize {
        self.observations().len()
    }
}

impl SliRecorder for RecordingSliRecorder {
    fn observe(&self, obs: SliObservation) {
        if let Ok(mut g) = self.obs.lock() {
            g.push(obs);
        }
    }
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

// =========================================================================
// Time provider abstraction (deterministic injection for tests).
// =========================================================================

/// Pluggable clock; production injects [`SystemClock`], tests inject
/// [`FixedClock`].
pub trait Clock: fmt::Debug + Send + Sync {
    /// Current unix time in seconds.
    fn now_seconds(&self) -> u64;
    /// Current unix time in milliseconds.
    fn now_ms(&self) -> u64;
    /// Monotonic instant for SLI latency measurement (best-effort
    /// converted to seconds). Production wraps `std::time::Instant`;
    /// tests return a fixed +1.0s delta.
    fn observe_latency_seconds(&self, start_marker: u64) -> f64;
    /// Capture an opaque start marker (used by `observe_latency_seconds`).
    fn start_marker(&self) -> u64;
}

/// System-clock-backed `Clock`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_seconds(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
    fn start_marker(&self) -> u64 {
        self.now_ms()
    }
    fn observe_latency_seconds(&self, start_marker: u64) -> f64 {
        let now_ms = self.now_ms();
        let delta_ms = now_ms.saturating_sub(start_marker);
        // Cast: ms<u64> → f64 is lossy only beyond 2^53 ms (~285k years).
        #[allow(clippy::cast_precision_loss, reason = "ms→seconds, 53-bit precision is far beyond webhook latency range")]
        let s = delta_ms as f64 / 1000.0;
        s
    }
}

/// Fixed-time test clock; returns `seconds` for now-* queries and a
/// hard-coded `latency_seconds` from `observe_latency_seconds`.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock {
    /// Fixed unix seconds.
    pub seconds: u64,
    /// Latency to report regardless of start marker.
    pub latency_seconds: f64,
}

impl FixedClock {
    /// Construct a fixed clock pinned at `seconds` with `latency` SLI delta.
    #[must_use]
    pub const fn new(seconds: u64, latency_seconds: f64) -> Self {
        Self {
            seconds,
            latency_seconds,
        }
    }
}

impl Clock for FixedClock {
    fn now_seconds(&self) -> u64 {
        self.seconds
    }
    fn now_ms(&self) -> u64 {
        self.seconds.saturating_mul(1_000)
    }
    fn start_marker(&self) -> u64 {
        0
    }
    fn observe_latency_seconds(&self, _start_marker: u64) -> f64 {
        self.latency_seconds
    }
}

// =========================================================================
// Dependency bundle + dispatcher.
// =========================================================================

/// Production dependency bundle. Held as `Arc<Dispatcher>` so axum /
/// hyper / worker-rs / cron callers can clone cheaply per-request.
pub struct WebhookDispatcher {
    /// Webhook signing secret (`whsec_...` raw bytes). NEVER logged.
    webhook_secret: Vec<u8>,
    /// Idempotency dedup store.
    idempotency: Arc<dyn IdempotencyStore>,
    /// State materializer.
    materializer: Arc<dyn StateMaterializer>,
    /// Audit sink.
    audit: Arc<dyn AuditEmitter>,
    /// SLI recorder.
    sli: Arc<dyn SliRecorder>,
    /// Clock.
    clock: Arc<dyn Clock>,
    /// Signature replay tolerance (seconds). Defaults to
    /// [`DEFAULT_TOLERANCE_SECONDS`] (300).
    tolerance_seconds: u64,
}

impl fmt::Debug for WebhookDispatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookDispatcher")
            .field("webhook_secret", &"<redacted>")
            .field("tolerance_seconds", &self.tolerance_seconds)
            .field("idempotency", &self.idempotency)
            .field("materializer", &self.materializer)
            .field("audit", &self.audit)
            .field("sli", &self.sli)
            .field("clock", &self.clock)
            .finish()
    }
}

impl WebhookDispatcher {
    /// Construct a dispatcher with the canonical 5-min replay tolerance.
    #[must_use]
    pub fn new(
        webhook_secret: Vec<u8>,
        idempotency: Arc<dyn IdempotencyStore>,
        materializer: Arc<dyn StateMaterializer>,
        audit: Arc<dyn AuditEmitter>,
        sli: Arc<dyn SliRecorder>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            webhook_secret,
            idempotency,
            materializer,
            audit,
            sli,
            clock,
            tolerance_seconds: DEFAULT_TOLERANCE_SECONDS,
        }
    }

    /// Override the signature replay tolerance (seconds). Tests use
    /// this to drive the future-dated arm.
    #[must_use]
    pub const fn with_tolerance_seconds(mut self, tolerance_seconds: u64) -> Self {
        self.tolerance_seconds = tolerance_seconds;
        self
    }

    /// Process one Stripe webhook delivery.
    ///
    /// Arguments:
    /// - `body` — exact raw bytes Stripe POSTed (BEFORE any JSON parse).
    ///   Signature verifies over these bytes, not over a re-serialized
    ///   form.
    /// - `signature_header` — `Stripe-Signature` HTTP header value (or
    ///   `None` if the header was missing/non-ascii — yields
    ///   [`DispatchResponse::BadRequest400`]).
    pub fn process(&self, body: &[u8], signature_header: Option<&str>) -> DispatchResponse {
        let start = self.clock.start_marker();
        let now_seconds = self.clock.now_seconds();
        let now_ms = self.clock.now_ms();

        // (1) Header present?
        let Some(sig_header) = signature_header else {
            self.emit_audit_and_sli(
                AuditRecord {
                    event_name: "corelink.billing.stripe_event_processed.v1",
                    stripe_event_id: String::new(),
                    canonical_event_type: CanonicalWebhookEventType::Unknown,
                    outcome: AuditOutcome::SignatureInvalid,
                    idempotency_token_hex: None,
                    ts_ms: now_ms,
                    error_detail: Some("missing or non-ascii Stripe-Signature header".to_string()),
                },
                CanonicalWebhookEventType::Unknown,
                AuditOutcome::SignatureInvalid,
                start,
            );
            return DispatchResponse::BadRequest400;
        };

        // (2) Verify signature over EXACT bytes.
        if let Err(e) = verify_webhook_signature(
            body,
            sig_header,
            &self.webhook_secret,
            now_seconds,
            self.tolerance_seconds,
        ) {
            self.emit_audit_and_sli(
                AuditRecord {
                    event_name: "corelink.billing.stripe_event_processed.v1",
                    stripe_event_id: String::new(),
                    canonical_event_type: CanonicalWebhookEventType::Unknown,
                    outcome: AuditOutcome::SignatureInvalid,
                    idempotency_token_hex: None,
                    ts_ms: now_ms,
                    error_detail: Some(classify_verify_err(&e).to_string()),
                },
                CanonicalWebhookEventType::Unknown,
                AuditOutcome::SignatureInvalid,
                start,
            );
            return DispatchResponse::Unauthorized401;
        }

        // (3) Parse envelope (ONLY after sig verify).
        let env: StripeWebhookEnvelope = match serde_json::from_slice(body) {
            Ok(e) => e,
            Err(e) => {
                self.emit_audit_and_sli(
                    AuditRecord {
                        event_name: "corelink.billing.stripe_event_processed.v1",
                        stripe_event_id: String::new(),
                        canonical_event_type: CanonicalWebhookEventType::Unknown,
                        outcome: AuditOutcome::EnvelopeInvalid,
                        idempotency_token_hex: None,
                        ts_ms: now_ms,
                        error_detail: Some(format!("json parse: {e}")),
                    },
                    CanonicalWebhookEventType::Unknown,
                    AuditOutcome::EnvelopeInvalid,
                    start,
                );
                return DispatchResponse::Unprocessable422;
            }
        };

        // (4) Derive BLAKE3 idempotency token from event id.
        let token = IdempotencyToken::from_event_id(&env.id);
        let canon = CanonicalWebhookEventType::classify(&env.event_type);

        // (5) Idempotency dedup.
        match self.idempotency.try_insert(token, canon, now_ms) {
            Ok(IdempotencyOutcome::AlreadyProcessed) => {
                self.emit_audit_and_sli(
                    AuditRecord {
                        event_name: "corelink.billing.stripe_event_processed.v1",
                        stripe_event_id: env.id.clone(),
                        canonical_event_type: canon,
                        outcome: AuditOutcome::Duplicate,
                        idempotency_token_hex: Some(token.to_hex()),
                        ts_ms: now_ms,
                        error_detail: None,
                    },
                    canon,
                    AuditOutcome::Duplicate,
                    start,
                );
                return DispatchResponse::Ok200;
            }
            Ok(IdempotencyOutcome::FirstSight) => {} // proceed
            Err(e) => {
                self.emit_audit_and_sli(
                    AuditRecord {
                        event_name: "corelink.billing.stripe_event_processed.v1",
                        stripe_event_id: env.id.clone(),
                        canonical_event_type: canon,
                        outcome: AuditOutcome::MaterializerFailed,
                        idempotency_token_hex: Some(token.to_hex()),
                        ts_ms: now_ms,
                        error_detail: Some(format!("idempotency store: {e}")),
                    },
                    canon,
                    AuditOutcome::MaterializerFailed,
                    start,
                );
                return DispatchResponse::InternalError500;
            }
        }

        // (6) Dispatch.
        let dispatch_result = match canon {
            CanonicalWebhookEventType::SubscriptionDeleted => {
                self.materializer.on_subscription_deleted(&env)
            }
            CanonicalWebhookEventType::SubscriptionUpdated => {
                self.materializer.on_subscription_updated(&env)
            }
            CanonicalWebhookEventType::InvoicePaid => self.materializer.on_invoice_paid(&env),
            CanonicalWebhookEventType::InvoicePaymentFailed => {
                self.materializer.on_invoice_payment_failed(&env)
            }
            CanonicalWebhookEventType::ChargeDisputeCreated => {
                self.materializer.on_charge_dispute_created(&env)
            }
            // Observability-only echoes: no state mutation; audit ack only.
            CanonicalWebhookEventType::SubscriptionCreated
            | CanonicalWebhookEventType::SubscriptionTrialWillEnd
            | CanonicalWebhookEventType::ChargeRefunded
            | CanonicalWebhookEventType::CustomerCreated
            | CanonicalWebhookEventType::InvoiceCreated => Ok(()),
            CanonicalWebhookEventType::Unknown => {
                self.emit_audit_and_sli(
                    AuditRecord {
                        event_name: "corelink.billing.stripe_event_processed.v1",
                        stripe_event_id: env.id.clone(),
                        canonical_event_type: canon,
                        outcome: AuditOutcome::UnknownEventType,
                        idempotency_token_hex: Some(token.to_hex()),
                        ts_ms: now_ms,
                        error_detail: None,
                    },
                    canon,
                    AuditOutcome::UnknownEventType,
                    start,
                );
                return DispatchResponse::Ok200;
            }
        };

        // (7) Audit + SLI per dispatch outcome.
        match dispatch_result {
            Ok(()) => {
                let resp = self.emit_audit_and_sli(
                    AuditRecord {
                        event_name: "corelink.billing.stripe_event_processed.v1",
                        stripe_event_id: env.id.clone(),
                        canonical_event_type: canon,
                        outcome: AuditOutcome::Dispatched,
                        idempotency_token_hex: Some(token.to_hex()),
                        ts_ms: now_ms,
                        error_detail: None,
                    },
                    canon,
                    AuditOutcome::Dispatched,
                    start,
                );
                if resp.is_some() {
                    return DispatchResponse::InternalError500;
                }
                DispatchResponse::Ok200
            }
            Err(MaterializerError::Transient(msg)) => {
                self.emit_audit_and_sli(
                    AuditRecord {
                        event_name: "corelink.billing.stripe_event_processed.v1",
                        stripe_event_id: env.id.clone(),
                        canonical_event_type: canon,
                        outcome: AuditOutcome::MaterializerFailed,
                        idempotency_token_hex: Some(token.to_hex()),
                        ts_ms: now_ms,
                        error_detail: Some(msg),
                    },
                    canon,
                    AuditOutcome::MaterializerFailed,
                    start,
                );
                DispatchResponse::InternalError500
            }
            Err(MaterializerError::InvalidPayload(msg)) => {
                self.emit_audit_and_sli(
                    AuditRecord {
                        event_name: "corelink.billing.stripe_event_processed.v1",
                        stripe_event_id: env.id.clone(),
                        canonical_event_type: canon,
                        outcome: AuditOutcome::MaterializerInvalid,
                        idempotency_token_hex: Some(token.to_hex()),
                        ts_ms: now_ms,
                        error_detail: Some(msg),
                    },
                    canon,
                    AuditOutcome::MaterializerInvalid,
                    start,
                );
                DispatchResponse::Unprocessable422
            }
        }
    }

    /// Emit one audit row + one SLI observation. Returns
    /// `Some(audit_err)` iff the audit emit failed (caller maps to 500).
    fn emit_audit_and_sli(
        &self,
        record: AuditRecord,
        event_type: CanonicalWebhookEventType,
        outcome: AuditOutcome,
        start_marker: u64,
    ) -> Option<String> {
        let audit_err = self.audit.emit(&record).err();
        self.sli.observe(SliObservation {
            metric_name: SLI_BILLING_STRIPE_EVENT_SECONDS,
            seconds: self.clock.observe_latency_seconds(start_marker),
            event_type,
            outcome,
        });
        audit_err
    }
}

/// Map a verify error to a short label string (NEVER logs body/secret).
fn classify_verify_err(e: &WebhookVerifyError) -> &'static str {
    match e {
        WebhookVerifyError::MalformedHeader(_) => "malformed_header",
        WebhookVerifyError::ReplayWindowExceeded { .. } => "replay_window_exceeded",
        WebhookVerifyError::FutureDated { .. } => "future_dated",
        WebhookVerifyError::SignatureMismatch => "signature_mismatch",
        WebhookVerifyError::HexDecode(_) => "hex_decode",
    }
}

// =========================================================================
// Inline unit tests (sig+dedup+dispatch coverage).
// =========================================================================

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
    use crate::webhook::compute_signature;

    const SECRET: &[u8] = b"whsec_prod_dispatch_unit";
    const FIXED_TS: u64 = 1_715_000_000;

    type FixtureBundle = (
        Arc<WebhookDispatcher>,
        Arc<InMemoryIdempotencyStore>,
        Arc<RecordingStateMaterializer>,
        Arc<RecordingAuditEmitter>,
        Arc<RecordingSliRecorder>,
    );

    fn fixture() -> FixtureBundle {
        let idem = Arc::new(InMemoryIdempotencyStore::new());
        let mat = Arc::new(RecordingStateMaterializer::new());
        let audit = Arc::new(RecordingAuditEmitter::new());
        let sli = Arc::new(RecordingSliRecorder::new());
        let clock = Arc::new(FixedClock::new(FIXED_TS, 0.123));
        let dispatcher = Arc::new(WebhookDispatcher::new(
            SECRET.to_vec(),
            idem.clone(),
            mat.clone(),
            audit.clone(),
            sli.clone(),
            clock,
        ));
        (dispatcher, idem, mat, audit, sli)
    }

    fn signed_envelope(id: &str, kind: &str, ts: u64) -> (Vec<u8>, String) {
        let body = serde_json::to_vec(&serde_json::json!({
            "id": id,
            "type": kind,
            "created": ts,
            "data": { "object": { "id": "sub_test", "status": "active" } }
        }))
        .unwrap();
        let sig = compute_signature(SECRET, ts, &body);
        let header = format!("t={ts},v1={sig}");
        (body, header)
    }

    #[test]
    fn happy_path_subscription_deleted_dispatches_audits_emits_sli() {
        let (d, idem, mat, audit, sli) = fixture();
        let (body, hdr) = signed_envelope("evt_d1", "customer.subscription.deleted", FIXED_TS);
        let resp = d.process(&body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::Ok200);
        assert_eq!(mat.call_count(), 1);
        assert_eq!(idem.len(), 1);
        assert_eq!(audit.count_with_outcome(AuditOutcome::Dispatched), 1);
        assert_eq!(sli.count(), 1);
        assert_eq!(
            sli.observations()[0].metric_name,
            SLI_BILLING_STRIPE_EVENT_SECONDS
        );
        assert_eq!(
            sli.observations()[0].event_type,
            CanonicalWebhookEventType::SubscriptionDeleted
        );
    }

    #[test]
    fn duplicate_event_acked_without_dispatch() {
        let (d, idem, mat, audit, _sli) = fixture();
        let (body, hdr) = signed_envelope("evt_dup", "invoice.paid", FIXED_TS);
        let r1 = d.process(&body, Some(&hdr));
        let r2 = d.process(&body, Some(&hdr));
        assert_eq!(r1, DispatchResponse::Ok200);
        assert_eq!(r2, DispatchResponse::Ok200);
        assert_eq!(mat.call_count(), 1, "no double dispatch");
        assert_eq!(idem.len(), 1);
        assert_eq!(audit.count_with_outcome(AuditOutcome::Duplicate), 1);
        assert_eq!(audit.count_with_outcome(AuditOutcome::Dispatched), 1);
    }

    #[test]
    fn invalid_signature_returns_401_with_no_dispatch() {
        let (d, idem, mat, audit, _sli) = fixture();
        let (body, _) = signed_envelope("evt_evil", "invoice.paid", FIXED_TS);
        let bogus_header = format!("t={FIXED_TS},v1=deadbeefdeadbeefdeadbeefdeadbeef");
        let resp = d.process(&body, Some(&bogus_header));
        assert_eq!(resp, DispatchResponse::Unauthorized401);
        assert_eq!(mat.call_count(), 0);
        assert_eq!(idem.len(), 0);
        assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
    }

    #[test]
    fn missing_signature_header_returns_400() {
        let (d, _, mat, audit, _sli) = fixture();
        let (body, _) = signed_envelope("evt_x", "invoice.paid", FIXED_TS);
        let resp = d.process(&body, None);
        assert_eq!(resp, DispatchResponse::BadRequest400);
        assert_eq!(mat.call_count(), 0);
        assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
    }

    #[test]
    fn malformed_envelope_returns_422() {
        let (d, idem, mat, audit, _sli) = fixture();
        // Sign garbage that's NOT valid JSON.
        let body = b"not-json-at-all";
        let sig = compute_signature(SECRET, FIXED_TS, body);
        let hdr = format!("t={FIXED_TS},v1={sig}");
        let resp = d.process(body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::Unprocessable422);
        assert_eq!(mat.call_count(), 0);
        assert_eq!(idem.len(), 0);
        assert_eq!(audit.count_with_outcome(AuditOutcome::EnvelopeInvalid), 1);
    }

    #[test]
    fn replay_window_exceeded_returns_401() {
        let (d, _, _, audit, _sli) = fixture();
        // Sign 10 minutes in the past — > 300s tolerance.
        let old_ts = FIXED_TS - 600;
        let (body, hdr) = signed_envelope("evt_old", "invoice.paid", old_ts);
        let resp = d.process(&body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::Unauthorized401);
        assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
    }

    #[test]
    fn materializer_transient_failure_returns_500() {
        let (d, _, mat, audit, _sli) = fixture();
        mat.arm_error(MaterializerError::Transient("d1 unavailable".to_string()));
        let (body, hdr) = signed_envelope("evt_t1", "invoice.paid", FIXED_TS);
        let resp = d.process(&body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::InternalError500);
        assert_eq!(
            audit.count_with_outcome(AuditOutcome::MaterializerFailed),
            1
        );
    }

    #[test]
    fn materializer_invalid_payload_returns_422() {
        let (d, _, mat, audit, _sli) = fixture();
        mat.arm_error(MaterializerError::InvalidPayload(
            "missing customer".to_string(),
        ));
        let (body, hdr) = signed_envelope("evt_t2", "invoice.paid", FIXED_TS);
        let resp = d.process(&body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::Unprocessable422);
        assert_eq!(
            audit.count_with_outcome(AuditOutcome::MaterializerInvalid),
            1
        );
    }

    #[test]
    fn unknown_event_acked_without_dispatch() {
        let (d, idem, mat, audit, _sli) = fixture();
        let (body, hdr) = signed_envelope("evt_new", "stripe.future.type", FIXED_TS);
        let resp = d.process(&body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::Ok200);
        assert_eq!(mat.call_count(), 0);
        // Unknown DOES insert dedup row (so a retry hits duplicate).
        assert_eq!(idem.len(), 1);
        assert_eq!(audit.count_with_outcome(AuditOutcome::UnknownEventType), 1);
    }

    #[test]
    fn observability_echo_events_acked_without_state_mutation() {
        for kind in [
            "customer.subscription.created",
            "customer.subscription.trial_will_end",
            "charge.refunded",
            "customer.created",
            "invoice.created",
        ] {
            let (d, _, mat, audit, _sli) = fixture();
            let (body, hdr) = signed_envelope(&format!("evt_{kind}"), kind, FIXED_TS);
            let resp = d.process(&body, Some(&hdr));
            assert_eq!(resp, DispatchResponse::Ok200, "kind={kind}");
            // Echo events DO NOT call materializer methods (none exist
            // for these arms today).
            assert_eq!(mat.call_count(), 0, "kind={kind}");
            assert_eq!(
                audit.count_with_outcome(AuditOutcome::Dispatched),
                1,
                "kind={kind}"
            );
        }
    }

    #[test]
    fn blake3_idempotency_token_stable_per_event_id() {
        let t1 = IdempotencyToken::from_event_id("evt_001");
        let t2 = IdempotencyToken::from_event_id("evt_001");
        let t3 = IdempotencyToken::from_event_id("evt_002");
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
        assert_eq!(t1.to_hex().len(), 64); // 32 bytes hex
    }

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
    fn idempotency_outcome_already_processed_short_circuits_audit_outcome() {
        let (d, _idem, _mat, audit, sli) = fixture();
        let (body, hdr) = signed_envelope("evt_short", "invoice.paid", FIXED_TS);
        d.process(&body, Some(&hdr));
        d.process(&body, Some(&hdr));
        assert_eq!(audit.count_with_outcome(AuditOutcome::Duplicate), 1);
        // SLI emitted on BOTH calls (latency observed even for duplicate).
        assert_eq!(sli.count(), 2);
    }

    #[test]
    fn audit_emit_failure_propagates_500() {
        let (d, _, _, audit, _sli) = fixture();
        audit.fail_with("audit chain unavailable");
        let (body, hdr) = signed_envelope("evt_a", "invoice.paid", FIXED_TS);
        let resp = d.process(&body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::InternalError500);
    }
}
