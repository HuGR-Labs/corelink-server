//! Criterion benchmark for WI-S02-004 §10.4.2 — side-channel
//! `|Δmedian|` ≤ 1 ms across the 3 [`MissReason`] arms with the
//! [`TimingPaddingLayer`] applied.
//!
//! The bench is **release-only** (criterion drives release builds by
//! default) and produces three groups:
//!
//! 1. `mwu_normal_approx` — micro-bench of the
//!    [`mann_whitney_u_p_value`] kernel across 100/1000/10000-sample
//!    inputs so a future regression that slows the statistical kernel
//!    surfaces in CI before the integration test budget blows up.
//! 2. `bootstrap_ci_200_iter_10k_samples` — micro-bench of
//!    [`bootstrap_median_ci`] with `iterations = 200` (the canonical
//!    setting in `timing_indistinguishability.rs`).
//! 3. `pad_target_kernel` — micro-bench of [`canonical_pad_target`]
//!    with seeded jitter so the per-request hot path stays
//!    sub-microsecond.
//!
//! ## Acceptance gates (asserted in CI by the integration test)
//!
//! - `|Δmedian|` point estimate ≤ 1 ms (per WI-S02-004 §10.4.2
//!   evidence-grade gate; replaces the looser `p99 diff < 5ms` gate
//!   per cycle 13 SEAL math correction).
//! - `mwu_normal_approx/10000` ≤ 30 ms wall-clock (so 9 invocations
//!   per CI run × 3 trials stays under the 5-minute budget).
//!
//! Criterion's HTML reports are saved under `target/criterion`; CI
//! tooling can parse `estimates.json` to enforce regressions on the
//! mean kernel duration.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    missing_docs,
    reason = "bench code: panics surface as bench failures by design; criterion macros emit undocumented fns"
)]

use std::time::Duration;

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use corelink_worker::middleware::timing_padding::{
    bootstrap_median_ci, canonical_pad_target, mann_whitney_u_p_value, JitterPolicy,
    TimingPaddingConfig,
};

fn bench_mwu(c: &mut Criterion) {
    let mut group = c.benchmark_group("mwu_normal_approx");
    for &n in &[100usize, 1000, 10_000] {
        let mut rng = ChaCha20Rng::seed_from_u64(0xCAFE_F00D);
        let xs: Vec<f64> = (0..n).map(|_| rng.random_range(0.0..1000.0)).collect();
        let ys: Vec<f64> = (0..n).map(|_| rng.random_range(0.0..1000.0)).collect();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let p = mann_whitney_u_p_value(black_box(&xs), black_box(&ys));
                black_box(p);
            });
        });
    }
    group.finish();
}

fn bench_bootstrap(c: &mut Criterion) {
    let mut rng = ChaCha20Rng::seed_from_u64(0x00C0_FFEE);
    let xs: Vec<f64> = (0..10_000)
        .map(|_| 200.0 + rng.random_range(-20.0..20.0))
        .collect();
    let ys: Vec<f64> = (0..10_000)
        .map(|_| 200.0 + rng.random_range(-20.0..20.0))
        .collect();
    c.bench_function("bootstrap_ci_200_iter_10k_samples", |b| {
        b.iter(|| {
            let ci = bootstrap_median_ci(black_box(&xs), black_box(&ys), 200, 0xBEEF);
            black_box(ci);
        });
    });
}

fn bench_pad_target(c: &mut Criterion) {
    let cfg = TimingPaddingConfig::canonical();
    let policy = JitterPolicy::Seeded;
    let elapsed = Duration::from_millis(50);
    c.bench_function("pad_target_seeded_jitter", |b| {
        let mut seed: u64 = 0;
        b.iter(|| {
            seed = seed.wrapping_add(1);
            let target = canonical_pad_target(
                black_box(cfg),
                black_box(&policy),
                black_box(seed),
                black_box(elapsed),
            );
            black_box(target);
        });
    });
}

/// 3-arm pairwise gate-cost benchmark per WI-S02-004 §10.4.2 (codex
/// round-2 P2 — the `|Δmedian|` evidence gate runs in the integration
/// test `three_arm_indistinguishability_with_padding`; this bench
/// measures the **cost** of running the gate on canonical-shape arms
/// so a regression that slows the gate enough to blow the CI budget
/// surfaces here. The gate's *correctness* (point estimate ≤ 1 ms,
/// CI upper ≤ 1 ms) is asserted at integration-test runtime; this
/// bench keeps the gate kernel cheap so the test suite stays green).
fn bench_three_arm_pairwise_median_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("three_arm_pairwise_median_diff");
    // Generate canonical 10k-sample padded vectors per arm via the
    // same seed used in the integration test fixture so the bench's
    // baseline matches.
    let cfg = TimingPaddingConfig::canonical();
    let policy = JitterPolicy::Seeded;
    let mut arms: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (arm_idx, base_ms) in [50.0_f64, 60.0, 80.0].iter().enumerate() {
        let mut rng = ChaCha20Rng::seed_from_u64(0xBEEF_F00D + arm_idx as u64);
        for i in 0..10_000usize {
            let resolution = base_ms + rng.random_range(0.0..15.0_f64);
            let elapsed = Duration::from_micros((resolution * 1000.0) as u64);
            let pad = canonical_pad_target(cfg, &policy, i as u64, elapsed);
            arms[arm_idx].push(pad.as_secs_f64() * 1000.0);
        }
    }
    let pairs: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];
    for (a, b) in pairs.iter().copied() {
        group.bench_with_input(
            BenchmarkId::new("pair", format!("{a}_{b}")),
            &(a, b),
            |bench, &(a, b)| {
                bench.iter(|| {
                    let ci =
                        bootstrap_median_ci(black_box(&arms[a]), black_box(&arms[b]), 100, 0xC1A0);
                    black_box(ci);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    side_channel_benches,
    bench_mwu,
    bench_bootstrap,
    bench_pad_target,
    bench_three_arm_pairwise_median_diff,
);
criterion_main!(side_channel_benches);
