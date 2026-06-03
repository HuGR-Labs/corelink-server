//! Canonical 4-feature shape for heuristic abuse scoring.
//!
//! Per sprint contract §5 R-S08-7 + WI-S08-004 §6.1.3, the feature set
//! is FROZEN at exactly four:
//!
//! 1. `cpu_wallclock_ratio` (`f64`) — `cpu_seconds / wallclock_seconds`
//!    over a 5min window. Abusive ≥ 0.95 sustained = compute farming
//!    (botnet / cryptominer using CoreLink as compute substrate).
//! 2. `egress_bytes_per_min` (`u64`) — egress throughput in bytes/min
//!    over the same window. Abusive ≥ 95th percentile WoW + 5σ.
//! 3. `action_digest_entropy_bits` (`f64`) — Shannon entropy (bits) of
//!    the distribution of distinct action_digests in the window.
//!    Abusive ≤ 1 bit = single-action spam (1 unique digest hammered
//!    10k times); legitimate workloads land ≥ 3 bits (≥ 8 distinct
//!    actions).
//! 4. `concurrent_exec_count` (`u32`) — peak count of parallel exec
//!    sessions during the window. Abusive ≥ 100× plan tier baseline.
//!
//! ## Why these four (heurística NOT ML)
//!
//! Sprint contract §10 anti-scope ML; LGPD Art. 20 transparency. Each
//! feature targets a distinct abuse vector:
//!
//! - cpu_wallclock_ratio → compute farming
//! - egress_bytes_per_min → scraping
//! - action_digest_entropy_bits → spam (high entropy = legit; low =
//!   spam)
//! - concurrent_exec_count → exec flood
//!
//! Combined-feature score makes adversarial gaming harder than any
//! single-feature threshold; calibrated em staging via 50+50 synthetic
//! workloads (Lote 10.8bis P0-E corrected; sprint contract §6 DoD).

/// 4-feature canonical observation shape (5min window aggregation per
/// WI §1 invariant 4 + sprint contract §10.s08.5).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct AbuseFeatures {
    /// Feature 1: `cpu_seconds / wallclock_seconds` over the window;
    /// real domain is `[0.0, ~ N_cpu_cores]`. The score normalisation
    /// clamps the contribution at 1.0 when ratio ≥ 1.0 (saturation
    /// indicates sustained 100% CPU on the wallclock — compute
    /// farming canary).
    pub cpu_wallclock_ratio: f64,

    /// Feature 2: egress throughput observed via S-09 Prometheus
    /// `rate(corelink_egress_bytes_total[5m]) * 60` (bytes/min).
    /// Normalises against tier baseline.
    pub egress_bytes_per_min: u64,

    /// Feature 3: Shannon entropy (bits) of the distribution of
    /// distinct action_digests in the window. `log2(N_unique)` upper
    /// bound when distribution is uniform. Domain `[0.0, ~64.0]`.
    pub action_digest_entropy_bits: f64,

    /// Feature 4: peak count of parallel exec sessions during the
    /// window (max). Normalises against tier baseline.
    pub concurrent_exec_count: u32,
}

impl AbuseFeatures {
    /// Construct canonical zero-valued features (typical "tenant did
    /// nothing this window" baseline).
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            cpu_wallclock_ratio: 0.0,
            egress_bytes_per_min: 0,
            action_digest_entropy_bits: 0.0,
            concurrent_exec_count: 0,
        }
    }

    /// Construct from raw values. NaN values in `cpu_wallclock_ratio`
    /// or `action_digest_entropy_bits` are coerced to 0.0 (defensive
    /// programming; production wiring's S-09 reader should never emit
    /// NaN but we belt-and-suspenders).
    #[must_use]
    pub fn new(
        cpu_wallclock_ratio: f64,
        egress_bytes_per_min: u64,
        action_digest_entropy_bits: f64,
        concurrent_exec_count: u32,
    ) -> Self {
        Self {
            cpu_wallclock_ratio: sanitize_f64(cpu_wallclock_ratio),
            egress_bytes_per_min,
            action_digest_entropy_bits: sanitize_f64(action_digest_entropy_bits),
            concurrent_exec_count,
        }
    }
}

impl Default for AbuseFeatures {
    fn default() -> Self {
        Self::zero()
    }
}

/// Coerce NaN / -Inf to 0.0; clamp +Inf to f64::MAX. Used by
/// [`AbuseFeatures::new`] to guard the score formula against pathologic
/// observability inputs.
fn sanitize_f64(v: f64) -> f64 {
    if v.is_nan() || v < 0.0 {
        0.0
    } else if v.is_infinite() {
        f64::MAX
    } else {
        v
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
    fn zero_features_are_all_zero() {
        let f = AbuseFeatures::zero();
        assert_eq!(f.cpu_wallclock_ratio, 0.0);
        assert_eq!(f.egress_bytes_per_min, 0);
        assert_eq!(f.action_digest_entropy_bits, 0.0);
        assert_eq!(f.concurrent_exec_count, 0);
    }

    #[test]
    fn default_matches_zero() {
        assert_eq!(AbuseFeatures::default(), AbuseFeatures::zero());
    }

    #[test]
    fn new_passes_values_through() {
        let f = AbuseFeatures::new(0.7, 100_000, 2.5, 42);
        assert_eq!(f.cpu_wallclock_ratio, 0.7);
        assert_eq!(f.egress_bytes_per_min, 100_000);
        assert_eq!(f.action_digest_entropy_bits, 2.5);
        assert_eq!(f.concurrent_exec_count, 42);
    }

    #[test]
    fn new_coerces_nan_cpu_ratio_to_zero() {
        let f = AbuseFeatures::new(f64::NAN, 0, 0.0, 0);
        assert_eq!(f.cpu_wallclock_ratio, 0.0);
    }

    #[test]
    fn new_coerces_negative_entropy_to_zero() {
        let f = AbuseFeatures::new(0.0, 0, -1.5, 0);
        assert_eq!(f.action_digest_entropy_bits, 0.0);
    }

    #[test]
    fn new_clamps_positive_infinity_to_max() {
        let f = AbuseFeatures::new(f64::INFINITY, 0, 0.0, 0);
        assert_eq!(f.cpu_wallclock_ratio, f64::MAX);
    }

    #[test]
    fn new_coerces_negative_infinity_to_zero() {
        let f = AbuseFeatures::new(f64::NEG_INFINITY, 0, 0.0, 0);
        assert_eq!(f.cpu_wallclock_ratio, 0.0);
    }

    #[test]
    fn equality_is_componentwise() {
        let a = AbuseFeatures::new(0.5, 100, 2.0, 10);
        let b = AbuseFeatures::new(0.5, 100, 2.0, 10);
        assert_eq!(a, b);
        let c = AbuseFeatures::new(0.5, 100, 2.0, 11);
        assert_ne!(a, c);
    }
}
