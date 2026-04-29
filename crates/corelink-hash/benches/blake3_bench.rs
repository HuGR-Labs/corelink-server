//! Criterion benchmark for `Digest::compute` + `VerifiedBody::new`.
//!
//! AC §8 throughput target is p99 ≤ 3.5 ms on a 5 MiB blob in WASM
//! (≥ 1.4 GB/s). Native release runs typically clock 2-3× faster; this
//! bench is the per-PR perf observatory, complemented by the
//! release-only `perf_regression_5mib_under_50ms` test gate.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    reason = "bench harness; macro-generated items"
)]

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn bench_digest_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("Digest::compute");
    for size_kib in &[1usize, 64, 1024, 5 * 1024] {
        let size_bytes = size_kib * 1024;
        let body = vec![0xA5u8; size_bytes];
        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{size_kib} KiB")),
            &body,
            |b, body| b.iter(|| Digest::compute(black_box(body))),
        );
    }
    group.finish();
}

fn bench_verified_body_new(c: &mut Criterion) {
    let body = Bytes::from(vec![0xA5u8; 1024 * 1024]);
    let claimed = Digest::compute(&body);
    c.bench_function("VerifiedBody::new (1 MiB, ok)", |b| {
        b.iter(|| VerifiedBody::new(black_box(body.clone()), black_box(claimed)).expect("ok"));
    });
}

fn bench_verify_constant_time(c: &mut Criterion) {
    let a = Digest::compute(b"left");
    let b = Digest::compute(b"right");
    c.bench_function("Digest::verify_constant_time", |bencher| {
        bencher.iter(|| {
            black_box(black_box(&a).verify_constant_time(black_box(&b)))
        });
    });
}

criterion_group!(
    benches,
    bench_digest_compute,
    bench_verified_body_new,
    bench_verify_constant_time
);
criterion_main!(benches);
