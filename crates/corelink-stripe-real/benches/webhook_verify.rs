//! Criterion benchmark — Stripe webhook HMAC-SHA256 signature verify.
//!
//! # Target
//!
//! - p99 < 100 µs on a 4 KiB payload (HMAC-SHA256 + constant-time compare).
//!
//! The verify path is on every Stripe webhook receipt; a regression here
//! directly inflates the SLO-LATENCY-WEBHOOK-INGEST budget (see
//! `specs/03_architecture/slo_catalog.md`).

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use corelink_stripe_real::{
    webhook::compute_signature, verify_webhook_signature, DEFAULT_TOLERANCE_SECONDS,
};
use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};

const SECRET: &[u8] = b"whsec_bench_super_secret_value_32_bytes_minimum";
const TS: u64 = 1_700_000_000;

fn header_for(payload: &[u8]) -> String {
    let sig = compute_signature(SECRET, TS, payload);
    format!("t={TS},v1={sig}")
}

fn bench_verify_sizes(c: &mut Criterion) {
    let mut g = c.benchmark_group("stripe_webhook/verify");
    for &size in &[256usize, 1024, 4096, 16_384] {
        let payload = vec![0x42u8; size];
        let header = header_for(&payload);
        g.throughput(Throughput::Bytes(size as u64));
        g.bench_with_input(
            BenchmarkId::from_parameter(format!("{size}B")),
            &(payload, header),
            |b, (payload, header)| {
                b.iter(|| {
                    let r = verify_webhook_signature(
                        black_box(payload),
                        black_box(header),
                        black_box(SECRET),
                        TS,
                        DEFAULT_TOLERANCE_SECONDS,
                    );
                    r.expect("verify ok");
                });
            },
        );
    }
    g.finish();
}

fn bench_compute_signature(c: &mut Criterion) {
    let payload = vec![0x42u8; 4096];
    c.bench_function("stripe_webhook/compute_signature_4KiB", |b| {
        b.iter(|| {
            let s = compute_signature(black_box(SECRET), TS, black_box(&payload));
            black_box(s);
        });
    });
}

criterion_group!(benches, bench_verify_sizes, bench_compute_signature);
criterion_main!(benches);
