//! Scheduler audit envelope — fail-CLOSED on every transition.
//!
//! Four canonical event types per wave-17 charter:
//!
//! - `corelink.privacy.statuspage_publish_scheduled.v1` — emitted at
//!   the start of [`crate::DsrStatuspagePublishScheduler::run_once`]
//!   BEFORE any D1 read or publish dispatch.
//! - `corelink.privacy.statuspage_publish_succeeded.v1` — emitted on
//!   2xx publish outcome.
//! - `corelink.privacy.statuspage_publish_failed.v1` — emitted on any
//!   non-success outcome (auth-fail / rate-limit / transport / D1
//!   read fail / dedupe ledger fail).
//! - `corelink.privacy.statuspage_publish_skipped.v1` — emitted when
//!   the publish is intentionally skipped (empty 24h window OR
//!   already-published-today dedupe).
//!
//! Per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis) every audit
//! emit returns `Result`; a failure of the audit sink propagates as
//! [`crate::SchedulerError::Audit`] and aborts the orchestration.

use std::sync::{Arc, Mutex};

use thiserror::Error;

/// Canonical scheduler audit outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SchedulerAuditOutcome {
    /// Cron tick fired — orchestration started.
    Scheduled,
    /// Publish accepted by Statuspage (2xx).
    Succeeded,
    /// Publish failed (auth / rate-limit / transport / D1 read /
    /// dedupe ledger).
    Failed,
    /// Publish intentionally skipped (empty window OR already-published-today).
    Skipped,
}

impl SchedulerAuditOutcome {
    /// Canonical event-type string emitted into the audit chain.
    #[must_use]
    pub const fn event_type(self) -> &'static str {
        match self {
            Self::Scheduled => "corelink.privacy.statuspage_publish_scheduled.v1",
            Self::Succeeded => "corelink.privacy.statuspage_publish_succeeded.v1",
            Self::Failed => "corelink.privacy.statuspage_publish_failed.v1",
            Self::Skipped => "corelink.privacy.statuspage_publish_skipped.v1",
        }
    }
}

/// Reason variants for the `Skipped` audit outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SkipReason {
    /// 24h D1 read returned zero rows (no DSR outcomes to publish).
    EmptyWindow,
    /// The `cron_run_log` ledger already has a row for
    /// `(today_yyyymmdd, metric_id)` — idempotency dedupe guard.
    AlreadyPublishedToday,
}

impl SkipReason {
    /// Short canonical label for the audit `reason` field.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::EmptyWindow => "empty_window",
            Self::AlreadyPublishedToday => "already_published_today",
        }
    }
}

/// Audit envelope for a single scheduler tick.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SchedulerAuditEvent {
    /// Canonical outcome.
    pub outcome: SchedulerAuditOutcome,
    /// Statuspage page ID this tick targeted.
    pub page_id: String,
    /// Statuspage metric ID this tick targeted.
    pub metric_id: String,
    /// `YYYYMMDD` UTC date the tick fired on (derived from
    /// `now_unix_s`). The dedupe ledger keys on this.
    pub date_yyyymmdd: u32,
    /// Number of rows read from D1 for the 24h window
    /// (`None` if the ledger / D1 read failed BEFORE the count was
    /// known).
    pub rows_read: Option<u64>,
    /// p95 hours observed in the aggregated window
    /// (`None` if the publish never reached the aggregator —
    /// e.g. on a `Skipped` / pre-aggregate `Failed` outcome).
    pub p95_hours_observed: Option<u64>,
    /// Skip reason — populated on `Skipped`, `None` otherwise.
    pub skip_reason: Option<SkipReason>,
    /// Optional reason / diagnostic string (populated on
    /// `Failed` / `Skipped`).
    pub reason: Option<String>,
}

/// Audit sink trait — fail-CLOSED contract.
pub trait SchedulerAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single event.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerAuditError::EmitFailed`] when the audit
    /// chain rejects the event. The scheduler caller MUST propagate
    /// this error and treat the tick as failed (no Statuspage publish
    /// has been observed in the audit trail).
    fn emit(&self, event: &SchedulerAuditEvent) -> Result<(), SchedulerAuditError>;
}

/// Audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SchedulerAuditError {
    /// Audit chain rejected (storage / lock / signature).
    #[error("scheduler audit emit failed: {0}")]
    EmitFailed(String),
}

/// In-memory recorder sink — used by integration tests + native
/// orchestration harnesses to assert on emitted events.
#[derive(Clone, Debug, Default)]
pub struct InMemorySchedulerAuditSink {
    events: Arc<Mutex<Vec<SchedulerAuditEvent>>>,
}

impl InMemorySchedulerAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<SchedulerAuditEvent> {
        match self.events.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.events.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True when no events recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SchedulerAuditSink for InMemorySchedulerAuditSink {
    fn emit(&self, event: &SchedulerAuditEvent) -> Result<(), SchedulerAuditError> {
        let mut g = self
            .events
            .lock()
            .map_err(|e| SchedulerAuditError::EmitFailed(format!("mutex poisoned: {e}")))?;
        g.push(event.clone());
        Ok(())
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
    fn canonical_event_types_pinned() {
        assert_eq!(
            SchedulerAuditOutcome::Scheduled.event_type(),
            "corelink.privacy.statuspage_publish_scheduled.v1"
        );
        assert_eq!(
            SchedulerAuditOutcome::Succeeded.event_type(),
            "corelink.privacy.statuspage_publish_succeeded.v1"
        );
        assert_eq!(
            SchedulerAuditOutcome::Failed.event_type(),
            "corelink.privacy.statuspage_publish_failed.v1"
        );
        assert_eq!(
            SchedulerAuditOutcome::Skipped.event_type(),
            "corelink.privacy.statuspage_publish_skipped.v1"
        );
    }

    #[test]
    fn skip_reason_labels_pinned() {
        assert_eq!(SkipReason::EmptyWindow.label(), "empty_window");
        assert_eq!(
            SkipReason::AlreadyPublishedToday.label(),
            "already_published_today"
        );
    }

    #[test]
    fn in_memory_sink_records_and_snapshots() {
        let sink = InMemorySchedulerAuditSink::new();
        let evt = SchedulerAuditEvent {
            outcome: SchedulerAuditOutcome::Scheduled,
            page_id: "p".to_owned(),
            metric_id: "m".to_owned(),
            date_yyyymmdd: 20_260_515,
            rows_read: None,
            p95_hours_observed: None,
            skip_reason: None,
            reason: None,
        };
        sink.emit(&evt).unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0], evt);
    }
}
