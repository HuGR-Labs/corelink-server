//! Fatigue threshold + handoff decision matrix.
//!
//! Canonical per Lote 10.17 codex P1 fix: alerts alone NÃO
//! suficientes. Soft thresholds raise alerts + manager review; HARD
//! thresholds trigger automatic rotation handoff via PagerDuty API
//! plus mandatory recovery rotation skip plus audit emit
//! `corelink.oncall.fatigue_threshold_breached`.
//!
//! ## Threshold matrix
//!
//! | Counter                            | Soft (alert)         | HARD (auto-handoff)              |
//! |------------------------------------|----------------------|----------------------------------|
//! | Sev1 / shift                       | > 2 (alert + 1:1)    | > 3 (rotate-to-backup; 48h hold) |
//! | Sev2 / shift                       | > 5 (alert)          | > 8 (rotate-to-backup; 48h hold) |
//! | Pages / 30d in non-rotation        | > 10 (burnout sig.)  | > 15 (1-month rotation block)    |

/// Canonical fatigue threshold taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on WIs (e.g. dedicated
/// privacy SEV-tier forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum FatigueThreshold {
    /// Soft Sev1/shift threshold: alert + manager review (> 2 Sev1).
    SoftSev1PerShift,
    /// HARD Sev1/shift threshold: automatic rotation handoff (> 3 Sev1).
    HardSev1PerShift,
    /// Soft Sev2/shift threshold: alert (> 5 Sev2).
    SoftSev2PerShift,
    /// HARD Sev2/shift threshold: automatic rotation handoff (> 8 Sev2).
    HardSev2PerShift,
    /// Soft pages/month non-rotation: burnout signal (> 10 pages).
    SoftPagesPerMonthNonRotation,
    /// HARD pages/month non-rotation: mandatory 1-month rotation block (> 15 pages).
    HardPagesPerMonthNonRotation,
}

impl FatigueThreshold {
    /// Canonical wire label (audit + dashboard `threshold` label).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SoftSev1PerShift => "soft_sev1_per_shift",
            Self::HardSev1PerShift => "hard_sev1_per_shift",
            Self::SoftSev2PerShift => "soft_sev2_per_shift",
            Self::HardSev2PerShift => "hard_sev2_per_shift",
            Self::SoftPagesPerMonthNonRotation => "soft_pages_per_month_non_rotation",
            Self::HardPagesPerMonthNonRotation => "hard_pages_per_month_non_rotation",
        }
    }

    /// Canonical numeric trigger value (strictly-greater-than).
    #[must_use]
    pub const fn trigger_count(self) -> u64 {
        match self {
            Self::SoftSev1PerShift => 2,
            Self::HardSev1PerShift => 3,
            Self::SoftSev2PerShift => 5,
            Self::HardSev2PerShift => 8,
            Self::SoftPagesPerMonthNonRotation => 10,
            Self::HardPagesPerMonthNonRotation => 15,
        }
    }

    /// True for HARD thresholds (require automatic action; soft
    /// thresholds raise alert + manual review only).
    #[must_use]
    pub const fn is_hard(self) -> bool {
        matches!(
            self,
            Self::HardSev1PerShift | Self::HardSev2PerShift | Self::HardPagesPerMonthNonRotation
        )
    }
}

impl core::fmt::Display for FatigueThreshold {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical handoff decision arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum HandoffDecision {
    /// No threshold breached; keep the current oncall.
    KeepCurrent,
    /// Soft threshold breached: alert + manager review; oncall stays
    /// on-shift but a 1:1 is scheduled.
    SoftAlert,
    /// HARD threshold breached: automatic rotation handoff to the
    /// backup engineer; outgoing oncall enters mandatory 48h
    /// protection (no further pages).
    RotateToBackup,
    /// HARD non-rotation breach: oncall removed from rotation for 1
    /// month; manager + HR notified per company policy.
    MandatoryRotationBlock,
}

impl HandoffDecision {
    /// Canonical wire label (audit + dashboard `decision` label).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeepCurrent => "keep_current",
            Self::SoftAlert => "soft_alert",
            Self::RotateToBackup => "rotate_to_backup",
            Self::MandatoryRotationBlock => "mandatory_rotation_block",
        }
    }

    /// True if the decision requires PagerDuty schedule mutation (vs
    /// alert-only).
    #[must_use]
    pub const fn requires_schedule_mutation(self) -> bool {
        matches!(self, Self::RotateToBackup | Self::MandatoryRotationBlock)
    }
}

impl core::fmt::Display for HandoffDecision {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 4-element list of [`HandoffDecision`] for
/// surface-stability regression tests.
#[must_use]
pub const fn canonical_handoff_decisions() -> &'static [HandoffDecision; 4] {
    &[
        HandoffDecision::KeepCurrent,
        HandoffDecision::SoftAlert,
        HandoffDecision::RotateToBackup,
        HandoffDecision::MandatoryRotationBlock,
    ]
}

/// Pure-logic threshold decision matrix. Given a snapshot of the
/// canonical fatigue counters, returns the HandoffDecision per the
/// Lote 10.17 codex P1 fix matrix.
///
/// Arguments:
/// - `sev1_per_shift`: count of Sev1 pages during the current shift
/// - `sev2_per_shift`: count of Sev2 pages during the current shift
/// - `pages_per_month_non_rotation`: count of pages received in the
///   trailing 30d while the engineer was NOT in active rotation
///
/// HARD arms take priority over soft arms (e.g. a Sev1=4 page +
/// Sev2=3 pages → `RotateToBackup`, not `SoftAlert`).
#[must_use]
pub fn decide_handoff(
    sev1_per_shift: u64,
    sev2_per_shift: u64,
    pages_per_month_non_rotation: u64,
) -> HandoffDecision {
    let hard_pages = pages_per_month_non_rotation
        > FatigueThreshold::HardPagesPerMonthNonRotation.trigger_count();
    if hard_pages {
        return HandoffDecision::MandatoryRotationBlock;
    }

    let hard_sev1 = sev1_per_shift > FatigueThreshold::HardSev1PerShift.trigger_count();
    let hard_sev2 = sev2_per_shift > FatigueThreshold::HardSev2PerShift.trigger_count();
    if hard_sev1 || hard_sev2 {
        return HandoffDecision::RotateToBackup;
    }

    let soft_sev1 = sev1_per_shift > FatigueThreshold::SoftSev1PerShift.trigger_count();
    let soft_sev2 = sev2_per_shift > FatigueThreshold::SoftSev2PerShift.trigger_count();
    let soft_pages = pages_per_month_non_rotation
        > FatigueThreshold::SoftPagesPerMonthNonRotation.trigger_count();
    if soft_sev1 || soft_sev2 || soft_pages {
        return HandoffDecision::SoftAlert;
    }

    HandoffDecision::KeepCurrent
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
    fn keep_current_when_below_all_thresholds() {
        assert_eq!(decide_handoff(0, 0, 0), HandoffDecision::KeepCurrent);
        assert_eq!(decide_handoff(2, 5, 10), HandoffDecision::KeepCurrent);
    }

    #[test]
    fn soft_alert_canonical() {
        assert_eq!(decide_handoff(3, 0, 0), HandoffDecision::SoftAlert);
        assert_eq!(decide_handoff(0, 6, 0), HandoffDecision::SoftAlert);
        assert_eq!(decide_handoff(0, 0, 11), HandoffDecision::SoftAlert);
    }

    #[test]
    fn rotate_to_backup_canonical() {
        assert_eq!(decide_handoff(4, 0, 0), HandoffDecision::RotateToBackup);
        assert_eq!(decide_handoff(0, 9, 0), HandoffDecision::RotateToBackup);
    }

    #[test]
    fn mandatory_block_canonical() {
        assert_eq!(
            decide_handoff(0, 0, 16),
            HandoffDecision::MandatoryRotationBlock
        );
    }

    #[test]
    fn hard_overrides_soft() {
        // Sev1=4 hard + Sev2=6 soft → hard wins (RotateToBackup, not SoftAlert).
        assert_eq!(decide_handoff(4, 6, 0), HandoffDecision::RotateToBackup);
        // Pages=16 hard takes priority over Sev1 hard.
        assert_eq!(
            decide_handoff(4, 0, 16),
            HandoffDecision::MandatoryRotationBlock
        );
    }

    #[test]
    fn threshold_canonical_strings() {
        assert_eq!(
            FatigueThreshold::SoftSev1PerShift.as_str(),
            "soft_sev1_per_shift"
        );
        assert_eq!(
            FatigueThreshold::HardSev1PerShift.as_str(),
            "hard_sev1_per_shift"
        );
        assert!(FatigueThreshold::HardSev1PerShift.is_hard());
        assert!(!FatigueThreshold::SoftSev1PerShift.is_hard());
    }
}
