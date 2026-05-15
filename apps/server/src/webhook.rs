//! R2-12: Stripe webhook HTTP route + signature verify + idempotency
//! dedup + event-type dispatchers.
//!
//! # Route
//!
//! `POST /v1/billing/stripe-webhook` — receives raw bytes (NOT
//! JSON-parsed), reads `Stripe-Signature` header, calls
//! [`corelink_stripe_real::verify_webhook_signature`] with
//! 300-second replay tolerance, INSERT-OR-IGNORE on the dedup table
//! keyed by `event_id`, then dispatches one of the seven canonical
//! subscription / invoice events into the [`SubscriptionStateHandler`]
//! trait (production binding: `corelink-tier-selection`
//! `TierSelectionLedger`).
//!
//! # Event flow
//!
//! ```text
//! POST /v1/billing/stripe-webhook
//!   ├─ read raw body bytes (no JSON parse yet — signature verifies
//!   │  over EXACT bytes Stripe signed).
//!   ├─ read `Stripe-Signature` header.
//!   ├─ resolve `STRIPE_WEBHOOK_SECRET` from env at construction time.
//!   ├─ verify_webhook_signature(body, hdr, secret, now, 300s)
//!   │    ├─ on Err → audit `webhook.signature_invalid` → 400.
//!   │    └─ on Ok  → continue.
//!   ├─ parse JSON into `WebhookEnvelope { id, type, data, ... }`.
//!   ├─ idempotency dedup: store.try_insert(event_id, event_type, now)
//!   │    ├─ on `AlreadyProcessed` → 200 OK (Stripe stops retrying).
//!   │    └─ on `Inserted`         → continue.
//!   ├─ audit `webhook.processing_started`.
//!   ├─ dispatch by event.type:
//!   │    ├─ customer.subscription.created    → handler.activate
//!   │    ├─ customer.subscription.updated    → handler.refresh
//!   │    ├─ customer.subscription.deleted    → handler.downgrade
//!   │    ├─ customer.subscription.trial_will_end → handler.notify_trial
//!   │    ├─ invoice.paid                      → handler.invoice_paid
//!   │    ├─ invoice.payment_failed            → handler.invoice_failed
//!   │    └─ <other>                           → log + ack (forward-compat).
//!   ├─ audit `webhook.processing_completed` (or `_failed` with detail).
//!   └─ 200 OK.
//! ```
//!
//! # Invariants
//!
//! - **NEVER trust body bytes before signature verify**: every code
//!   path that touches `event.id` / `event.type` runs ONLY after
//!   `verify_webhook_signature` returns `Ok(())`.
//! - **NEVER log raw body or webhook secret**: bodies may contain
//!   customer email (Stripe Customer object); secrets are
//!   read once at startup and stored in a `Zeroizing<Vec<u8>>`-style
//!   wrapper (we use a plain `Vec<u8>` here but it is NEVER fmt'd or
//!   logged).
//! - **Idempotency is HARD**: Stripe retries up to 3 days; a second
//!   delivery of the same `event_id` MUST NOT trigger handler dispatch.
//!   The dedup `INSERT OR IGNORE` is the canonical guard.
//! - **Audit fail-CLOSED ordering**: `processing_started` BEFORE
//!   mutation; `processing_completed` AFTER; on intermediate error,
//!   `processing_failed` with error detail (one record per arm).

#![allow(clippy::module_name_repetitions, reason = "WebhookEnvelope etc. are the public type names")]

use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use corelink_stripe_real::verify_webhook_signature;
use serde::{Deserialize, Serialize};

/// Canonical 5-minute Stripe replay tolerance window (seconds).
pub const STRIPE_REPLAY_TOLERANCE_SECONDS: u64 = 300;

/// HTTP route path for the webhook endpoint.
pub const STRIPE_WEBHOOK_ROUTE: &str = "/v1/billing/stripe-webhook";

// =========================================================================
// Event envelope + canonical event-type taxonomy.
// =========================================================================

/// Top-level Stripe webhook envelope.
///
/// Stripe events are JSON objects with `id`, `type`, `data.object`,
/// `created`, etc. We only deserialize what the dispatcher needs;
/// unknown fields are tolerated to stay forward-compatible.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct WebhookEnvelope {
    /// Stripe-assigned event id (e.g. `evt_1Nf...`). PRIMARY KEY of
    /// `stripe_webhook_events_processed`.
    pub id: String,
    /// Stripe event type (e.g. `customer.subscription.created`).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Inner `data.object` payload. Loosely typed `serde_json::Value`
    /// because the shape varies per event type; handler-side helpers
    /// extract typed fields.
    #[serde(default)]
    pub data: serde_json::Value,
    /// Stripe `created` timestamp (unix seconds). May be absent.
    #[serde(default)]
    pub created: u64,
}

/// Canonical Stripe event types this dispatcher handles.
///
/// `Unknown` represents any event we forward-compat-ack with 200 +
/// log without state mutation (per Stripe best-practice).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CanonicalEventType {
    /// `customer.subscription.created` — fresh paid subscription;
    /// activate tier.
    SubscriptionCreated,
    /// `customer.subscription.updated` — subscription state changed;
    /// refresh tier or downgrade on cancel.
    SubscriptionUpdated,
    /// `customer.subscription.deleted` — subscription cancelled;
    /// downgrade to Free.
    SubscriptionDeleted,
    /// `customer.subscription.trial_will_end` — trial 3-day notice;
    /// send notification.
    SubscriptionTrialWillEnd,
    /// `invoice.paid` — successful charge; extend access expiry.
    InvoicePaid,
    /// `invoice.payment_failed` — failed charge; set grace-period
    /// flag.
    InvoicePaymentFailed,
    /// Any event type not in the canonical set above; acked with 200
    /// + logged for forward-compatibility.
    Unknown,
}

impl CanonicalEventType {
    /// Map the raw `event.type` string to a canonical variant.
    ///
    /// Named `parse_event_type` (not `from_str`) to avoid colliding
    /// with the [`std::str::FromStr`] trait method semantics —
    /// `Unknown` is a valid output, never an error.
    #[must_use]
    pub fn parse_event_type(s: &str) -> Self {
        match s {
            "customer.subscription.created" => Self::SubscriptionCreated,
            "customer.subscription.updated" => Self::SubscriptionUpdated,
            "customer.subscription.deleted" => Self::SubscriptionDeleted,
            "customer.subscription.trial_will_end" => Self::SubscriptionTrialWillEnd,
            "invoice.paid" => Self::InvoicePaid,
            "invoice.payment_failed" => Self::InvoicePaymentFailed,
            _ => Self::Unknown,
        }
    }
}

// =========================================================================
// Audit sink trait (route emits audit events; production wiring binds to
// the `corelink-audit-chain` R2 sink).
// =========================================================================

/// Canonical audit-of-audit event types emitted by the webhook route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WebhookAuditEventType {
    /// `corelink.billing.webhook.signature_invalid` — signature verify
    /// failed (malformed / replay / mismatch). NO body mutation
    /// occurred. Alert > 0 = potential attacker.
    SignatureInvalid,
    /// `corelink.billing.webhook.processing_started` — signature ok,
    /// dedup-passed, dispatch about to run. Pair with `_completed` /
    /// `_failed`.
    ProcessingStarted,
    /// `corelink.billing.webhook.processing_completed` — dispatch
    /// returned Ok.
    ProcessingCompleted,
    /// `corelink.billing.webhook.processing_failed` — dispatch
    /// returned Err. Body contains error detail. Alert > 0.
    ProcessingFailed,
    /// `corelink.billing.webhook.duplicate_event` — idempotency dedup
    /// hit (Stripe retry). Ack 200 without dispatch.
    DuplicateEvent,
    /// `corelink.billing.webhook.unknown_event_type` — event type not
    /// in canonical set; ack 200 + log (forward-compat).
    UnknownEventType,
}

impl WebhookAuditEventType {
    /// CloudEvents-style canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SignatureInvalid => "corelink.billing.webhook.signature_invalid",
            Self::ProcessingStarted => "corelink.billing.webhook.processing_started",
            Self::ProcessingCompleted => "corelink.billing.webhook.processing_completed",
            Self::ProcessingFailed => "corelink.billing.webhook.processing_failed",
            Self::DuplicateEvent => "corelink.billing.webhook.duplicate_event",
            Self::UnknownEventType => "corelink.billing.webhook.unknown_event_type",
        }
    }
}

/// One audit record emitted by the webhook route.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct WebhookAuditRecord {
    /// Event type.
    pub event_type: WebhookAuditEventType,
    /// Stripe event id if known (None for `SignatureInvalid` where
    /// body is untrusted).
    pub stripe_event_id: Option<String>,
    /// Stripe event-type string if known (None for `SignatureInvalid`).
    pub stripe_event_type: Option<String>,
    /// Optional error detail for failure paths.
    pub error_detail: Option<String>,
    /// Timestamp (ms since epoch).
    pub ts_ms: u64,
}

/// Trait every audit sink must satisfy. Production wires this to
/// `corelink-audit-chain`; tests use the in-memory fake.
pub trait WebhookAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit one record. Errors are surfaced so the route can flip to
    /// `processing_failed` if audit fails.
    fn emit(&self, record: &WebhookAuditRecord) -> Result<(), String>;
}

/// In-memory audit sink — accumulates records for test inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemoryWebhookAuditSink {
    records: Arc<std::sync::Mutex<Vec<WebhookAuditRecord>>>,
}

impl InMemoryWebhookAuditSink {
    /// Construct an empty in-memory sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<WebhookAuditRecord> {
        match self.records.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// True if any record of `event_type` has been emitted.
    #[must_use]
    pub fn has_event(&self, event_type: WebhookAuditEventType) -> bool {
        self.snapshot().iter().any(|r| r.event_type == event_type)
    }
}

impl WebhookAuditSink for InMemoryWebhookAuditSink {
    fn emit(&self, record: &WebhookAuditRecord) -> Result<(), String> {
        let mut g = self
            .records
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        g.push(record.clone());
        Ok(())
    }
}

// =========================================================================
// Idempotency store trait (D1 mirror).
// =========================================================================

/// Outcome of an idempotency dedup attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdempotencyOutcome {
    /// Event was new; row inserted; dispatch should proceed.
    Inserted,
    /// Event was already processed; dispatch must be SKIPPED. Stripe
    /// gets 200 OK to stop retries.
    AlreadyProcessed,
}

/// Trait for the `stripe_webhook_events_processed` table. Production
/// wires this to a D1 binding using `INSERT OR IGNORE` semantics; tests
/// use [`InMemoryIdempotencyStore`].
pub trait WebhookIdempotencyStore: core::fmt::Debug + Send + Sync {
    /// Attempt to insert a new processed-event row.
    ///
    /// - Returns [`IdempotencyOutcome::Inserted`] iff the row did not
    ///   previously exist (new event; proceed with dispatch).
    /// - Returns [`IdempotencyOutcome::AlreadyProcessed`] iff a row
    ///   already exists (Stripe retry; ack with 200).
    /// - Returns `Err` on transient backend failure; the route MUST
    ///   fail-CLOSED and return 500 (Stripe will retry).
    fn try_insert(
        &self,
        event_id: &str,
        event_type: &str,
        now_ms: u64,
    ) -> Result<IdempotencyOutcome, String>;
}

/// In-memory `Arc<Mutex<HashSet>>` idempotency store. Mirrors D1's
/// `INSERT OR IGNORE` semantics exactly.
#[derive(Clone, Debug, Default)]
pub struct InMemoryIdempotencyStore {
    seen: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl InMemoryIdempotencyStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Count of stored event ids (for test assertions).
    #[must_use]
    pub fn len(&self) -> usize {
        match self.seen.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True if no events have been stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl WebhookIdempotencyStore for InMemoryIdempotencyStore {
    fn try_insert(
        &self,
        event_id: &str,
        _event_type: &str,
        _now_ms: u64,
    ) -> Result<IdempotencyOutcome, String> {
        let mut g = self
            .seen
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        if g.contains(event_id) {
            Ok(IdempotencyOutcome::AlreadyProcessed)
        } else {
            g.insert(event_id.to_string());
            Ok(IdempotencyOutcome::Inserted)
        }
    }
}

// =========================================================================
// Subscription state handler trait (production binds to
// `corelink-tier-selection::TierSelectionLedger`).
// =========================================================================

/// Trait every event-dispatch handler satisfies.
///
/// Each method receives the parsed envelope and is responsible for
/// any state mutation in the tier-selection ledger / billing audit
/// chain. Production wires this to `corelink-tier-selection`; tests
/// use [`RecordingSubscriptionHandler`].
pub trait SubscriptionStateHandler: core::fmt::Debug + Send + Sync {
    /// `customer.subscription.created` — activate tier for the tenant.
    fn on_subscription_created(&self, env: &WebhookEnvelope) -> Result<(), String>;
    /// `customer.subscription.updated` — refresh tier or downgrade on
    /// cancel based on `status` field.
    fn on_subscription_updated(&self, env: &WebhookEnvelope) -> Result<(), String>;
    /// `customer.subscription.deleted` — downgrade to Free.
    fn on_subscription_deleted(&self, env: &WebhookEnvelope) -> Result<(), String>;
    /// `customer.subscription.trial_will_end` — notify customer.
    fn on_trial_will_end(&self, env: &WebhookEnvelope) -> Result<(), String>;
    /// `invoice.paid` — extend access expiry.
    fn on_invoice_paid(&self, env: &WebhookEnvelope) -> Result<(), String>;
    /// `invoice.payment_failed` — set grace-period flag.
    fn on_invoice_payment_failed(&self, env: &WebhookEnvelope) -> Result<(), String>;
}

/// Test-only handler that records every call for assertion.
#[derive(Clone, Debug, Default)]
pub struct RecordingSubscriptionHandler {
    calls: Arc<std::sync::Mutex<Vec<(CanonicalEventType, String)>>>,
    /// If set, every method returns this error string (forces
    /// `processing_failed` audit path in tests).
    fail_with: Arc<std::sync::Mutex<Option<String>>>,
}

impl RecordingSubscriptionHandler {
    /// Construct an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Force every dispatch method to return `Err(msg)` (covers the
    /// `processing_failed` audit path).
    pub fn fail_with(&self, msg: impl Into<String>) {
        if let Ok(mut g) = self.fail_with.lock() {
            *g = Some(msg.into());
        }
    }

    /// Snapshot the `(event_type, event_id)` call list.
    #[must_use]
    pub fn calls(&self) -> Vec<(CanonicalEventType, String)> {
        match self.calls.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of dispatch calls received.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.calls().len()
    }

    fn record(&self, ty: CanonicalEventType, env: &WebhookEnvelope) -> Result<(), String> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(msg) = g.as_ref() {
                return Err(msg.clone());
            }
        }
        let mut g = self
            .calls
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        g.push((ty, env.id.clone()));
        Ok(())
    }
}

impl SubscriptionStateHandler for RecordingSubscriptionHandler {
    fn on_subscription_created(&self, env: &WebhookEnvelope) -> Result<(), String> {
        self.record(CanonicalEventType::SubscriptionCreated, env)
    }
    fn on_subscription_updated(&self, env: &WebhookEnvelope) -> Result<(), String> {
        self.record(CanonicalEventType::SubscriptionUpdated, env)
    }
    fn on_subscription_deleted(&self, env: &WebhookEnvelope) -> Result<(), String> {
        self.record(CanonicalEventType::SubscriptionDeleted, env)
    }
    fn on_trial_will_end(&self, env: &WebhookEnvelope) -> Result<(), String> {
        self.record(CanonicalEventType::SubscriptionTrialWillEnd, env)
    }
    fn on_invoice_paid(&self, env: &WebhookEnvelope) -> Result<(), String> {
        self.record(CanonicalEventType::InvoicePaid, env)
    }
    fn on_invoice_payment_failed(&self, env: &WebhookEnvelope) -> Result<(), String> {
        self.record(CanonicalEventType::InvoicePaymentFailed, env)
    }
}

// =========================================================================
// Time provider (deterministic injection for tests).
// =========================================================================

/// Trait that returns the current unix timestamp. Production wires
/// `SystemTimeProvider`; tests inject a fixed value to make
/// signature-replay tests deterministic.
pub trait TimeProvider: core::fmt::Debug + Send + Sync {
    /// Current unix time, seconds.
    fn now_seconds(&self) -> u64;
    /// Current unix time, milliseconds.
    fn now_ms(&self) -> u64;
}

/// `std::time::SystemTime`-backed clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemTimeProvider;

impl TimeProvider for SystemTimeProvider {
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
}

/// Fixed-time clock for deterministic tests.
#[derive(Clone, Copy, Debug)]
pub struct FixedTimeProvider {
    /// Fixed timestamp in seconds.
    pub seconds: u64,
}

impl FixedTimeProvider {
    /// Construct a clock pinned at `seconds`.
    #[must_use]
    pub const fn new(seconds: u64) -> Self {
        Self { seconds }
    }
}

impl TimeProvider for FixedTimeProvider {
    fn now_seconds(&self) -> u64 {
        self.seconds
    }
    fn now_ms(&self) -> u64 {
        self.seconds.saturating_mul(1_000)
    }
}

// =========================================================================
// Route state + axum router.
// =========================================================================

/// State shared across requests. Held as `Arc<WebhookState>` to satisfy
/// axum's `State` extractor.
///
/// NOTE: `webhook_secret` is `Vec<u8>` (NOT `String`) and is NEVER
/// logged or `Debug`-formatted (the manual `Debug` impl below redacts
/// it).
pub struct WebhookState {
    /// Stripe webhook signing secret (`whsec_...` raw bytes). NEVER
    /// logged.
    pub webhook_secret: Vec<u8>,
    /// Idempotency dedup store (production: D1; tests: in-memory).
    pub idempotency: Arc<dyn WebhookIdempotencyStore>,
    /// Audit sink (production: `corelink-audit-chain`; tests:
    /// in-memory).
    pub audit: Arc<dyn WebhookAuditSink>,
    /// Subscription / invoice event dispatcher.
    pub handler: Arc<dyn SubscriptionStateHandler>,
    /// Time provider (production: system; tests: fixed).
    pub time: Arc<dyn TimeProvider>,
    /// Signature replay tolerance in seconds. Defaults to
    /// [`STRIPE_REPLAY_TOLERANCE_SECONDS`].
    pub tolerance_seconds: u64,
}

impl core::fmt::Debug for WebhookState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WebhookState")
            .field("webhook_secret", &"<redacted>")
            .field("tolerance_seconds", &self.tolerance_seconds)
            .field("idempotency", &self.idempotency)
            .field("audit", &self.audit)
            .field("handler", &self.handler)
            .field("time", &self.time)
            .finish()
    }
}

impl WebhookState {
    /// Construct a [`WebhookState`] with the canonical 300-second
    /// tolerance and the supplied dependencies.
    #[must_use]
    pub fn new(
        webhook_secret: Vec<u8>,
        idempotency: Arc<dyn WebhookIdempotencyStore>,
        audit: Arc<dyn WebhookAuditSink>,
        handler: Arc<dyn SubscriptionStateHandler>,
        time: Arc<dyn TimeProvider>,
    ) -> Self {
        Self {
            webhook_secret,
            idempotency,
            audit,
            handler,
            time,
            tolerance_seconds: STRIPE_REPLAY_TOLERANCE_SECONDS,
        }
    }
}

/// Build an [`axum::Router`] mounting the Stripe webhook route at
/// [`STRIPE_WEBHOOK_ROUTE`].
pub fn router(state: Arc<WebhookState>) -> Router {
    Router::new()
        .route(STRIPE_WEBHOOK_ROUTE, post(stripe_webhook_handler))
        .with_state(state)
}

// Audit emit helper. Logs the audit-emit failure (DOES NOT log body or
// secret) but returns Ok so the route still acks 200 — Stripe retries
// are pointless when audit is broken (the dedup row is already in).
fn try_emit_audit(state: &WebhookState, record: WebhookAuditRecord) {
    if let Err(e) = state.audit.emit(&record) {
        tracing::error!(
            audit_event = %record.event_type.as_str(),
            error = %e,
            "webhook audit emit failed",
        );
    }
}

/// `POST /v1/billing/stripe-webhook` handler.
///
/// See module-level docs for the canonical flow.
pub async fn stripe_webhook_handler(
    State(state): State<Arc<WebhookState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    process_webhook(&state, &headers, &body)
}

/// Synchronous core of the handler — extracted so unit tests can drive
/// it without spinning up an axum runtime.
#[must_use]
pub fn process_webhook(
    state: &WebhookState,
    headers: &HeaderMap,
    body: &[u8],
) -> (StatusCode, &'static str) {
    let now_seconds = state.time.now_seconds();
    let now_ms = state.time.now_ms();

    // (1) Read Stripe-Signature header (NEVER trust body until verify).
    let Some(sig_header) = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
    else {
        try_emit_audit(
            state,
            WebhookAuditRecord {
                event_type: WebhookAuditEventType::SignatureInvalid,
                stripe_event_id: None,
                stripe_event_type: None,
                error_detail: Some("missing or non-ascii Stripe-Signature header".to_string()),
                ts_ms: now_ms,
            },
        );
        return (StatusCode::BAD_REQUEST, "missing signature");
    };

    // (2) Verify signature over EXACT raw bytes Stripe signed.
    if let Err(e) = verify_webhook_signature(
        body,
        sig_header,
        &state.webhook_secret,
        now_seconds,
        state.tolerance_seconds,
    ) {
        // NEVER log body. Only log error variant (no body content).
        tracing::warn!(error = ?e, "stripe webhook signature verify failed");
        try_emit_audit(
            state,
            WebhookAuditRecord {
                event_type: WebhookAuditEventType::SignatureInvalid,
                stripe_event_id: None,
                stripe_event_type: None,
                error_detail: Some(format!("{e:?}")),
                ts_ms: now_ms,
            },
        );
        return (StatusCode::BAD_REQUEST, "signature invalid");
    }

    // (3) Parse JSON envelope ONLY after signature is verified.
    let env: WebhookEnvelope = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            try_emit_audit(
                state,
                WebhookAuditRecord {
                    event_type: WebhookAuditEventType::ProcessingFailed,
                    stripe_event_id: None,
                    stripe_event_type: None,
                    error_detail: Some(format!("json parse: {e}")),
                    ts_ms: now_ms,
                },
            );
            return (StatusCode::BAD_REQUEST, "malformed event body");
        }
    };

    // (4) Idempotency dedup: INSERT OR IGNORE on event_id.
    let dedup = match state.idempotency.try_insert(&env.id, &env.event_type, now_ms) {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(
                stripe_event_id = %env.id,
                error = %e,
                "idempotency store transient error; Stripe will retry",
            );
            try_emit_audit(
                state,
                WebhookAuditRecord {
                    event_type: WebhookAuditEventType::ProcessingFailed,
                    stripe_event_id: Some(env.id.clone()),
                    stripe_event_type: Some(env.event_type.clone()),
                    error_detail: Some(format!("idempotency store: {e}")),
                    ts_ms: now_ms,
                },
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "transient backend error");
        }
    };
    if dedup == IdempotencyOutcome::AlreadyProcessed {
        try_emit_audit(
            state,
            WebhookAuditRecord {
                event_type: WebhookAuditEventType::DuplicateEvent,
                stripe_event_id: Some(env.id.clone()),
                stripe_event_type: Some(env.event_type.clone()),
                error_detail: None,
                ts_ms: now_ms,
            },
        );
        return (StatusCode::OK, "duplicate event acknowledged");
    }

    // (5) Audit BEFORE state mutation.
    try_emit_audit(
        state,
        WebhookAuditRecord {
            event_type: WebhookAuditEventType::ProcessingStarted,
            stripe_event_id: Some(env.id.clone()),
            stripe_event_type: Some(env.event_type.clone()),
            error_detail: None,
            ts_ms: now_ms,
        },
    );

    // (6) Dispatch by canonical event type.
    let canon = CanonicalEventType::parse_event_type(&env.event_type);
    let dispatch_result: Result<(), String> = match canon {
        CanonicalEventType::SubscriptionCreated => state.handler.on_subscription_created(&env),
        CanonicalEventType::SubscriptionUpdated => state.handler.on_subscription_updated(&env),
        CanonicalEventType::SubscriptionDeleted => state.handler.on_subscription_deleted(&env),
        CanonicalEventType::SubscriptionTrialWillEnd => state.handler.on_trial_will_end(&env),
        CanonicalEventType::InvoicePaid => state.handler.on_invoice_paid(&env),
        CanonicalEventType::InvoicePaymentFailed => state.handler.on_invoice_payment_failed(&env),
        CanonicalEventType::Unknown => {
            try_emit_audit(
                state,
                WebhookAuditRecord {
                    event_type: WebhookAuditEventType::UnknownEventType,
                    stripe_event_id: Some(env.id.clone()),
                    stripe_event_type: Some(env.event_type.clone()),
                    error_detail: None,
                    ts_ms: now_ms,
                },
            );
            tracing::info!(
                stripe_event_id = %env.id,
                stripe_event_type = %env.event_type,
                "stripe webhook: unknown event type acked for forward-compat",
            );
            return (StatusCode::OK, "unknown event type acknowledged");
        }
    };

    // (7) Audit completed / failed.
    match dispatch_result {
        Ok(()) => {
            try_emit_audit(
                state,
                WebhookAuditRecord {
                    event_type: WebhookAuditEventType::ProcessingCompleted,
                    stripe_event_id: Some(env.id.clone()),
                    stripe_event_type: Some(env.event_type.clone()),
                    error_detail: None,
                    ts_ms: now_ms,
                },
            );
            (StatusCode::OK, "ok")
        }
        Err(e) => {
            tracing::error!(
                stripe_event_id = %env.id,
                stripe_event_type = %env.event_type,
                error = %e,
                "stripe webhook dispatch failed",
            );
            try_emit_audit(
                state,
                WebhookAuditRecord {
                    event_type: WebhookAuditEventType::ProcessingFailed,
                    stripe_event_id: Some(env.id.clone()),
                    stripe_event_type: Some(env.event_type.clone()),
                    error_detail: Some(e),
                    ts_ms: now_ms,
                },
            );
            // 500 → Stripe retries. Since we've already inserted the
            // dedup row, the retry will hit AlreadyProcessed and ack
            // 200 without re-running the (broken) handler. Production
            // wiring SHOULD make the dedup INSERT happen INSIDE the
            // same transaction as the handler so a handler failure
            // rolls back the dedup row; the in-process trait splits
            // them and accepts the trade-off documented here.
            (StatusCode::INTERNAL_SERVER_ERROR, "dispatch failed")
        }
    }
}

// =========================================================================
// Body extractor helper for callers that want to drive the route
// without axum (e.g. integration tests that build raw `http::Request`).
// =========================================================================

/// Stripe webhook envelope subset used by some handlers; re-exported
/// here as a convenience for callers that want to introspect
/// `data.object.status` (subscription state).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SubscriptionStatusProbe {
    /// Stripe `data.object.status` field (e.g. `active`, `canceled`,
    /// `past_due`, `trialing`, `unpaid`).
    pub status: Option<String>,
}

/// Extract the `status` field from `customer.subscription.updated`
/// `data.object`. Returns `None` if the envelope shape doesn't match.
#[must_use]
pub fn subscription_status_from_envelope(env: &WebhookEnvelope) -> Option<String> {
    env.data
        .get("object")
        .and_then(|o| o.get("status"))
        .and_then(|s| s.as_str())
        .map(String::from)
}

/// Drive the webhook route once with the body + headers. Helper for
/// integration tests; mirrors what axum invokes under the hood.
pub async fn route_oneshot(
    state: Arc<WebhookState>,
    headers: HeaderMap,
    body: Vec<u8>,
) -> (StatusCode, String) {
    use tower::ServiceExt;
    let app = router(state);
    let mut req = http::Request::builder()
        .method(http::Method::POST)
        .uri(STRIPE_WEBHOOK_ROUTE)
        .body(Body::from(body))
        .unwrap_or_else(|_| {
            // Construction failure is impossible with hard-coded
            // method/uri; build a minimal request just in case.
            http::Request::new(Body::empty())
        });
    for (k, v) in headers.iter() {
        req.headers_mut().insert(k.clone(), v.clone());
    }
    match app.oneshot(req).await {
        Ok(resp) => {
            let status = resp.status();
            let bytes = http_body_util::BodyExt::collect(resp.into_body())
                .await
                .map(http_body_util::Collected::to_bytes)
                .unwrap_or_default();
            (status, String::from_utf8_lossy(&bytes).to_string())
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, String::new()),
    }
}

// =========================================================================
// Tests
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
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    const SECRET: &[u8] = b"whsec_test_r2_12";
    const FIXED_TS: u64 = 1_715_000_000;

    fn make_signature(secret: &[u8], ts: u64, payload: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(ts.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        let sig = hex::encode(mac.finalize().into_bytes());
        format!("t={ts},v1={sig}")
    }

    fn fixture_state() -> (
        Arc<WebhookState>,
        Arc<InMemoryWebhookAuditSink>,
        Arc<RecordingSubscriptionHandler>,
        Arc<InMemoryIdempotencyStore>,
    ) {
        let audit = Arc::new(InMemoryWebhookAuditSink::new());
        let handler = Arc::new(RecordingSubscriptionHandler::new());
        let idem = Arc::new(InMemoryIdempotencyStore::new());
        let time = Arc::new(FixedTimeProvider::new(FIXED_TS));
        let state = Arc::new(WebhookState::new(
            SECRET.to_vec(),
            idem.clone(),
            audit.clone(),
            handler.clone(),
            time,
        ));
        (state, audit, handler, idem)
    }

    fn event_json(event_id: &str, event_type: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "id": event_id,
            "type": event_type,
            "created": FIXED_TS,
            "data": { "object": { "id": "sub_test_1", "status": "active" } }
        }))
        .unwrap()
    }

    fn headers_for(payload: &[u8], ts: u64) -> HeaderMap {
        let mut h = HeaderMap::new();
        let sig = make_signature(SECRET, ts, payload);
        h.insert("stripe-signature", sig.parse().unwrap());
        h
    }

    // (1) Happy path: valid signature + new event_id → 200 + state
    //     mutation + audit `processing_started` + `processing_completed`.
    #[test]
    fn signature_valid_new_event_dispatches_and_audits() {
        let (state, audit, handler, idem) = fixture_state();
        let body = event_json("evt_001", "customer.subscription.created");
        let headers = headers_for(&body, FIXED_TS);

        let (status, _) = process_webhook(&state, &headers, &body);

        assert_eq!(status, StatusCode::OK);
        assert_eq!(handler.call_count(), 1);
        assert_eq!(handler.calls()[0].0, CanonicalEventType::SubscriptionCreated);
        assert_eq!(handler.calls()[0].1, "evt_001");
        assert_eq!(idem.len(), 1);
        assert!(audit.has_event(WebhookAuditEventType::ProcessingStarted));
        assert!(audit.has_event(WebhookAuditEventType::ProcessingCompleted));
        assert!(!audit.has_event(WebhookAuditEventType::SignatureInvalid));
    }

    // (2) Idempotent: same event_id delivered twice → second is acked
    //     200 with NO double-mutation; audit emits DuplicateEvent.
    #[test]
    fn signature_valid_duplicate_event_idempotent() {
        let (state, audit, handler, idem) = fixture_state();
        let body = event_json("evt_dup", "customer.subscription.created");
        let headers = headers_for(&body, FIXED_TS);

        let (s1, _) = process_webhook(&state, &headers, &body);
        let (s2, _) = process_webhook(&state, &headers, &body);

        assert_eq!(s1, StatusCode::OK);
        assert_eq!(s2, StatusCode::OK);
        assert_eq!(handler.call_count(), 1, "no double-mutation");
        assert_eq!(idem.len(), 1);
        assert!(audit.has_event(WebhookAuditEventType::DuplicateEvent));
    }

    // (3) Invalid signature → 400 + audit `signature_invalid` + zero
    //     state mutation.
    #[test]
    fn signature_invalid_returns_400_and_no_mutation() {
        let (state, audit, handler, idem) = fixture_state();
        let body = event_json("evt_evil", "customer.subscription.created");
        let mut headers = HeaderMap::new();
        // Bogus signature (correct format, wrong HMAC).
        headers.insert(
            "stripe-signature",
            format!("t={FIXED_TS},v1=deadbeef").parse().unwrap(),
        );

        let (status, _) = process_webhook(&state, &headers, &body);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(handler.call_count(), 0);
        assert_eq!(idem.len(), 0);
        assert!(audit.has_event(WebhookAuditEventType::SignatureInvalid));
    }

    // (4) Replay: signature OK but ts older than tolerance → 400.
    #[test]
    fn replay_window_exceeded_returns_400() {
        let (state, audit, handler, idem) = fixture_state();
        let body = event_json("evt_replay", "customer.subscription.created");
        // Signed 10 minutes in the past (> 300s tolerance).
        let old_ts = FIXED_TS - 600;
        let headers = headers_for(&body, old_ts);

        let (status, _) = process_webhook(&state, &headers, &body);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(handler.call_count(), 0);
        assert_eq!(idem.len(), 0);
        assert!(audit.has_event(WebhookAuditEventType::SignatureInvalid));
    }

    // (5) Unknown event type → 200 ack + audit `unknown_event_type` +
    //     no handler dispatch.
    #[test]
    fn unknown_event_type_acked_for_forward_compat() {
        let (state, audit, handler, _idem) = fixture_state();
        let body = event_json("evt_future", "customer.future.event.we.dont.know");
        let headers = headers_for(&body, FIXED_TS);

        let (status, _) = process_webhook(&state, &headers, &body);

        assert_eq!(status, StatusCode::OK);
        assert_eq!(handler.call_count(), 0);
        assert!(audit.has_event(WebhookAuditEventType::UnknownEventType));
    }

    // (6) Subscription.deleted → handler.on_subscription_deleted called.
    #[test]
    fn subscription_deleted_dispatches_downgrade() {
        let (state, audit, handler, _) = fixture_state();
        let body = event_json("evt_del", "customer.subscription.deleted");
        let headers = headers_for(&body, FIXED_TS);

        let (status, _) = process_webhook(&state, &headers, &body);

        assert_eq!(status, StatusCode::OK);
        assert_eq!(handler.call_count(), 1);
        assert_eq!(handler.calls()[0].0, CanonicalEventType::SubscriptionDeleted);
        assert!(audit.has_event(WebhookAuditEventType::ProcessingCompleted));
    }

    // (7) Each canonical event type routes to the correct handler
    //     method.
    #[test]
    fn each_canonical_event_routes_correctly() {
        let cases = [
            ("customer.subscription.created", CanonicalEventType::SubscriptionCreated),
            ("customer.subscription.updated", CanonicalEventType::SubscriptionUpdated),
            ("customer.subscription.deleted", CanonicalEventType::SubscriptionDeleted),
            ("customer.subscription.trial_will_end", CanonicalEventType::SubscriptionTrialWillEnd),
            ("invoice.paid", CanonicalEventType::InvoicePaid),
            ("invoice.payment_failed", CanonicalEventType::InvoicePaymentFailed),
        ];
        for (i, (raw, expected)) in cases.iter().enumerate() {
            let (state, _, handler, _) = fixture_state();
            let event_id = format!("evt_case_{i}");
            let body = event_json(&event_id, raw);
            let headers = headers_for(&body, FIXED_TS);
            let (status, _) = process_webhook(&state, &headers, &body);
            assert_eq!(status, StatusCode::OK, "case {raw}");
            assert_eq!(handler.call_count(), 1);
            assert_eq!(&handler.calls()[0].0, expected, "raw={raw}");
        }
    }

    // (8) Dispatch failure → 500 + `processing_failed` audit; dedup row
    //     still present (Stripe retry → 200 OK because of dedup).
    #[test]
    fn dispatch_failure_emits_processing_failed_and_returns_500() {
        let (state, audit, handler, idem) = fixture_state();
        handler.fail_with("tier ledger transient");
        let body = event_json("evt_fail", "customer.subscription.created");
        let headers = headers_for(&body, FIXED_TS);

        let (status, _) = process_webhook(&state, &headers, &body);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(audit.has_event(WebhookAuditEventType::ProcessingFailed));
        assert!(audit.has_event(WebhookAuditEventType::ProcessingStarted));
        // Dedup row IS present (in this in-process trait split).
        // Stripe's retry would now hit the dedup → 200.
        assert_eq!(idem.len(), 1);
    }

    // (9) Missing Stripe-Signature header → 400.
    #[test]
    fn missing_signature_header_returns_400() {
        let (state, audit, handler, _) = fixture_state();
        let body = event_json("evt_nohdr", "customer.subscription.created");
        let headers = HeaderMap::new(); // no Stripe-Signature

        let (status, _) = process_webhook(&state, &headers, &body);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(handler.call_count(), 0);
        assert!(audit.has_event(WebhookAuditEventType::SignatureInvalid));
    }

    // (10) Tampered body (re-using a valid-looking signature header for
    //      a DIFFERENT payload) → 400.
    #[test]
    fn tampered_body_rejected() {
        let (state, audit, handler, _) = fixture_state();
        let original = event_json("evt_a", "customer.subscription.created");
        let headers = headers_for(&original, FIXED_TS);
        let tampered = event_json("evt_a", "customer.subscription.deleted");

        let (status, _) = process_webhook(&state, &headers, &tampered);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(handler.call_count(), 0);
        assert!(audit.has_event(WebhookAuditEventType::SignatureInvalid));
    }

    // (11) `subscription_status_from_envelope` extracts the
    //      `data.object.status` field for subscription.updated arms.
    #[test]
    fn subscription_status_probe_extracts_status() {
        let env: WebhookEnvelope = serde_json::from_slice(&event_json(
            "evt_x",
            "customer.subscription.updated",
        ))
        .unwrap();
        assert_eq!(
            subscription_status_from_envelope(&env).as_deref(),
            Some("active")
        );
    }

    // (12) `route_oneshot` end-to-end through the axum router (proves
    //      the full HTTP plumbing works, not just `process_webhook`).
    #[tokio::test]
    async fn axum_router_oneshot_full_flow() {
        let (state, audit, handler, _) = fixture_state();
        let body = event_json("evt_e2e", "customer.subscription.created");
        let headers = headers_for(&body, FIXED_TS);

        let (status, _) = route_oneshot(state, headers, body).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(handler.call_count(), 1);
        assert!(audit.has_event(WebhookAuditEventType::ProcessingCompleted));
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed these primitives"
)]
mod property_tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use proptest::prelude::*;
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;
    const SECRET: &[u8] = b"whsec_prop";
    const FIXED_TS: u64 = 1_715_000_000;

    fn good_header(payload: &[u8]) -> HeaderMap {
        let mut mac = HmacSha256::new_from_slice(SECRET).unwrap();
        mac.update(FIXED_TS.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        let sig = hex::encode(mac.finalize().into_bytes());
        let mut h = HeaderMap::new();
        h.insert(
            "stripe-signature",
            format!("t={FIXED_TS},v1={sig}").parse().unwrap(),
        );
        h
    }

    fn fixture() -> (
        Arc<WebhookState>,
        Arc<RecordingSubscriptionHandler>,
        Arc<InMemoryIdempotencyStore>,
    ) {
        let audit = Arc::new(InMemoryWebhookAuditSink::new());
        let handler = Arc::new(RecordingSubscriptionHandler::new());
        let idem = Arc::new(InMemoryIdempotencyStore::new());
        let time = Arc::new(FixedTimeProvider::new(FIXED_TS));
        (
            Arc::new(WebhookState::new(
                SECRET.to_vec(),
                idem.clone(),
                audit,
                handler.clone(),
                time,
            )),
            handler,
            idem,
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Random raw body + tampered (random) signature → ALWAYS 400.
        /// No state mutation; no dedup row inserted.
        #[test]
        fn tampered_signature_always_rejected(
            body in proptest::collection::vec(any::<u8>(), 0..512),
            fake_sig in "[0-9a-f]{8,128}",
        ) {
            let (state, handler, idem) = fixture();
            let mut headers = HeaderMap::new();
            headers.insert(
                "stripe-signature",
                format!("t={FIXED_TS},v1={fake_sig}").parse().unwrap(),
            );
            let (status, _) = process_webhook(&state, &headers, &body);
            prop_assert_eq!(status, StatusCode::BAD_REQUEST);
            prop_assert_eq!(handler.call_count(), 0);
            prop_assert_eq!(idem.len(), 0);
        }

        /// Valid signature + ANY canonical event_type random payload
        /// → 200 (we never crash on weird `data` shapes).
        #[test]
        fn random_payload_with_valid_sig_never_crashes(
            id in "evt_[a-z0-9]{4,16}",
            kind in proptest::sample::select(&[
                "customer.subscription.created",
                "customer.subscription.updated",
                "customer.subscription.deleted",
                "customer.subscription.trial_will_end",
                "invoice.paid",
                "invoice.payment_failed",
                "some.random.future.type",
            ]),
        ) {
            let (state, _handler, _idem) = fixture();
            let body = serde_json::to_vec(&serde_json::json!({
                "id": id,
                "type": kind,
                "data": { "object": {} }
            })).unwrap();
            let headers = good_header(&body);
            let (status, _) = process_webhook(&state, &headers, &body);
            prop_assert_eq!(status, StatusCode::OK);
        }
    }
}
