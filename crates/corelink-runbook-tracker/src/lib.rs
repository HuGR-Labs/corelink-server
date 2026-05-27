//! `corelink-runbook-tracker` — WI-S17-003.
//!
//! Operationalizes **PAT-RUNBOOK-DRILL-001** (resilience_patterns canonical
//! monthly cadence) + **FM-202** (runbook desatualizado) drift detection
//! mitigation.
//!
//! ## Contract
//!
//! Pure-logic library — no I/O, no async runtime. Host adapters (D1, CF Cron,
//! CLI) bind via the [`DrillRecorder`] trait and the standalone cadence /
//! drift helpers ([`is_overdue`], [`compute_drift`]).
//!
//! ## Domain model
//!
//! * [`RunbookId`] — newtype around `String`, validated `RB-<ID>` shape.
//! * [`DrillRecord`] — one dry-run execution: executor, timestamps, outcome,
//!   evidence URL, optional notes.
//! * [`Outcome`] — `Pass`, `Fail`, `DriftFlagged` (the last fires the
//!   FM-202 post-mortem trigger per spec §15 + Quality Standard 14.s17.2).
//!
//! ## Cadence canonical
//!
//! Per `WI-S17-003` §1 + resilience_patterns PAT-RUNBOOK-DRILL-001:
//!
//! * Monthly window = **30 days** (canonical, not calendar-month — calendar
//!   months drift on cadence math; spec contract §5.3 chose fixed 30d).
//! * `is_overdue` returns `true` when `now - last_drill_ts >= 30 days`.
//!
//! ## Drift detection canonical
//!
//! Per Quality Standard 14.s17.2 + WI-S17-003 §9.3:
//!
//! * `duration_ratio = duration_actual / duration_expected`.
//! * `ratio > 2.0` → [`Outcome::DriftFlagged`] + post-mortem trigger.
//! * `1.5x` rejected (false-positive flood); `3x` rejected (misses drift).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical 30-day cadence window in seconds (per WI-S17-003 §1 + spec
/// contract §5.3). Fixed seconds (not calendar months) — calendar arithmetic
/// drifts cadence math.
pub const CADENCE_WINDOW_SECS: i64 = 30 * 24 * 60 * 60;

/// Canonical drift threshold: `duration_actual / duration_expected > 2.0` is
/// flagged per Quality Standard 14.s17.2 + WI-S17-003 §9.3.
pub const DRIFT_THRESHOLD_RATIO: f64 = 2.0;

/// Errors produced by the tracker.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrackerError {
    /// Runbook id failed shape validation (`RB-<UPPER ALNUM/HYPHEN>+`).
    #[error("invalid runbook id: {0}")]
    InvalidRunbookId(String),
    /// Executor id was empty.
    #[error("empty executor id")]
    EmptyExecutor,
    /// Evidence URL was empty.
    #[error("empty evidence url")]
    EmptyEvidence,
    /// Duration was negative.
    #[error("negative duration: {0}")]
    NegativeDuration(i64),
    /// Expected duration was zero (would divide by zero in drift math).
    #[error("expected duration must be positive (got {0})")]
    ZeroExpectedDuration(i64),
}

/// Validated runbook identifier — `RB-<UPPER ALNUM / hyphen>+`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RunbookId(String);

impl RunbookId {
    /// Validate and wrap a string as a [`RunbookId`].
    ///
    /// Accepts `RB-<id>` where the suffix is non-empty and contains only
    /// uppercase ASCII letters, ASCII digits, or hyphens. Casing is preserved
    /// (id is stored verbatim) but validation is case-sensitive — runbook
    /// catalog ids are uppercase in spec corpus.
    pub fn new(raw: impl Into<String>) -> Result<Self, TrackerError> {
        let s: String = raw.into();
        if !s.starts_with("RB-") || s.len() < 4 {
            return Err(TrackerError::InvalidRunbookId(s));
        }
        let suffix = s.get(3..).unwrap_or("");
        if suffix.is_empty()
            || !suffix
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(TrackerError::InvalidRunbookId(s));
        }
        Ok(Self(s))
    }

    /// Borrow the wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RunbookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for RunbookId {
    type Error = TrackerError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl From<RunbookId> for String {
    fn from(r: RunbookId) -> Self {
        r.0
    }
}

/// Outcome of a dry-run execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Outcome {
    /// Drill executed cleanly within drift threshold.
    Pass,
    /// Drill failed (process broken; runbook needs urgent update).
    Fail,
    /// Drill exceeded `2x` duration ratio — FM-202 post-mortem trigger.
    DriftFlagged,
}

impl Outcome {
    /// Snake-case label for Prometheus + audit JSON.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::DriftFlagged => "drift_flagged",
        }
    }

    /// Whether this outcome fires the FM-202 post-mortem trigger.
    #[must_use]
    pub const fn triggers_post_mortem(self) -> bool {
        matches!(self, Self::DriftFlagged | Self::Fail)
    }
}

/// One drill execution record — persisted in D1 `runbook_drills`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DrillRecord {
    /// Unique drill id (caller-provided UUID v4 stringified).
    pub drill_id: String,
    /// Runbook executed.
    pub runbook_id: RunbookId,
    /// Operator id (e.g. `op_gschneiter`); zero PII per CTRL-PRIV-001.
    pub executor: String,
    /// Unix epoch seconds when execution completed.
    pub executed_at: i64,
    /// Wall-clock duration in seconds.
    pub duration_seconds: i64,
    /// Expected duration (from runbook spec).
    pub expected_seconds: i64,
    /// Outcome (computed canonical from durations + caller-asserted success).
    pub outcome: Outcome,
    /// URL to EVT-017 evidence (asciinema cast in R2 bucket).
    pub evidence_url: String,
    /// Optional free-text notes (sanitized; CTRL-PRIV-001).
    pub notes: Option<String>,
}

impl DrillRecord {
    /// Construct a validated drill record. Computes `Outcome` from
    /// `success` + drift math:
    ///
    /// * `success == false` → [`Outcome::Fail`].
    /// * `success == true && ratio > 2.0` → [`Outcome::DriftFlagged`].
    /// * `success == true && ratio <= 2.0` → [`Outcome::Pass`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        drill_id: impl Into<String>,
        runbook_id: RunbookId,
        executor: impl Into<String>,
        executed_at: i64,
        duration_seconds: i64,
        expected_seconds: i64,
        success: bool,
        evidence_url: impl Into<String>,
        notes: Option<String>,
    ) -> Result<Self, TrackerError> {
        let executor = executor.into();
        if executor.trim().is_empty() {
            return Err(TrackerError::EmptyExecutor);
        }
        let evidence_url = evidence_url.into();
        if evidence_url.trim().is_empty() {
            return Err(TrackerError::EmptyEvidence);
        }
        if duration_seconds < 0 {
            return Err(TrackerError::NegativeDuration(duration_seconds));
        }
        if expected_seconds <= 0 {
            return Err(TrackerError::ZeroExpectedDuration(expected_seconds));
        }

        let outcome = if success {
            let drift = compute_drift(duration_seconds, expected_seconds)?;
            if drift.is_drift {
                Outcome::DriftFlagged
            } else {
                Outcome::Pass
            }
        } else {
            Outcome::Fail
        };

        Ok(Self {
            drill_id: drill_id.into(),
            runbook_id,
            executor,
            executed_at,
            duration_seconds,
            expected_seconds,
            outcome,
            evidence_url,
            notes,
        })
    }
}

/// Drift computation result for emitting Prometheus `corelink_runbook_dry_run_duration_ratio`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct DriftReport {
    /// `duration_actual / duration_expected`.
    pub ratio: f64,
    /// Whether ratio > [`DRIFT_THRESHOLD_RATIO`].
    pub is_drift: bool,
}

/// Compute drift ratio + flag. Returns `Err(ZeroExpectedDuration)` if
/// `expected_seconds <= 0`.
pub fn compute_drift(
    duration_seconds: i64,
    expected_seconds: i64,
) -> Result<DriftReport, TrackerError> {
    if expected_seconds <= 0 {
        return Err(TrackerError::ZeroExpectedDuration(expected_seconds));
    }
    if duration_seconds < 0 {
        return Err(TrackerError::NegativeDuration(duration_seconds));
    }
    // i64 → f64 cast: acceptable precision loss for duration math (seconds at
    // the scale of minutes-to-hours; ratio fits exactly).
    #[allow(clippy::cast_precision_loss)]
    let ratio = (duration_seconds as f64) / (expected_seconds as f64);
    let is_drift = ratio > DRIFT_THRESHOLD_RATIO;
    Ok(DriftReport { ratio, is_drift })
}

/// Whether a runbook is overdue for its monthly cadence.
///
/// * `last_drill_ts` is the unix epoch seconds of the most recent drill.
/// * `now_ts` is the current unix epoch seconds.
/// * Returns `true` when `now_ts - last_drill_ts >= CADENCE_WINDOW_SECS`
///   (canonical: 30 days fixed).
///
/// Saturating arithmetic — if `now_ts < last_drill_ts` the result is `false`
/// (clock skew protection: never flag in the future).
#[must_use]
pub fn is_overdue(last_drill_ts: i64, now_ts: i64) -> bool {
    let elapsed = now_ts.saturating_sub(last_drill_ts);
    elapsed >= CADENCE_WINDOW_SECS
}

/// Days elapsed since last drill (floored, saturating, never negative).
#[must_use]
pub fn days_since(last_drill_ts: i64, now_ts: i64) -> i64 {
    let elapsed = now_ts.saturating_sub(last_drill_ts).max(0);
    elapsed / (24 * 60 * 60)
}

/// Trait for persistence — host adapter binds to D1 (CF Worker) or any other
/// store. Tracker library is pure; recorder is the I/O boundary.
pub trait DrillRecorder {
    /// Persistence error type (opaque to library; adapter chooses).
    type Error: std::error::Error + Send + Sync + 'static;

    /// Insert a drill record. MUST be idempotent on `drill_id` (D1 PRIMARY KEY).
    ///
    /// # Errors
    ///
    /// Returns adapter-defined errors (D1 transport, schema mismatch, etc.).
    fn record(&self, drill: &DrillRecord) -> Result<(), Self::Error>;

    /// Most-recent drill timestamp for a runbook, or `None` if never drilled.
    ///
    /// # Errors
    ///
    /// Returns adapter-defined errors.
    fn last_drill(&self, runbook_id: &RunbookId) -> Result<Option<i64>, Self::Error>;
}

/// Result of the monthly overdue scan — emitted as a CF Cron alert event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OverdueAlert {
    /// Runbook that is overdue.
    pub runbook_id: RunbookId,
    /// Days since last drill (or [`None`] if never drilled).
    pub days_overdue: Option<i64>,
    /// Last drill timestamp (or [`None`] if never drilled).
    pub last_drill_ts: Option<i64>,
}

/// Scan a catalog of P0/P1 runbooks against the recorder and return overdue
/// alerts (≥ 30 days since last drill, or never drilled).
///
/// # Errors
///
/// Propagates the recorder's error type on lookup failure.
pub fn scan_overdue<R: DrillRecorder>(
    recorder: &R,
    catalog: &[RunbookId],
    now_ts: i64,
) -> Result<Vec<OverdueAlert>, R::Error> {
    let mut alerts = Vec::new();
    for rb in catalog {
        let last = recorder.last_drill(rb)?;
        match last {
            None => alerts.push(OverdueAlert {
                runbook_id: rb.clone(),
                days_overdue: None,
                last_drill_ts: None,
            }),
            Some(ts) if is_overdue(ts, now_ts) => alerts.push(OverdueAlert {
                runbook_id: rb.clone(),
                days_overdue: Some(days_since(ts, now_ts)),
                last_drill_ts: Some(ts),
            }),
            Some(_) => {}
        }
    }
    Ok(alerts)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[test]
    fn runbook_id_accepts_canonical_shape() {
        let id = RunbookId::new("RB-FM-202").expect("valid id");
        assert_eq!(id.as_str(), "RB-FM-202");
    }

    #[test]
    fn runbook_id_rejects_lowercase_suffix() {
        assert!(matches!(
            RunbookId::new("RB-fm-202"),
            Err(TrackerError::InvalidRunbookId(_))
        ));
    }

    #[test]
    fn runbook_id_rejects_missing_prefix() {
        assert!(matches!(
            RunbookId::new("FM-202"),
            Err(TrackerError::InvalidRunbookId(_))
        ));
    }

    #[test]
    fn runbook_id_rejects_empty_suffix() {
        assert!(matches!(
            RunbookId::new("RB-"),
            Err(TrackerError::InvalidRunbookId(_))
        ));
    }

    #[test]
    fn drill_record_pass_within_threshold() {
        let rb = RunbookId::new("RB-FM-051").expect("valid");
        let rec = DrillRecord::new(
            "drill-1",
            rb,
            "op_gschneiter",
            10_000,
            720, // 12 min
            600, // 10 min expected → ratio 1.2
            true,
            "https://r2.example/evidence-runbooks/cast-1.cast",
            None,
        )
        .expect("record builds");
        assert_eq!(rec.outcome, Outcome::Pass);
    }

    #[test]
    fn drill_record_drift_flagged_above_2x() {
        let rb = RunbookId::new("RB-FM-202").expect("valid");
        let rec = DrillRecord::new(
            "drill-2",
            rb,
            "op_gschneiter",
            10_000,
            2_100, // 35 min
            900,   // 15 min expected → ratio 2.33
            true,
            "https://r2.example/cast-2.cast",
            Some("meta-process documentation outdated".into()),
        )
        .expect("record builds");
        assert_eq!(rec.outcome, Outcome::DriftFlagged);
        assert!(rec.outcome.triggers_post_mortem());
    }

    #[test]
    fn drill_record_fail_overrides_drift() {
        let rb = RunbookId::new("RB-FM-057").expect("valid");
        let rec = DrillRecord::new(
            "drill-3",
            rb,
            "op_oncall",
            10_000,
            1_800,
            900,
            false, // explicit fail wins
            "https://r2.example/cast-3.cast",
            None,
        )
        .expect("record builds");
        assert_eq!(rec.outcome, Outcome::Fail);
    }

    #[test]
    fn drill_record_rejects_empty_executor() {
        let rb = RunbookId::new("RB-FM-051").expect("valid");
        assert_eq!(
            DrillRecord::new(
                "drill-x",
                rb,
                "  ",
                10_000,
                600,
                600,
                true,
                "url",
                None,
            ),
            Err(TrackerError::EmptyExecutor)
        );
    }

    #[test]
    fn drill_record_rejects_empty_evidence() {
        let rb = RunbookId::new("RB-FM-051").expect("valid");
        assert_eq!(
            DrillRecord::new(
                "drill-x",
                rb,
                "op_a",
                10_000,
                600,
                600,
                true,
                "",
                None,
            ),
            Err(TrackerError::EmptyEvidence)
        );
    }

    #[test]
    fn compute_drift_exactly_2x_is_not_flagged() {
        // Canonical: ratio > 2.0 (strict). Exactly 2.0 → no flag (matches
        // WI-S17-003 §9.3 — `> 2.0` not `>= 2.0`).
        let r = compute_drift(1_200, 600).expect("ok");
        assert!((r.ratio - 2.0).abs() < f64::EPSILON);
        assert!(!r.is_drift);
    }

    #[test]
    fn compute_drift_just_above_2x_is_flagged() {
        let r = compute_drift(1_201, 600).expect("ok");
        assert!(r.is_drift);
    }

    #[test]
    fn compute_drift_rejects_zero_expected() {
        assert!(matches!(
            compute_drift(100, 0),
            Err(TrackerError::ZeroExpectedDuration(0))
        ));
    }

    #[test]
    fn is_overdue_at_exactly_30_days() {
        let ts = 1_000_000;
        let now = ts + CADENCE_WINDOW_SECS;
        assert!(is_overdue(ts, now));
    }

    #[test]
    fn is_overdue_just_under_30_days() {
        let ts = 1_000_000;
        let now = ts + CADENCE_WINDOW_SECS - 1;
        assert!(!is_overdue(ts, now));
    }

    #[test]
    fn is_overdue_clock_skew_safe() {
        // Negative elapsed (now < ts) MUST NOT flag.
        assert!(!is_overdue(2_000_000, 1_000_000));
    }

    #[test]
    fn outcome_post_mortem_triggers_canonical() {
        assert!(Outcome::DriftFlagged.triggers_post_mortem());
        assert!(Outcome::Fail.triggers_post_mortem());
        assert!(!Outcome::Pass.triggers_post_mortem());
    }

    // --- DrillRecorder fixture for scan_overdue ---

    #[derive(Debug, Default)]
    struct FakeRecorder {
        last: RefCell<HashMap<String, i64>>,
    }

    #[derive(Debug, Error)]
    enum FakeErr {}

    impl DrillRecorder for FakeRecorder {
        type Error = FakeErr;
        fn record(&self, drill: &DrillRecord) -> Result<(), Self::Error> {
            self.last
                .borrow_mut()
                .insert(drill.runbook_id.as_str().to_owned(), drill.executed_at);
            Ok(())
        }
        fn last_drill(&self, runbook_id: &RunbookId) -> Result<Option<i64>, Self::Error> {
            Ok(self.last.borrow().get(runbook_id.as_str()).copied())
        }
    }

    #[test]
    fn scan_overdue_flags_never_drilled_and_stale() {
        let rec = FakeRecorder::default();
        let fresh = RunbookId::new("RB-FM-051").expect("v");
        let stale = RunbookId::new("RB-FM-057").expect("v");
        let never = RunbookId::new("RB-FM-202").expect("v");
        let now = 100_000_000;
        rec.record(
            &DrillRecord::new(
                "d1",
                fresh.clone(),
                "op",
                now - 60, // 1 min ago
                600,
                600,
                true,
                "url",
                None,
            )
            .expect("ok"),
        )
        .expect("rec");
        rec.record(
            &DrillRecord::new(
                "d2",
                stale.clone(),
                "op",
                now - (CADENCE_WINDOW_SECS + 86_400), // 31 days ago
                600,
                600,
                true,
                "url",
                None,
            )
            .expect("ok"),
        )
        .expect("rec");

        let catalog = vec![fresh.clone(), stale.clone(), never.clone()];
        let alerts = scan_overdue(&rec, &catalog, now).expect("scan");
        assert_eq!(alerts.len(), 2);
        let ids: Vec<_> = alerts.iter().map(|a| a.runbook_id.as_str()).collect();
        assert!(ids.contains(&"RB-FM-057"));
        assert!(ids.contains(&"RB-FM-202"));
        // never-drilled has days_overdue == None
        let never_alert = alerts
            .iter()
            .find(|a| a.runbook_id == never)
            .expect("never present");
        assert!(never_alert.days_overdue.is_none());
    }

    // --- proptest: cadence + drift math invariants ---

    use proptest::prelude::*;

    /// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 256
    /// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
    fn proptest_cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(256)
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: proptest_cases(), .. ProptestConfig::default()
        })]

        #[test]
        fn prop_is_overdue_monotone_in_elapsed(
            base in 0_i64..1_000_000_000_i64,
            elapsed_a in 0_i64..(CADENCE_WINDOW_SECS * 4),
            elapsed_b in 0_i64..(CADENCE_WINDOW_SECS * 4),
        ) {
            // Monotonicity: if elapsed_a <= elapsed_b and elapsed_a overdue,
            // then elapsed_b is also overdue.
            let (a, b) = if elapsed_a <= elapsed_b { (elapsed_a, elapsed_b) } else { (elapsed_b, elapsed_a) };
            if is_overdue(base, base + a) {
                prop_assert!(is_overdue(base, base + b));
            }
        }

        #[test]
        fn prop_drift_ratio_monotone(
            expected in 1_i64..10_000_i64,
            actual_a in 0_i64..100_000_i64,
            actual_b in 0_i64..100_000_i64,
        ) {
            let (lo, hi) = if actual_a <= actual_b { (actual_a, actual_b) } else { (actual_b, actual_a) };
            let r_lo = compute_drift(lo, expected).expect("ok");
            let r_hi = compute_drift(hi, expected).expect("ok");
            prop_assert!(r_hi.ratio >= r_lo.ratio);
            // If lo is drift, hi must also be drift (monotone in ratio).
            if r_lo.is_drift { prop_assert!(r_hi.is_drift); }
        }

        #[test]
        fn prop_drift_flag_threshold_exact(
            expected in 1_i64..10_000_i64,
        ) {
            // At ratio == 2.0 exactly (actual == 2*expected) → NOT flagged
            // per canonical strict `> 2.0`.
            let r = compute_drift(expected * 2, expected).expect("ok");
            prop_assert!(!r.is_drift);
        }

        #[test]
        fn prop_days_since_never_negative(
            last in 0_i64..1_000_000_000_i64,
            now in 0_i64..1_000_000_000_i64,
        ) {
            prop_assert!(days_since(last, now) >= 0);
        }

        #[test]
        fn prop_drill_record_outcome_matches_drift(
            duration in 1_i64..100_000_i64,
            expected in 1_i64..10_000_i64,
        ) {
            let rb = RunbookId::new("RB-FM-202").expect("v");
            let rec = DrillRecord::new(
                "d", rb, "op", 1_000, duration, expected, true, "url", None,
            ).expect("ok");
            let drift = compute_drift(duration, expected).expect("ok");
            if drift.is_drift {
                prop_assert_eq!(rec.outcome, Outcome::DriftFlagged);
            } else {
                prop_assert_eq!(rec.outcome, Outcome::Pass);
            }
        }
    }
}
