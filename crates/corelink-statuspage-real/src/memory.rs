//! In-memory [`StatuspageBackend`] fake — records every publish.
//!
//! Used by unit tests downstream (e.g. the privacy-erasure-worker
//! publish job) so they do not need to spin up a WireMock server.

use std::sync::{Arc, Mutex};

use crate::audit::{
    StatuspageAuditEvent, StatuspageAuditOutcome, StatuspageAuditSink,
};
use crate::backend::{PublishOutcome, StatuspageBackend, StatuspageClientError};
use crate::rate_limit::{RateLimitDecision, StatuspageRateLimiter};
use crate::redact::redact_api_key;
use crate::report::DsrCompletionReport;

/// One recorded publish (in-memory backend test fixture).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedPublish {
    /// Statuspage page ID.
    pub page_id: String,
    /// Statuspage metric ID.
    pub metric_id: String,
    /// The published report (deep copy).
    pub report: DsrCompletionReport,
    /// Wall-clock epoch ms at publish time.
    pub now_epoch_ms: u64,
}

/// In-memory backend: records every publish + delegates to the same
/// rate-limiter as production wiring (so tests can assert quota
/// behaviour without HTTP).
#[derive(Clone, Debug)]
pub struct InMemoryStatuspageBackend {
    page_id: String,
    metric_id: String,
    api_key_redacted: String,
    audit: Arc<dyn StatuspageAuditSink>,
    rate_limiter: StatuspageRateLimiter,
    recorded: Arc<Mutex<Vec<RecordedPublish>>>,
}

impl InMemoryStatuspageBackend {
    /// Construct.
    #[must_use]
    pub fn new(
        page_id: impl Into<String>,
        metric_id: impl Into<String>,
        api_key: &str,
        audit: Arc<dyn StatuspageAuditSink>,
    ) -> Self {
        Self {
            page_id: page_id.into(),
            metric_id: metric_id.into(),
            api_key_redacted: redact_api_key(api_key),
            audit,
            rate_limiter: StatuspageRateLimiter::new(),
            recorded: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Snapshot recorded publishes.
    #[must_use]
    pub fn snapshot(&self) -> Vec<RecordedPublish> {
        match self.recorded.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded publishes.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.recorded.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True when nothing recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl StatuspageBackend for InMemoryStatuspageBackend {
    fn publish_dsr_metric(
        &self,
        report: &DsrCompletionReport,
        now_epoch_ms: u64,
    ) -> Result<PublishOutcome, StatuspageClientError> {
        match self
            .rate_limiter
            .decide(&self.page_id, &self.metric_id, now_epoch_ms)
        {
            RateLimitDecision::DenyBackoff { retry_after, jitter } => {
                let retry_after_ms = u64::try_from(retry_after.as_millis()).unwrap_or(u64::MAX);
                let jitter_ms = u64::try_from(jitter.as_millis()).unwrap_or(u64::MAX);
                let evt = StatuspageAuditEvent {
                    outcome: StatuspageAuditOutcome::RateLimited,
                    page_id: self.page_id.clone(),
                    metric_id: self.metric_id.clone(),
                    api_key_redacted: self.api_key_redacted.clone(),
                    final_status: None,
                    attempts: 0,
                    reason: Some(format!(
                        "local rate-limiter: retry_after_ms={retry_after_ms} jitter_ms={jitter_ms}"
                    )),
                    p95_hours_observed: report.p95_resolution_hours,
                    window_end_unix_s: report.window_end_unix_s,
                };
                self.audit.emit(&evt)?;
                return Err(StatuspageClientError::RateLimited {
                    retry_after_ms,
                    jitter_ms,
                });
            }
            RateLimitDecision::Allow => {}
        }

        let recorded = RecordedPublish {
            page_id: self.page_id.clone(),
            metric_id: self.metric_id.clone(),
            report: report.clone(),
            now_epoch_ms,
        };
        match self.recorded.lock() {
            Ok(mut g) => g.push(recorded),
            Err(p) => p.into_inner().push(recorded),
        }
        let evt = StatuspageAuditEvent {
            outcome: StatuspageAuditOutcome::Published,
            page_id: self.page_id.clone(),
            metric_id: self.metric_id.clone(),
            api_key_redacted: self.api_key_redacted.clone(),
            final_status: Some(201),
            attempts: 1,
            reason: None,
            p95_hours_observed: report.p95_resolution_hours,
            window_end_unix_s: report.window_end_unix_s,
        };
        self.audit.emit(&evt)?;
        Ok(PublishOutcome {
            page_id: self.page_id.clone(),
            metric_id: self.metric_id.clone(),
            status: 201,
            attempts: 1,
        })
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
    use crate::audit::InMemoryStatuspageAuditSink;

    fn sample_report() -> DsrCompletionReport {
        DsrCompletionReport::new(0, 86_400, 5, 1, 0, 12).unwrap()
    }

    #[test]
    fn happy_path_records_publish_and_emits_audit() {
        let audit = Arc::new(InMemoryStatuspageAuditSink::new());
        let b = InMemoryStatuspageBackend::new("p", "m", "abcdef1234567890", audit.clone());
        let r = sample_report();
        let out = b.publish_dsr_metric(&r, 1_000).unwrap();
        assert_eq!(out.page_id, "p");
        assert_eq!(out.metric_id, "m");
        assert_eq!(out.status, 201);
        assert_eq!(b.len(), 1);
        assert_eq!(audit.len(), 1);
        let evt = audit.snapshot();
        assert_eq!(evt.first().unwrap().outcome, StatuspageAuditOutcome::Published);
        assert_eq!(evt.first().unwrap().api_key_redacted, "OAuth ***7890");
    }

    #[test]
    fn rate_limit_blocks_second_publish_within_window() {
        let audit = Arc::new(InMemoryStatuspageAuditSink::new());
        let b = InMemoryStatuspageBackend::new("p", "m", "abcdef1234567890", audit.clone());
        let r = sample_report();
        let _ = b.publish_dsr_metric(&r, 0).unwrap();
        let err = b.publish_dsr_metric(&r, 60_000).unwrap_err();
        let is_rl = matches!(err, StatuspageClientError::RateLimited { .. });
        assert!(is_rl);
        // 1 success audit + 1 rate-limited audit = 2 total
        assert_eq!(audit.len(), 2);
        assert_eq!(
            audit.snapshot().get(1).unwrap().outcome,
            StatuspageAuditOutcome::RateLimited
        );
    }

    #[test]
    fn rate_limit_releases_after_window() {
        let audit = Arc::new(InMemoryStatuspageAuditSink::new());
        let b = InMemoryStatuspageBackend::new("p", "m", "abcdef1234567890", audit);
        let r = sample_report();
        let _ = b.publish_dsr_metric(&r, 0).unwrap();
        // 5 min + 1 ms — should allow.
        let out = b
            .publish_dsr_metric(&r, 5 * 60 * 1_000 + 1)
            .unwrap();
        assert_eq!(out.status, 201);
        assert_eq!(b.len(), 2);
    }
}
