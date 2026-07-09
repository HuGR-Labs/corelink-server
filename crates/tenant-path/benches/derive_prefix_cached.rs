//! Criterion bench for [`TenantPrefixCache`] (OPT-01).
//!
//! Two groups:
//! - `cold_miss` — cache empty, every call falls through to `derive_prefix`.
//! - `warm_hit` — cache pre-populated with 50 tenants, all calls hit.
//!
//! Acceptance gate per
//! `specs/_audits/sealed/perf-optimization-followup-tickets.md` OPT-01:
//! `warm_hit` p99 must be ≤ 100 ns.
//!
//! The cold-miss path is intentionally close to (slightly slower than)
//! the bare `derive_prefix` bench — overhead = one HashMap read + miss
//! branch + write-lock insert.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::indexing_slicing,
    reason = "bench harness; macros generate items we do not own"
)]

use corelink_tenant_path::{TdkVersion, TenantDerivationKey, TenantPrefixCache};
use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use uuid::Uuid;
use zeroize::Zeroizing;

fn bench_cached_miss(c: &mut Criterion) {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0xA5; 32]));
    let v = TdkVersion(1);
    c.bench_function("tenant_path/derive_prefix_cached/cold_miss", |b| {
        b.iter(|| {
            // Fresh cache per iter — every call is a miss.
            let cache = TenantPrefixCache::new();
            let tid = Uuid::now_v7();
            let p = cache.get_or_derive(black_box(&tdk), v, black_box(tid));
            black_box(p);
        });
    });
}

fn bench_cached_hit(c: &mut Criterion) {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0xA5; 32]));
    let v = TdkVersion(1);
    let cache = TenantPrefixCache::new();
    // Pre-populate 50 tenants.
    let tenants: Vec<Uuid> = (0..50).map(|_| Uuid::now_v7()).collect();
    for t in &tenants {
        let _ = cache.get_or_derive(&tdk, v, *t);
    }
    let mut i = 0usize;
    c.bench_function("tenant_path/derive_prefix_cached/warm_hit", |b| {
        b.iter(|| {
            let idx = i % tenants.len();
            i = i.wrapping_add(1);
            // `.get` returns Option; use the safe accessor to satisfy
            // the workspace's `clippy::indexing_slicing` deny lint.
            if let Some(t) = tenants.get(idx) {
                let p = cache.get_or_derive(black_box(&tdk), v, black_box(*t));
                black_box(p);
            }
        });
    });
}

criterion_group!(benches, bench_cached_miss, bench_cached_hit);
criterion_main!(benches);
