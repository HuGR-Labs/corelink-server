//! End-to-end materializer integration tests.
//!
//! Drives the canonical `corelink_stripe_real::webhook_dispatch::WebhookDispatcher`
//! end-to-end with the wave-17 production materializer +
//! `RealStripeAuditEmitter` + `D1IdempotencyStore`. Verifies the full
//! pipeline: HTTP-shape input bytes → signature verify → dedup →
//! classify → materialize → D1 row(s) + audit chain emit.
//!
//! Scenarios covered:
//!
//! 1. Each of the 10 SLA event types round-trips to the right
//!    D1 table + the right canonical audit event name.
//! 2. Tier change scenario: subscription.updated `basic` → `pro` →
//!    `tier_selections.tier` updated + `tier_changed.v1` emitted.
//! 3. Idempotency: same stripe_event_id replayed → no second
//!    materialization (D1 dedup table holds it).
//! 4. Audit fail-CLOSED: if D1 write fails, dispatcher still emits
//!    the dispatcher audit + returns 500.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed these primitives"
)]

use std::sync::Arc;

use corelink_billing_stripe_materializer::{
    BillingAuditError, BillingD1Error, BillingD1Writer, D1IdempotencyStore,
    D1SubscriptionStateHandler, InMemoryBillingAuditEmitter, InMemoryBillingD1,
    InMemoryTierSelector, RealStripeAuditEmitter, EVENT_MATERIALIZATION_MATRIX,
};
use corelink_stripe_real::webhook_dispatch::{
    CanonicalWebhookEventType, DispatchResponse, FixedClock, RecordingSliRecorder, StateMaterializer,
    StripeWebhookEnvelope, WebhookDispatcher,
};
use corelink_stripe_real::webhook::compute_signature;
use corelink_tier_selection::tier::TierKind;

const SECRET: &[u8] = b"whsec_wave17_materializer_e2e";
const FIXED_TS: u64 = 1_715_000_000;

struct Bundle {
    dispatcher: Arc<WebhookDispatcher>,
    d1: Arc<InMemoryBillingD1>,
    audit: Arc<InMemoryBillingAuditEmitter>,
    handler: Arc<D1SubscriptionStateHandler>,
}

fn build_bundle() -> Bundle {
    let d1 = Arc::new(InMemoryBillingD1::new());
    let audit_sink = Arc::new(InMemoryBillingAuditEmitter::new());
    let tier_selector = Arc::new(InMemoryTierSelector::with_mapping(&[
        ("plan_basic", TierKind::Starter),
        ("plan_team", TierKind::Team),
        ("plan_pro", TierKind::Pro),
    ]));
    let handler = Arc::new(D1SubscriptionStateHandler::new(
        d1.clone() as Arc<dyn BillingD1Writer>,
        audit_sink.clone(),
        tier_selector,
    ));
    let dispatcher_audit = Arc::new(RealStripeAuditEmitter::new(audit_sink.clone()));
    let idem = Arc::new(D1IdempotencyStore::new(d1.clone() as Arc<dyn BillingD1Writer>));
    let sli = Arc::new(RecordingSliRecorder::new());
    let clock = Arc::new(FixedClock::new(FIXED_TS, 0.0));
    let dispatcher = Arc::new(WebhookDispatcher::new(
        SECRET.to_vec(),
        idem,
        handler.clone() as Arc<dyn StateMaterializer>,
        dispatcher_audit,
        sli,
        clock,
    ));
    Bundle {
        dispatcher,
        d1,
        audit: audit_sink,
        handler,
    }
}

fn signed_envelope_with_object(
    id: &str,
    kind: &str,
    object: serde_json::Value,
) -> (Vec<u8>, String) {
    let body = serde_json::to_vec(&serde_json::json!({
        "id": id,
        "type": kind,
        "created": FIXED_TS,
        "data": { "object": object },
    }))
    .unwrap();
    let sig = compute_signature(SECRET, FIXED_TS, &body);
    let header = format!("t={FIXED_TS},v1={sig}");
    (body, header)
}

fn envelope_for(event_type: &str, id: &str) -> (Vec<u8>, String) {
    let object = match event_type {
        "customer.subscription.deleted" => serde_json::json!({
            "id": "sub_canceled",
            "status": "canceled",
            "metadata": { "tenant_id": "ten_1" },
        }),
        "customer.subscription.updated" => serde_json::json!({
            "id": "sub_updated",
            "status": "active",
            "metadata": { "tenant_id": "ten_1" },
            "plan": { "id": "plan_pro" },
            "quantity": 1,
        }),
        "invoice.paid" => serde_json::json!({
            "id": "in_paid",
            "metadata": { "tenant_id": "ten_1" },
        }),
        "invoice.payment_failed" => serde_json::json!({
            "id": "in_failed",
            "metadata": { "tenant_id": "ten_1" },
        }),
        "charge.dispute.created" => serde_json::json!({
            "id": "dp_1",
            "metadata": { "tenant_id": "ten_1" },
        }),
        "customer.subscription.created" => serde_json::json!({
            "id": "sub_created",
            "status": "active",
            "metadata": { "tenant_id": "ten_1" },
            "plan": { "id": "plan_basic" },
        }),
        "customer.subscription.trial_will_end" => serde_json::json!({
            "id": "sub_trial",
            "metadata": { "tenant_id": "ten_1" },
        }),
        "charge.refunded" => serde_json::json!({
            "id": "ch_refund",
            "metadata": { "tenant_id": "ten_1" },
        }),
        "customer.created" => serde_json::json!({
            "id": "cus_1",
            "metadata": { "tenant_id": "ten_1" },
        }),
        "invoice.created" => serde_json::json!({
            "id": "in_created",
            "metadata": { "tenant_id": "ten_1" },
        }),
        other => panic!("unsupported event_type in fixture: {other}"),
    };
    signed_envelope_with_object(id, event_type, object)
}

// ---------------------------------------------------------------------------
// 10-event matrix: each SLA event round-trips through the dispatcher
// and lands the canonical D1 row(s) + audit row(s) per the matrix.
// ---------------------------------------------------------------------------

#[test]
fn ten_event_matrix_round_trips_dispatcher_through_materializer() {
    let bundle = build_bundle();
    let mut idx = 0_usize;
    for (event_type, table_or_none, audit_name) in EVENT_MATERIALIZATION_MATRIX {
        idx += 1;
        let (body, hdr) = envelope_for(event_type, &format!("evt_matrix_{idx}"));
        // The dispatcher routes the FIVE state mutators through the
        // typed materializer methods; the remaining FIVE observability
        // echoes are not routed by the dispatcher today (per wave-15
        // contract). For the echo arms we drive
        // `materialize_echo` directly so the e2e test pins the full
        // 10-row matrix.
        let canon = CanonicalWebhookEventType::classify(event_type);
        if canon.is_state_mutator() {
            let resp = bundle.dispatcher.process(&body, Some(&hdr));
            assert_eq!(
                resp,
                DispatchResponse::Ok200,
                "dispatch state-mutator {event_type} should return 200"
            );
        } else if matches!(canon, CanonicalWebhookEventType::Unknown) {
            panic!("matrix entry classified as Unknown: {event_type}");
        } else {
            // The dispatcher acks echo arms but does not call the
            // materializer. Drive the materializer's echo entry-point
            // directly so the audit + D1 trail is pinned for all 10.
            let env: StripeWebhookEnvelope = serde_json::from_slice(&body).unwrap();
            bundle
                .handler
                .materialize_echo(canon, &env)
                .expect("echo materialize");
        }

        // Per-arm audit expectation:
        let audit_count = bundle.audit.count_event(audit_name);
        assert!(
            audit_count >= 1,
            "event_type={event_type} → expected audit_name `{audit_name}` to appear, got {audit_count}"
        );

        // Per-arm D1 expectation:
        if let Some(table) = table_or_none {
            assert!(
                bundle.d1.count_table(table) >= 1,
                "event_type={event_type} → expected D1 table `{table}` to have ≥1 row"
            );
        }
    }
    // Sanity: 10 distinct events were processed.
    assert_eq!(EVENT_MATERIALIZATION_MATRIX.len(), 10);
}

// ---------------------------------------------------------------------------
// Tier-change scenario.
// ---------------------------------------------------------------------------

#[test]
fn tier_change_scenario_basic_to_pro_emits_tier_changed_audit() {
    let bundle = build_bundle();
    // Seed: tenant is at `starter`.
    bundle
        .d1
        .upsert_tier("ten_1", "starter", "init")
        .expect("seed tier");

    // subscription.updated → plan_pro
    let (body, hdr) = signed_envelope_with_object(
        "evt_tier_change",
        "customer.subscription.updated",
        serde_json::json!({
            "id": "sub_x",
            "status": "active",
            "metadata": { "tenant_id": "ten_1" },
            "plan": { "id": "plan_pro" },
            "quantity": 5,
        }),
    );
    let resp = bundle.dispatcher.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::Ok200);

    // Tier flipped.
    assert_eq!(bundle.d1.tier_for("ten_1"), Some("pro".to_string()));
    // Tier-changed audit emitted exactly once.
    assert_eq!(
        bundle.audit.count_event("corelink.tenant.tier_changed.v1"),
        1
    );
    // Subscription audit also emitted exactly once.
    assert_eq!(
        bundle
            .audit
            .count_event("corelink.billing.subscription.materialized.v1"),
        1
    );
}

#[test]
fn tier_change_no_op_when_target_tier_equals_current() {
    let bundle = build_bundle();
    bundle
        .d1
        .upsert_tier("ten_1", "pro", "init")
        .expect("seed tier");

    let (body, hdr) = signed_envelope_with_object(
        "evt_same_tier",
        "customer.subscription.updated",
        serde_json::json!({
            "id": "sub_y",
            "status": "active",
            "metadata": { "tenant_id": "ten_1" },
            "plan": { "id": "plan_pro" },
            "quantity": 1,
        }),
    );
    let resp = bundle.dispatcher.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::Ok200);

    // No tier_changed audit because the wire value is identical.
    assert_eq!(
        bundle.audit.count_event("corelink.tenant.tier_changed.v1"),
        0
    );
}

// ---------------------------------------------------------------------------
// Idempotency: replay produces no second materialization.
// ---------------------------------------------------------------------------

#[test]
fn idempotency_replay_does_not_materialize_twice() {
    let bundle = build_bundle();
    let (body, hdr) = envelope_for("invoice.paid", "evt_idem_1");

    let r1 = bundle.dispatcher.process(&body, Some(&hdr));
    let r2 = bundle.dispatcher.process(&body, Some(&hdr));
    assert_eq!(r1, DispatchResponse::Ok200);
    assert_eq!(r2, DispatchResponse::Ok200);
    // Materialization happened exactly once.
    assert_eq!(bundle.d1.count_table("stripe_invoices"), 1);
    assert_eq!(
        bundle
            .audit
            .count_event("corelink.billing.invoice.materialized.v1"),
        1
    );
    // Dedup table holds exactly one entry.
    assert_eq!(bundle.d1.distinct_events(), 1);
}

// ---------------------------------------------------------------------------
// Audit fail-CLOSED.
// ---------------------------------------------------------------------------

#[test]
fn audit_failure_propagates_500_and_no_d1_write() {
    let bundle = build_bundle();
    // Arm the audit sink to fail.
    bundle
        .audit
        .arm_failure(BillingAuditError::Transient("audit chain unavailable".into()));

    let (body, hdr) = envelope_for("invoice.paid", "evt_audit_fail");
    let resp = bundle.dispatcher.process(&body, Some(&hdr));
    // Dispatcher returns 500 because the dispatcher audit emit ALSO
    // fails (shares the same sink) — wave-15 fail-CLOSED contract.
    assert_eq!(resp, DispatchResponse::InternalError500);
    // No invoice row was written.
    assert_eq!(bundle.d1.count_table("stripe_invoices"), 0);
}

#[test]
fn d1_failure_during_state_mutation_returns_500() {
    let bundle = build_bundle();
    // Arm ONLY the D1 writer to fail; the audit sink keeps working
    // so the dispatcher can still log the failure outcome.
    bundle
        .d1
        .arm_failure(BillingD1Error::Transient("d1 unreachable".into()));

    let (body, hdr) = envelope_for("invoice.paid", "evt_d1_fail");
    let resp = bundle.dispatcher.process(&body, Some(&hdr));
    // The idempotency-store insert ALSO routes through the D1 writer
    // (`try_record_event`), so the dispatcher returns 500 at the dedup
    // step BEFORE reaching the materializer. Per wave-15 contract:
    // dedup-store transient → 500 with MaterializerFailed audit.
    assert_eq!(resp, DispatchResponse::InternalError500);
    assert_eq!(bundle.d1.count_table("stripe_invoices"), 0);
}

// ---------------------------------------------------------------------------
// Charter / surface invariants.
// ---------------------------------------------------------------------------

#[test]
fn matrix_covers_all_ten_canonical_event_types() {
    let mut have = std::collections::HashSet::new();
    for (et, _, _) in EVENT_MATERIALIZATION_MATRIX {
        have.insert(*et);
    }
    for canon in CanonicalWebhookEventType::sla_event_types() {
        assert!(
            have.contains(canon.label()),
            "matrix missing canonical event: {}",
            canon.label()
        );
    }
}

#[test]
fn matrix_audit_event_names_versioned_v1() {
    for (et, _, audit_name) in EVENT_MATERIALIZATION_MATRIX {
        assert!(
            audit_name.ends_with(".v1"),
            "matrix row {et} → audit name `{audit_name}` not .v1-versioned"
        );
        assert!(
            audit_name.starts_with("corelink."),
            "matrix row {et} → audit name `{audit_name}` missing corelink. prefix"
        );
    }
}
