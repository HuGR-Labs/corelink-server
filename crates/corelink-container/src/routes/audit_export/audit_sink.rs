//! Audit-sink trait + in-memory capture fake + the wave-20 shared
//! fail-CLOSED `emit_or_503` helper for the `/v1/audit/export` route.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Verbatim move of the trait + struct + helper; no behavioural change.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use axum::{http::StatusCode, response::IntoResponse};

use super::types::ExportAuditRow;

/// Audit sink trait — the route's audit-emit boundary. Production
/// wiring binds this to the durable CloudEvents emitter that lifts
/// the row into the standard audit-chain envelope; the in-memory
/// fake captures every emit for unit + integration test inspection.
pub trait ExportAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one [`ExportAuditRow`]. Production wiring is fail-
    /// CLOSED (the route MUST receive an `Err` if the emit failed;
    /// callers convert that to a 503 to honour
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns a static error string when the audit pipeline is
    /// closed / poisoned. The route converts to 503.
    fn emit(&self, row: ExportAuditRow) -> Result<(), &'static str>;
}

/// In-memory capture sink for the export-audit emits. Cloning
/// shares the captured buffer so the test harness can inspect
/// emit ordering without re-handing the sink to the route state.
///
/// **Contention bound (wave-20 A-P2-04 closure):** every `emit` takes a
/// global `Mutex` lock on the captured-rows vector — every emit on this
/// sink is sequenced. The native target is TEST-ONLY (production wires the
/// CloudEvents emitter), so the operational impact is bounded by the test
/// matrix throughput (single-digit emits per integration test). The
/// production wiring slots a lock-free CloudEvents publisher behind the
/// `dyn ExportAuditSink` trait surface, so this contention bound never
/// reaches a customer-facing path.
#[derive(Clone, Debug, Default)]
pub struct InMemoryExportAuditSink {
    inner: Arc<Mutex<Vec<ExportAuditRow>>>,
    /// When set to `Some`, every `emit` returns the contained error
    /// string — used to drive the fail-CLOSED 503 regression.
    injected_failure: Arc<Mutex<Option<&'static str>>>,
}

impl InMemoryExportAuditSink {
    /// Construct a fresh empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured audit row in emit order.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<ExportAuditRow>, &'static str> {
        let g = self
            .inner
            .lock()
            .map_err(|_| "export audit sink mutex poisoned")?;
        Ok(g.clone())
    }

    /// Inject a static failure to drive the 503 fail-CLOSED test.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn inject_failure(&self, msg: &'static str) -> Result<(), &'static str> {
        let mut g = self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        *g = Some(msg);
        Ok(())
    }
}

/// Wave-20 — shared fail-CLOSED audit-emit helper. Emits `row` via
/// `sink`; on success returns `None`. On failure returns
/// `Some(503 + "audit pipeline closed")` so the caller can early-
/// return without serving any bytes. Mirrors the wave-15 discipline
/// previously inlined only on the `export_request.v1` arm — every
/// pre-byte-stream emit on a non-happy path now routes through this
/// helper.
///
/// Closes audit findings A-P1-02 (cross-tenant-reject), A-P1-03 (mid-
/// stream-break — see `super::stream::emit_mid_stream_break_audit`
/// for the post-header trade-off), A-P1-05 (verify-failed-sev0), A-P2-01
/// (rate-limit-deny).
#[must_use]
pub fn emit_or_503(
    sink: &Arc<dyn ExportAuditSink>,
    row: ExportAuditRow,
) -> Option<axum::response::Response> {
    if sink.emit(row).is_err() {
        return Some((StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response());
    }
    None
}

impl ExportAuditSink for InMemoryExportAuditSink {
    fn emit(&self, row: ExportAuditRow) -> Result<(), &'static str> {
        // Fail-CLOSED check FIRST — never buffer a row we'd have
        // returned an error for.
        let injected = *self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        if let Some(msg) = injected {
            return Err(msg);
        }
        let mut g = self
            .inner
            .lock()
            .map_err(|_| "export audit sink mutex poisoned")?;
        g.push(row);
        Ok(())
    }
}
