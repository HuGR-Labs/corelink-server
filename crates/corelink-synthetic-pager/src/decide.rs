//! Pure decision tree mapping (emit_ts, ack_ts?) → [`AckOutcome`].

use crate::error::SyntheticDrillError;
use crate::outcome::{AckOutcome, MttaMs, MTTA_BUDGET_MS, UNACK_HARD_WINDOW_MS};

/// Pure decision tree per WI-S20-006 §2.1 + RB-INCIDENT-ESCALATION-
/// MATRIX P0 row.
///
/// Inputs:
/// * `emit_ts_ms` — when the synthetic page was dispatched.
/// * `ack_ts_ms` — `Some(ts)` if the on-call acked within the
///   observation window; `None` if the drill window closed without
///   ack.
/// * `now_ms` — the current wall-clock at decision time. The decider
///   refuses to classify `Unacked` until `now_ms >= emit_ts_ms +
///   UNACK_HARD_WINDOW_MS` — before that point an absent ack is
///   ambiguous (the engineer may still ack).
///
/// Returns `(outcome, mtta)`:
///
/// * `mtta_ms` is `Some(diff)` iff an ack was observed; `None` if the
///   drill closed without ack.
///
/// # Errors
///
/// * [`SyntheticDrillError::AckBeforeEmit`] if `ack_ts_ms < emit_ts_ms`.
/// * [`SyntheticDrillError::EmitTimestampInFuture`] if `emit_ts_ms >
///   now_ms`.
/// * [`SyntheticDrillError::Internal`] if `now_ms < emit_ts_ms +
///   UNACK_HARD_WINDOW_MS` AND `ack_ts_ms` is `None` (caller asked for
///   premature classification — bug surface).
pub fn decide_drill_outcome(
    emit_ts_ms: i64,
    ack_ts_ms: Option<i64>,
    now_ms: i64,
) -> Result<(AckOutcome, Option<MttaMs>), SyntheticDrillError> {
    if emit_ts_ms > now_ms {
        return Err(SyntheticDrillError::EmitTimestampInFuture {
            emit_ts_ms,
            now_ms,
        });
    }

    match ack_ts_ms {
        Some(ts) if ts < emit_ts_ms => Err(SyntheticDrillError::AckBeforeEmit {
            emit_ts_ms,
            ack_ts_ms: ts,
        }),
        Some(ts) => {
            let mtta = MttaMs::from_diff(emit_ts_ms, ts);
            let outcome = classify_acked(mtta);
            Ok((outcome, Some(mtta)))
        }
        None => {
            let elapsed = now_ms.saturating_sub(emit_ts_ms);
            if elapsed < UNACK_HARD_WINDOW_MS {
                Err(SyntheticDrillError::Internal(format!(
                    "premature classification: only {elapsed} ms elapsed; need >= {UNACK_HARD_WINDOW_MS} ms"
                )))
            } else {
                Ok((AckOutcome::Unacked, None))
            }
        }
    }
}

/// Pure classifier for an acked drill — extracted for proptest reuse.
#[must_use]
pub fn classify_acked(mtta: MttaMs) -> AckOutcome {
    if mtta.as_ms() <= MTTA_BUDGET_MS {
        AckOutcome::Acked
    } else if mtta.as_ms() <= UNACK_HARD_WINDOW_MS {
        AckOutcome::Escalated
    } else {
        // Acked AFTER the hard window — classified as Escalated still
        // because we DID get an ack, but the runbook escalation will
        // have already kicked in. Tracks via the dashboard ack_vector
        // column with a `late_ack` flag.
        AckOutcome::Escalated
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
    fn acked_within_budget_is_green() {
        let (o, m) = decide_drill_outcome(1_000, Some(1_000 + 60_000), 1_000 + 60_001).unwrap();
        assert_eq!(o, AckOutcome::Acked);
        assert_eq!(m.unwrap().as_ms(), 60_000);
    }

    #[test]
    fn acked_over_budget_is_escalated() {
        let over = MTTA_BUDGET_MS + 1;
        let (o, m) = decide_drill_outcome(1_000, Some(1_000 + over), 1_000 + over + 1).unwrap();
        assert_eq!(o, AckOutcome::Escalated);
        assert_eq!(m.unwrap().as_ms(), over);
    }

    #[test]
    fn no_ack_after_hard_window_is_unacked() {
        let (o, m) = decide_drill_outcome(0, None, UNACK_HARD_WINDOW_MS + 1).unwrap();
        assert_eq!(o, AckOutcome::Unacked);
        assert!(m.is_none());
    }

    #[test]
    fn premature_no_ack_classification_is_internal_error() {
        let r = decide_drill_outcome(0, None, UNACK_HARD_WINDOW_MS - 1);
        assert!(matches!(r, Err(SyntheticDrillError::Internal(_))));
    }

    #[test]
    fn ack_before_emit_rejected() {
        let r = decide_drill_outcome(1_000, Some(500), 2_000);
        assert!(matches!(r, Err(SyntheticDrillError::AckBeforeEmit { .. })));
    }

    #[test]
    fn emit_in_future_rejected() {
        let r = decide_drill_outcome(2_000, Some(2_500), 1_000);
        assert!(matches!(
            r,
            Err(SyntheticDrillError::EmitTimestampInFuture { .. })
        ));
    }

    #[test]
    fn budget_boundary_inclusive() {
        let (o, _) = decide_drill_outcome(0, Some(MTTA_BUDGET_MS), MTTA_BUDGET_MS + 1).unwrap();
        assert_eq!(o, AckOutcome::Acked);

        let (o2, _) =
            decide_drill_outcome(0, Some(MTTA_BUDGET_MS + 1), MTTA_BUDGET_MS + 2).unwrap();
        assert_eq!(o2, AckOutcome::Escalated);
    }
}
