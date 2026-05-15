//! Canonical scheduler composing the wave-16 publish layers into a
//! single 24h-cron-firable orchestration.
//!
//! # Composition
//!
//! ```text
//!  D1RowSource::fetch_window         ───┐
//!                                       │
//!  aggregate_24h_window (wave-16)    ───┤
//!                                       │
//!  bridge_to_report (wave-16)        ───┼──► StatuspageBackend::publish_dsr_metric
//!                                       │
//!  CronRunLog dedupe                 ───┘
//! ```
//!
//! Every transition is fail-CLOSED via [`crate::SchedulerAuditSink`]
//! emitting one of the four canonical event types (`scheduled` →
//! `succeeded` / `failed` / `skipped`).
//!
//! # Cron expression
//!
//! [`CRON_EXPRESSION`] is the canonical cron tick (`0 6 * * *` —
//! 06:00 UTC daily, scheduled AFTER the audit-chain daily-verify at
//! 02:00 UTC and BEFORE SF business start). The CF Worker
//! `wrangler.toml` `[triggers]` table pins this string verbatim.

use std::sync::Arc;

use corelink_privacy_erasure_worker::aggregate_24h_window;
use corelink_statuspage_real::{
    bridge_to_report, DsrCompletionReportError, StatuspageBackend, StatuspageClientError,
};
use thiserror::Error;

use crate::audit::{
    SchedulerAuditError, SchedulerAuditEvent, SchedulerAuditOutcome, SchedulerAuditSink, SkipReason,
};
use crate::cron_log::{date_yyyymmdd_utc, CronRunLog, CronRunLogError, RecordedRunStatus};
use crate::row_source::{D1RowSource, D1RowSourceError};

/// Canonical cron expression — 06:00 UTC daily.
///
/// Wired verbatim in `crates/corelink-clerk-cf/wrangler.toml`
/// `[triggers]` table.
pub const CRON_EXPRESSION: &str = "0 6 * * *";

/// Canonical publish-window length (seconds — 24h exactly).
pub const PUBLISH_WINDOW_SECONDS: u64 = 86_400;

/// Run outcome — what the cron tick concluded with.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunOutcome {
    /// Publish succeeded (2xx). The canonical `succeeded` audit has
    /// already been emitted at this point.
    Published {
        /// p95 hours that left the system.
        p95_hours_observed: u64,
        /// Number of rows aggregated.
        rows_aggregated: u64,
        /// Statuspage HTTP status code (always 2xx on this arm).
        statuspage_status: u16,
        /// Attempts taken (1 = first-shot).
        attempts: u32,
    },
    /// Publish skipped by canonical guard. The `skipped` audit has
    /// already been emitted.
    Skipped {
        /// Skip reason.
        reason: SkipReason,
    },
}

/// Canonical scheduler error taxonomy.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SchedulerError {
    /// D1 row source failure (read / parse).
    #[error("scheduler: d1 row source: {0}")]
    RowSource(#[from] D1RowSourceError),
    /// Aggregator bridge rejected the produced aggregate (clock-skew
    /// guard — `DsrCompletionReportError`).
    #[error("scheduler: aggregate bridge: {0}")]
    Bridge(#[from] DsrCompletionReportError),
    /// Statuspage client failure (auth / rate-limit / transport / 4xx).
    #[error("scheduler: statuspage publish: {0}")]
    Publish(#[from] StatuspageClientError),
    /// Audit sink rejected (fail-CLOSED — caller MUST treat the tick
    /// as failed).
    #[error("scheduler: audit: {0}")]
    Audit(#[from] SchedulerAuditError),
    /// Cron-run-log backend failure (parse / lock / D1). NOTE:
    /// `AlreadyRecorded` is NOT propagated as an error variant — the
    /// scheduler maps it onto `RunOutcome::Skipped /
    /// AlreadyPublishedToday`.
    #[error("scheduler: cron run log backend: {0}")]
    CronLog(String),
}

impl From<CronRunLogError> for SchedulerError {
    fn from(value: CronRunLogError) -> Self {
        match value {
            // `AlreadyRecorded` is handled before it reaches this
            // conversion; if it ever does, we still convert it to a
            // best-effort `CronLog` error to surface the anomaly.
            CronRunLogError::AlreadyRecorded { .. } => Self::CronLog(value.to_string()),
            CronRunLogError::Backend(msg) => Self::CronLog(msg),
        }
    }
}

/// Canonical scheduler. Construct once at CF Worker boot; call
/// [`Self::run_once`] from the `#[event(scheduled)]` handler.
///
/// Trait surfaces are stored as `Arc<dyn …>` so the CF Worker boot
/// path can swap real D1 / real audit-chain / real Statuspage HTTP
/// client with no test-time changes.
pub struct DsrStatuspagePublishScheduler {
    row_source: Arc<dyn D1RowSource>,
    cron_log: Arc<dyn CronRunLog>,
    backend: Arc<dyn StatuspageBackend>,
    audit: Arc<dyn SchedulerAuditSink>,
    page_id: String,
    metric_id: String,
}

impl core::fmt::Debug for DsrStatuspagePublishScheduler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DsrStatuspagePublishScheduler")
            .field("page_id", &self.page_id)
            .field("metric_id", &self.metric_id)
            .finish_non_exhaustive()
    }
}

impl DsrStatuspagePublishScheduler {
    /// Construct with the four trait-bound collaborators + the
    /// page/metric identifiers sourced from the CF Worker
    /// `STATUSPAGE_PAGE_ID` + `STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS`
    /// bindings.
    #[must_use]
    pub fn new(
        row_source: Arc<dyn D1RowSource>,
        cron_log: Arc<dyn CronRunLog>,
        backend: Arc<dyn StatuspageBackend>,
        audit: Arc<dyn SchedulerAuditSink>,
        page_id: impl Into<String>,
        metric_id: impl Into<String>,
    ) -> Self {
        Self {
            row_source,
            cron_log,
            backend,
            audit,
            page_id: page_id.into(),
            metric_id: metric_id.into(),
        }
    }

    /// Run a single 24h publish tick.
    ///
    /// `now_unix_s` is the canonical "now" supplied by the cron
    /// trigger (CF Workers `scheduled` event handler computes this
    /// via `Date.now()` truncated to seconds). The 24h aggregation
    /// window is `[now_unix_s - 86_400, now_unix_s)`.
    ///
    /// # Errors
    ///
    /// - [`SchedulerError::RowSource`] on D1 read failure.
    /// - [`SchedulerError::Bridge`] on aggregator bridge rejection.
    /// - [`SchedulerError::Publish`] on Statuspage publish failure.
    /// - [`SchedulerError::Audit`] on audit emit failure (fail-CLOSED).
    /// - [`SchedulerError::CronLog`] on dedupe ledger backend failure.
    pub fn run_once(&self, now_unix_s: u64) -> Result<RunOutcome, SchedulerError> {
        let date_yyyymmdd = date_yyyymmdd_utc(now_unix_s);
        let now_unix_ms = now_unix_s.saturating_mul(1_000);
        let window_start_unix_s = now_unix_s.saturating_sub(PUBLISH_WINDOW_SECONDS);

        // 1. Emit `scheduled` audit BEFORE any read or publish.
        self.audit.emit(&SchedulerAuditEvent {
            outcome: SchedulerAuditOutcome::Scheduled,
            page_id: self.page_id.clone(),
            metric_id: self.metric_id.clone(),
            date_yyyymmdd,
            rows_read: None,
            p95_hours_observed: None,
            skip_reason: None,
            reason: None,
        })?;

        // 2. Dedupe pre-flight check — if the canonical
        // `(date, metric_id)` slot is already taken, short-circuit
        // with `Skipped / already_published_today`.
        if self
            .cron_log
            .is_recorded(date_yyyymmdd, &self.metric_id)?
        {
            self.audit.emit(&SchedulerAuditEvent {
                outcome: SchedulerAuditOutcome::Skipped,
                page_id: self.page_id.clone(),
                metric_id: self.metric_id.clone(),
                date_yyyymmdd,
                rows_read: None,
                p95_hours_observed: None,
                skip_reason: Some(SkipReason::AlreadyPublishedToday),
                reason: Some("cron_run_log dedupe slot already populated".to_owned()),
            })?;
            return Ok(RunOutcome::Skipped {
                reason: SkipReason::AlreadyPublishedToday,
            });
        }

        // 3. Read 24h slice of VerificationOutcome rows.
        let rows = match self.row_source.fetch_window(window_start_unix_s) {
            Ok(r) => r,
            Err(e) => {
                let reason = format!("d1 read: {e}");
                self.audit.emit(&SchedulerAuditEvent {
                    outcome: SchedulerAuditOutcome::Failed,
                    page_id: self.page_id.clone(),
                    metric_id: self.metric_id.clone(),
                    date_yyyymmdd,
                    rows_read: None,
                    p95_hours_observed: None,
                    skip_reason: None,
                    reason: Some(reason),
                })?;
                return Err(SchedulerError::RowSource(e));
            }
        };
        let rows_read = rows.len() as u64;

        // 4. Empty-window short-circuit — emit `Skipped / empty_window`
        // + seize the dedupe slot so retries from the CF runtime do
        // not re-issue the same dry call.
        if rows.is_empty() {
            self.audit.emit(&SchedulerAuditEvent {
                outcome: SchedulerAuditOutcome::Skipped,
                page_id: self.page_id.clone(),
                metric_id: self.metric_id.clone(),
                date_yyyymmdd,
                rows_read: Some(0),
                p95_hours_observed: None,
                skip_reason: Some(SkipReason::EmptyWindow),
                reason: Some("zero VerificationOutcome rows in 24h window".to_owned()),
            })?;
            // Record the skip in the ledger. If the ledger fails
            // (NOT `AlreadyRecorded` — that race already returned
            // above), surface as a hard failure.
            if let Err(e) = self.cron_log.record(
                date_yyyymmdd,
                &self.metric_id,
                now_unix_ms,
                RecordedRunStatus::Skipped,
            ) {
                if !matches!(e, CronRunLogError::AlreadyRecorded { .. }) {
                    return Err(e.into());
                }
            }
            return Ok(RunOutcome::Skipped {
                reason: SkipReason::EmptyWindow,
            });
        }

        // 5. Aggregate + bridge.
        let stats = aggregate_24h_window(&rows, window_start_unix_s);
        let report = match bridge_to_report(&stats) {
            Ok(r) => r,
            Err(e) => {
                let reason = format!("bridge: {e}");
                self.audit.emit(&SchedulerAuditEvent {
                    outcome: SchedulerAuditOutcome::Failed,
                    page_id: self.page_id.clone(),
                    metric_id: self.metric_id.clone(),
                    date_yyyymmdd,
                    rows_read: Some(rows_read),
                    p95_hours_observed: None,
                    skip_reason: None,
                    reason: Some(reason),
                })?;
                return Err(SchedulerError::Bridge(e));
            }
        };
        let p95_hours_observed = report.p95_resolution_hours;

        // 6. Publish (fail-CLOSED on any non-success outcome).
        let publish_result = self
            .backend
            .publish_dsr_metric(&report, now_unix_ms);
        match publish_result {
            Ok(outcome) => {
                self.audit.emit(&SchedulerAuditEvent {
                    outcome: SchedulerAuditOutcome::Succeeded,
                    page_id: self.page_id.clone(),
                    metric_id: self.metric_id.clone(),
                    date_yyyymmdd,
                    rows_read: Some(rows_read),
                    p95_hours_observed: Some(p95_hours_observed),
                    skip_reason: None,
                    reason: None,
                })?;
                // Seize the dedupe slot. A racing duplicate becomes
                // `AlreadyRecorded`; the publish has already succeeded
                // upstream, so we mask the race onto a successful
                // return (the second cron tick saw the slot taken on
                // its own dedupe check anyway — getting here on a
                // race means two concurrent ticks each thought the
                // slot was free; the audit chain pins both side's
                // intent).
                if let Err(e) = self.cron_log.record(
                    date_yyyymmdd,
                    &self.metric_id,
                    now_unix_ms,
                    RecordedRunStatus::Succeeded,
                ) {
                    if !matches!(e, CronRunLogError::AlreadyRecorded { .. }) {
                        return Err(e.into());
                    }
                }
                Ok(RunOutcome::Published {
                    p95_hours_observed,
                    rows_aggregated: rows_read,
                    statuspage_status: outcome.status,
                    attempts: outcome.attempts,
                })
            }
            Err(e) => {
                let reason = format!("publish: {e}");
                self.audit.emit(&SchedulerAuditEvent {
                    outcome: SchedulerAuditOutcome::Failed,
                    page_id: self.page_id.clone(),
                    metric_id: self.metric_id.clone(),
                    date_yyyymmdd,
                    rows_read: Some(rows_read),
                    p95_hours_observed: Some(p95_hours_observed),
                    skip_reason: None,
                    reason: Some(reason),
                })?;
                // Even on failure, seize the dedupe slot with a
                // `Failed` status. This is the canonical "fail-once-
                // per-day" policy: the failure is loud (audit-chain
                // emit + caller-visible Err), but the cron runtime
                // does NOT retry the same publish multiple times in
                // the same UTC day. The next cron tick (next day)
                // will see a free slot for the new date and try
                // again. A best-effort record-failure here does NOT
                // mask the upstream Err.
                let _ = self.cron_log.record(
                    date_yyyymmdd,
                    &self.metric_id,
                    now_unix_ms,
                    RecordedRunStatus::Failed,
                );
                Err(SchedulerError::Publish(e))
            }
        }
    }

    /// Borrow the configured page ID (for boot-time logging).
    #[must_use]
    pub fn page_id(&self) -> &str {
        &self.page_id
    }

    /// Borrow the configured metric ID (for boot-time logging).
    #[must_use]
    pub fn metric_id(&self) -> &str {
        &self.metric_id
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
    use crate::audit::InMemorySchedulerAuditSink;
    use crate::cron_log::InMemoryCronRunLog;
    use crate::row_source::InMemoryD1RowSource;
    use corelink_statuspage_real::InMemoryStatuspageBackend;

    #[test]
    fn canonical_constants_pinned() {
        assert_eq!(CRON_EXPRESSION, "0 6 * * *");
        assert_eq!(PUBLISH_WINDOW_SECONDS, 86_400);
    }

    #[test]
    fn empty_window_emits_skipped_and_records_skip_in_ledger() {
        let row_source: Arc<dyn D1RowSource> = Arc::new(InMemoryD1RowSource::new());
        let cron_log = Arc::new(InMemoryCronRunLog::new());
        let backend: Arc<dyn StatuspageBackend> = Arc::new(InMemoryStatuspageBackend::new(
            "page-1",
            "metric-1",
            "abcd1234",
            Arc::new(corelink_statuspage_real::InMemoryStatuspageAuditSink::new()),
        ));
        let audit = Arc::new(InMemorySchedulerAuditSink::new());
        let sched = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log.clone(),
            backend,
            audit.clone(),
            "page-1",
            "metric-1",
        );
        let outcome = sched.run_once(1_778_824_800).unwrap();
        let is_skip = matches!(
            outcome,
            RunOutcome::Skipped {
                reason: SkipReason::EmptyWindow
            }
        );
        assert!(is_skip);
        let snap = audit.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].outcome, SchedulerAuditOutcome::Scheduled);
        assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Skipped);
        assert_eq!(snap[1].skip_reason, Some(SkipReason::EmptyWindow));
        // Ledger row inserted.
        assert_eq!(cron_log.snapshot().len(), 1);
    }

    #[test]
    fn already_recorded_short_circuits_with_already_published_today() {
        let row_source: Arc<dyn D1RowSource> = Arc::new(InMemoryD1RowSource::new());
        let cron_log = Arc::new(InMemoryCronRunLog::new());
        // Pre-seed the ledger for today's date.
        let today = date_yyyymmdd_utc(1_778_824_800);
        cron_log
            .record(today, "metric-1", 0, RecordedRunStatus::Succeeded)
            .unwrap();
        let backend: Arc<dyn StatuspageBackend> = Arc::new(InMemoryStatuspageBackend::new(
            "page-1",
            "metric-1",
            "abcd1234",
            Arc::new(corelink_statuspage_real::InMemoryStatuspageAuditSink::new()),
        ));
        let audit = Arc::new(InMemorySchedulerAuditSink::new());
        let sched = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log,
            backend,
            audit.clone(),
            "page-1",
            "metric-1",
        );
        let outcome = sched.run_once(1_778_824_800).unwrap();
        let is_dup = matches!(
            outcome,
            RunOutcome::Skipped {
                reason: SkipReason::AlreadyPublishedToday
            }
        );
        assert!(is_dup);
        let snap = audit.snapshot();
        assert_eq!(snap[0].outcome, SchedulerAuditOutcome::Scheduled);
        assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Skipped);
        assert_eq!(
            snap[1].skip_reason,
            Some(SkipReason::AlreadyPublishedToday)
        );
    }

    #[test]
    fn d1_read_failure_propagates_and_emits_failed_audit() {
        let row_source: Arc<dyn D1RowSource> = Arc::new(InMemoryD1RowSource::with_read_failure());
        let cron_log = Arc::new(InMemoryCronRunLog::new());
        let backend: Arc<dyn StatuspageBackend> = Arc::new(InMemoryStatuspageBackend::new(
            "page-1",
            "metric-1",
            "abcd1234",
            Arc::new(corelink_statuspage_real::InMemoryStatuspageAuditSink::new()),
        ));
        let audit = Arc::new(InMemorySchedulerAuditSink::new());
        let sched = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log,
            backend,
            audit.clone(),
            "page-1",
            "metric-1",
        );
        let err = sched.run_once(1_778_824_800).unwrap_err();
        let is_row = matches!(err, SchedulerError::RowSource(_));
        assert!(is_row);
        let snap = audit.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].outcome, SchedulerAuditOutcome::Scheduled);
        assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Failed);
    }
}
