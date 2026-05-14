//! Ack outcome + MTTA newtype + canonical budgets.

/// Canonical 5-minute MTTA budget for synthetic SEV-2 drills, in
/// milliseconds. Per WI §2.1 + spec contract §10.s20.8 +
/// RB-INCIDENT-ESCALATION-MATRIX P0 row T+5.
pub const MTTA_BUDGET_MS: i64 = 5 * 60 * 1_000;

/// Canonical 15-minute hard unack window, in milliseconds. After this
/// the drill is classified as [`AckOutcome::Unacked`] regardless of
/// any later ack. Per RB-INCIDENT-ESCALATION-MATRIX P0 row T+15.
pub const UNACK_HARD_WINDOW_MS: i64 = 15 * 60 * 1_000;

/// Mean-time-to-ack measurement, in milliseconds. Newtype to keep the
/// dashboard / D1 schema units explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MttaMs(i64);

impl MttaMs {
    /// Construct from `ack_ts_ms - emit_ts_ms`. Saturating: never
    /// negative.
    #[must_use]
    pub fn from_diff(emit_ts_ms: i64, ack_ts_ms: i64) -> Self {
        let diff = ack_ts_ms.saturating_sub(emit_ts_ms).max(0);
        Self(diff)
    }

    /// Borrow the raw millisecond count.
    #[must_use]
    pub const fn as_ms(self) -> i64 {
        self.0
    }

    /// Whether the MTTA is within the 5-minute budget.
    #[must_use]
    pub const fn within_budget(self) -> bool {
        self.0 <= MTTA_BUDGET_MS
    }
}

/// Canonical 3-outcome taxonomy for a synthetic drill ack. The
/// `#[non_exhaustive]` marker reserves growth for follow-on WIs (e.g.
/// `AckOutcome::SilencedByMaintenance` if maintenance windows are
/// added forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AckOutcome {
    /// Acked within the 5-minute MTTA budget — GREEN dashboard tile.
    Acked,
    /// Acked AFTER the 5-min budget but BEFORE the 15-min hard
    /// window — AMBER dashboard tile (counts as escalation event).
    Escalated,
    /// No ack received within the 15-min hard window — RED dashboard
    /// tile + RB-SYNTHETIC-PAGE-DRILL escalation steps fire.
    Unacked,
}

impl AckOutcome {
    /// Canonical wire slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Acked => "acked",
            Self::Escalated => "escalated",
            Self::Unacked => "unacked",
        }
    }

    /// Whether the outcome counts as a SLO-met drill.
    #[must_use]
    pub const fn is_green(self) -> bool {
        matches!(self, Self::Acked)
    }
}

impl core::fmt::Display for AckOutcome {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
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
    fn mtta_diff_canonical() {
        let m = MttaMs::from_diff(1_000, 4_000);
        assert_eq!(m.as_ms(), 3_000);
    }

    #[test]
    fn mtta_diff_saturates_at_zero() {
        let m = MttaMs::from_diff(5_000, 1_000);
        assert_eq!(m.as_ms(), 0);
    }

    #[test]
    fn within_budget_boundary() {
        assert!(MttaMs(MTTA_BUDGET_MS).within_budget());
        assert!(!MttaMs(MTTA_BUDGET_MS + 1).within_budget());
    }

    #[test]
    fn ack_outcome_labels() {
        assert_eq!(AckOutcome::Acked.as_str(), "acked");
        assert_eq!(AckOutcome::Escalated.as_str(), "escalated");
        assert_eq!(AckOutcome::Unacked.as_str(), "unacked");
    }

    #[test]
    fn only_acked_is_green() {
        assert!(AckOutcome::Acked.is_green());
        assert!(!AckOutcome::Escalated.is_green());
        assert!(!AckOutcome::Unacked.is_green());
    }
}
