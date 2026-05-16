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
//! # Wave-31 wallet-broker series (stream-1): credential brokerage
//!
//! Outbound Stripe calls route through the HuGR Wallet remote
//! credential broker. CoreLink NEVER holds a real upstream Stripe API
//! secret.
//! Credentials resolve via [`StripeClientConfig::from_env`]:
//!
//! - `HUGR_WALLET_BASE`  — wallet broker base URL (default
//!   `https://api.humangr.com`).
//! - `HUGR_WALLET_TOKEN` — CoreLink-side `hugrw_` token (held in a
//!   redacting [`secrecy::SecretString`]).
//! - `HUGR_STRIPE_REF`   — wallet ref name (default `stripe-prod`).
//! - `STRIPE_WEBHOOK_SECRET` resolved at [`verify_webhook_signature`]
//!   caller — passed in explicitly so callers can rotate without
//!   rebuilding the client. Webhook flow stays direct (inbound +
//!   local HMAC verify); no upstream key involved.
//! - SECRETS ARE NEVER LOGGED. The `Debug` impls on
//!   [`StripeClientConfig`] and [`StripeRealClient`] redact the token.
//!
//! See `specs/_audits/2026-05-16-wallet-broker-stripe.md` for the
//! full architecture diagram + blast-radius analysis.
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
//! callers use the in-memory fake or proxy to the native worker.
//!
//! Wave-19 (R-PREP) lifts the crate-root gate so wasm32-safe surface
//! (`webhook` signature verify + `webhook_dispatch` trait pipeline +
//! `error` taxonomy + `retry` pure-logic + `dlq` types + `portal` audit
//! types) compiles on `wasm32-unknown-unknown`. The native-only HTTPS
//! client (`client.rs`, depends on `reqwest::blocking`) is per-module
//! gated to non-wasm32 targets. This mirrors the `r2_real / d1_real /
//! kv_real / do_real` pattern documented in
//! `specs/_audits/2026-05-15-cf-binding-real-pattern.md` and
//! `specs/_audits/2026-05-16-stripe-wasm32-gate-lift.md`.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

#[cfg(not(target_arch = "wasm32"))]
pub mod client;
pub mod clock;
pub mod dlq;
pub mod error;
pub mod portal;
pub mod retry;
pub mod webhook;
pub mod webhook_dispatch;

#[cfg(not(target_arch = "wasm32"))]
pub use client::{
    StripeClientConfig, StripeRealClient, StripeRealClientBuilder,
    DEFAULT_HUGR_STRIPE_REF, DEFAULT_HUGR_WALLET_BASE,
};
pub use clock::{Clock, InMemoryFakeClock};
#[cfg(not(target_arch = "wasm32"))]
pub use clock::SystemClock;
#[cfg(target_arch = "wasm32")]
pub use clock::WasmWorkerClock;
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
    AuditEmitter, AuditOutcome, AuditRecord, CanonicalWebhookEventType,
    DispatchResponse, FixedClock, IdempotencyOutcome, IdempotencyStore, IdempotencyToken,
    InMemoryIdempotencyStore, MaterializerError, RecordingAuditEmitter, RecordingSliRecorder,
    RecordingStateMaterializer, SliObservation, SliRecorder, StateMaterializer,
    StripeWebhookEnvelope, WebhookDispatcher, SLI_BILLING_STRIPE_EVENT_SECONDS,
};
