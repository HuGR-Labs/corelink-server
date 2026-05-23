//! Statistical primitives — Mann-Whitney U + Šidák correction +
//! bootstrap CI + median helper + erfc.
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3).

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

/// Two-sided Mann-Whitney U p-value via the canonical normal
/// approximation with tie correction (Mann & Whitney 1947;
/// Hollander & Wolfe 1973 §4.1 ties).
///
/// Returns `None` when `xs` or `ys` is empty (undefined p-value).
///
/// The implementation is intentionally textbook + auditable:
///
/// 1. Concatenate `xs` and `ys`, sort by value, assign **mid-ranks**
///    to ties (canonical Wilcoxon rank-sum convention).
/// 2. Sum the ranks of the `xs` group → `R1`.
/// 3. `U1 = R1 − n1·(n1+1)/2`; `U = min(U1, n1·n2 − U1)`.
/// 4. `μ_U = n1·n2 / 2`; tie-corrected
///    `σ_U² = (n1·n2/12) · ((N+1) − Σ(t³−t)/(N·(N−1)))`.
/// 5. `z = (|U − μ_U| − 0.5) / σ_U` (continuity correction; sign
///    chosen so the two-sided p stays symmetric in the no-effect
///    limit).
/// 6. `p = 2 · (1 − Φ(|z|))` via `erfc` for the right-tail tail prob.
///
/// Returns the **two-sided** p-value (null hypothesis: distributions
/// are identical; large p ⇒ fail to reject ⇒ indistinguishable, which
/// is the desired outcome for the timing-padding test).
#[must_use]
pub fn mann_whitney_u_p_value(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let n1 = xs.len();
    let n2 = ys.len();
    if n1 == 0 || n2 == 0 {
        return None;
    }
    let big_n = n1 + n2;

    let mut pooled: Vec<(f64, bool)> = Vec::with_capacity(big_n);
    pooled.extend(xs.iter().map(|&v| (v, true)));
    pooled.extend(ys.iter().map(|&v| (v, false)));
    pooled.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

    let mut rank_sum_x: f64 = 0.0;
    let mut tie_correction_sum: f64 = 0.0; // Σ(t³ − t)
    let mut idx: usize = 0;
    while idx < big_n {
        let mut run_end = idx + 1;
        let val = match pooled.get(idx) {
            Some((v, _)) => *v,
            None => break,
        };
        while run_end < big_n {
            let next_val = match pooled.get(run_end) {
                Some((v, _)) => *v,
                None => break,
            };
            if (next_val - val).abs() <= f64::EPSILON {
                run_end += 1;
            } else {
                break;
            }
        }
        let t = run_end - idx;
        // Mid-rank: ranks are 1-based; mid-rank for a tie of `t` items
        // at positions [idx+1 .. idx+t] is `idx + (t+1)/2`.
        let mid_rank = (idx as f64) + ((t as f64) + 1.0) / 2.0;
        for k in idx..run_end {
            if let Some((_, is_x)) = pooled.get(k) {
                if *is_x {
                    rank_sum_x += mid_rank;
                }
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
    let p_two_sided = erfc(z / core::f64::consts::SQRT_2);
    Some(p_two_sided.clamp(0.0, 1.0))
}

/// Standard erfc via Abramowitz & Stegun §7.1.26 + §7.1.28 (max
/// relative error ≈ 1.5e-7; ample for the indistinguishability gate
/// which only needs three significant digits at the α threshold).
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

/// Šidák correction per-test α' for `k` independent tests with
/// combined familywise α = `alpha`.
///
/// `α' = 1 − (1 − α)^(1/k)`.
///
/// Returns `None` when `k == 0` (vacuous family) or `alpha` is outside
/// `[0, 1]` (mis-call).
#[must_use]
pub fn sidak_per_test_alpha(alpha: f64, k: usize) -> Option<f64> {
    if k == 0 || !(0.0..=1.0).contains(&alpha) {
        return None;
    }
    let one_minus = 1.0 - alpha;
    let inv_k = 1.0 / (k as f64);
    Some(1.0 - one_minus.powf(inv_k))
}

/// Bootstrap 95 % CI on the absolute median difference `|median(xs) - median(ys)|`.
///
/// Returns `None` if either sample is empty. The CI is derived by
/// resampling each input independently `iterations` times via the
/// supplied `seed`-driven `ChaCha20Rng`; the 2.5 / 97.5 percentiles
/// of the resulting `|median diff|` distribution form the CI.
///
/// Interpretation: the test passes the WI-S02-004 §10.4.1 gate iff
/// **both** `point_estimate ≤ 1 ms` AND `ci_upper ≤ 1 ms` (codex
/// round-1 P1 fix — the upper bound is the load-bearing
/// practical-equivalence claim; `ci_lower` of `|·|` is trivially
/// `≥ 0` and was redundant in earlier drafts).
#[must_use]
pub fn bootstrap_median_ci(
    xs: &[f64],
    ys: &[f64],
    iterations: usize,
    seed: u64,
) -> Option<BootstrapMedianCi> {
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
            *slot = match xs.get(idx) {
                Some(v) => *v,
                None => 0.0,
            };
        }
        for slot in y_buf.iter_mut() {
            let idx = rng.random_range(0..ys.len());
            *slot = match ys.get(idx) {
                Some(v) => *v,
                None => 0.0,
            };
        }
        let mx = median(&mut x_buf.clone());
        let my = median(&mut y_buf.clone());
        diffs.push((mx - my).abs());
    }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let lower_idx = ((iterations as f64) * 0.025) as usize;
    let upper_idx = (((iterations as f64) * 0.975) as usize).min(iterations - 1);
    let median_x = median(&mut xs.to_vec());
    let median_y = median(&mut ys.to_vec());
    let point_estimate = (median_x - median_y).abs();
    Some(BootstrapMedianCi {
        point_estimate,
        ci_lower: diffs.get(lower_idx).copied().unwrap_or(0.0),
        ci_upper: diffs.get(upper_idx).copied().unwrap_or(0.0),
    })
}

/// Bootstrap 95 % CI result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BootstrapMedianCi {
    /// |Δmedian| point estimate from the original samples.
    pub point_estimate: f64,
    /// 2.5-percentile of the resampled |Δmedian| distribution.
    pub ci_lower: f64,
    /// 97.5-percentile of the resampled |Δmedian| distribution.
    pub ci_upper: f64,
}

pub(super) fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values.get(mid).copied().unwrap_or(0.0)
    } else {
        let lo = values.get(mid - 1).copied().unwrap_or(0.0);
        let hi = values.get(mid).copied().unwrap_or(0.0);
        (lo + hi) / 2.0
    }
}
