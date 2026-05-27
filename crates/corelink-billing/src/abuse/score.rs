//! Canonical [`AbuseScore`] newtype + 4-feature weighted-sum scoring
//! formula + decision-arm threshold mapping.
//!
//! ## Score formula (sprint contract §5 R-S08-7 + WI §6.1.3)
//!
//! Each raw feature is normalised to `[0.0, 1.0]` via tier-specific
//! baselines, then combined via the canonical weighted sum:
//!
//! ```text
//! cpu_norm     = clamp((cpu_wallclock_ratio - 0.5) * 2.0, 0.0, 1.0)
//!                 # 0.0 when ratio ≤ 0.5; 1.0 when ratio ≥ 1.0.
//!
//! egress_norm  = clamp((egress_bytes_per_min / egress_baseline - 1.0)
//!                       / 10.0, 0.0, 1.0)
//!                 # 0.0 at ≤ baseline; 1.0 at ≥ 11× baseline.
//!
//! entropy_norm = clamp((3.0 - entropy_bits) / 3.0, 0.0, 1.0)
//!                 # 0.0 when entropy ≥ 3 bits (≥ 8 unique actions);
//!                 # 1.0 when entropy = 0 (single action; spam canary).
//!
//! exec_norm    = clamp((concurrent_exec_count / exec_baseline - 1.0)
//!                       / 100.0, 0.0, 1.0)
//!                 # 0.0 at ≤ baseline; 1.0 at ≥ 101× baseline.
//!
//! score = w_cpu * cpu_norm
//!       + w_egress * egress_norm
//!       + w_entropy * entropy_norm
//!       + w_exec * exec_norm
//! ```
//!
//! With canonical weights `(0.30, 0.25, 0.25, 0.20)` summing to 1.0,
//! the resulting score is structurally bounded in `[0.0, 1.0]` per
//! WI §1 invariant 5.
//!
//! ## Decision thresholds (WI §1 invariant 3)
//!
//! - score < SUSPICIOUS_THRESHOLD (0.5) → [`AbuseDecision::Benign`].
//! - SUSPICIOUS_THRESHOLD ≤ score < MALICIOUS_THRESHOLD (0.8) →
//!   [`AbuseDecision::Suspicious`].
//! - score ≥ MALICIOUS_THRESHOLD → [`AbuseDecision::Malicious`].
//!
//! ## Why monotonic in each feature
//!
//! Each `*_norm` is non-decreasing in its input feature (when other
//! features held constant); each weight is non-negative; therefore the
//! score is monotonic in each feature. Pinned by
//! `prop_score_monotonic_in_features`.

use corelink_eviction::Tier;

use super::config::{
    egress_baseline_for_tier, exec_baseline_for_tier, AbuseConfig,
    AbuseFeatureWeights,
};
use super::features::AbuseFeatures;

/// Canonical clamped score newtype. Domain `[0.0, 1.0]`; constructor
/// clamps NaN to 0.0 (defensive; the score formula structurally cannot
/// produce NaN given canonical weights + sanitised features but we
/// guard against admin-tunable weights drifting).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct AbuseScore(f64);

impl AbuseScore {
    /// Canonical zero score (Benign baseline).
    pub const ZERO: Self = Self(0.0);

    /// Canonical one score (saturated upper bound).
    pub const ONE: Self = Self(1.0);

    /// Construct + clamp to `[0.0, 1.0]`. NaN coerces to 0.0.
    #[must_use]
    pub fn clamp(v: f64) -> Self {
        if v.is_nan() {
            return Self(0.0);
        }
        Self(v.clamp(0.0, 1.0))
    }

    /// Inner f64.
    #[must_use]
    pub const fn as_f64(self) -> f64 {
        self.0
    }
}

impl core::fmt::Display for AbuseScore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:.3}", self.0)
    }
}

/// Canonical 3-arm gradient response decision. The full 4-tier
/// production ladder (SilentDowngrade50pct1h /
/// AdminReviewTriggerSev2 / SuspendCandidateHumanReviewOnly) is
/// emitted via the audit + metrics layer; this enum is the
/// `#[non_exhaustive]` orchestrator-level decision shape that
/// production wiring at WI-S08-006 expands additively.
///
/// Per LGPD Art. 20 + GDPR Art. 22 humane response (sprint contract
/// §7.10.s08.3), the `Malicious` arm flags admin review BUT NEVER
/// auto-suspends; suspend execution requires explicit admin manual
/// action (programmatic suspend attempts hit
/// [`super::error::AbuseError::AutoSuspendForbidden`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AbuseDecision {
    /// score < SUSPICIOUS_THRESHOLD; tier Noop.
    Benign,
    /// SUSPICIOUS_THRESHOLD ≤ score < MALICIOUS_THRESHOLD; silent
    /// downgrade applied (auto-recoverable; reduces refill rate via
    /// `tenant_rate_override` per WI §6.1.4 CI-2).
    Suspicious,
    /// score ≥ MALICIOUS_THRESHOLD; admin review trigger SEV-2;
    /// suspend candidate sub-tier kicks in at ≥ 0.95 but suspend
    /// execution NEVER auto-applies per LGPD Art. 20.
    Malicious,
}

impl AbuseDecision {
    /// Canonical lower-snake-case mnemonic for storage / metric labels.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Benign => "Benign",
            Self::Suspicious => "Suspicious",
            Self::Malicious => "Malicious",
        }
    }
}

impl core::fmt::Display for AbuseDecision {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve [`AbuseDecision`] from a raw score via the canonical
/// thresholds carried on [`AbuseConfig`]. Boundary semantics: ≥ on
/// each threshold (score exactly at SUSPICIOUS_THRESHOLD → Suspicious;
/// score exactly at MALICIOUS_THRESHOLD → Malicious).
#[must_use]
pub fn decide(score: AbuseScore, config: &AbuseConfig) -> AbuseDecision {
    let s = score.as_f64();
    if s >= config.malicious_threshold() {
        AbuseDecision::Malicious
    } else if s >= config.suspicious_threshold() {
        AbuseDecision::Suspicious
    } else {
        AbuseDecision::Benign
    }
}

/// Compute the canonical 4-feature weighted-sum abuse score for a
/// tenant given features, weights, and tier baselines. Result clamped
/// to `[0.0, 1.0]`.
///
/// Per the canonical weights (0.30 cpu / 0.25 egress / 0.25 entropy /
/// 0.20 exec) summing to 1.0 with each `*_norm` ∈ `[0.0, 1.0]`, the
/// score is structurally in `[0.0, 1.0]`. The `AbuseScore::clamp`
/// envelope is a defense-in-depth guard against admin-tunable weights
/// drifting outside canonical bounds.
#[must_use]
pub fn compute_score(
    features: &AbuseFeatures,
    weights: &AbuseFeatureWeights,
    tier: Tier,
) -> AbuseScore {
    let cpu_norm = normalise_cpu(features.cpu_wallclock_ratio);
    let egress_norm =
        normalise_egress(features.egress_bytes_per_min, tier);
    let entropy_norm =
        normalise_entropy(features.action_digest_entropy_bits);
    let exec_norm = normalise_exec(features.concurrent_exec_count, tier);

    let raw = weights.cpu() * cpu_norm
        + weights.egress() * egress_norm
        + weights.entropy() * entropy_norm
        + weights.exec() * exec_norm;
    AbuseScore::clamp(raw)
}

fn clamp_unit(v: f64) -> f64 {
    if v.is_nan() {
        return 0.0;
    }
    v.clamp(0.0, 1.0)
}

/// Normalise cpu_wallclock_ratio: 0.0 if ratio ≤ 0.5; linear ramp to
/// 1.0 at ratio = 1.0; saturated 1.0 above.
fn normalise_cpu(ratio: f64) -> f64 {
    clamp_unit((ratio - 0.5) * 2.0)
}

/// Normalise egress_bytes_per_min against tier baseline: 0.0 at ≤
/// baseline; linear to 1.0 at 11× baseline; saturated above.
fn normalise_egress(egress: u64, tier: Tier) -> f64 {
    let baseline = egress_baseline_for_tier(tier);
    if baseline == 0 {
        // Defensive — canonical baselines are non-zero, but guard.
        return 0.0;
    }
    let ratio = (egress as f64) / (baseline as f64);
    clamp_unit((ratio - 1.0) / 10.0)
}

/// Normalise action_digest_entropy_bits: 1.0 at 0 bits (single
/// action; spam canary); linear ramp to 0.0 at 3 bits (≥ 8 distinct
/// actions = legitimate workload).
fn normalise_entropy(entropy_bits: f64) -> f64 {
    clamp_unit((3.0 - entropy_bits) / 3.0)
}

/// Normalise concurrent_exec_count against tier baseline: 0.0 at ≤
/// baseline; linear to 1.0 at 101× baseline; saturated above.
fn normalise_exec(exec: u32, tier: Tier) -> f64 {
    let baseline = exec_baseline_for_tier(tier);
    if baseline == 0 {
        return 0.0;
    }
    let ratio = f64::from(exec) / f64::from(baseline);
    clamp_unit((ratio - 1.0) / 100.0)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn score_zero_features_saturates_entropy_only() {
        // Zero features = "no observations": cpu=0 (norm 0) + egress=0
        // (norm 0) + entropy=0 (norm 1.0; single-action spam canary by
        // Shannon convention) + exec=0 (norm 0). With canonical weights
        // 0.30/0.25/0.25/0.20, score = 0.25 (entropy_weight × 1.0).
        // The Shannon entropy convention treats 0-bit as the maximum
        // spam signal; production wiring's S-09 reader will never emit
        // exactly 0 bits unless the tenant truly served only one
        // action_digest in the window — which IS a spam signal.
        let f = AbuseFeatures::zero();
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Team);
        assert!((s.as_f64() - 0.25).abs() < 0.001);
    }

    #[test]
    fn score_typical_workload_with_high_entropy_is_zero() {
        // True baseline = legit workload: 8+ distinct actions
        // (entropy ≥ 3 bits) drives entropy_norm → 0.
        let f = AbuseFeatures::new(0.0, 0, 5.0, 0);
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Team);
        assert_eq!(s.as_f64(), 0.0);
    }

    #[test]
    fn score_max_features_saturates_at_one() {
        let f = AbuseFeatures::new(2.0, u64::MAX / 2, 0.0, u32::MAX);
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Free);
        assert_eq!(s.as_f64(), 1.0);
    }

    #[test]
    fn score_compute_farming_only_is_about_30pct() {
        // cpu = 1.0 (saturated cpu_norm); other features zero.
        let f = AbuseFeatures::new(1.0, 0, 5.0, 0);
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Team);
        // Score should be ≈ 0.30 (cpu weight × 1.0).
        assert!((s.as_f64() - 0.30).abs() < 0.001);
    }

    #[test]
    fn score_pure_spam_only_is_about_25pct() {
        // entropy = 0.0 (1 action; spam saturated); other features
        // zero or above-noise.
        let f = AbuseFeatures::new(0.4, 0, 0.0, 0);
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Team);
        // entropy_norm = 1.0; cpu_norm = 0.0 (ratio 0.4 ≤ 0.5).
        // score = 0.25 * 1.0 = 0.25.
        assert!((s.as_f64() - 0.25).abs() < 0.001);
    }

    #[test]
    fn score_high_intensity_combo_lands_above_malicious() {
        // cpu=0.99 + entropy=0.3 + exec=80×baseline + egress=20×baseline
        // mirrors WI Persona 4.
        let f = AbuseFeatures::new(0.99, 200_000_000_000, 0.3, 4_000);
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Team);
        // Expected: cpu_norm ≈ 0.98; egress_norm ≈ 1.0
        // (200B / 50M = 4000 → way > 11x); entropy_norm = 0.9; exec_norm
        // = (4000/50 - 1) / 100 = 79/100 → clamped to 0.79.
        // score ≈ 0.30*0.98 + 0.25*1.0 + 0.25*0.9 + 0.20*0.79
        //       ≈ 0.294 + 0.25 + 0.225 + 0.158 = 0.927.
        assert!(s.as_f64() >= 0.8, "expected Malicious tier; got {}", s.as_f64());
    }

    #[test]
    fn score_typical_bazel_lands_benign() {
        // cpu=0.3, egress=10MB/min on Team baseline 50MB/min, entropy=4
        // (16 actions), exec=20 (within baseline 50). Mirror WI Persona 1.
        let f = AbuseFeatures::new(0.3, 10_000_000, 4.0, 20);
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Team);
        assert!(s.as_f64() < 0.5, "expected Benign; got {}", s.as_f64());
    }

    #[test]
    fn score_heavy_ml_training_not_false_positive() {
        // ML training: egress=5GB/min on Team 50MB; cpu=0.4, entropy=3.5,
        // exec=10. Mirror WI Persona 2.
        let f = AbuseFeatures::new(0.4, 5_000_000_000, 3.5, 10);
        let w = AbuseFeatureWeights::canonical();
        let s = compute_score(&f, &w, Tier::Team);
        // egress_norm = (100 - 1) / 10 → clamped 1.0; others ≈ 0.
        // score ≈ 0.25 → still Benign (< 0.5).
        assert!(s.as_f64() < 0.5, "expected Benign; got {}", s.as_f64());
    }

    #[test]
    fn decide_exact_threshold_boundary_suspicious() {
        let cfg = AbuseConfig::canonical();
        let s = AbuseScore::clamp(0.5);
        // Exactly at SUSPICIOUS → Suspicious arm.
        assert_eq!(decide(s, &cfg), AbuseDecision::Suspicious);
    }

    #[test]
    fn decide_just_below_suspicious_is_benign() {
        let cfg = AbuseConfig::canonical();
        let s = AbuseScore::clamp(0.4999);
        assert_eq!(decide(s, &cfg), AbuseDecision::Benign);
    }

    #[test]
    fn decide_exact_threshold_boundary_malicious() {
        let cfg = AbuseConfig::canonical();
        let s = AbuseScore::clamp(0.8);
        assert_eq!(decide(s, &cfg), AbuseDecision::Malicious);
    }

    #[test]
    fn decide_just_below_malicious_is_suspicious() {
        let cfg = AbuseConfig::canonical();
        let s = AbuseScore::clamp(0.7999);
        assert_eq!(decide(s, &cfg), AbuseDecision::Suspicious);
    }

    #[test]
    fn decide_zero_is_benign() {
        let cfg = AbuseConfig::canonical();
        assert_eq!(decide(AbuseScore::ZERO, &cfg), AbuseDecision::Benign);
    }

    #[test]
    fn decide_one_is_malicious() {
        let cfg = AbuseConfig::canonical();
        assert_eq!(decide(AbuseScore::ONE, &cfg), AbuseDecision::Malicious);
    }

    #[test]
    fn score_clamp_nan_is_zero() {
        let s = AbuseScore::clamp(f64::NAN);
        assert_eq!(s.as_f64(), 0.0);
    }

    #[test]
    fn score_clamp_above_one_is_one() {
        let s = AbuseScore::clamp(1.5);
        assert_eq!(s.as_f64(), 1.0);
    }

    #[test]
    fn score_clamp_negative_is_zero() {
        let s = AbuseScore::clamp(-0.5);
        assert_eq!(s.as_f64(), 0.0);
    }

    #[test]
    fn score_normalise_cpu_at_boundary() {
        assert_eq!(normalise_cpu(0.5), 0.0);
        assert_eq!(normalise_cpu(1.0), 1.0);
        assert_eq!(normalise_cpu(0.75), 0.5);
        assert_eq!(normalise_cpu(2.0), 1.0); // saturated.
        assert_eq!(normalise_cpu(0.0), 0.0); // floor.
    }

    #[test]
    fn score_normalise_entropy_at_boundary() {
        assert_eq!(normalise_entropy(0.0), 1.0); // single action; spam.
        assert_eq!(normalise_entropy(3.0), 0.0); // 8 distinct actions; legit.
        assert_eq!(normalise_entropy(1.5), 0.5);
        assert_eq!(normalise_entropy(10.0), 0.0); // many actions; legit.
    }

    #[test]
    fn score_per_tier_egress_normalise() {
        // 10 MB/min on Free baseline 1 MB → ratio 10 → norm = (10-1)/10 = 0.9.
        let n = normalise_egress(10_000_000, Tier::Free);
        assert!((n - 0.9).abs() < 0.001);
        // Same egress on Enterprise baseline 5 GB → tiny ratio → 0.0.
        let n = normalise_egress(10_000_000, Tier::Enterprise);
        assert_eq!(n, 0.0);
    }

    #[test]
    fn score_display_formats_three_decimal() {
        let s = AbuseScore::clamp(0.123456);
        assert_eq!(format!("{s}"), "0.123");
    }

    #[test]
    fn decision_display_matches_as_str() {
        assert_eq!(format!("{}", AbuseDecision::Benign), "Benign");
        assert_eq!(format!("{}", AbuseDecision::Suspicious), "Suspicious");
        assert_eq!(format!("{}", AbuseDecision::Malicious), "Malicious");
    }
}
