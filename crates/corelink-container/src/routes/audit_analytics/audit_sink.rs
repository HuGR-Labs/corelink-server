//! Audit-sink trait + in-memory capture fake + fail-CLOSED `emit_or_503`
//! helper for the `/v1/audit/analytics/*` routes.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Verbatim move; no behavioural change.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use axum::{http::StatusCode, response::IntoResponse};

use super::state::AuditAnalyticsRouteState;
use super::types::AnalyticsAuditRow;

/// Audit emit trait for the analytics routes.
pub trait AnalyticsAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one [`AnalyticsAuditRow`]. Fail-CLOSED on `Err` —
    /// the route returns 503.
    ///
    /// # Errors
    ///
    /// Static error string when the pipeline is closed.
    fn emit(&self, row: AnalyticsAuditRow) -> Result<(), &'static str>;
}

/// In-memory capture sink for analytics audit emits.
#[derive(Clone, Debug, Default)]
pub struct InMemoryAnalyticsAuditSink {
    inner: Arc<Mutex<Vec<AnalyticsAuditRow>>>,
}

impl InMemoryAnalyticsAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured row.
    ///
    /// # Errors
    ///
    /// Static error string when the inner mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<AnalyticsAuditRow>, &'static str> {
        let g = self
            .inner
            .lock()
            .map_err(|_| "analytics audit sink mutex poisoned")?;
        Ok(g.clone())
    }
}

impl AnalyticsAuditSink for InMemoryAnalyticsAuditSink {
    fn emit(&self, row: AnalyticsAuditRow) -> Result<(), &'static str> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| "analytics audit sink mutex poisoned")?;
        g.push(row);
        Ok(())
    }
}

/// Emit an audit row and return a response. If the emit fails the
/// caller-supplied `success_resp` is dropped and a `503 Service
/// Unavailable` is returned instead — preserving the
/// `audit_emit ⇔ handler` atomicity contract (an emit failure on
/// ANY error arm — including tenant-isolation violations, bad-request,
/// backend-error, etc. — must surface as 503 so the security team's
/// SEV-2 anchor is never silently lost).
///
/// Mirrors the `audit_export` wave-20 fix-stream pattern; see
/// `specs/_audits/sealed/2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md`
/// findings B-P1-02 + B-P1-03 for the discipline drift this closes.
pub(super) fn emit_or_503(
    state: &AuditAnalyticsRouteState,
    row: AnalyticsAuditRow,
    success_resp: axum::response::Response,
) -> axum::response::Response {
    if state.audit_sink.emit(row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    success_resp
}
