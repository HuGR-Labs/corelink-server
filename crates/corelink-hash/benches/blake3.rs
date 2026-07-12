//! Criterion benchmark — BLAKE3 hash throughput at canonical sizes.
//!
//! # Targets
//!
//! - 1 B / 1 KiB / 1 MiB: latency p99 reported per size.
//! - 100 MiB: throughput target ≥ 1.4 GiB/s on native release (WASM
//!   target ≥ 1.4 GB/s @ 5 MiB per AC §8; native typically 2-3x faster).
//!
//! Complements the existing `blake3_bench.rs` (1 KiB / 64 KiB / 1 MiB /
//! 5 MiB) by extending the range to 1 B (latency-dominated; setup cost)
//! and 100 MiB (throughput-saturation regime).

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use corelink_hash::Digest;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

fn bench_blake3_sizes(c: &mut Criterion) {
    // Each tuple: (label, byte size). Use small sample size for the 100 MiB
    // arm so the bench finishes in reasonable wall-clock on dev laptops.
    let cases: [(&'static str, usize); 4] = [
        ("1B", 1),
        ("1KiB", 1024),
        ("1MiB", 1024 * 1024),
        ("100MiB", 100 * 1024 * 1024),
    ];
    let mut g = c.benchmark_group("blake3/hash");
    for (label, size) in cases {
        let body = vec![0xA5u8; size];
        g.throughput(Throughput::Bytes(size as u64));
        // The 100 MiB case is large; reduce sample size + give it more time.
        if size >= 64 * 1024 * 1024 {
            g.sample_size(10);
            g.measurement_time(std::time::Duration::from_secs(20));
        } else {
            g.sample_size(100);
            g.measurement_time(std::time::Duration::from_secs(5));
        }
        g.bench_with_input(BenchmarkId::from_parameter(label), &body, |b, body| {
            b.iter(|| {
                let d = Digest::compute(black_box(body));
                black_box(d);
            });
        });
    }
    g.finish();
}

criterion_group!(benches, bench_blake3_sizes);
criterion_main!(benches);
