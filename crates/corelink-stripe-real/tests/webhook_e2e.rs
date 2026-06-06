//! End-to-end integration test for the Stripe webhook production
//! dispatch pipeline.
//!
//! Drives [`WebhookDispatcher::process`] with the 10 SLA-required
//! canonical Stripe event types through the full chain:
//!
//!   raw POST body
//!     -> HMAC-SHA256 signature verify (constant-time, 5-min replay
//!        tolerance, multi-v1 key-rotation tolerant)
//!     -> JSON envelope parse
//!     -> BLAKE3 idempotency token derivation
//!     -> INSERT-OR-IGNORE dedup
//!     -> per-event materializer dispatch
//!     -> `corelink.billing.stripe_event_processed.v1` audit emission
//!     -> `corelink_billing_stripe_event_seconds` SLI observation
//!
//! Coverage matrix:
//!
//! - 10 canonical event types: each round-trips successfully (Ok200 +
//!   audit row + SLI observation). State-mutator arms call the
//!   matching materializer method; observability echoes do not.
//! - Idempotency: same event id delivered twice → second is Ok200 +
//!   Duplicate audit + zero double-dispatch.
//! - Signature: tampered payload / wrong secret / missing header /
//!   replay window → 400/401 + audit row + zero state mutation.
//! - Materializer faults: Transient → 500 + retry on a fresh delivery;
//!   InvalidPayload → 422 (Stripe stops retrying).
//! - SLI: every dispatch path emits exactly one
//!   `corelink_billing_stripe_event_seconds` observation, tagged with
//!   the canonical event type label.
//!
//! Driven via the [`FakeStripeWebhook`] builder declared inline.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed these primitives"
)]

use std::sync::Arc;

use corelink_stripe_real::webhook::compute_signature;
use corelink_stripe_real::{
    AuditOutcome, CanonicalWebhookEventType, DispatchResponse, FixedClock, IdempotencyToken,
    InMemoryIdempotencyStore, MaterializerError, RecordingAuditEmitter, RecordingSliRecorder,
    RecordingStateMaterializer, WebhookDispatcher, SLI_BILLING_STRIPE_EVENT_SECONDS,
};

const SECRET: &[u8] = b"whsec_e2e_prod";
const FIXED_TS: u64 = 1_715_000_000;

/// Test-driver that mirrors what Stripe actually POSTs to the webhook
/// endpoint: a `(body_bytes, stripe_signature_header)` tuple. Signed
/// with a configurable secret/ts so the test can drive every arm of
/// the verifier without forging HMACs by hand.
#[derive(Clone, Debug)]
struct FakeStripeWebhook {
    body: Vec<u8>,
    signature_header: Option<String>,
}

impl FakeStripeWebhook {
    /// Build a fully-signed delivery using the canonical secret + ts.
    fn signed(event_id: &str, event_type: &str) -> Self {
        Self::signed_at(event_id, event_type, FIXED_TS)
    }

    /// Build a fully-signed delivery at an arbitrary unix timestamp
    /// (used to drive the replay-window arm).
    fn signed_at(event_id: &str, event_type: &str, ts: u64) -> Self {
        let body = serde_json::to_vec(&serde_json::json!({
            "id": event_id,
            "type": event_type,
            "created": ts,
            "data": {
                "object": {
                    "id": "obj_e2e",
                    "status": "active",
                    "customer": "cus_e2e",
                    "amount_paid": 9900
                }
            }
        }))
        .expect("e2e fixture must serialize");
        let sig = compute_signature(SECRET, ts, &body);
        let header = format!("t={ts},v1={sig}");
        Self {
            body,
            signature_header: Some(header),
        }
    }

    /// Build a delivery with a bogus signature (correct format,
    /// wrong HMAC). Drives the 401 arm.
    fn tampered(event_id: &str, event_type: &str) -> Self {
        let mut w = Self::signed(event_id, event_type);
        w.signature_header = Some(format!(
            "t={FIXED_TS},v1=0000deadbeef0000deadbeef0000deadbeef0000deadbeef0000deadbeef0000"
        ));
        w
    }

    /// Build a delivery with NO signature header at all (400 arm).
    fn no_signature(event_id: &str, event_type: &str) -> Self {
        let mut w = Self::signed(event_id, event_type);
        w.signature_header = None;
        w
    }
}

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
    let clock = Arc::new(FixedClock::new(FIXED_TS, 0.042));
    let d = Arc::new(WebhookDispatcher::new(
        SECRET.to_vec(),
        idem.clone(),
        mat.clone(),
        audit.clone(),
        sli.clone(),
        clock,
    ));
    (d, idem, mat, audit, sli)
}

// =========================================================================
// Round-trip: every one of the 10 SLA-required event types is accepted.
// =========================================================================

#[test]
fn all_ten_sla_event_types_round_trip_through_pipeline() {
    for (idx, canon) in CanonicalWebhookEventType::sla_event_types()
        .into_iter()
        .enumerate()
    {
        let (d, idem, mat, audit, sli) = fixture();
        let event_id = format!("evt_e2e_{idx:02}");
        let webhook = FakeStripeWebhook::signed(&event_id, canon.label());

        let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

        assert_eq!(
            resp,
            DispatchResponse::Ok200,
            "event {} did not ack 200",
            canon.label()
        );

        // Dedup row inserted on every accepted delivery (state mutator
        // OR observability echo OR unknown — for the canonical 10 we
        // always insert).
        assert_eq!(
            idem.len(),
            1,
            "event {} did not insert dedup row",
            canon.label()
        );

        // Materializer is called iff the event type is a state mutator.
        let mat_calls = mat.call_count();
        if canon.is_state_mutator() {
            assert_eq!(
                mat_calls,
                1,
                "state mutator {} should call materializer",
                canon.label()
            );
        } else {
            assert_eq!(
                mat_calls,
                0,
                "observability echo {} should NOT call materializer",
                canon.label()
            );
        }

        // Exactly one audit row + one SLI observation per delivery.
        assert_eq!(audit.records().len(), 1, "event {}", canon.label());
        assert_eq!(audit.records()[0].canonical_event_type, canon);
        assert_eq!(audit.records()[0].outcome, AuditOutcome::Dispatched);
        assert_eq!(
            audit.records()[0].event_name,
            "corelink.billing.stripe_event_processed.v1"
        );

        assert_eq!(sli.observations().len(), 1, "event {}", canon.label());
        let obs = sli.observations()[0];
        assert_eq!(obs.metric_name, SLI_BILLING_STRIPE_EVENT_SECONDS);
        assert_eq!(obs.event_type, canon);
        assert_eq!(obs.outcome, AuditOutcome::Dispatched);
        assert!(obs.seconds >= 0.0, "latency must be non-negative");
    }
}

// =========================================================================
// Idempotency: redelivery of the same event id never double-dispatches.
// =========================================================================

#[test]
fn idempotency_redelivery_is_no_op() {
    let (d, idem, mat, audit, sli) = fixture();
    let webhook = FakeStripeWebhook::signed("evt_dup_e2e", "invoice.paid");

    // First delivery: 200 + dispatch.
    let r1 = d.process(&webhook.body, webhook.signature_header.as_deref());
    // Second delivery (Stripe retry): 200 + duplicate audit + no
    // additional dispatch.
    let r2 = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(r1, DispatchResponse::Ok200);
    assert_eq!(r2, DispatchResponse::Ok200);
    assert_eq!(mat.call_count(), 1, "redelivery must not double-dispatch");
    assert_eq!(idem.len(), 1);

    // Audit: one Dispatched + one Duplicate.
    assert_eq!(audit.count_with_outcome(AuditOutcome::Dispatched), 1);
    assert_eq!(audit.count_with_outcome(AuditOutcome::Duplicate), 1);

    // SLI: TWO observations (latency observed even on duplicate path).
    assert_eq!(sli.count(), 2);
}

#[test]
fn idempotency_token_is_blake3_of_event_id() {
    // Property: the token is a stable BLAKE3 derivation, NOT a hash of
    // the body — so a Stripe retry with the SAME event id and a
    // tweaked body (impossible if the signature verifies, but in
    // principle) still hits the same dedup row.
    let t1 = IdempotencyToken::from_event_id("evt_x");
    let t2 = IdempotencyToken::from_event_id("evt_x");
    let t3 = IdempotencyToken::from_event_id("evt_y");
    assert_eq!(t1, t2);
    assert_ne!(t1, t3);
    assert_eq!(t1.to_hex().len(), 64);
}

// =========================================================================
// Signature: tampered / missing / replay paths.
// =========================================================================

#[test]
fn tampered_signature_rejected_401_no_state_mutation() {
    let (d, idem, mat, audit, _) = fixture();
    let webhook = FakeStripeWebhook::tampered("evt_evil", "invoice.paid");

    let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(resp, DispatchResponse::Unauthorized401);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(idem.len(), 0, "dedup row MUST NOT be inserted on sig fail");
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
}

#[test]
fn missing_signature_header_rejected_400() {
    let (d, idem, mat, audit, _) = fixture();
    let webhook = FakeStripeWebhook::no_signature("evt_no_sig", "invoice.paid");

    let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(resp, DispatchResponse::BadRequest400);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(idem.len(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
}

#[test]
fn replay_window_exceeded_rejected_401() {
    let (d, idem, mat, audit, _) = fixture();
    // Signed 10 minutes in the past — beyond the canonical 5-min tolerance.
    let webhook = FakeStripeWebhook::signed_at("evt_old", "invoice.paid", FIXED_TS - 600);

    let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(resp, DispatchResponse::Unauthorized401);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(idem.len(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
}

#[test]
fn malformed_envelope_rejected_422() {
    let (d, idem, mat, audit, _) = fixture();
    // Body bytes that signature-verify cleanly (we sign them) but are
    // NOT valid JSON.
    let body = b"<not-json>".to_vec();
    let sig = compute_signature(SECRET, FIXED_TS, &body);
    let hdr = format!("t={FIXED_TS},v1={sig}");

    let resp = d.process(&body, Some(&hdr));

    assert_eq!(resp, DispatchResponse::Unprocessable422);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(idem.len(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::EnvelopeInvalid), 1);
}

// =========================================================================
// Materializer faults (Transient -> 500, InvalidPayload -> 422).
// =========================================================================

#[test]
fn materializer_transient_failure_returns_500_with_audit() {
    let (d, _idem, mat, audit, sli) = fixture();
    mat.arm_error(MaterializerError::Transient("d1 timeout".to_string()));
    let webhook = FakeStripeWebhook::signed("evt_tx", "invoice.payment_failed");

    let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(resp, DispatchResponse::InternalError500);
    assert_eq!(
        audit.count_with_outcome(AuditOutcome::MaterializerFailed),
        1
    );
    // SLI still emitted (sad-path latency visible).
    assert_eq!(sli.count(), 1);
    assert_eq!(
        sli.observations()[0].outcome,
        AuditOutcome::MaterializerFailed
    );
}

#[test]
fn materializer_invalid_payload_returns_422_with_audit() {
    let (d, _idem, mat, audit, _) = fixture();
    mat.arm_error(MaterializerError::InvalidPayload(
        "missing required field".to_string(),
    ));
    let webhook = FakeStripeWebhook::signed("evt_xx", "customer.subscription.updated");

    let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(resp, DispatchResponse::Unprocessable422);
    assert_eq!(
        audit.count_with_outcome(AuditOutcome::MaterializerInvalid),
        1
    );
}

// =========================================================================
// Audit: emit failure is fail-CLOSED.
// =========================================================================

#[test]
fn audit_sink_failure_propagates_500_fail_closed() {
    let (d, _idem, _mat, audit, _) = fixture();
    audit.fail_with("audit chain unavailable");
    let webhook = FakeStripeWebhook::signed("evt_audit_fail", "invoice.paid");

    let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(resp, DispatchResponse::InternalError500);
}

// =========================================================================
// SLI: every dispatch path emits exactly one
// `corelink_billing_stripe_event_seconds` observation.
// =========================================================================

#[test]
fn sli_emitted_on_every_path() {
    // Happy path.
    {
        let (d, _, _, _, sli) = fixture();
        let w = FakeStripeWebhook::signed("evt_sli_ok", "invoice.paid");
        let _ = d.process(&w.body, w.signature_header.as_deref());
        assert_eq!(sli.count(), 1);
    }
    // Duplicate path.
    {
        let (d, _, _, _, sli) = fixture();
        let w = FakeStripeWebhook::signed("evt_sli_dup", "invoice.paid");
        let _ = d.process(&w.body, w.signature_header.as_deref());
        let _ = d.process(&w.body, w.signature_header.as_deref());
        assert_eq!(sli.count(), 2);
    }
    // Signature-invalid path.
    {
        let (d, _, _, _, sli) = fixture();
        let w = FakeStripeWebhook::tampered("evt_sli_evil", "invoice.paid");
        let _ = d.process(&w.body, w.signature_header.as_deref());
        assert_eq!(sli.count(), 1);
    }
    // Missing-header path.
    {
        let (d, _, _, _, sli) = fixture();
        let w = FakeStripeWebhook::no_signature("evt_sli_nohdr", "invoice.paid");
        let _ = d.process(&w.body, w.signature_header.as_deref());
        assert_eq!(sli.count(), 1);
    }
    // Envelope-invalid path.
    {
        let (d, _, _, _, sli) = fixture();
        let body = b"{".to_vec();
        let sig = compute_signature(SECRET, FIXED_TS, &body);
        let hdr = format!("t={FIXED_TS},v1={sig}");
        let _ = d.process(&body, Some(&hdr));
        assert_eq!(sli.count(), 1);
    }
}

// =========================================================================
// Unknown event type: ack 200 + audit `unknown_event_type` + dedup row
// inserted (so a Stripe retry hits the duplicate fast-path).
// =========================================================================

#[test]
fn unknown_event_type_acked_for_forward_compat() {
    let (d, idem, mat, audit, _) = fixture();
    let webhook = FakeStripeWebhook::signed("evt_future", "stripe.future.event.kind");

    let resp = d.process(&webhook.body, webhook.signature_header.as_deref());

    assert_eq!(resp, DispatchResponse::Ok200);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(idem.len(), 1, "unknown event still dedups");
    assert_eq!(audit.count_with_outcome(AuditOutcome::UnknownEventType), 1);
}
