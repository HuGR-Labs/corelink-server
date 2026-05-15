//! `corelink-stripe-real` — production HTTPS Stripe API client implementing
//! the [`StripeClient`] trait from `corelink-tier-selection`.
//!
//! # Scope
//!
//! Production wiring of Stripe Checkout / Subscriptions / Customers /
//! Billing Portal + webhook signature verification. Replaces the
//! `InMemoryStripeClient` fake for `apps/server` deployments.
//!
//! # Endpoints
//!
//! - `POST /v1/checkout/sessions` — create Checkout Session.
//! - `POST /v1/subscriptions` + `GET /v1/subscriptions/:id` — subscription
//!   lifecycle.
//! - `POST /v1/customers` + `GET /v1/customers/:id` — customer CRUD.
//! - `POST /v1/billing_portal/sessions` — Stripe Customer Portal URL.
//!
//! # API key resolution
//!
//! - `STRIPE_SECRET_KEY` (live) or `STRIPE_SECRET_KEY_TEST` (test mode)
//!   resolved at [`StripeRealClient::from_env`].
//! - `STRIPE_WEBHOOK_SECRET` resolved at [`verify_webhook_signature`]
//!   caller — passed in explicitly so callers can rotate without
//!   rebuilding the client.
//! - SECRETS ARE NEVER LOGGED. The `Debug` impl redacts the key.
//!
//! # Idempotency
//!
//! Every POST sends an `Idempotency-Key` header. Stripe holds the
//! cached response for 24h; same key → identical response.
//!
//! # Retry policy
//!
//! Exponential backoff on `5xx` and `429`. Respects `Retry-After`
//! header when present. Max 5 retries. `4xx` is NOT retried (caller
//! error per Stripe spec).
//!
//! # wasm32 strategy
//!
//! Stripe HTTPS calls run from `apps/server` (native Rust server
//! worker context). Cloudflare Worker (`wasm32-unknown-unknown`)
//! callers use the in-memory fake or proxy to the native worker. This
//! crate is GATED OUT of wasm32 builds via the inner-attribute below
//! so `cargo build --target wasm32-unknown-unknown` skips it instead
//! of failing on native-TLS dependencies.
#![cfg(not(target_arch = "wasm32"))]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod client;
pub mod dlq;
pub mod error;
pub mod portal;
pub mod retry;
pub mod webhook;
pub mod webhook_dispatch;

pub use client::{StripeRealClient, StripeRealClientBuilder};
pub use portal::{
    BillingPortalSessionCreator, InMemoryPortalAuditSink, InMemoryPortalSessionCreator,
    PortalAuditEvent, PortalAuditSink, PortalSessionError, PortalSessionUrl,
    RecordedPortalEvent,
};
pub use dlq::{
    DlqError, DlqQuarantineOutcome, DlqReplayOutcome, InMemoryWebhookDlqStore,
    WebhookDlqRow, WebhookDlqStore, DEFAULT_DLQ_TTL_MS, DLQ_DEPTH_GAUGE,
    DLQ_OLDEST_AGE_SECONDS_GAUGE, DLQ_PAGE_OLDEST_AGE_SECONDS, DLQ_PRUNED_TOTAL,
    DLQ_QUARANTINED_TOTAL, DLQ_REPLAYED_TOTAL, DLQ_WARN_DEPTH,
};
pub use error::{StripeError, WebhookVerifyError};
pub use retry::{RetryPolicy, DEFAULT_MAX_RETRIES};
pub use webhook::{verify_webhook_signature, DEFAULT_TOLERANCE_SECONDS};
pub use webhook_dispatch::{
    AuditEmitter, AuditOutcome, AuditRecord, CanonicalWebhookEventType, Clock,
    DispatchResponse, FixedClock, IdempotencyOutcome, IdempotencyStore, IdempotencyToken,
    InMemoryIdempotencyStore, MaterializerError, RecordingAuditEmitter, RecordingSliRecorder,
    RecordingStateMaterializer, SliObservation, SliRecorder, StateMaterializer,
    StripeWebhookEnvelope, SystemClock, WebhookDispatcher, SLI_BILLING_STRIPE_EVENT_SECONDS,
};
