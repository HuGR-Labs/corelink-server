//! Quota config knobs surfaced by the production binding.
//!
//! Defaults pin canonical thresholds from WI-S07-003 §6.1 + spec
//! contract §1 boundary (95% eviction trigger; 100% PROVISIONAL deny).

/// Canonical 95% trigger threshold (CAP-EVICT-003 boundary; eviction
/// fires async-spawn fire-and-forget). Mirrors
/// `corelink-eviction::QUOTA_TRIGGER_THRESHOLD_PCT`.
pub const DEFAULT_TRIGGER_THRESHOLD_PCT: f64 = 0.95;

/// Canonical 100% deny threshold (CAP-EVICT-003 boundary; PROVISIONAL
/// 429 + Retry-After arm).
pub const DEFAULT_DENY_THRESHOLD_PCT: f64 = 1.0;

/// Canonical Retry-After floor (seconds). The PROVISIONAL formula
/// returns at least this many seconds even when the over-quota ratio
/// would compute a smaller value. 60s mirrors common rate-limit floors
/// (SES, GitHub API).
pub const DEFAULT_RETRY_AFTER_FLOOR_SECS: u64 = 60;

/// Knobs driving the quota-check decision engine.
///
/// All fields are private; access via inherent methods so the
/// production wiring cannot accidentally drift from canonical defaults
/// without going through [`QuotaConfig::with_overrides`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuotaConfig {
    /// Threshold at which the 95% eviction trigger fires (default 0.95).
    trigger_threshold_pct: f64,
    /// Threshold at which the PROVISIONAL 429 + Retry-After arm fires
    /// (default 1.0).
    deny_threshold_pct: f64,
    /// Retry-After floor (seconds; default 60s).
    retry_after_floor_secs: u64,
}

impl QuotaConfig {
    /// Construct with the canonical defaults pinned to the spec
    /// contract.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            trigger_threshold_pct: DEFAULT_TRIGGER_THRESHOLD_PCT,
            deny_threshold_pct: DEFAULT_DENY_THRESHOLD_PCT,
            retry_after_floor_secs: DEFAULT_RETRY_AFTER_FLOOR_SECS,
        }
    }

    /// Construct with explicit knob overrides (test wiring +
    /// admin-tier customisation in S-13 forward).
    ///
    /// Returns `None` when:
    /// - `trigger_threshold_pct` is not in `(0.0, 1.0]`.
    /// - `deny_threshold_pct` is not in `(0.0, 1.0]`.
    /// - `trigger_threshold_pct >= deny_threshold_pct` (boundary
    ///   inversion would make the trigger arm unreachable).
    #[must_use]
    pub fn with_overrides(
        trigger_threshold_pct: f64,
        deny_threshold_pct: f64,
        retry_after_floor_secs: u64,
    ) -> Option<Self> {
        if !(trigger_threshold_pct > 0.0 && trigger_threshold_pct <= 1.0) {
            return None;
        }
        if !(deny_threshold_pct > 0.0 && deny_threshold_pct <= 1.0) {
            return None;
        }
        if trigger_threshold_pct >= deny_threshold_pct {
            return None;
        }
        Some(Self {
            trigger_threshold_pct,
            deny_threshold_pct,
            retry_after_floor_secs,
        })
    }

    /// Eviction-trigger threshold pct.
    #[must_use]
    pub const fn trigger_threshold_pct(&self) -> f64 {
        self.trigger_threshold_pct
    }

    /// Deny-arm threshold pct.
    #[must_use]
    pub const fn deny_threshold_pct(&self) -> f64 {
        self.deny_threshold_pct
    }

    /// Retry-After floor (seconds).
    #[must_use]
    pub const fn retry_after_floor_secs(&self) -> u64 {
        self.retry_after_floor_secs
    }
}

impl Default for QuotaConfig {
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
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn canonical_constants_pinned() {
        assert!((DEFAULT_TRIGGER_THRESHOLD_PCT - 0.95).abs() < f64::EPSILON);
        assert!((DEFAULT_DENY_THRESHOLD_PCT - 1.0).abs() < f64::EPSILON);
        assert_eq!(DEFAULT_RETRY_AFTER_FLOOR_SECS, 60);
    }

    #[test]
    fn canonical_config_uses_canonical_defaults() {
        let c = QuotaConfig::canonical();
        assert!((c.trigger_threshold_pct() - 0.95).abs() < f64::EPSILON);
        assert!((c.deny_threshold_pct() - 1.0).abs() < f64::EPSILON);
        assert_eq!(c.retry_after_floor_secs(), 60);
    }

    #[test]
    fn default_matches_canonical() {
        assert_eq!(QuotaConfig::default(), QuotaConfig::canonical());
    }

    #[test]
    fn with_overrides_accepts_valid() {
        let c = QuotaConfig::with_overrides(0.80, 0.99, 30).unwrap();
        assert!((c.trigger_threshold_pct() - 0.80).abs() < f64::EPSILON);
        assert!((c.deny_threshold_pct() - 0.99).abs() < f64::EPSILON);
        assert_eq!(c.retry_after_floor_secs(), 30);
    }

    #[test]
    fn with_overrides_rejects_trigger_at_or_above_deny() {
        // Inversion — trigger MUST be strictly below deny.
        assert!(QuotaConfig::with_overrides(0.99, 0.99, 60).is_none());
        assert!(QuotaConfig::with_overrides(1.0, 0.99, 60).is_none());
    }

    #[test]
    fn with_overrides_rejects_zero_or_negative_thresholds() {
        assert!(QuotaConfig::with_overrides(0.0, 0.95, 60).is_none());
        assert!(QuotaConfig::with_overrides(-0.1, 0.95, 60).is_none());
        assert!(QuotaConfig::with_overrides(0.5, 0.0, 60).is_none());
        assert!(QuotaConfig::with_overrides(0.5, -0.1, 60).is_none());
    }

    #[test]
    fn with_overrides_rejects_thresholds_above_one() {
        assert!(QuotaConfig::with_overrides(1.1, 1.2, 60).is_none());
        assert!(QuotaConfig::with_overrides(0.5, 1.5, 60).is_none());
    }
}
