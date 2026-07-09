//! Criterion benchmark — tier-selection orchestrator (Free + Starter paths).
//!
//! # Target
//!
//! - p99 < 20 ms (InMemory deps; production target inflated by D1 row-lock
//!   contention + Stripe Checkout Session creation RTT for paid tiers).
//!
//! Bench shape:
//! 1. `tier_selection/select_free` — DPA-accepted, Free tier, instant
//!    activation (no Stripe call).
//! 2. `tier_selection/select_starter` — DPA-accepted, Starter tier, opens
//!    a Checkout Session via the InMemory Stripe stub.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use std::sync::Arc;

use corelink_tier_selection::{
    InMemoryDpaGate, InMemoryStripeClient, InMemoryTierSelectionAuditSink, TenantCtx, TenantId,
    TierKind, TierSelectionLedger,
};
use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};

fn fresh_ledger() -> TierSelectionLedger {
    let dpa = InMemoryDpaGate::new();
    // Accept up front so every iteration takes the DPA-OK path.
    dpa.accept(TenantId::new("bench-tenant"), "v1");
    TierSelectionLedger::new(
        Arc::new(dpa),
        Arc::new(InMemoryStripeClient::new()),
        Arc::new(InMemoryTierSelectionAuditSink::new()),
        "v1",
        "https://app.example/success",
        "https://app.example/cancel",
    )
}

fn ctx(now_ms: u64) -> TenantCtx {
    TenantCtx::new(TenantId::new("bench-tenant"), now_ms, "corr-bench")
}

fn bench_select_free(c: &mut Criterion) {
    c.bench_function("tier_selection/select_free", |b| {
        // Fresh ledger per iteration; lock + UNIQUE-active checks must run
        // on a clean slate so the Free path completes without `AlreadyActive`.
        let mut now: u64 = 1_700_000_000_000;
        b.iter(|| {
            now = now.saturating_add(1);
            let ledger = fresh_ledger();
            let receipt = ledger
                .select_tier(
                    black_box(&ctx(now)),
                    black_box(TierKind::Free),
                    "user@example.com",
                )
                .expect("free activation");
            black_box(receipt);
        });
    });
}

fn bench_select_starter(c: &mut Criterion) {
    c.bench_function("tier_selection/select_starter", |b| {
        let mut now: u64 = 1_700_000_000_000;
        b.iter(|| {
            now = now.saturating_add(1);
            let ledger = fresh_ledger();
            let receipt = ledger
                .select_tier(
                    black_box(&ctx(now)),
                    black_box(TierKind::Starter),
                    "user@example.com",
                )
                .expect("starter checkout");
            black_box(receipt);
        });
    });
}

criterion_group!(benches, bench_select_free, bench_select_starter);
criterion_main!(benches);
