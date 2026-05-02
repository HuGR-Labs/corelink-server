//! Canonical [`QuotaCasConfig`] knobs.
//!
//! ## F-001 closure
//!
//! Every knob lives on a per-instance struct (NO process-global static).
//! Tests instantiate fresh configs per case so the harness cannot leak
//! state across cases.

/// Canonical hard-block percentage (relative to `bytes_quota`). The
/// CAS predicate fires the deny arm when
/// `bytes_used + request_bytes >= DEFAULT_HARD_BLOCK_PCT × bytes_quota`
/// (equivalent to the strict-< canonical predicate at 100% boundary).
///
/// Per ADR-0020 FROZEN, S-08 owns the 100% hard-block; S-07 owns the
/// ≤95% eviction trigger. The 95–100% transition window is monitored
/// by S-08 (alert SEV-3 customer-facing + SEV-2 internal signaling
/// eviction lag). This config knob exposes the boundary so admin can
/// configure a more conservative soft-block (e.g. 99%) for staged
/// rollouts; default is 1.0 (canonical).
pub const DEFAULT_HARD_BLOCK_PCT: f64 = 1.0;

/// Canonical maximum CAS retry attempts per request before the
/// orchestrator surfaces [`crate::error::QuotaCasError::CasRaceExhausted`].
///
/// 3 attempts strikes the canonical balance: enough to absorb the
/// transient `cas_version` bump from a concurrent commit_reservation /
/// eviction_reclaim (which advances version at most once per their
/// quantum), but bounded so a pathologically contended tenant never
/// wedges the request thread (the hot path stays sub-ms even under
/// adversarial concurrent-write floods).
pub const DEFAULT_MAX_CAS_ATTEMPTS: u32 = 3;

/// Canonical config knobs for [`crate::cas::InMemoryAtomicQuotaChecker`].
///
/// Per the autonomous execution charter (`F-001 closure`), config is
/// per-instance; production wiring derives canonical defaults via
/// [`QuotaCasConfig::canonical`] and exposes admin-plane override only
/// at the S-13 layer (forward; staging stub OK at this WI per spec
/// `Soft` dependency).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct QuotaCasConfig {
    hard_block_pct: f64,
    max_cas_attempts: u32,
}

impl QuotaCasConfig {
    /// Canonical defaults (0.95 alert / 1.0 hard-block / 3 retries).
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            hard_block_pct: DEFAULT_HARD_BLOCK_PCT,
            max_cas_attempts: DEFAULT_MAX_CAS_ATTEMPTS,
        }
    }

    /// Construct with explicit knobs. Defensive clamping:
    /// - `hard_block_pct` clamped to `[0.5, 1.0]` (admin cannot
    ///   accidentally configure a 0% block which would deny every
    ///   request, nor > 100% which would never fire).
    /// - `max_cas_attempts` clamped to `[1, 16]` (lower bound prevents
    ///   immediate exhaustion; upper bound prevents pathological
    ///   thread-occupancy under adversarial contention).
    #[must_use]
    pub fn new(hard_block_pct: f64, max_cas_attempts: u32) -> Self {
        let pct = if hard_block_pct.is_nan() {
            DEFAULT_HARD_BLOCK_PCT
        } else {
            hard_block_pct.clamp(0.5, 1.0)
        };
        let attempts = max_cas_attempts.clamp(1, 16);
        Self {
            hard_block_pct: pct,
            max_cas_attempts: attempts,
        }
    }

    /// Hard-block percentage knob (canonical 1.0).
    #[must_use]
    pub const fn hard_block_pct(&self) -> f64 {
        self.hard_block_pct
    }

    /// Max CAS retry attempts per request (canonical 3).
    #[must_use]
    pub const fn max_cas_attempts(&self) -> u32 {
        self.max_cas_attempts
    }
}

impl Default for QuotaCasConfig {
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
    fn canonical_pinned_to_documented_constants() {
        let c = QuotaCasConfig::canonical();
        assert_eq!(c.hard_block_pct(), 1.0);
        assert_eq!(c.max_cas_attempts(), 3);
    }

    #[test]
    fn default_matches_canonical() {
        assert_eq!(QuotaCasConfig::default(), QuotaCasConfig::canonical());
    }

    #[test]
    fn clamps_low_pct_to_half() {
        let c = QuotaCasConfig::new(0.1, 3);
        assert_eq!(c.hard_block_pct(), 0.5);
    }

    #[test]
    fn clamps_high_pct_to_one() {
        let c = QuotaCasConfig::new(2.0, 3);
        assert_eq!(c.hard_block_pct(), 1.0);
    }

    #[test]
    fn nan_falls_back_to_canonical() {
        let c = QuotaCasConfig::new(f64::NAN, 3);
        assert_eq!(c.hard_block_pct(), DEFAULT_HARD_BLOCK_PCT);
    }

    #[test]
    fn clamps_zero_attempts_to_one() {
        let c = QuotaCasConfig::new(1.0, 0);
        assert_eq!(c.max_cas_attempts(), 1);
    }

    #[test]
    fn clamps_huge_attempts_to_sixteen() {
        let c = QuotaCasConfig::new(1.0, 1024);
        assert_eq!(c.max_cas_attempts(), 16);
    }
}
