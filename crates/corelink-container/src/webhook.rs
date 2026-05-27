//! Axum HTTP shell for the canonical Stripe webhook pipeline.
//!
//! Wave 16 (R-prep) — this module is the **HTTP boundary only**. All
//! business logic (signature verify → BLAKE3 idempotency dedup →
//! 10-event taxonomy dispatch → audit emit → SLI emit) is owned by
//! [`corelink_stripe_real::webhook_dispatch::WebhookDispatcher`], the
//! canonical pipeline shipped in wave 15 (commit `654cbbb`,
//! `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`).
//!
//! # Responsibilities of this file
//!
//! - Mount `POST /v1/billing/stripe-webhook` on an [`axum::Router`].
//! - Extract the `Stripe-Signature` header + raw body bytes.
//! - Hand both to `WebhookDispatcher::process(...)`.
//! - Map [`DispatchResponse`] to an axum response with the canonical
//!   PCI DSS SAQ-A status code (200 / 400 / 401 / 422 / 500 per the
//!   audit doc §"PCI DSS SAQ-A error envelope").
//!
//! # What this file does **not** do
//!
//! - Sign verify (delegated to `webhook_dispatch::WebhookDispatcher`
//!   → `webhook::verify_webhook_signature`).
//! - Idempotency / dedup (delegated; BLAKE3 token derivation lives in
//!   the dispatcher).
//! - Event-type classification (delegated; the 10-element taxonomy is
//!   defined exactly once in
//!   [`corelink_stripe_real::webhook_dispatch::CanonicalWebhookEventType`]).
//! - Audit emit / SLI emit (delegated to the
//!   [`AuditEmitter`] / [`SliRecorder`] traits the dispatcher owns).
//!
//! Before wave 16 this file owned a parallel trait surface
//! (`SubscriptionStateHandler`, `WebhookAuditSink`, `WebhookIdempotencyStore`,
//! `TimeProvider`, plus six audit event-type variants and seven canonical
//! event-type variants). That duplication has been removed — there is now
//! exactly one canonical business pipeline and one HTTP shell that binds
//! to it.

#![allow(
    clippy::module_name_repetitions,
    reason = "WebhookState is the public type name for the axum extractor"
)]

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use corelink_billing::stripe::real::webhook_dispatch::{DispatchResponse, WebhookDispatcher};

/// HTTP route path for the webhook endpoint.
pub const STRIPE_WEBHOOK_ROUTE: &str = "/v1/billing/stripe-webhook";

/// Axum-side state — wraps the canonical
/// [`WebhookDispatcher`] from the `corelink-stripe-real` crate.
///
/// The dispatcher is constructed once at server boot (in `main.rs`)
/// with the production [`corelink_stripe_real::webhook_dispatch::IdempotencyStore`],
/// [`corelink_stripe_real::webhook_dispatch::StateMaterializer`],
/// [`corelink_stripe_real::webhook_dispatch::AuditEmitter`],
/// [`corelink_stripe_real::webhook_dispatch::SliRecorder`], and
/// [`corelink_stripe_real::webhook_dispatch::Clock`] implementations.
/// The HTTP shell is intentionally **stateless** beyond holding the
/// dispatcher `Arc`.
#[derive(Debug)]
pub struct WebhookState {
    /// The single canonical dispatcher driving signature verify →
    /// idempotency → 10-event dispatch → audit → SLI.
    pub dispatcher: Arc<WebhookDispatcher>,
}

impl WebhookState {
    /// Construct a state wrapping the supplied [`WebhookDispatcher`].
    #[must_use]
    pub const fn new(dispatcher: Arc<WebhookDispatcher>) -> Self {
        Self { dispatcher }
    }
}

/// Build an [`axum::Router`] mounting the Stripe webhook route at
/// [`STRIPE_WEBHOOK_ROUTE`].
pub fn router(state: Arc<WebhookState>) -> Router {
    Router::new()
        .route(STRIPE_WEBHOOK_ROUTE, post(stripe_webhook_handler))
        .with_state(state)
}

/// `POST /v1/billing/stripe-webhook` handler.
///
/// Extracts the `Stripe-Signature` header + raw body, hands both to
/// [`WebhookDispatcher::process`], maps [`DispatchResponse`] to a
/// PCI DSS SAQ-A canonical status code.
pub async fn stripe_webhook_handler(
    State(state): State<Arc<WebhookState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let sig_header = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok());
    let resp = state.dispatcher.process(&body, sig_header);
    dispatch_response_to_axum(resp)
}

/// Map a canonical [`DispatchResponse`] to an axum response tuple.
///
/// The body strings are short, deterministic, PCI-safe (no event id,
/// no body bytes, no signature material) — they exist purely for
/// human grep-ability of access logs.
#[must_use]
pub fn dispatch_response_to_axum(resp: DispatchResponse) -> (StatusCode, &'static str) {
    match resp {
        DispatchResponse::Ok200 => (StatusCode::OK, "ok"),
        DispatchResponse::BadRequest400 => (StatusCode::BAD_REQUEST, "missing signature"),
        DispatchResponse::Unauthorized401 => (StatusCode::UNAUTHORIZED, "signature invalid"),
        DispatchResponse::Unprocessable422 => {
            (StatusCode::UNPROCESSABLE_ENTITY, "malformed event body")
        }
        DispatchResponse::InternalError500 => {
            (StatusCode::INTERNAL_SERVER_ERROR, "transient backend error")
        }
        // `DispatchResponse` is `#[non_exhaustive]` — if the upstream
        // pipeline gains a new variant, fail CLOSED with 500 so the
        // route never silently downgrades a new failure mode to 200.
        // Updating this arm to handle a new variant is a wave-N follow-up.
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "unhandled dispatch response variant",
        ),
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
    use corelink_billing::stripe::real::webhook_dispatch::{
        AuditOutcome, DispatchResponse, FixedClock, InMemoryIdempotencyStore,
        RecordingAuditEmitter, RecordingSliRecorder, RecordingStateMaterializer,
    };

    #[test]
    fn dispatch_response_status_code_mapping_is_pci_canonical() {
        // PCI DSS SAQ-A error envelope per audit doc §"PCI DSS SAQ-A error envelope".
        assert_eq!(
            dispatch_response_to_axum(DispatchResponse::Ok200).0,
            StatusCode::OK
        );
        assert_eq!(
            dispatch_response_to_axum(DispatchResponse::BadRequest400).0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            dispatch_response_to_axum(DispatchResponse::Unauthorized401).0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            dispatch_response_to_axum(DispatchResponse::Unprocessable422).0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            dispatch_response_to_axum(DispatchResponse::InternalError500).0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn webhook_state_holds_dispatcher_arc() {
        let dispatcher = Arc::new(WebhookDispatcher::new(
            b"whsec_unit_test".to_vec(),
            Arc::new(InMemoryIdempotencyStore::new()),
            Arc::new(RecordingStateMaterializer::new()),
            Arc::new(RecordingAuditEmitter::new()),
            Arc::new(RecordingSliRecorder::new()),
            Arc::new(FixedClock::new(1_715_000_000, 0.0)),
        ));
        let state = WebhookState::new(dispatcher.clone());
        // Same Arc — no clone-on-write.
        assert!(Arc::ptr_eq(&state.dispatcher, &dispatcher));
        // Sanity: the unused `AuditOutcome` import is intentional to
        // keep the test module's use list aligned with what the
        // integration test in `apps/server/tests/webhook_unified.rs`
        // re-exports.
        let _ = AuditOutcome::Dispatched;
    }
}
