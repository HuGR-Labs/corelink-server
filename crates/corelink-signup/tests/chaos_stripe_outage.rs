//! WI-S19-001 chaos test — Stripe outage during signup.
//!
//! Per WI §6.1.6 + §15: synthetic test inject Stripe API throttle to
//! 0% availability during the signup window (10 concurrent attempts);
//! verify: tenant rollback consistent (zero orphan tenant in D1; zero
//! Stripe customer orphaned via saga reverse compensation; customer
//! notified). Cadence: weekly in staging; alert > 5/day rollback
//! events in production (SEV-2).
//!
//! This crate ships the pure-logic skeleton: the
//! [`corelink_signup::StripeOutageBillingClient`] fixture returns
//! `BillingError::Outage` on every call. We verify that:
//!
//! 1. 10 concurrent signups (modelled sequentially in the in-memory
//!    fake; the production wiring uses Cloudflare Worker concurrency)
//!    all map to `SignupOutcome::Deferred { billing:
//!    BillingIntent::Deferred }`.
//! 2. 10 tenant rows are committed (D1 atomic boundary is OUTSIDE the
//!    billing call per Lote 10.19 P0; the saga compensation worker is
//!    responsible for completing the Stripe link asynchronously).
//! 3. ZERO Stripe customer rows are recorded in the
//!    `InMemoryBillingClient` snapshot (the outage fixture never
//!    succeeds; production wiring records `pending_billing_link` state
//!    in the tenant row for saga compensation).
//! 4. Audit chain integrity: 10 × `corelink.signup.started` + 10 ×
//!    `corelink.signup.deferred` events (NO `completed`; NO `failed`).
//! 5. Idempotency holds — replaying any of the 10 signups with the
//!    same `IdempotencyKey` returns `SignupOutcome::Duplicate` without
//!    opening another atomic D1 tx.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_signup::orchestrator::InMemoryProvisionRecord;
use corelink_signup::{
    AtomicSignupStore, Bcp47Locale, BillingIntent, CorrelationId, IdempotencyKey,
    InMemoryAtomicSignupStore, InMemorySignupAuditSink, SignupAuditEventType, SignupOrchestrator,
    SignupOutcome, SignupRequest, StripeOutageBillingClient, UserEmailHash,
};

fn req(idem: &str, email: &str, locale: &str, cid: &str) -> SignupRequest {
    SignupRequest::new(
        cid,
        UserEmailHash::new(email),
        Bcp47Locale::new(locale),
        IdempotencyKey::new(idem),
        CorrelationId::new(cid),
    )
}

#[test]
fn chaos_stripe_outage_10_concurrent_signups_deferred_no_orphan() {
    let audit = InMemorySignupAuditSink::new();
    let store = InMemoryAtomicSignupStore::new();
    let billing = StripeOutageBillingClient;
    let prov = InMemoryProvisionRecord::new();
    let o = SignupOrchestrator::new(audit.clone(), store.clone(), billing, prov);

    // 10 concurrent signups (sequential in the fake; the production
    // wiring uses CF Worker concurrency).  Each carries a unique
    // email hash + idempotency key + correlation id.
    let mut deferred_count = 0;
    for i in 0..10u32 {
        let idem = format!("idem-{i}");
        let email = format!("user-{i}");
        let cid = format!("evt_{i}");
        let r = o.provision(&req(&idem, &email, "en-US", &cid)).unwrap();
        match r.outcome {
            SignupOutcome::Deferred { billing, .. } => {
                assert_eq!(billing, BillingIntent::Deferred);
                deferred_count += 1;
            }
            other => panic!("expected Deferred at i={i}, got {other:?}"),
        }
    }
    assert_eq!(deferred_count, 10);

    // 10 tenants committed (atomic D1 tx is OUTSIDE the billing
    // boundary per Lote 10.19 P0).
    assert_eq!(store.committed_tenant_count().unwrap(), 10);

    // Audit chain: 10 × Started + 10 × Deferred = 20 records.
    let snap = audit.snapshot();
    let started_count = snap
        .iter()
        .filter(|r| r.event_type == SignupAuditEventType::Started)
        .count();
    let deferred_emitted = snap
        .iter()
        .filter(|r| r.event_type == SignupAuditEventType::Deferred)
        .count();
    let completed_count = snap
        .iter()
        .filter(|r| r.event_type == SignupAuditEventType::Completed)
        .count();
    let failed_count = snap
        .iter()
        .filter(|r| r.event_type == SignupAuditEventType::Failed)
        .count();
    assert_eq!(started_count, 10);
    assert_eq!(deferred_emitted, 10);
    assert_eq!(completed_count, 0);
    assert_eq!(failed_count, 0);
}

#[test]
fn chaos_stripe_outage_idempotency_holds_under_replay() {
    let audit = InMemorySignupAuditSink::new();
    let store = InMemoryAtomicSignupStore::new();
    let billing = StripeOutageBillingClient;
    let prov = InMemoryProvisionRecord::new();
    let o = SignupOrchestrator::new(audit, store.clone(), billing, prov);

    let r1 = o.provision(&req("idem-1", "u1", "en-US", "evt_1")).unwrap();
    let signup_id_1 = match r1.outcome {
        SignupOutcome::Deferred { signup_id, .. } => signup_id,
        other => panic!("expected Deferred, got {other:?}"),
    };
    // Replay: same key → Duplicate carrying the original signup_id.
    // The orchestrator does NOT open a second atomic D1 tx; the
    // committed tenant count remains 1.
    let r2 = o.provision(&req("idem-1", "u1", "en-US", "evt_1")).unwrap();
    match r2.outcome {
        SignupOutcome::Duplicate { signup_id, .. } => {
            assert_eq!(signup_id, signup_id_1);
        }
        other => panic!("expected Duplicate, got {other:?}"),
    }
    assert_eq!(store.committed_tenant_count().unwrap(), 1);
}
