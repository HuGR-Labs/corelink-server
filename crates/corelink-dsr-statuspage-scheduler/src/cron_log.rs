//! `CronRunLog` — idempotency dedupe ledger for the daily
//! `(date_yyyymmdd, metric_id)` publish.
//!
//! Backed in production by an additive D1 table:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS cron_run_log (
//!     date_yyyymmdd INTEGER NOT NULL,
//!     metric_id     TEXT    NOT NULL,
//!     run_at_ms     INTEGER NOT NULL,
//!     status        TEXT    NOT NULL,    -- 'succeeded' | 'failed' | 'skipped'
//!     PRIMARY KEY (date_yyyymmdd, metric_id)
//! );
//! ```
//!
//! The primary-key UNIQUE constraint enforces the canonical
//! "one publish per UTC day per metric" idempotency invariant: if
//! the cron fires twice in the same UTC day (e.g. a CF Workers
//! `scheduled` event is retried by the runtime), the second `record`
//! call returns [`CronRunLogError::AlreadyRecorded`] and the
//! orchestrator short-circuits with the `Skipped /
//! already_published_today` audit + a successful return (no fail).
//!
//! Day-boundary helper [`date_yyyymmdd_utc`] is exposed publicly
//! because the scheduler audit envelope embeds it.

use std::sync::{Arc, Mutex};

use thiserror::Error;

/// Canonical recorded-run status — stored verbatim in the D1
/// `cron_run_log.status` column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum RecordedRunStatus {
    /// Publish succeeded (2xx).
    Succeeded,
    /// Publish failed (any non-success outcome).
    Failed,
    /// Publish skipped (empty 24h window — fail-OPEN-ish status that
    /// still seizes the dedupe slot for the day so retries from the
    /// CF runtime do not re-issue the same dry call).
    Skipped,
}

impl RecordedRunStatus {
    /// Canonical column-value string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Trait surface for the dedupe ledger.
pub trait CronRunLog: core::fmt::Debug + Send + Sync {
    /// Atomic test-and-set: record a `(date_yyyymmdd, metric_id)`
    /// run.
    ///
    /// # Errors
    ///
    /// - [`CronRunLogError::AlreadyRecorded`] if a row already exists
    ///   for `(date_yyyymmdd, metric_id)` — the canonical idempotency
    ///   dedupe signal (NOT a real failure; the scheduler maps this
    ///   onto a `Skipped` audit + success return).
    /// - [`CronRunLogError::Backend`] for D1 / lock failures.
    fn record(
        &self,
        date_yyyymmdd: u32,
        metric_id: &str,
        run_at_unix_ms: u64,
        status: RecordedRunStatus,
    ) -> Result<(), CronRunLogError>;

    /// Pre-flight check: returns `Ok(true)` iff a row already exists
    /// for `(date_yyyymmdd, metric_id)`. Used by the scheduler to
    /// emit the `Skipped` audit BEFORE attempting the record (so the
    /// dedupe surface is observable in the audit chain).
    ///
    /// # Errors
    ///
    /// [`CronRunLogError::Backend`] on D1 / lock failures.
    fn is_recorded(
        &self,
        date_yyyymmdd: u32,
        metric_id: &str,
    ) -> Result<bool, CronRunLogError>;
}

/// Error variants for the dedupe ledger.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CronRunLogError {
    /// A row already exists for `(date_yyyymmdd, metric_id)`. The
    /// scheduler treats this as the canonical `Skipped /
    /// already_published_today` outcome.
    #[error("cron_run_log: already recorded for ({date_yyyymmdd}, {metric_id})")]
    AlreadyRecorded {
        /// Date the existing row was recorded against.
        date_yyyymmdd: u32,
        /// Metric ID the existing row was recorded against.
        metric_id: String,
    },
    /// Backend (D1 / lock / parse) failure.
    #[error("cron_run_log: backend: {0}")]
    Backend(String),
}

/// Canonical recorded row tuple shape (`date_yyyymmdd`, `metric_id`,
/// `run_at_unix_ms`, `status`). Exposed as a type alias so the
/// `Arc<Mutex<Vec<_>>>` field on [`InMemoryCronRunLog`] stays under
/// clippy's `type_complexity` threshold.
pub type RecordedRunRow = (u32, String, u64, RecordedRunStatus);

/// In-memory dedupe ledger. The internal `Vec` is acceptable for
/// integration tests (the production ledger lives in D1 with the
/// PRIMARY KEY constraint enforced server-side).
#[derive(Clone, Debug, Default)]
pub struct InMemoryCronRunLog {
    rows: Arc<Mutex<Vec<RecordedRunRow>>>,
}

impl InMemoryCronRunLog {
    /// Construct an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of recorded rows (for assertions).
    #[must_use]
    pub fn snapshot(&self) -> Vec<RecordedRunRow> {
        match self.rows.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl CronRunLog for InMemoryCronRunLog {
    fn record(
        &self,
        date_yyyymmdd: u32,
        metric_id: &str,
        run_at_unix_ms: u64,
        status: RecordedRunStatus,
    ) -> Result<(), CronRunLogError> {
        let mut g = self
            .rows
            .lock()
            .map_err(|e| CronRunLogError::Backend(format!("mutex poisoned: {e}")))?;
        let exists = g
            .iter()
            .any(|(d, m, _, _)| *d == date_yyyymmdd && m == metric_id);
        if exists {
            return Err(CronRunLogError::AlreadyRecorded {
                date_yyyymmdd,
                metric_id: metric_id.to_owned(),
            });
        }
        g.push((date_yyyymmdd, metric_id.to_owned(), run_at_unix_ms, status));
        Ok(())
    }

    fn is_recorded(
        &self,
        date_yyyymmdd: u32,
        metric_id: &str,
    ) -> Result<bool, CronRunLogError> {
        let g = self
            .rows
            .lock()
            .map_err(|e| CronRunLogError::Backend(format!("mutex poisoned: {e}")))?;
        Ok(g.iter()
            .any(|(d, m, _, _)| *d == date_yyyymmdd && m == metric_id))
    }
}

/// Convert a Unix-second timestamp to a `YYYYMMDD` integer in UTC.
///
/// Uses a no-deps day-counter algorithm (civil-from-days, after
/// Howard Hinnant's `date.h`) so the crate stays free of `chrono` /
/// `time` dependencies (consistent with the wave-16 wasm32-clean
/// stance).
#[must_use]
pub fn date_yyyymmdd_utc(unix_s: u64) -> u32 {
    // Days since 1970-01-01.
    let days = (unix_s / 86_400) as i64;
    // Algorithm: civil_from_days (Hinnant, "date algorithms")
    // Operates on days since 1970-01-01. Returns (y, m, d).
    let z = days + 719_468; // shift epoch to 0000-03-01
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = (y + i64::from(m <= 2)) as u32;
    y * 10_000 + m * 100 + d
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
    fn record_then_record_same_day_metric_returns_already_recorded() {
        let log = InMemoryCronRunLog::new();
        log.record(20_260_515, "m1", 1_000, RecordedRunStatus::Succeeded)
            .unwrap();
        let err = log
            .record(20_260_515, "m1", 2_000, RecordedRunStatus::Succeeded)
            .unwrap_err();
        let is_dup = matches!(err, CronRunLogError::AlreadyRecorded { .. });
        assert!(is_dup);
    }

    #[test]
    fn record_distinct_days_same_metric_succeeds() {
        let log = InMemoryCronRunLog::new();
        log.record(20_260_515, "m1", 1_000, RecordedRunStatus::Succeeded)
            .unwrap();
        log.record(20_260_516, "m1", 2_000, RecordedRunStatus::Succeeded)
            .unwrap();
        assert_eq!(log.snapshot().len(), 2);
    }

    #[test]
    fn is_recorded_returns_false_then_true_after_record() {
        let log = InMemoryCronRunLog::new();
        assert!(!log.is_recorded(20_260_515, "m1").unwrap());
        log.record(20_260_515, "m1", 1_000, RecordedRunStatus::Skipped)
            .unwrap();
        assert!(log.is_recorded(20_260_515, "m1").unwrap());
    }

    #[test]
    fn date_yyyymmdd_utc_canonical_anchors() {
        // 1970-01-01 00:00:00 UTC → 19700101.
        assert_eq!(date_yyyymmdd_utc(0), 19_700_101);
        // 2026-05-15 06:00:00 UTC → 20260515.
        // unix_s = 1778824800 (Python `datetime(2026,5,15,6,
        // tzinfo=utc).timestamp()`).
        assert_eq!(date_yyyymmdd_utc(1_778_824_800), 20_260_515);
        // 2026-05-15 23:59:59 UTC stays on 20260515.
        assert_eq!(date_yyyymmdd_utc(1_778_889_599), 20_260_515);
        // 2026-05-16 00:00:00 UTC rolls to 20260516.
        assert_eq!(date_yyyymmdd_utc(1_778_889_600), 20_260_516);
    }

    #[test]
    fn recorded_run_status_label_canonical() {
        assert_eq!(RecordedRunStatus::Succeeded.as_str(), "succeeded");
        assert_eq!(RecordedRunStatus::Failed.as_str(), "failed");
        assert_eq!(RecordedRunStatus::Skipped.as_str(), "skipped");
    }
}
