//! Canonical state-machine types: [`QuotaState`] + [`QuotaTransition`] +
//! [`UtilizationPct`] + [`InvoiceFailureCount`] + [`QuotaFsmConfig`].
//!
//! ## Why typed enums (NOT string discriminants)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 (absorbed via WI-S10-005 §1
//! invariant 7): the audit envelope payload written to the S-09 audit
//! chain immutable 7y archive must be byte-deterministic across the
//! auditor evidence trail. Untyped `serde_json::Value` defeats
//! compile-time taxonomy enforcement; the typed enum + per-variant
//! payload struct is the canonical Lote 10.9-quinquies absorption
//! pattern (mirrors WI-S10-001 `UsageEventKind` + WI-S10-002
//! `AggregationDecision` + WI-S10-003 `StripeAdapterDecision` +
//! WI-S10-004 `ReconcileDecision`).
//!
//! All public enums are `#[non_exhaustive]` so follow-on WIs can extend
//! the taxonomy additively without breaking downstream `match` sites
//! (e.g. WI-S10-006 replay forensic feed; S-13 admin plane in-app
//! notification consumer; future enterprise grace-period flag).
//!
//! ## Email send DEFERRED
//!
//! Per the autonomous execution charter brief + ADR-0020 FROZEN: email
//! send is REJECTED inside S-10. The 80%/95% transitions emit the
//! canonical `corelink.billing_quota.overage_telemetry_recorded` audit
//! ONLY; the actual email delivery is the S-13 admin/notifications
//! consumer that subscribes to the audit chain and dispatches the
//! customer-facing notification. THIS WI ships the state machine + the
//! telemetry emission contract — the consumer is out of scope.

use serde::{Deserialize, Serialize};

/// Canonical 5-state quota machine taxonomy. The graduated ladder mirrors
/// sprint contract §5.5 R-S10-10 (`under_80 → soft_alert → ticket →
/// hard_block`) extended to incorporate the **`SuspendedForNonPayment`**
/// terminal arm fired after 3 invoice-failure webhooks from
/// `corelink-billing-stripe` (WI-S10-003 inheritance).
///
/// State semantics:
///
/// - [`QuotaState::WithinPlan`]: utilization ∈ `[0%, 80%)`; silent
///   normal operation; default state.
/// - [`QuotaState::SoftWarning80pct`]: utilization ∈ `[80%, 95%)`;
///   telemetry-only audit `corelink.billing_quota.overage_telemetry_recorded`
///   fires (S-13 consumer dispatches email); customer reaches plan
///   warning band.
/// - [`QuotaState::SoftWarning95pct`]: utilization ∈ `[95%, 100%)`;
///   second telemetry audit fires (S-13 consumer escalates to support
///   ticket); customer enters hard-cap warning band.
/// - [`QuotaState::OverQuota100pct`]: utilization `≥ 100%`; the canonical
///   429 hard-block arm — the hot-path Tower middleware (WI-S10-007)
///   reads this state and writes `429 + X-RateLimit-Layer: quota +
///   Retry-After: <seconds_until_period_reset>` per the S-08 alignment
///   inheritance.
/// - [`QuotaState::SuspendedForNonPayment`]: terminal arm — the canonical
///   `n` consecutive Stripe `invoice.payment_failed` webhooks accumulate
///   the per-tenant counter; on threshold breach the tenant is suspended
///   (CAS write returns 403 forbidden in the production hot-path; this
///   crate ships the decision arm only). Cleared via the explicit
///   `reinstate()` operator-driven path.
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs
/// (e.g. an `EnterpriseGracePeriod` flag carved as the 6th state at
/// S-13 admin plane; we ship 5 canonical here per the orchestrator
/// brief).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[non_exhaustive]
pub enum QuotaState {
    /// `[0%, 80%)` utilization — silent normal operation.
    WithinPlan,
    /// `[80%, 95%)` utilization — first warning telemetry band.
    SoftWarning80pct,
    /// `[95%, 100%)` utilization — second warning telemetry band.
    SoftWarning95pct,
    /// `≥ 100%` utilization — hard-block 429 band; CAS write returns
    /// `429 + X-RateLimit-Layer: quota` per the S-08 alignment.
    OverQuota100pct,
    /// `n` consecutive invoice-payment-failed webhooks — terminal
    /// suspension arm; CAS write returns `403 forbidden`. Cleared via
    /// the operator-driven `reinstate()` path.
    SuspendedForNonPayment,
}

impl QuotaState {
    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WithinPlan => "within_plan",
            Self::SoftWarning80pct => "soft_warning_80pct",
            Self::SoftWarning95pct => "soft_warning_95pct",
            Self::OverQuota100pct => "over_quota_100pct",
            Self::SuspendedForNonPayment => "suspended_for_non_payment",
        }
    }

    /// Whether this state writes a 429 at the hot-path Tower middleware
    /// (consumed by WI-S10-007 production wiring).
    #[must_use]
    pub const fn writes_429(self) -> bool {
        matches!(self, Self::OverQuota100pct)
    }

    /// Whether this state is the terminal payment-suspension arm.
    #[must_use]
    pub const fn is_suspended(self) -> bool {
        matches!(self, Self::SuspendedForNonPayment)
    }
}

impl core::fmt::Display for QuotaState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 5-element list of all states. Pinned for cardinality
/// estimate + cross-component regression tests.
#[must_use]
pub const fn canonical_quota_states() -> &'static [QuotaState; 5] {
    &[
        QuotaState::WithinPlan,
        QuotaState::SoftWarning80pct,
        QuotaState::SoftWarning95pct,
        QuotaState::OverQuota100pct,
        QuotaState::SuspendedForNonPayment,
    ]
}

/// Canonical 6-element transition outcome taxonomy. The orchestrator
/// returns one of these per call to `evaluate_utilization()` /
/// `record_invoice_failure()` / `reinstate()`.
///
/// The `NoChange` arm is the canonical idempotent-rerun outcome (per WI
/// brief: re-firing the same state transition is a no-op). The other
/// arms each correspond to a concrete state edge in the canonical
/// graduated ladder + the suspension/reinstate operator path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum QuotaTransition {
    /// Same state observed before and after the call — no audit, no
    /// state-store mutation, no telemetry emission.
    NoChange {
        /// The state observed (== current).
        state: QuotaState,
    },
    /// `WithinPlan → SoftWarning80pct` (fresh entry into the first
    /// warning band). Also fires when the FSM observes a downward
    /// transition from a higher band into 80pct (rare; Lote 10.10-bis
    /// remediation lesson — the orchestrator does NOT emit a fresh
    /// telemetry row for a downward bounce; we route through `NoChange`
    /// when the destination matches the current state).
    TransitionedTo80pct {
        /// Predecessor state.
        from: QuotaState,
    },
    /// Transition into the 95pct second-warning band.
    TransitionedTo95pct {
        /// Predecessor state.
        from: QuotaState,
    },
    /// Transition into the 100pct hard-block band — the hot-path Tower
    /// middleware reads this and writes 429.
    TransitionedTo100pct {
        /// Predecessor state.
        from: QuotaState,
    },
    /// Transition into the terminal suspension arm — fired ONLY by
    /// `record_invoice_failure()` when the per-tenant counter reaches
    /// the canonical threshold (`config.suspension_invoice_failure_threshold`,
    /// default 3; WI brief).
    Suspended {
        /// Per-tenant invoice-failure counter at the moment of the
        /// transition (≥ threshold).
        invoice_failures: u32,
    },
    /// Reinstatement from the terminal suspension arm (operator-driven
    /// path; the state machine resets the per-tenant counter to 0 and
    /// flips back to the utilization-derived state).
    Reinstated {
        /// The new state after reinstatement (utilization-derived).
        new_state: QuotaState,
    },
}

impl QuotaTransition {
    /// Canonical lower-snake-case mnemonic for metrics + audit context.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoChange { .. } => "no_change",
            Self::TransitionedTo80pct { .. } => "transitioned_to_80pct",
            Self::TransitionedTo95pct { .. } => "transitioned_to_95pct",
            Self::TransitionedTo100pct { .. } => "transitioned_to_100pct",
            Self::Suspended { .. } => "suspended",
            Self::Reinstated { .. } => "reinstated",
        }
    }

    /// Whether this transition mutated the state store + emitted an
    /// audit row (the inverse of `NoChange`).
    #[must_use]
    pub const fn mutated(self) -> bool {
        !matches!(self, Self::NoChange { .. })
    }
}

/// Per-percent utilization wrapper. Bounded `[0, 200]` per WI-S10-005
/// §1 line 119 R5 P1-H (allows over 100% reporting capped at 200% for
/// sanity).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct UtilizationPct(f64);

impl UtilizationPct {
    /// Construct a fresh utilization wrapper from a `f64`.
    ///
    /// # Errors
    ///
    /// Returns [`super::error::QuotaFsmError::Config`] when the value is
    /// `NaN`, infinite, negative, or above 200.
    pub fn new(value: f64) -> Result<Self, super::error::QuotaFsmError> {
        if !value.is_finite() {
            return Err(super::error::QuotaFsmError::Config(format!(
                "utilization must be finite; got {value}"
            )));
        }
        if !(0.0..=200.0).contains(&value) {
            return Err(super::error::QuotaFsmError::Config(format!(
                "utilization must be in [0, 200]; got {value}"
            )));
        }
        Ok(Self(value))
    }

    /// Read the underlying `f64`.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

/// Per-tenant invoice-failure counter wrapper. Saturates at `u32::MAX`
/// (defensive — the production wiring's Stripe webhook adapter will
/// trigger suspension long before any realistic counter overflow).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InvoiceFailureCount(u32);

impl InvoiceFailureCount {
    /// Construct a fresh counter at `0`.
    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Construct a counter with an explicit value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Read the underlying `u32`.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Increment by one (saturating at `u32::MAX`).
    #[must_use]
    pub const fn incremented(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Canonical 80% utilization threshold (sprint contract §5.5 R-S10-10).
/// Transition trigger: `utilization >= SOFT_WARNING_80PCT_THRESHOLD AND
/// utilization < SOFT_WARNING_95PCT_THRESHOLD` lands in
/// [`QuotaState::SoftWarning80pct`].
pub const SOFT_WARNING_80PCT_THRESHOLD: f64 = 80.0;

/// Canonical 95% utilization threshold (sprint contract §5.5 R-S10-10).
/// Transition trigger: `utilization >= SOFT_WARNING_95PCT_THRESHOLD AND
/// utilization < OVER_QUOTA_100PCT_THRESHOLD` lands in
/// [`QuotaState::SoftWarning95pct`].
pub const SOFT_WARNING_95PCT_THRESHOLD: f64 = 95.0;

/// Canonical 100% utilization threshold (sprint contract §5.5 R-S10-10).
/// Transition trigger: `utilization >= OVER_QUOTA_100PCT_THRESHOLD`
/// lands in [`QuotaState::OverQuota100pct`] (the hot-path Tower
/// middleware reads this and writes the canonical 429).
pub const OVER_QUOTA_100PCT_THRESHOLD: f64 = 100.0;

/// Canonical invoice-failure threshold for terminal suspension (WI
/// brief: 3 invoice failures from Stripe webhook → suspension).
pub const SUSPENSION_INVOICE_FAILURE_THRESHOLD: u32 = 3;

/// Knobs driving the quota state-machine orchestrator. Defaults pin the
/// canonical thresholds from sprint contract §5.5 R-S10-10 + the WI
/// brief's 3-invoice-failure suspension threshold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuotaFsmConfig {
    soft_warning_80pct_threshold: f64,
    soft_warning_95pct_threshold: f64,
    over_quota_100pct_threshold: f64,
    suspension_invoice_failure_threshold: u32,
}

impl Default for QuotaFsmConfig {
    fn default() -> Self {
        Self {
            soft_warning_80pct_threshold: SOFT_WARNING_80PCT_THRESHOLD,
            soft_warning_95pct_threshold: SOFT_WARNING_95PCT_THRESHOLD,
            over_quota_100pct_threshold: OVER_QUOTA_100PCT_THRESHOLD,
            suspension_invoice_failure_threshold: SUSPENSION_INVOICE_FAILURE_THRESHOLD,
        }
    }
}

impl QuotaFsmConfig {
    /// Construct a custom config. Production wiring lifts the values
    /// from `wrangler.toml` env overrides
    /// (`CORELINK_QUOTA_FSM_*`).
    ///
    /// # Errors
    ///
    /// Returns [`super::error::QuotaFsmError::Config`] when an invariant
    /// is violated (non-finite threshold; threshold ladder
    /// non-monotonic; suspension threshold below 1).
    pub fn new(
        soft_warning_80pct_threshold: f64,
        soft_warning_95pct_threshold: f64,
        over_quota_100pct_threshold: f64,
        suspension_invoice_failure_threshold: u32,
    ) -> Result<Self, super::error::QuotaFsmError> {
        for (name, v) in [
            (
                "soft_warning_80pct_threshold",
                soft_warning_80pct_threshold,
            ),
            (
                "soft_warning_95pct_threshold",
                soft_warning_95pct_threshold,
            ),
            ("over_quota_100pct_threshold", over_quota_100pct_threshold),
        ] {
            if !v.is_finite() || v < 0.0 {
                return Err(super::error::QuotaFsmError::Config(format!(
                    "{name} must be finite and >= 0; got {v}"
                )));
            }
        }
        if soft_warning_80pct_threshold >= soft_warning_95pct_threshold {
            return Err(super::error::QuotaFsmError::Config(format!(
                "soft_warning_80pct_threshold ({soft_warning_80pct_threshold}) must be strictly less than soft_warning_95pct_threshold ({soft_warning_95pct_threshold})"
            )));
        }
        if soft_warning_95pct_threshold >= over_quota_100pct_threshold {
            return Err(super::error::QuotaFsmError::Config(format!(
                "soft_warning_95pct_threshold ({soft_warning_95pct_threshold}) must be strictly less than over_quota_100pct_threshold ({over_quota_100pct_threshold})"
            )));
        }
        if suspension_invoice_failure_threshold < 1 {
            return Err(super::error::QuotaFsmError::Config(format!(
                "suspension_invoice_failure_threshold must be >= 1; got {suspension_invoice_failure_threshold}"
            )));
        }
        Ok(Self {
            soft_warning_80pct_threshold,
            soft_warning_95pct_threshold,
            over_quota_100pct_threshold,
            suspension_invoice_failure_threshold,
        })
    }

    /// 80pct utilization threshold.
    #[must_use]
    pub const fn soft_warning_80pct_threshold(self) -> f64 {
        self.soft_warning_80pct_threshold
    }

    /// 95pct utilization threshold.
    #[must_use]
    pub const fn soft_warning_95pct_threshold(self) -> f64 {
        self.soft_warning_95pct_threshold
    }

    /// 100pct utilization threshold (the hard-block 429 floor).
    #[must_use]
    pub const fn over_quota_100pct_threshold(self) -> f64 {
        self.over_quota_100pct_threshold
    }

    /// Invoice-failure count that triggers terminal suspension.
    #[must_use]
    pub const fn suspension_invoice_failure_threshold(self) -> u32 {
        self.suspension_invoice_failure_threshold
    }
}

/// Pure-logic primitive: derive the utilization-bucket state from a
/// percentage value (does NOT consider suspension — that arm lives in
/// the orchestrator + counter store).
///
/// Boundary semantics: `>=` on the lower-bound + `<` on the upper-bound
/// (Prometheus-style inclusive-start exclusive-end; mirrors WI-S10-002
/// PeriodWindow + WI-S10-004 ladder boundaries):
///
/// - `[0, 80)`     → [`QuotaState::WithinPlan`]
/// - `[80, 95)`    → [`QuotaState::SoftWarning80pct`]
/// - `[95, 100)`   → [`QuotaState::SoftWarning95pct`]
/// - `[100, …]`    → [`QuotaState::OverQuota100pct`]
///
/// At exactly 80 the bucket is `SoftWarning80pct`; at exactly 79.999 it
/// is `WithinPlan`. The unit tests pin these boundaries.
#[must_use]
pub fn utilization_bucket(util: UtilizationPct, config: &QuotaFsmConfig) -> QuotaState {
    let v = util.value();
    if v < config.soft_warning_80pct_threshold() {
        QuotaState::WithinPlan
    } else if v < config.soft_warning_95pct_threshold() {
        QuotaState::SoftWarning80pct
    } else if v < config.over_quota_100pct_threshold() {
        QuotaState::SoftWarning95pct
    } else {
        QuotaState::OverQuota100pct
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives; float_cmp is acceptable for canonical-percentage-pin assertions where the values are constructed deterministically."
)]
mod tests {
    use super::*;

    #[test]
    fn canonical_states_pinned() {
        let s = canonical_quota_states();
        assert_eq!(s.len(), 5);
        assert_eq!(s[0].as_str(), "within_plan");
        assert_eq!(s[1].as_str(), "soft_warning_80pct");
        assert_eq!(s[2].as_str(), "soft_warning_95pct");
        assert_eq!(s[3].as_str(), "over_quota_100pct");
        assert_eq!(s[4].as_str(), "suspended_for_non_payment");
    }

    #[test]
    fn state_writes_429_only_for_over_quota() {
        for s in canonical_quota_states() {
            assert_eq!(s.writes_429(), *s == QuotaState::OverQuota100pct);
        }
    }

    #[test]
    fn state_is_suspended_only_for_suspension_arm() {
        for s in canonical_quota_states() {
            assert_eq!(s.is_suspended(), *s == QuotaState::SuspendedForNonPayment);
        }
    }

    #[test]
    fn state_display_matches_str() {
        assert_eq!(format!("{}", QuotaState::OverQuota100pct), "over_quota_100pct");
    }

    #[test]
    fn canonical_constants_pinned() {
        assert_eq!(SOFT_WARNING_80PCT_THRESHOLD, 80.0);
        assert_eq!(SOFT_WARNING_95PCT_THRESHOLD, 95.0);
        assert_eq!(OVER_QUOTA_100PCT_THRESHOLD, 100.0);
        assert_eq!(SUSPENSION_INVOICE_FAILURE_THRESHOLD, 3);
    }

    #[test]
    fn config_default_matches_canonical_constants() {
        let c = QuotaFsmConfig::default();
        assert_eq!(c.soft_warning_80pct_threshold(), SOFT_WARNING_80PCT_THRESHOLD);
        assert_eq!(c.soft_warning_95pct_threshold(), SOFT_WARNING_95PCT_THRESHOLD);
        assert_eq!(c.over_quota_100pct_threshold(), OVER_QUOTA_100PCT_THRESHOLD);
        assert_eq!(
            c.suspension_invoice_failure_threshold(),
            SUSPENSION_INVOICE_FAILURE_THRESHOLD
        );
    }

    #[test]
    fn config_rejects_non_finite() {
        let r = QuotaFsmConfig::new(f64::NAN, 95.0, 100.0, 3);
        assert!(matches!(r, Err(super::super::error::QuotaFsmError::Config(_))));
    }

    #[test]
    fn config_rejects_negative() {
        let r = QuotaFsmConfig::new(-1.0, 95.0, 100.0, 3);
        assert!(matches!(r, Err(super::super::error::QuotaFsmError::Config(_))));
    }

    #[test]
    fn config_rejects_non_monotonic_ladder() {
        let r = QuotaFsmConfig::new(95.0, 90.0, 100.0, 3);
        assert!(matches!(r, Err(super::super::error::QuotaFsmError::Config(_))));
    }

    #[test]
    fn config_rejects_zero_suspension_threshold() {
        let r = QuotaFsmConfig::new(80.0, 95.0, 100.0, 0);
        assert!(matches!(r, Err(super::super::error::QuotaFsmError::Config(_))));
    }

    #[test]
    fn config_accepts_canonical() {
        QuotaFsmConfig::new(
            SOFT_WARNING_80PCT_THRESHOLD,
            SOFT_WARNING_95PCT_THRESHOLD,
            OVER_QUOTA_100PCT_THRESHOLD,
            SUSPENSION_INVOICE_FAILURE_THRESHOLD,
        )
        .unwrap();
    }

    #[test]
    fn utilization_pct_rejects_nan() {
        let r = UtilizationPct::new(f64::NAN);
        assert!(r.is_err());
    }

    #[test]
    fn utilization_pct_rejects_negative() {
        let r = UtilizationPct::new(-1.0);
        assert!(r.is_err());
    }

    #[test]
    fn utilization_pct_rejects_above_200() {
        let r = UtilizationPct::new(201.0);
        assert!(r.is_err());
    }

    #[test]
    fn utilization_pct_accepts_zero_and_two_hundred() {
        UtilizationPct::new(0.0).unwrap();
        UtilizationPct::new(200.0).unwrap();
    }

    #[test]
    fn invoice_failure_count_increments() {
        let c = InvoiceFailureCount::zero();
        let c1 = c.incremented();
        let c2 = c1.incremented();
        assert_eq!(c.value(), 0);
        assert_eq!(c1.value(), 1);
        assert_eq!(c2.value(), 2);
    }

    #[test]
    fn invoice_failure_count_saturates_at_u32_max() {
        let c = InvoiceFailureCount::new(u32::MAX);
        assert_eq!(c.incremented().value(), u32::MAX);
    }

    #[test]
    fn utilization_bucket_canonical_boundaries() {
        let c = QuotaFsmConfig::default();
        // Below 80pct.
        assert_eq!(
            utilization_bucket(UtilizationPct::new(0.0).unwrap(), &c),
            QuotaState::WithinPlan
        );
        assert_eq!(
            utilization_bucket(UtilizationPct::new(79.999).unwrap(), &c),
            QuotaState::WithinPlan
        );
        // At 80pct (inclusive lower bound).
        assert_eq!(
            utilization_bucket(UtilizationPct::new(80.0).unwrap(), &c),
            QuotaState::SoftWarning80pct
        );
        // At 80.0001pct.
        assert_eq!(
            utilization_bucket(UtilizationPct::new(80.0001).unwrap(), &c),
            QuotaState::SoftWarning80pct
        );
        // Just under 95pct.
        assert_eq!(
            utilization_bucket(UtilizationPct::new(94.999).unwrap(), &c),
            QuotaState::SoftWarning80pct
        );
        // At 95pct.
        assert_eq!(
            utilization_bucket(UtilizationPct::new(95.0).unwrap(), &c),
            QuotaState::SoftWarning95pct
        );
        // Just under 100pct.
        assert_eq!(
            utilization_bucket(UtilizationPct::new(99.999).unwrap(), &c),
            QuotaState::SoftWarning95pct
        );
        // At 100pct.
        assert_eq!(
            utilization_bucket(UtilizationPct::new(100.0).unwrap(), &c),
            QuotaState::OverQuota100pct
        );
        // Above 100pct (capped at 200).
        assert_eq!(
            utilization_bucket(UtilizationPct::new(150.0).unwrap(), &c),
            QuotaState::OverQuota100pct
        );
        assert_eq!(
            utilization_bucket(UtilizationPct::new(200.0).unwrap(), &c),
            QuotaState::OverQuota100pct
        );
    }

    #[test]
    fn transition_as_str_pins_taxonomy() {
        assert_eq!(
            QuotaTransition::NoChange {
                state: QuotaState::WithinPlan
            }
            .as_str(),
            "no_change"
        );
        assert_eq!(
            QuotaTransition::TransitionedTo80pct {
                from: QuotaState::WithinPlan
            }
            .as_str(),
            "transitioned_to_80pct"
        );
        assert_eq!(
            QuotaTransition::TransitionedTo95pct {
                from: QuotaState::SoftWarning80pct
            }
            .as_str(),
            "transitioned_to_95pct"
        );
        assert_eq!(
            QuotaTransition::TransitionedTo100pct {
                from: QuotaState::SoftWarning95pct
            }
            .as_str(),
            "transitioned_to_100pct"
        );
        assert_eq!(
            QuotaTransition::Suspended {
                invoice_failures: 3
            }
            .as_str(),
            "suspended"
        );
        assert_eq!(
            QuotaTransition::Reinstated {
                new_state: QuotaState::WithinPlan
            }
            .as_str(),
            "reinstated"
        );
    }

    #[test]
    fn transition_mutated_inverse_of_no_change() {
        assert!(!QuotaTransition::NoChange {
            state: QuotaState::WithinPlan
        }
        .mutated());
        assert!(QuotaTransition::TransitionedTo80pct {
            from: QuotaState::WithinPlan
        }
        .mutated());
        assert!(QuotaTransition::TransitionedTo95pct {
            from: QuotaState::SoftWarning80pct
        }
        .mutated());
        assert!(QuotaTransition::TransitionedTo100pct {
            from: QuotaState::SoftWarning95pct
        }
        .mutated());
        assert!(QuotaTransition::Suspended {
            invoice_failures: 3
        }
        .mutated());
        assert!(QuotaTransition::Reinstated {
            new_state: QuotaState::WithinPlan
        }
        .mutated());
    }

    #[test]
    fn state_serde_round_trips() {
        for s in canonical_quota_states() {
            let json = serde_json::to_string(s).unwrap();
            let back: QuotaState = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, back);
        }
    }

    #[test]
    fn transition_serde_round_trips() {
        let v = [
            QuotaTransition::NoChange {
                state: QuotaState::WithinPlan,
            },
            QuotaTransition::TransitionedTo80pct {
                from: QuotaState::WithinPlan,
            },
            QuotaTransition::Suspended {
                invoice_failures: 3,
            },
            QuotaTransition::Reinstated {
                new_state: QuotaState::WithinPlan,
            },
        ];
        for t in v {
            let json = serde_json::to_string(&t).unwrap();
            let back: QuotaTransition = serde_json::from_str(&json).unwrap();
            assert_eq!(t, back);
        }
    }
}
