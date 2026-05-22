//! Mann-Whitney 3-prong cripto-grade constant-time gate
//! (WI-S04-004 §6.1.10 + §10.s04.004.2).
//!
//! ## Test plan
//!
//! 1. Three arms × 10 000 samples × 3 trials of [`HkdfVerifier::verify`]
//!    against:
//!    - **Arm A** — valid signature (cripto match path).
//!    - **Arm B** — invalid signature with a 1-byte flip near the
//!      front (offset 5).
//!    - **Arm C** — invalid signature with a 1-byte flip near the
//!      back (offset 27).
//! 2. Mann-Whitney U pairwise (3 pairs / trial × 3 trials = 9 tests).
//! 3. **Strict per-test gate**: `p > sidak_per_test_alpha(0.05, 9)`
//!    ≈ 0.005 685 8. ALL 9 tests must pass; combined familywise α =
//!    0.05 via Šidák correction.
//! 4. **Practical-equivalence gate** on every pair's bootstrap 95 % CI
//!    on `|Δ(trimmed_mean)|`:
//!    - `point_estimate ≤ 0.5 ms` AND
//!    - `ci_upper ≤ 0.5 ms` (cripto-grade tighter than the
//!      middleware 1 ms gate).
//!
//!    Per WI-S03-008 ct-variance lesson + WI charter, the central
//!    estimator is the 10/80/10 trimmed mean (NOT arithmetic mean) so
//!    wall-clock contention spikes from workspace-parallel test
//!    pressure don't inflate the apparent leak.
//!
//! ## Release-mode-only gating
//!
//! Debug HKDF + BLAKE3 is roughly 30-100× slower than release; the
//! signal-to-noise ratio drops below the cripto-grade |Δ| ≤ 0.5 ms
//! threshold. The test is gated `#[cfg_attr(debug_assertions, ignore)]`
//! mirroring the WI-S01-001 / WI-S02-004 pattern.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::print_stderr,
    clippy::ptr_arg,
    clippy::needless_range_loop,
    reason = "test code: panics surface as test failures by design; eprintln! emits informational stats to the test log; ptr_arg/needless_range_loop are stylistic"
)]
#![cfg_attr(
    debug_assertions,
    allow(unused_imports, dead_code, reason = "release-gated tests")
)]

use std::sync::Arc;
use std::time::Instant;

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use uuid::Uuid;

use corelink_ac_core::sig::{
    compose_canonical_bytes, HkdfSigner, HkdfVerifier, MockTdkHandle, SignatureSigner,
    SignatureVerifier, TdkHandle, AC_ENVELOPE_SIG_LEN,
};

const SAMPLES_PER_ARM: usize = 10_000;
const TRIALS: usize = 3;
const PER_TEST_TOTAL: usize = TRIALS * 3; // 3 trials × 3 pairs
const ALPHA: f64 = 0.05;
const TRIMMED_MEAN_DIFF_LIMIT_MS: f64 = 0.5;
const BOOTSTRAP_ITERATIONS: usize = 200;
/// Per-sample measurement batch size. The HKDF + BLAKE3 verify path
/// runs in ~2-3 µs per call, well below the timer resolution on
/// virtualized CI runners (~25-100 ns wall-clock granularity is the
/// best case; macOS / Linux containers often coarsen to ~1 µs). We
/// time `BATCH_SIZE` consecutive verify invocations under a single
/// `Instant` window so the per-sample latency is measurable above the
/// noise floor and Mann-Whitney's tie correction is not overwhelmed
/// by clock-resolution-induced ties. Per dudect §III.A the same
/// batching strategy is the canonical fix for constant-time gates on
/// fast operations.
const BATCH_SIZE: usize = 256;

/// 10/80/10 trimmed mean — drop the bottom 10% and top 10% (the
/// outlier-dominated tails), average the central 80%. Per WI-S03-008
/// ct-variance lesson, this is the SOTA central estimator for
/// constant-time gates because it preserves leak-detection sensitivity
/// (a graded CT leak shifts the bulk of the distribution) while
/// rejecting wall-clock contention spikes that inflate arithmetic
/// means under workspace-parallel test pressure.
fn trimmed_mean(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let trim = n / 10; // 10% off each tail = 80% middle.
    let lo = trim;
    let hi = n.saturating_sub(trim);
    if lo >= hi {
        return sorted[n / 2]; // pathological tiny n; fall back to median
    }
    let slice = &sorted[lo..hi];
    let sum: f64 = slice.iter().sum();
    sum / (slice.len() as f64)
}

/// Median (used as a secondary diagnostic).
fn median(samples: &mut Vec<f64>) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    samples[samples.len() / 2]
}

/// Šidák per-test α' = `1 − (1 − α)^(1/k)`. Returns `None` when
/// `k == 0` or `alpha` is outside `[0, 1]`.
fn sidak_per_test_alpha(alpha: f64, k: usize) -> Option<f64> {
    if k == 0 || !(0.0..=1.0).contains(&alpha) {
        return None;
    }
    let one_minus = 1.0 - alpha;
    let inv_k = 1.0 / (k as f64);
    Some(1.0 - one_minus.powf(inv_k))
}

/// Mann-Whitney U two-sided p-value via normal approximation with
/// continuity + tie correction. Mirrors the canonical implementation
/// in `corelink-worker::middleware::timing_padding`.
fn mann_whitney_u_p_value(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let n1 = xs.len();
    let n2 = ys.len();
    if n1 == 0 || n2 == 0 {
        return None;
    }
    let big_n = n1 + n2;
    let mut pooled: Vec<(f64, bool)> = Vec::with_capacity(big_n);
    pooled.extend(xs.iter().map(|&v| (v, true)));
    pooled.extend(ys.iter().map(|&v| (v, false)));
    pooled.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut rank_sum_x: f64 = 0.0;
    let mut tie_correction_sum: f64 = 0.0; // Σ(t³ − t)
    let mut idx: usize = 0;
    while idx < big_n {
        let mut run_end = idx + 1;
        let val = pooled[idx].0;
        while run_end < big_n && (pooled[run_end].0 - val).abs() <= f64::EPSILON {
            run_end += 1;
        }
        let t = run_end - idx;
        let mid_rank = (idx as f64) + ((t as f64) + 1.0) / 2.0;
        for k in idx..run_end {
            if pooled[k].1 {
                rank_sum_x += mid_rank;
            }
        }
        if t > 1 {
            let t_f = t as f64;
            tie_correction_sum += t_f * t_f * t_f - t_f;
        }
        idx = run_end;
    }
    let n1_f = n1 as f64;
    let n2_f = n2 as f64;
    let big_n_f = big_n as f64;
    let u1 = rank_sum_x - n1_f * (n1_f + 1.0) / 2.0;
    let u2 = n1_f * n2_f - u1;
    let u = if u1 < u2 { u1 } else { u2 };
    let mu_u = n1_f * n2_f / 2.0;
    let big_n_minus_one = big_n_f - 1.0;
    let tie_term = if big_n_minus_one <= 0.0 {
        0.0
    } else {
        tie_correction_sum / (big_n_f * big_n_minus_one)
    };
    let sigma_sq = (n1_f * n2_f / 12.0) * ((big_n_f + 1.0) - tie_term);
    if sigma_sq <= 0.0 {
        return Some(1.0);
    }
    let sigma = sigma_sq.sqrt();
    let raw_diff = (u - mu_u).abs();
    let corrected = (raw_diff - 0.5).max(0.0);
    let z = corrected / sigma;
    let p_two_sided = erfc(z / std::f64::consts::SQRT_2);
    Some(p_two_sided.clamp(0.0, 1.0))
}

/// Standard erfc via Abramowitz & Stegun §7.1.26 (max relative error
/// ≈ 1.5e-7; ample for the indistinguishability gate).
fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let sign = x.signum();
    let abs_x = x.abs();
    let a1 = 0.254_829_592;
    let a2 = -0.284_496_736;
    let a3 = 1.421_413_741;
    let a4 = -1.453_152_027;
    let a5 = 1.061_405_429;
    let p_factor = 0.327_591_1;
    let t = 1.0 / (1.0 + p_factor * abs_x);
    let poly = ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t;
    let erf_pos = 1.0 - poly * (-abs_x * abs_x).exp();
    let erf = sign * erf_pos;
    1.0 - erf
}

#[derive(Clone, Copy, Debug)]
struct BootstrapTrimmedCi {
    point_estimate: f64,
    ci_lower: f64,
    ci_upper: f64,
}

/// Bootstrap 95% CI on `|Δ(trimmed_mean)|` (10/80/10).
fn bootstrap_trimmed_mean_ci(
    xs: &[f64],
    ys: &[f64],
    iterations: usize,
    seed: u64,
) -> Option<BootstrapTrimmedCi> {
    use rand::Rng as _;
    if xs.is_empty() || ys.is_empty() || iterations == 0 {
        return None;
    }
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let mut diffs = Vec::with_capacity(iterations);
    let mut x_buf = vec![0.0f64; xs.len()];
    let mut y_buf = vec![0.0f64; ys.len()];
    for _ in 0..iterations {
        for slot in x_buf.iter_mut() {
            let idx = rng.random_range(0..xs.len());
            *slot = xs[idx];
        }
        for slot in y_buf.iter_mut() {
            let idx = rng.random_range(0..ys.len());
            *slot = ys[idx];
        }
        let mx = trimmed_mean(&x_buf);
        let my = trimmed_mean(&y_buf);
        diffs.push((mx - my).abs());
    }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lower_idx = ((iterations as f64) * 0.025) as usize;
    let upper_idx = (((iterations as f64) * 0.975) as usize).min(iterations - 1);
    let point_estimate = (trimmed_mean(xs) - trimmed_mean(ys)).abs();
    Some(BootstrapTrimmedCi {
        point_estimate,
        ci_lower: diffs[lower_idx],
        ci_upper: diffs[upper_idx],
    })
}

#[derive(Clone, Copy)]
enum Arm {
    Valid,
    InvalidFront,
    InvalidBack,
}

fn fixed_tenant() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
}

/// Build a fresh signer + verifier pair using the canonical mock TDK.
fn build_pair() -> (HkdfSigner, HkdfVerifier) {
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(fixed_tenant(), 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let signer = HkdfSigner::new(Arc::clone(&handle), 1).unwrap();
    let verifier = HkdfVerifier::new(handle, vec![1]).unwrap();
    (signer, verifier)
}

fn arm_signature(
    valid_sig: &[u8; AC_ENVELOPE_SIG_LEN],
    arm: Arm,
    iter: usize,
) -> [u8; AC_ENVELOPE_SIG_LEN] {
    let mut sig = *valid_sig;
    match arm {
        Arm::Valid => {} // sig unchanged
        Arm::InvalidFront => {
            // Flip a single bit at offset 5; varies per iter so
            // the verifier's branch-prediction can't lock onto a
            // single tampered shape.
            sig[5] ^= 1u8 << ((iter as u8) & 0x07);
        }
        Arm::InvalidBack => {
            sig[27] ^= 1u8 << ((iter as u8) & 0x07);
        }
    }
    sig
}

/// Run all three arms with interleaved sample collection.
///
/// Why interleaved: running each arm sequentially induces a systemic
/// monotonic bias in CPU frequency scaling / cache state that causes
/// Mann-Whitney to detect a difference between arms purely from
/// ordering noise (e.g. arm 0 runs in the cold cache window, arm 2 in
/// the warm one). Interleaving by sample-index spreads the cache
/// state evenly across all three arms so any residual cache-warming
/// effect is shared and therefore cancels out under the null
/// hypothesis. This is the canonical pattern from the dudect
/// constant-time validation methodology.
fn one_trial(samples: usize) -> [Vec<f64>; 3] {
    let (signer, verifier) = build_pair();
    let action_hash = [0xAA; 32];
    let result_hash = [0xBB; 32];
    let canonical_bytes =
        compose_canonical_bytes(1, 1, fixed_tenant(), &action_hash, 1234, &result_hash).unwrap();
    let valid_sig: [u8; AC_ENVELOPE_SIG_LEN] =
        signer.sign(fixed_tenant(), 1, &canonical_bytes).unwrap();

    // Warm-up — 1024 invocations interleaved across all three arms to
    // amortize first-call cache effects uniformly.
    for i in 0..1024 {
        let arm = match i % 3 {
            0 => Arm::Valid,
            1 => Arm::InvalidFront,
            _ => Arm::InvalidBack,
        };
        let sig = arm_signature(&valid_sig, arm, i);
        let _ = verifier.verify(fixed_tenant(), 1, &canonical_bytes, &sig);
    }

    let mut a_valid = Vec::with_capacity(samples);
    let mut a_front = Vec::with_capacity(samples);
    let mut a_back = Vec::with_capacity(samples);
    // Interleave: for each i, run arms in a rotated order so neither
    // arm 0 nor arm 2 systematically gets the cold-cache or hot-cache
    // slot. Each per-arm sample is the AVERAGE of `BATCH_SIZE`
    // consecutive verify calls measured under a single `Instant`
    // window — this raises the per-sample latency well above the
    // timer resolution so Mann-Whitney's tie correction has the
    // dynamic range it needs (per dudect §III.A canonical batching).
    for i in 0..samples {
        let order = match i % 3 {
            0 => [Arm::Valid, Arm::InvalidFront, Arm::InvalidBack],
            1 => [Arm::InvalidFront, Arm::InvalidBack, Arm::Valid],
            _ => [Arm::InvalidBack, Arm::Valid, Arm::InvalidFront],
        };
        for arm in order {
            let sig = arm_signature(&valid_sig, arm, i);
            let start = Instant::now();
            for _ in 0..BATCH_SIZE {
                let res = verifier.verify(
                    std::hint::black_box(fixed_tenant()),
                    std::hint::black_box(1),
                    std::hint::black_box(&canonical_bytes),
                    std::hint::black_box(&sig),
                );
                std::hint::black_box(res.ok());
            }
            let elapsed = start.elapsed();
            // Per-call wall-clock in milliseconds, divided by the
            // batch size so the per-sample value is comparable to the
            // canonical |Δmedian| ≤ 0.5 ms threshold without further
            // scaling.
            let per_call_ms = (elapsed.as_secs_f64() * 1000.0) / (BATCH_SIZE as f64);
            match arm {
                Arm::Valid => a_valid.push(per_call_ms),
                Arm::InvalidFront => a_front.push(per_call_ms),
                Arm::InvalidBack => a_back.push(per_call_ms),
            }
        }
    }
    [a_valid, a_front, a_back]
}

#[derive(Clone, Copy)]
struct PairOutcome {
    p_value: f64,
    ci: BootstrapTrimmedCi,
}

fn run_gate(trials: &[[Vec<f64>; 3]], bootstrap_seed_offset: u64) -> Vec<(usize, usize, PairOutcome)> {
    let mut out = Vec::with_capacity(PER_TEST_TOTAL);
    for (trial_idx, arms) in trials.iter().enumerate() {
        let pairs: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];
        for (i, (a, b)) in pairs.iter().copied().enumerate() {
            let p = mann_whitney_u_p_value(&arms[a], &arms[b]).expect("non-empty arms");
            let ci = bootstrap_trimmed_mean_ci(
                &arms[a],
                &arms[b],
                BOOTSTRAP_ITERATIONS,
                bootstrap_seed_offset
                    .wrapping_add((trial_idx as u64).wrapping_mul(7))
                    .wrapping_add(i as u64),
            )
            .expect("non-empty arms");
            out.push((trial_idx, i, PairOutcome { p_value: p, ci }));
        }
    }
    out
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn three_arm_indistinguishability_cripto_grade() {
    let alpha_prime = sidak_per_test_alpha(ALPHA, PER_TEST_TOTAL).unwrap();
    eprintln!(
        "Šidák per-test α' for k={PER_TEST_TOTAL}: {alpha_prime:.6} (the strict gate)"
    );

    let mut all_arms = Vec::with_capacity(TRIALS);
    for trial in 0..TRIALS {
        let arms = one_trial(SAMPLES_PER_ARM);
        // Diagnostic: per-arm trimmed mean.
        for (i, arm_samples) in arms.iter().enumerate() {
            let tm = trimmed_mean(arm_samples);
            let mut s = arm_samples.clone();
            let med = median(&mut s);
            eprintln!(
                "trial {trial} arm {i}: trimmed_mean = {tm:.4} ms; median = {med:.4} ms; n = {}",
                arm_samples.len()
            );
        }
        all_arms.push(arms);
    }

    let outcomes = run_gate(&all_arms, 0xC1A0_BEEF);

    // ---- Strict gate: p > α' for ALL 9 tests. ----
    // ---- Practical gate: |Δtrimmed_mean| upper CI ≤ 0.5 ms. ----
    let mut strict_failures = Vec::new();
    let mut practical_failures = Vec::new();
    for (trial, pair, o) in &outcomes {
        eprintln!(
            "trial {trial} pair {pair} p = {:.6}; |Δtrimmed_mean| point = {:.4} ms; CI = [{:.4}, {:.4}] ms",
            o.p_value, o.ci.point_estimate, o.ci.ci_lower, o.ci.ci_upper
        );
        if o.p_value <= alpha_prime {
            strict_failures.push((*trial, *pair, o.p_value));
        }
        if o.ci.point_estimate > TRIMMED_MEAN_DIFF_LIMIT_MS
            || o.ci.ci_upper > TRIMMED_MEAN_DIFF_LIMIT_MS
        {
            practical_failures.push((*trial, *pair, o.ci));
        }
    }

    assert!(
        strict_failures.is_empty(),
        "{} of 9 Mann-Whitney pair-tests rejected at α' = {alpha_prime:.6}; \
         distributions are NOT statistically indistinguishable; failures = {:?}",
        strict_failures.len(),
        strict_failures
    );
    assert!(
        practical_failures.is_empty(),
        "{} of 9 pair bootstrap CIs (10/80/10 trimmed mean) exceeded {TRIMMED_MEAN_DIFF_LIMIT_MS} ms; \
         distributions are NOT practically equivalent at cripto grade; failures = {:?}",
        practical_failures.len(),
        practical_failures
    );
}
