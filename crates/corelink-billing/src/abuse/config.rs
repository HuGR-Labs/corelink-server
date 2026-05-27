//! Canonical [`AbuseConfig`] knobs — thresholds, feature weights, and
//! tier-aware baselines.
//!
//! ## F-001 closure
//!
//! Every knob lives on a per-instance struct (NO process-global static).
//! Tests instantiate fresh configs per case so the harness cannot leak
//! state across cases.
//!
//! ## Canonical weights (sprint contract §5 R-S08-7 + WI §6.1.3)
//!
//! Initial weights from sprint contract benchmarks; tunable via admin
//! S-13 ADR future (post-staging calibration). All weights ∈ [0.0, 1.0]
//! and sum to 1.0:
//!
//! - `cpu`: 0.30 — compute farming detection (highest weight; sustained
//!   CPU near wallclock indicates botnet/cryptominer using CoreLink as
//!   compute substrate).
//! - `egress`: 0.25 — scraping detection.
//! - `entropy`: 0.25 — spam detection (Shannon entropy of action_digest
//!   set).
//! - `exec`: 0.20 — exec flood detection.
//!
//! ## Canonical thresholds (WI §1 invariant 3 + §6.1.4)
//!
//! - SUSPICIOUS_THRESHOLD = 0.5: silent downgrade applied
//!   (auto-recoverable).
//! - MALICIOUS_THRESHOLD = 0.8: admin review trigger (SEV-2);
//!   suspend candidate at ≥ 0.95 NEVER auto-applied (LGPD Art. 20).
//!
//! ## Tier-aware baselines (5-tier canonical Lote 10.7bis P0-7)
//!
//! Egress + exec features normalise against tier-specific baselines so
//! a Free-tier tenant doing 100 RPS doesn't get the same score as an
//! Enterprise tenant doing 100 RPS.

use corelink_eviction::Tier;

/// Canonical CPU feature weight (compute farming detection; highest).
pub const DEFAULT_CPU_WEIGHT: f64 = 0.30;
/// Canonical egress feature weight (scraping detection).
pub const DEFAULT_EGRESS_WEIGHT: f64 = 0.25;
/// Canonical entropy feature weight (spam detection).
pub const DEFAULT_ENTROPY_WEIGHT: f64 = 0.25;
/// Canonical exec feature weight (flood detection).
pub const DEFAULT_EXEC_WEIGHT: f64 = 0.20;

/// Canonical SUSPICIOUS threshold — score ≥ this triggers silent
/// downgrade (auto-recoverable; reduces refill rate via
/// `tenant_rate_override` mechanism per WI §6.1.4 CI-2).
pub const SUSPICIOUS_THRESHOLD: f64 = 0.5;

/// Canonical MALICIOUS threshold — score ≥ this triggers admin review
/// SEV-2 PagerDuty notification. The SuspendCandidate sub-tier kicks
/// in at ≥ 0.95 but suspend execution NEVER auto-applies per LGPD
/// Art. 20 + GDPR Art. 22 humane response (sprint contract §7.10.s08.3).
pub const MALICIOUS_THRESHOLD: f64 = 0.8;

/// Canonical 4-feature weights (sum to exactly 1.0).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct AbuseFeatureWeights {
    cpu: f64,
    egress: f64,
    entropy: f64,
    exec: f64,
}

impl AbuseFeatureWeights {
    /// Canonical weights from sprint contract §5 R-S08-7 + WI §6.1.3.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            cpu: DEFAULT_CPU_WEIGHT,
            egress: DEFAULT_EGRESS_WEIGHT,
            entropy: DEFAULT_ENTROPY_WEIGHT,
            exec: DEFAULT_EXEC_WEIGHT,
        }
    }

    /// Construct with explicit weights. Defensive clamping:
    /// each weight is clamped to `[0.0, 1.0]`; if the resulting sum is
    /// not within `[0.999, 1.001]` of unity, returns the canonical
    /// default (NaN-safe; production wiring observes the
    /// `calibration_drift_total` counter for this fallback signal).
    #[must_use]
    pub fn new(cpu: f64, egress: f64, entropy: f64, exec: f64) -> Self {
        let cpu_c = clamp_unit(cpu);
        let egress_c = clamp_unit(egress);
        let entropy_c = clamp_unit(entropy);
        let exec_c = clamp_unit(exec);
        let sum = cpu_c + egress_c + entropy_c + exec_c;
        if !(0.999..=1.001).contains(&sum) {
            return Self::canonical();
        }
        Self {
            cpu: cpu_c,
            egress: egress_c,
            entropy: entropy_c,
            exec: exec_c,
        }
    }

    /// CPU weight (canonical 0.30).
    #[must_use]
    pub const fn cpu(&self) -> f64 {
        self.cpu
    }

    /// Egress weight (canonical 0.25).
    #[must_use]
    pub const fn egress(&self) -> f64 {
        self.egress
    }

    /// Entropy weight (canonical 0.25).
    #[must_use]
    pub const fn entropy(&self) -> f64 {
        self.entropy
    }

    /// Exec weight (canonical 0.20).
    #[must_use]
    pub const fn exec(&self) -> f64 {
        self.exec
    }

    /// Sum of all four weights. Pinned at exactly 1.0 by canonical
    /// construction; tunable weights are clamped + sum-validated by
    /// [`AbuseFeatureWeights::new`].
    #[must_use]
    pub fn sum(&self) -> f64 {
        self.cpu + self.egress + self.entropy + self.exec
    }
}

impl Default for AbuseFeatureWeights {
    fn default() -> Self {
        Self::canonical()
    }
}

fn clamp_unit(v: f64) -> f64 {
    if v.is_nan() {
        return 0.0;
    }
    v.clamp(0.0, 1.0)
}

/// Egress baseline (bytes/min) per tier — feature normalises against
/// this so a Free-tier tenant doing 50 MB/min doesn't score the same
/// as an Enterprise tenant doing 50 MB/min.
#[must_use]
pub const fn egress_baseline_for_tier(tier: Tier) -> u64 {
    match tier {
        Tier::Free => 1_000_000,                     // 1 MB/min.
        Tier::Solo => 10_000_000,                    // 10 MB/min.
        Tier::Team => 50_000_000,                    // 50 MB/min.
        Tier::Business => 500_000_000,               // 500 MB/min.
        Tier::Enterprise => 5_000_000_000,           // 5 GB/min.
        // Forward-compatibility — unknown tier gets enterprise default
        // (most-permissive; conservative-on-availability).
        _ => 5_000_000_000,
    }
}

/// Concurrent exec baseline per tier — feature normalises against this
/// so abusive ≥ 100× tier baseline maps to score component 1.0.
#[must_use]
pub const fn exec_baseline_for_tier(tier: Tier) -> u32 {
    match tier {
        Tier::Free => 5,
        Tier::Solo => 20,
        Tier::Team => 50,
        Tier::Business => 200,
        Tier::Enterprise => 1_000,
        _ => 1_000,
    }
}

/// Canonical config knobs for [`super::scorer::InMemoryAbuseScorer`].
///
/// Per the autonomous execution charter (`F-001 closure`), config is
/// per-instance; production wiring derives canonical defaults via
/// [`AbuseConfig::canonical`] and exposes admin-plane override only at
/// the S-13 layer (forward; staging stub OK at this WI per spec `Soft`
/// dependency).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct AbuseConfig {
    weights: AbuseFeatureWeights,
    suspicious_threshold: f64,
    malicious_threshold: f64,
}

impl AbuseConfig {
    /// Canonical defaults (weights 0.30/0.25/0.25/0.20; thresholds
    /// 0.5/0.8 per WI §1 invariant 3).
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            weights: AbuseFeatureWeights::canonical(),
            suspicious_threshold: SUSPICIOUS_THRESHOLD,
            malicious_threshold: MALICIOUS_THRESHOLD,
        }
    }

    /// Construct with explicit knobs. Defensive clamping:
    /// thresholds are clamped to `[0.0, 1.0]`; if `suspicious >=
    /// malicious` after clamping, returns the canonical default
    /// (consistency guard).
    #[must_use]
    pub fn new(
        weights: AbuseFeatureWeights,
        suspicious_threshold: f64,
        malicious_threshold: f64,
    ) -> Self {
        let s = clamp_unit(suspicious_threshold);
        let m = clamp_unit(malicious_threshold);
        if s >= m {
            return Self::canonical();
        }
        Self {
            weights,
            suspicious_threshold: s,
            malicious_threshold: m,
        }
    }

    /// Active feature weights.
    #[must_use]
    pub const fn weights(&self) -> &AbuseFeatureWeights {
        &self.weights
    }

    /// Suspicious threshold (canonical 0.5).
    #[must_use]
    pub const fn suspicious_threshold(&self) -> f64 {
        self.suspicious_threshold
    }

    /// Malicious threshold (canonical 0.8).
    #[must_use]
    pub const fn malicious_threshold(&self) -> f64 {
        self.malicious_threshold
    }
}

impl Default for AbuseConfig {
    fn default() -> Self {
        Self::canonical()
    }
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
    fn canonical_weights_sum_to_one() {
        let w = AbuseFeatureWeights::canonical();
        assert_eq!(w.sum(), 1.0);
    }

    #[test]
    fn canonical_weights_pinned_to_documented_constants() {
        let w = AbuseFeatureWeights::canonical();
        assert_eq!(w.cpu(), 0.30);
        assert_eq!(w.egress(), 0.25);
        assert_eq!(w.entropy(), 0.25);
        assert_eq!(w.exec(), 0.20);
    }

    #[test]
    fn weights_new_sum_must_be_one() {
        let w = AbuseFeatureWeights::new(0.4, 0.3, 0.2, 0.1);
        // Sum = 1.0 → accepted.
        assert_eq!(w.cpu(), 0.4);
        assert_eq!(w.egress(), 0.3);
    }

    #[test]
    fn weights_new_rejects_non_unity_sum_falls_back_to_canonical() {
        let w = AbuseFeatureWeights::new(0.5, 0.5, 0.5, 0.5);
        // Sum = 2.0 → falls back to canonical.
        assert_eq!(w, AbuseFeatureWeights::canonical());
    }

    #[test]
    fn weights_new_rejects_nan_falls_back_to_canonical() {
        let w = AbuseFeatureWeights::new(f64::NAN, 0.25, 0.25, 0.20);
        // NaN clamps to 0.0 → sum 0.7 → falls back.
        assert_eq!(w, AbuseFeatureWeights::canonical());
    }

    #[test]
    fn config_canonical_pinned() {
        let c = AbuseConfig::canonical();
        assert_eq!(c.suspicious_threshold(), 0.5);
        assert_eq!(c.malicious_threshold(), 0.8);
    }

    #[test]
    fn config_default_matches_canonical() {
        assert_eq!(AbuseConfig::default(), AbuseConfig::canonical());
    }

    #[test]
    fn config_new_rejects_inverted_thresholds_falls_back() {
        // Suspicious > Malicious → falls back.
        let c = AbuseConfig::new(AbuseFeatureWeights::canonical(), 0.9, 0.7);
        assert_eq!(c, AbuseConfig::canonical());
    }

    #[test]
    fn config_new_accepts_valid_thresholds() {
        let c = AbuseConfig::new(AbuseFeatureWeights::canonical(), 0.4, 0.85);
        assert_eq!(c.suspicious_threshold(), 0.4);
        assert_eq!(c.malicious_threshold(), 0.85);
    }

    #[test]
    fn egress_baseline_strictly_increasing() {
        let mut last = 0_u64;
        for tier in [
            Tier::Free,
            Tier::Solo,
            Tier::Team,
            Tier::Business,
            Tier::Enterprise,
        ] {
            let b = egress_baseline_for_tier(tier);
            assert!(b > last, "egress baseline must increase across tiers");
            last = b;
        }
    }

    #[test]
    fn exec_baseline_strictly_increasing() {
        let mut last = 0_u32;
        for tier in [
            Tier::Free,
            Tier::Solo,
            Tier::Team,
            Tier::Business,
            Tier::Enterprise,
        ] {
            let b = exec_baseline_for_tier(tier);
            assert!(b > last, "exec baseline must increase across tiers");
            last = b;
        }
    }

    // Compile-time pinning of canonical threshold ordering so future
    // drift is caught at compile (NOT runtime) — `assert!(true)` would
    // be optimised out per clippy::assertions_on_constants.
    const _: () = {
        assert!(SUSPICIOUS_THRESHOLD < MALICIOUS_THRESHOLD);
        assert!(SUSPICIOUS_THRESHOLD > 0.0);
        assert!(MALICIOUS_THRESHOLD < 1.0);
    };
}
