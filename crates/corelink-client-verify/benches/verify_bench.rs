//! Criterion benchmarks for `corelink-client-verify`.
//!
//! Tracks WI-S02-003 §14.3.4: verify p99 ≤ 3.5 ms for 5 MiB blob
//! (criterion sustained, native release; WASM is 2-3× the budget).

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    reason = "bench harness; macro-generated items"
)]

use corelink_client_verify::{ClientVerifier, Digest};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

fn bench_verify_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("ClientVerifier::verify");
    let v = ClientVerifier::default_on();
    for size_kib in &[1usize, 64, 1024, 5 * 1024] {
        let size_bytes = size_kib * 1024;
        let body = vec![0xA5u8; size_bytes];
        let expected = Digest::compute(&body);
        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{size_kib} KiB")),
            &(body, expected),
            |b, (body, expected)| {
                b.iter(|| {
                    v.verify(black_box(body.as_slice()), black_box(expected))
                        .expect("ok");
                });
            },
        );
    }
    group.finish();
}

fn bench_verify_constant_time_partial(c: &mut Criterion) {
    let truth = Digest::compute(b"the canonical body for ct-variance bench");
    // Crafted near-match: identical except for the very last byte. The
    // bench should produce a flat curve: constant-time compare must
    // not short-circuit on a long correct prefix.
    let mut bytes = *truth.as_bytes();
    bytes[31] ^= 0x01;
    let almost_match = Digest::from_hex(&hex::encode(bytes)).expect("hex");
    let zero = Digest::from_hex(&"0".repeat(64)).expect("hex");
    let body = b"the canonical body for ct-variance bench";

    let v = ClientVerifier::default_on();
    let mut group = c.benchmark_group("verify_partial_match");
    group.bench_function("zero_prefix", |b| {
        b.iter(|| {
            let _ = v.verify(black_box(body), black_box(&zero));
        });
    });
    group.bench_function("31_byte_correct_prefix", |b| {
        b.iter(|| {
            let _ = v.verify(black_box(body), black_box(&almost_match));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_verify_sync,
    bench_verify_constant_time_partial
);
criterion_main!(benches);
