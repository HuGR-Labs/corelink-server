//! Criterion benchmark — signup orchestrator happy-path provisioning with
//! InMemory deps.
//!
//! # Target
//!
//! - p99 < 50 ms (InMemory deps; production target inflated by D1
//!   BEGIN/COMMIT + Stripe customer create RTT — see
//!   `specs/03_architecture/slo_catalog.md` SLO-LATENCY-SIGNUP).
//!
//! Bench shape:
//! 1. `signup/provision_new` — fresh idempotency key, full pipeline runs.
//! 2. `signup/provision_idempotent_replay` — duplicate idempotency hit (cache).

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use corelink_signup::orchestrator::InMemoryProvisionRecord;
use corelink_signup::{
    Bcp47Locale, CorrelationId, IdempotencyKey, InMemoryAtomicSignupStore, InMemoryBillingClient,
    InMemorySignupAuditSink, SignupOrchestrator, SignupRequest, UserEmailHash,
};
use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};

fn build_request(idem: &str) -> SignupRequest {
    SignupRequest::new(
        "evt_bench",
        UserEmailHash::new("deadbeefcafe"),
        Bcp47Locale::new("en-US"),
        IdempotencyKey::new(idem),
        CorrelationId::new("corr_bench"),
    )
}

fn build_orchestrator() -> SignupOrchestrator<
    InMemorySignupAuditSink,
    InMemoryAtomicSignupStore,
    InMemoryBillingClient,
    InMemoryProvisionRecord,
> {
    SignupOrchestrator::new(
        InMemorySignupAuditSink::new(),
        InMemoryAtomicSignupStore::new(),
        InMemoryBillingClient::new(),
        InMemoryProvisionRecord::new(),
    )
}

fn bench_provision_new(c: &mut Criterion) {
    c.bench_function("signup/provision_new", |b| {
        let mut counter: u64 = 0;
        b.iter(|| {
            // Each iter uses a fresh idempotency key + a fresh orchestrator
            // so the cache miss path is exercised every time.
            counter = counter.saturating_add(1);
            let o = build_orchestrator();
            let req = build_request(&format!("idem-{counter}"));
            let resp = o.provision(black_box(&req)).expect("provision");
            black_box(resp);
        });
    });
}

fn bench_provision_idempotent_replay(c: &mut Criterion) {
    c.bench_function("signup/provision_idempotent_replay", |b| {
        let o = build_orchestrator();
        let req = build_request("idem-replay");
        // Seed the orchestrator with a successful provision so the replay
        // hits the idempotency cache.
        let _ = o.provision(&req).expect("seed");
        b.iter(|| {
            let resp = o.provision(black_box(&req)).expect("replay");
            black_box(resp);
        });
    });
}

criterion_group!(
    benches,
    bench_provision_new,
    bench_provision_idempotent_replay
);
criterion_main!(benches);
