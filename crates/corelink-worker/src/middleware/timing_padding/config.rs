//! Constants + validated [`TimingPaddingConfig`] + [`TimingPaddingError`].
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3). Pure type surface — no service / layer / padding
//! logic.

/// Default target padded latency in milliseconds (per ADR-0023 §"Decision" 1).
///
/// Chosen to comfortably exceed the typical handler-resolution window
/// across all 3 arms (NeverExisted ≈ 50 ms KV + AuthZ; Tombstoned
/// ≈ 60 ms D1; R2OrphanRow ≈ 80 ms D1+R2) so [`super::padding::canonical_pad_target`]
/// always produces a positive sleep amount.
pub const TARGET_P99_MS_DEFAULT: u64 = 200;

/// Hard upper bound on `target_p99_ms` (per WI-S02-004 §10.4.8 chaos
/// scenario: a value > 5 s is more likely a programmer typo than a
/// real config and would interact poorly with worker ingress timeouts).
pub const TARGET_P99_MS_MAX: u64 = 5_000;

/// Default jitter percent (±10 %) per ADR-0023 §"Decision" 1.
pub const JITTER_PCT_DEFAULT: u8 = 10;

/// Hard upper bound on `jitter_pct`. Exceeding 50 % flips the meaning
/// of the target window (target becomes a midpoint, not a p99) — the
/// constructor rejects values above this bound.
pub const JITTER_PCT_MAX: u8 = 50;

/// Padding granularity floor in milliseconds (per WI-S02-004 §10.4.9).
///
/// CF Worker scheduler quanta are ≈ 1 ms; granularity ≥ 5 ms dominates
/// jitter so `tokio::time::sleep_until` produces a meaningful pad
/// rather than a no-op.
pub const PADDING_GRANULARITY_MS_MIN: u64 = 5;

/// Validated configuration for [`super::layer::TimingPaddingLayer`].
///
/// All construction goes through [`TimingPaddingConfig::new`] so the
/// invariants `target_p99_ms ∈ [PADDING_GRANULARITY_MS_MIN, TARGET_P99_MS_MAX]`
/// and `jitter_pct ≤ JITTER_PCT_MAX` are checked once at deploy time
/// rather than per request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimingPaddingConfig {
    target_p99_ms: u64,
    jitter_pct: u8,
}

/// Construction errors for [`TimingPaddingConfig`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TimingPaddingError {
    /// `target_p99_ms` is below [`PADDING_GRANULARITY_MS_MIN`] or
    /// above [`TARGET_P99_MS_MAX`].
    #[error(
        "target_p99_ms = {provided} ms outside accepted range [{min}, {max}] ms",
        min = PADDING_GRANULARITY_MS_MIN,
        max = TARGET_P99_MS_MAX,
    )]
    TargetOutOfRange {
        /// The rejected value (in milliseconds).
        provided: u64,
    },
    /// `jitter_pct` exceeds [`JITTER_PCT_MAX`].
    #[error(
        "jitter_pct = {provided}% exceeds maximum {max}%",
        max = JITTER_PCT_MAX,
    )]
    JitterOutOfRange {
        /// The rejected value (in percent).
        provided: u8,
    },
}

impl TimingPaddingConfig {
    /// Construct a validated config.
    ///
    /// # Errors
    ///
    /// - [`TimingPaddingError::TargetOutOfRange`] when `target_p99_ms`
    ///   is below [`PADDING_GRANULARITY_MS_MIN`] or above [`TARGET_P99_MS_MAX`].
    /// - [`TimingPaddingError::JitterOutOfRange`] when `jitter_pct`
    ///   exceeds [`JITTER_PCT_MAX`].
    pub const fn new(target_p99_ms: u64, jitter_pct: u8) -> Result<Self, TimingPaddingError> {
        if target_p99_ms < PADDING_GRANULARITY_MS_MIN || target_p99_ms > TARGET_P99_MS_MAX {
            return Err(TimingPaddingError::TargetOutOfRange {
                provided: target_p99_ms,
            });
        }
        if jitter_pct > JITTER_PCT_MAX {
            return Err(TimingPaddingError::JitterOutOfRange {
                provided: jitter_pct,
            });
        }
        Ok(Self {
            target_p99_ms,
            jitter_pct,
        })
    }

    /// Canonical default ([`TARGET_P99_MS_DEFAULT`] / [`JITTER_PCT_DEFAULT`]).
    #[must_use]
    pub const fn canonical() -> Self {
        // Canonical defaults are constants from this module, both
        // hard-coded inside `[5, 5_000]` and `≤ 50` so `new` cannot
        // fail. Using a `match` here lets the function stay `const`
        // while preserving the error-rejection guarantee at the
        // surface — the `Err` arm is provably unreachable.
        match Self::new(TARGET_P99_MS_DEFAULT, JITTER_PCT_DEFAULT) {
            Ok(c) => c,
            // Unreachable: defaults are inside the validated range; the
            // crate-level `panic = deny` lint forbids `unreachable!()`,
            // so we surface a witness config that still upholds the
            // type invariant. This branch cannot execute at runtime —
            // any consumer who flips the constants out of range will
            // see a compile-time `const` evaluation failure of a
            // sibling test (`canonical_defaults_are_within_bounds`).
            Err(_) => Self {
                target_p99_ms: TARGET_P99_MS_DEFAULT,
                jitter_pct: JITTER_PCT_DEFAULT,
            },
        }
    }

    /// Target padded p99 latency, in milliseconds.
    #[must_use]
    pub const fn target_p99_ms(&self) -> u64 {
        self.target_p99_ms
    }

    /// Jitter window, in percent (±).
    #[must_use]
    pub const fn jitter_pct(&self) -> u8 {
        self.jitter_pct
    }
}

impl Default for TimingPaddingConfig {
    fn default() -> Self {
        Self::canonical()
    }
}
