//! End-to-end integration test for the wave-16 Stripe webhook
//! unification.
//!
//! Drives the axum HTTP shell (`apps/server/src/webhook.rs`) →
//! canonical `WebhookDispatcher` pipeline
//! (`crates/corelink-stripe-real/src/webhook_dispatch.rs`) through the
//! full HTTP plumbing. This proves the unification refactor binds end
//! to end and preserves the 10-event SLA taxonomy + idempotency +
//! audit emission contracts pinned by wave-15 audit doc
//! `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`.
//!
//! Coverage:
//!
//! 1. Each of the 10 canonical event types round-trips
//!    `POST /v1/billing/stripe-webhook` → dispatcher → 200 OK,
//!    canonical `corelink.billing.stripe_event_processed.v1` audit
//!    row emitted with `outcome=Dispatched`.
//! 2. Idempotency: same `event_id` POSTed twice → 200 + 200, audit
//!    emits `Dispatched` then `Duplicate`, materializer called exactly
//!    once.
//! 3. Bad signature → 401 + audit `outcome=SignatureInvalid`.
//! 4. Missing `Stripe-Signature` header → 400.
//! 5. Malformed envelope → 422.
//! 6. Status-code envelope round-trip is the canonical PCI DSS SAQ-A
//!    shape (audit doc §"PCI DSS SAQ-A error envelope").

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{self, HeaderMap, Request, StatusCode};
use corelink_server::webhook::{router, WebhookState, STRIPE_WEBHOOK_ROUTE};
use corelink_billing::stripe::real::webhook::compute_signature;
use corelink_billing::stripe::real::webhook_dispatch::{
    AuditOutcome, CanonicalWebhookEventType, FixedClock, InMemoryIdempotencyStore,
    RecordingAuditEmitter, RecordingSliRecorder, RecordingStateMaterializer, WebhookDispatcher,
};
use tower::ServiceExt;

const SECRET: &[u8] = b"whsec_unified_e2e";
const FIXED_TS: u64 = 1_715_000_000;

type Fixture = (
    Arc<WebhookState>,
    Arc<InMemoryIdempotencyStore>,
    Arc<RecordingStateMaterializer>,
    Arc<RecordingAuditEmitter>,
    Arc<RecordingSliRecorder>,
);

fn build_fixture() -> Fixture {
    let idem = Arc::new(InMemoryIdempotencyStore::new());
    let mat = Arc::new(RecordingStateMaterializer::new());
    let audit = Arc::new(RecordingAuditEmitter::new());
    let sli = Arc::new(RecordingSliRecorder::new());
    let dispatcher = Arc::new(WebhookDispatcher::new(
        SECRET.to_vec(),
        idem.clone(),
        mat.clone(),
        audit.clone(),
        sli.clone(),
        Arc::new(FixedClock::new(FIXED_TS, 0.0)),
    ));
    let state = Arc::new(WebhookState::new(dispatcher));
    (state, idem, mat, audit, sli)
}

fn event_body(event_id: &str, event_type: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "id": event_id,
        "type": event_type,
        "created": FIXED_TS,
        "data": { "object": { "id": "obj_e2e", "status": "active" } }
    }))
    .unwrap()
}

fn signed_headers(body: &[u8], ts: u64) -> HeaderMap {
    let mut h = HeaderMap::new();
    let sig = compute_signature(SECRET, ts, body);
    h.insert(
        "stripe-signature",
        format!("t={ts},v1={sig}").parse().unwrap(),
    );
    h
}

/// POST `body` with `headers` to the unified webhook route, returning
/// `(status, body_text)`.
async fn post_webhook(
    state: Arc<WebhookState>,
    headers: HeaderMap,
    body: Vec<u8>,
) -> (StatusCode, String) {
    let app = router(state);
    let mut req = Request::builder()
        .method(http::Method::POST)
        .uri(STRIPE_WEBHOOK_ROUTE)
        .body(Body::from(body))
        .unwrap();
    for (k, v) in headers.iter() {
        req.headers_mut().insert(k.clone(), v.clone());
    }
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .map(http_body_util::Collected::to_bytes)
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

// =========================================================================
// (1) Each of the 10 canonical SLA event types round-trips HTTP → 200.
// =========================================================================

#[tokio::test]
async fn ten_canonical_event_types_round_trip_to_200() {
    let all_ten = CanonicalWebhookEventType::sla_event_types();
    assert_eq!(all_ten.len(), 10, "SLA taxonomy is exactly 10 events");

    for (i, canon) in all_ten.iter().enumerate() {
        let (state, _idem, _mat, audit, sli) = build_fixture();
        let event_id = format!("evt_canon_{i}");
        let body = event_body(&event_id, canon.label());
        let headers = signed_headers(&body, FIXED_TS);

        let (status, _) = post_webhook(state, headers, body).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "canonical event {} expected 200 OK",
            canon.label(),
        );

        // One Dispatched audit row + one SLI observation per event.
        assert_eq!(
            audit.count_with_outcome(AuditOutcome::Dispatched),
            1,
            "Dispatched audit missing for {}",
            canon.label(),
        );
        assert_eq!(
            sli.count(),
            1,
            "SLI observation missing for {}",
            canon.label(),
        );

        // Audit row carries the canonical event_name.
        let rec = &audit.records()[0];
        assert_eq!(rec.event_name, "corelink.billing.stripe_event_processed.v1");
        assert_eq!(rec.canonical_event_type, *canon);
        assert_eq!(rec.stripe_event_id, event_id);
    }
}

// =========================================================================
// (2) Idempotency: same event_id POSTed twice → 200, 200 with one dispatch
//     and a Duplicate audit row on the second hit.
// =========================================================================

#[tokio::test]
async fn idempotent_event_id_dispatches_once_audits_duplicate() {
    let (state, idem, mat, audit, sli) = build_fixture();
    let body = event_body("evt_dup_e2e", "invoice.paid");
    let headers = signed_headers(&body, FIXED_TS);

    let (s1, _) = post_webhook(state.clone(), headers.clone(), body.clone()).await;
    let (s2, _) = post_webhook(state, headers, body).await;

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);

    // State mutator (invoice.paid) called exactly once.
    assert_eq!(mat.call_count(), 1, "no double-mutation under retry");
    assert_eq!(idem.len(), 1, "single dedup row");

    // Audit emitted twice: first Dispatched, then Duplicate.
    assert_eq!(audit.count_with_outcome(AuditOutcome::Dispatched), 1);
    assert_eq!(audit.count_with_outcome(AuditOutcome::Duplicate), 1);

    // SLI observed on BOTH calls (latency observed even for duplicate
    // ack — sad-path latency goes into dashboards).
    assert_eq!(sli.count(), 2);
}

// =========================================================================
// (3) Bad signature → 401 + SignatureInvalid audit.
// =========================================================================

#[tokio::test]
async fn bad_signature_returns_401_and_emits_signature_invalid_audit() {
    let (state, _idem, mat, audit, _sli) = build_fixture();
    let body = event_body("evt_evil", "invoice.paid");

    // Construct a syntactically-valid-looking but HMAC-bogus header.
    let mut headers = HeaderMap::new();
    headers.insert(
        "stripe-signature",
        format!(
            "t={FIXED_TS},v1=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        )
        .parse()
        .unwrap(),
    );

    let (status, _) = post_webhook(state, headers, body).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "bad signature must surface 401 per PCI DSS SAQ-A envelope",
    );
    assert_eq!(mat.call_count(), 0, "no dispatch on bad signature");

    // The canonical audit row `corelink.billing.stripe_event_processed.v1`
    // is emitted with outcome=SignatureInvalid.
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
    let rec = audit
        .records()
        .into_iter()
        .find(|r| r.outcome == AuditOutcome::SignatureInvalid)
        .expect("SignatureInvalid audit row must be present");
    assert_eq!(rec.event_name, "corelink.billing.stripe_event_processed.v1");
}

// =========================================================================
// (4) Missing `Stripe-Signature` header → 400 + SignatureInvalid audit.
// =========================================================================

#[tokio::test]
async fn missing_signature_header_returns_400() {
    let (state, _idem, mat, audit, _sli) = build_fixture();
    let body = event_body("evt_nohdr", "invoice.paid");

    let (status, _) = post_webhook(state, HeaderMap::new(), body).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
}

// =========================================================================
// (5) Malformed envelope (signed garbage) → 422 + EnvelopeInvalid audit.
// =========================================================================

#[tokio::test]
async fn malformed_envelope_returns_422() {
    let (state, _idem, mat, audit, _sli) = build_fixture();
    let body = b"this-is-not-json".to_vec();
    let headers = signed_headers(&body, FIXED_TS);

    let (status, _) = post_webhook(state, headers, body).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::EnvelopeInvalid), 1);
}

// =========================================================================
// (6) State-mutator events go to the right materializer method (smoke
//     test that the canonical 10-event taxonomy is preserved).
// =========================================================================

#[tokio::test]
async fn state_mutators_route_to_correct_materializer_methods() {
    let pairs = [
        (
            "customer.subscription.deleted",
            CanonicalWebhookEventType::SubscriptionDeleted,
        ),
        (
            "customer.subscription.updated",
            CanonicalWebhookEventType::SubscriptionUpdated,
        ),
        ("invoice.paid", CanonicalWebhookEventType::InvoicePaid),
        (
            "invoice.payment_failed",
            CanonicalWebhookEventType::InvoicePaymentFailed,
        ),
        (
            "charge.dispute.created",
            CanonicalWebhookEventType::ChargeDisputeCreated,
        ),
    ];
    for (i, (raw, expected)) in pairs.iter().enumerate() {
        let (state, _idem, mat, _audit, _sli) = build_fixture();
        let event_id = format!("evt_smut_{i}");
        let body = event_body(&event_id, raw);
        let headers = signed_headers(&body, FIXED_TS);

        let (status, _) = post_webhook(state, headers, body).await;
        assert_eq!(status, StatusCode::OK, "raw={raw}");

        let calls = mat.calls();
        assert_eq!(calls.len(), 1, "raw={raw}");
        assert_eq!(&calls[0].0, expected, "raw={raw}");
        assert_eq!(calls[0].1, event_id);
    }
}

// =========================================================================
// (7) Observability-echo events do NOT touch materializer but still
//     audit Dispatched + return 200 (forward-compat).
// =========================================================================

#[tokio::test]
async fn observability_echoes_audit_without_state_mutation() {
    let echoes = [
        "customer.subscription.created",
        "customer.subscription.trial_will_end",
        "charge.refunded",
        "customer.created",
        "invoice.created",
    ];
    for (i, raw) in echoes.iter().enumerate() {
        let (state, _idem, mat, audit, _sli) = build_fixture();
        let event_id = format!("evt_echo_{i}");
        let body = event_body(&event_id, raw);
        let headers = signed_headers(&body, FIXED_TS);

        let (status, _) = post_webhook(state, headers, body).await;
        assert_eq!(status, StatusCode::OK, "raw={raw}");

        // No state mutation method called for echoes.
        assert_eq!(mat.call_count(), 0, "raw={raw}");
        // Audit Dispatched still emitted (echo = ack + audit).
        assert_eq!(
            audit.count_with_outcome(AuditOutcome::Dispatched),
            1,
            "raw={raw}",
        );
    }
}
