//! Criterion benchmark — extended `derive_prefix` coverage (batch + key
//! rotation paths).
//!
//! # Target
//!
//! - Single-shot p99 < 100 µs (canonical from WI-S01-001 SEAL).
//! - Batch-of-1000 p99 < 100 ms (i.e. ≤ 100 µs amortised) — exercises the
//!   loop-tight path used by the tenant-listing endpoint when paginated
//!   prefix derivation happens server-side.
//! - Distinct-TDK rotation rebuild path — measures the cost when TDK
//!   rotation triggers re-derivation across a tenant batch.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use uuid::Uuid;
use zeroize::Zeroizing;

fn fixed_tdk(seed: u8) -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new([seed; 32]))
}

fn synthetic_uuid(n: u128) -> Uuid {
    // Pack n into the 128-bit body so the bench corpus is deterministic.
    Uuid::from_u128(n)
}

fn bench_batch_1000(c: &mut Criterion) {
    let tdk = fixed_tdk(0xA5);
    let ids: Vec<Uuid> = (0..1000u128).map(synthetic_uuid).collect();
    let mut g = c.benchmark_group("tenant_path/derive_prefix_batch");
    g.throughput(Throughput::Elements(1000));
    g.bench_function("1000-distinct-tenants", |b| {
        b.iter(|| {
            for tid in &ids {
                let p = derive_prefix(black_box(&tdk), black_box(*tid));
                black_box(p);
            }
        });
    });
    g.finish();
}

fn bench_rotation_rebuild(c: &mut Criterion) {
    // Each iter rebuilds 100 prefixes against a fresh TDK — mirrors the
    // TDK-rotation re-derivation path (S-13 rotation cadence 7d).
    let ids: Vec<Uuid> = (0..100u128).map(synthetic_uuid).collect();
    let mut g = c.benchmark_group("tenant_path/derive_prefix_rotation");
    g.throughput(Throughput::Elements(100));
    g.bench_function("100-tenants-fresh-tdk", |b| {
        let mut seed: u8 = 0;
        b.iter(|| {
            seed = seed.wrapping_add(1);
            let tdk = fixed_tdk(seed);
            for tid in &ids {
                let p = derive_prefix(black_box(&tdk), black_box(*tid));
                black_box(p);
            }
        });
    });
    g.finish();
}

criterion_group!(benches, bench_batch_1000, bench_rotation_rebuild);
criterion_main!(benches);
