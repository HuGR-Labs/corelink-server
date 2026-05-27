//! Production Stripe webhook dispatch pipeline.
//!
//! This module wires the canonical end-to-end flow that replaces the
//! in-process fake dispatch attached to `apps/server`. The pipeline is
//! intentionally pure / sync at this layer (no tokio) so it can be
//! driven from any HTTP server crate (axum, hyper, worker-rs, tests)
//! without runtime coupling.
//!
//! # Wave-36 Trigger A — trait surface moved
//!
//! The four port traits (`AuditEmitter`, `IdempotencyStore`,
//! `StateMaterializer`, `SliRecorder`) plus the identity / outcome /
//! error types (`IdempotencyToken`, `CanonicalWebhookEventType`,
//! `StripeWebhookEnvelope`, `IdempotencyOutcome`, `AuditRecord`,
//! `AuditOutcome`, `SliObservation`, `DispatchResponse`,
//! `MaterializerError`) now live in the leaf crate
//! `corelink-billing-stripe-traits`. They are re-exported from this
//! module at the previous paths so consumers using
//! `corelink_stripe_real::webhook_dispatch::*` keep working
//! unchanged. The concrete HTTPS dispatcher + in-memory store + test
//! fakes (`WebhookDispatcher`, `InMemoryIdempotencyStore`,
//! `RecordingStateMaterializer`, `RecordingAuditEmitter`,
//! `RecordingSliRecorder`, `FixedClock`) remain defined here; the
//! `impl` blocks now reference the traits crate explicitly. See
//! `specs/_audits/2026-05-27-w36-trigger-a-seal.md` for the cycle
//! resolution rationale.
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

use crate::error::WebhookVerifyError;
use crate::webhook::{verify_webhook_signature, DEFAULT_TOLERANCE_SECONDS};

// =========================================================================
// Wave-36 Trigger A: re-export the trait + type surface from the leaf
// `corelink-billing-stripe-traits` crate. Preserves the canonical
// `corelink_stripe_real::webhook_dispatch::*` public paths.
// =========================================================================

pub use corelink_billing_stripe_traits::{
    AuditEmitter, AuditOutcome, AuditRecord, CanonicalWebhookEventType, DispatchResponse,
    IdempotencyOutcome, IdempotencyStore, IdempotencyToken, MaterializerError, SliObservation,
    SliRecorder, StateMaterializer, StripeWebhookEnvelope, SLI_BILLING_STRIPE_EVENT_SECONDS,
};

// =========================================================================
// Idempotency in-memory fake (concrete adapter; trait def is upstream).
// =========================================================================

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
// State materializer test-only recorder (concrete fake; trait def is upstream).
// =========================================================================

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
// Audit emitter test-only recorder (concrete fake; trait def is upstream).
// =========================================================================

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
// SLI recorder (concrete fake; trait def is upstream).
// =========================================================================

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
// Time provider abstraction.
// =========================================================================
//
// Wave-20: the `Clock` trait + `SystemClock` impl moved to
// `crate::clock` so production wasm32 callers can inject
// `WasmWorkerClock` (which reads `js_sys::Date::now()` instead of
// panicking on `SystemTime::now()`). The trait + `SystemClock` are
// re-exported below for back-compat with all 51 existing tests and
// downstream callers; the `FixedClock` SLI helper stays here because
// it carries a hard-coded `latency_seconds` value used exclusively
// by webhook-pipeline tests.

pub use crate::clock::Clock;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::clock::SystemClock;

/// Fixed-time test clock; returns `seconds` for now-* queries and a
/// hard-coded `latency_seconds` from `observe_latency_seconds`.
///
/// Distinct from [`crate::clock::InMemoryFakeClock`]: this variant
/// pins the SLI latency to a fixed value (for deterministic SLI
/// histogram assertions in the webhook pipeline tests) rather than
/// computing it from the elapsed delta.
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
    fn now(&self) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(self.seconds)
    }
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
                AuditRecord::new(
                    "corelink.billing.stripe_event_processed.v1",
                    String::new(),
                    CanonicalWebhookEventType::Unknown,
                    AuditOutcome::SignatureInvalid,
                    None,
                    now_ms,
                    Some("missing or non-ascii Stripe-Signature header".to_string()),
                ),
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
                AuditRecord::new(
                    "corelink.billing.stripe_event_processed.v1",
                    String::new(),
                    CanonicalWebhookEventType::Unknown,
                    AuditOutcome::SignatureInvalid,
                    None,
                    now_ms,
                    Some(classify_verify_err(&e).to_string()),
                ),
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
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        String::new(),
                        CanonicalWebhookEventType::Unknown,
                        AuditOutcome::EnvelopeInvalid,
                        None,
                        now_ms,
                        Some(format!("json parse: {e}")),
                    ),
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
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        env.id.clone(),
                        canon,
                        AuditOutcome::Duplicate,
                        Some(token.to_hex()),
                        now_ms,
                        None,
                    ),
                    canon,
                    AuditOutcome::Duplicate,
                    start,
                );
                return DispatchResponse::Ok200;
            }
            Ok(IdempotencyOutcome::FirstSight) => {} // proceed
            // `IdempotencyOutcome` is `#[non_exhaustive]` from the
            // upstream `corelink-billing-stripe-traits` crate; treat
            // any future variant conservatively as "proceed".
            Ok(_) => {}
            Err(e) => {
                self.emit_audit_and_sli(
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        env.id.clone(),
                        canon,
                        AuditOutcome::MaterializerFailed,
                        Some(token.to_hex()),
                        now_ms,
                        Some(format!("idempotency store: {e}")),
                    ),
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
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        env.id.clone(),
                        canon,
                        AuditOutcome::UnknownEventType,
                        Some(token.to_hex()),
                        now_ms,
                        None,
                    ),
                    canon,
                    AuditOutcome::UnknownEventType,
                    start,
                );
                return DispatchResponse::Ok200;
            }
            // `CanonicalWebhookEventType` is `#[non_exhaustive]` from
            // the upstream traits crate; treat any future variant as
            // an observability-only echo (no state mutation).
            _ => Ok(()),
        };

        // (7) Audit + SLI per dispatch outcome.
        match dispatch_result {
            Ok(()) => {
                let resp = self.emit_audit_and_sli(
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        env.id.clone(),
                        canon,
                        AuditOutcome::Dispatched,
                        Some(token.to_hex()),
                        now_ms,
                        None,
                    ),
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
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        env.id.clone(),
                        canon,
                        AuditOutcome::MaterializerFailed,
                        Some(token.to_hex()),
                        now_ms,
                        Some(msg),
                    ),
                    canon,
                    AuditOutcome::MaterializerFailed,
                    start,
                );
                DispatchResponse::InternalError500
            }
            Err(MaterializerError::InvalidPayload(msg)) => {
                self.emit_audit_and_sli(
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        env.id.clone(),
                        canon,
                        AuditOutcome::MaterializerInvalid,
                        Some(token.to_hex()),
                        now_ms,
                        Some(msg),
                    ),
                    canon,
                    AuditOutcome::MaterializerInvalid,
                    start,
                );
                DispatchResponse::Unprocessable422
            }
            // `MaterializerError` is `#[non_exhaustive]` in the leaf traits
            // crate; treat any future variants as transient so the
            // dispatcher returns 500 (Stripe retries) rather than panicking.
            Err(_) => {
                self.emit_audit_and_sli(
                    AuditRecord::new(
                        "corelink.billing.stripe_event_processed.v1",
                        env.id.clone(),
                        canon,
                        AuditOutcome::MaterializerFailed,
                        Some(token.to_hex()),
                        now_ms,
                        Some("unknown materializer error variant".to_string()),
                    ),
                    canon,
                    AuditOutcome::MaterializerFailed,
                    start,
                );
                DispatchResponse::InternalError500
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
        self.sli.observe(SliObservation::new(
            SLI_BILLING_STRIPE_EVENT_SECONDS,
            self.clock.observe_latency_seconds(start_marker),
            event_type,
            outcome,
        ));
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
