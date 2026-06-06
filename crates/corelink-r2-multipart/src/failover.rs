//! Multipart in-flight inventory at failover
//! (closes DEBT-011 R-PREP-REPL-P2-003).
//!
//! # What this module ships
//!
//! At active-failover (RB-ACTIVE-FAILOVER §3 Drain), clients with multi-GiB
//! multipart uploads in flight on the old primary will see their sessions
//! aborted — the new primary cannot resume them (multipart sessions are bound
//! to the R2 region they were initiated on; audit §3.5 edge case (b)). The
//! customer impact is: client must restart the entire upload.
//!
//! Per the audit acceptance criterion P2-003 §1, the drain step MUST emit a
//! per-tenant inventory of aborted sessions + a `failover.multipart_aborted.v1`
//! CloudEvent per session, so on-call can proactively notify enterprise
//! tenants and forensic walks can correlate "you restarted at T+12 min because
//! we drained at T".
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the production CF Worker wiring (real R2 `AbortMultipartUpload` calls + D1
//! `multipart_sessions` mark-aborted + CloudEvent emit to the audit bus) is
//! deferred. Here we ship:
//!
//! 1. [`MultipartFailoverInventory`] trait — fixed boundary the drain step
//!    calls, returning the inventory + emitting one CloudEvent per session.
//! 2. [`InMemoryMultipartFailoverInventory`] — deterministic fixture wrapping
//!    a list of in-flight sessions (per-instance `Mutex<>` F-001 closure).
//! 3. [`MultipartAbortedEvent`] — CloudEvent payload (`type =
//!    "corelink.failover.multipart_aborted.v1"`).
//! 4. Per-tenant aggregation helper [`MultipartFailoverReport::by_tenant`] —
//!    the on-call comms template consumes this map (tenant_id →
//!    [`AbortedSessionRow`]) directly.
//!
//! # Audit fail-CLOSED ordering (S-06 P0-2 lesson, reaffirmed)
//!
//! Per acceptance criterion §1: the inventory call emits ONE CloudEvent **per
//! aborted session BEFORE** returning the inventory to the caller. If any
//! emit fails, the inventory call returns
//! [`MultipartFailoverError::AuditEmitFailed`] — the drain step MUST treat
//! this as fail-CLOSED (do NOT proceed silently; emit SEV-2 + manual
//! reconciliation). 100% of in-flight sessions accounted for in audit is the
//! ticket's success metric.
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Per-session events emit at the audit-chain layer (D1 / CloudEvent bus),
//! not as Prometheus séries — the Prometheus signal is the
//! `corelink_failover_multipart_aborted_total{region, tenant_tier}` counter
//! (4 region × 4 tier = 16 séries; budget-safe; the tenant_id stays in the
//! CloudEvent payload only).

use crate::types::SessionState;
use std::sync::Mutex;
use std::time::SystemTime;
use uuid::Uuid;

/// Canonical Prometheus counter name for the per-failover aborted-multipart count.
///
/// LOAD-BEARING: alerting rules + on-call dashboard panels rely on this exact
/// name. Labels: `{region, tenant_tier}` (NOT `tenant_id` — cardinality
/// budget). 4 × 4 = 16 séries max.
pub const METRIC_FAILOVER_MULTIPART_ABORTED_TOTAL: &str =
    "corelink_failover_multipart_aborted_total";

/// Canonical CloudEvents `type` string for the per-session aborted event.
///
/// LOAD-BEARING: audit chain matchers + the on-call comms template both
/// dispatch on this exact string. Mirrors the
/// `corelink.region.failover.{detected,resolved}` taxonomy in
/// `corelink-replica-worker` but lives at the multipart-adapter boundary,
/// where the in-flight session inventory is observable.
pub const CLOUDEVENT_TYPE_MULTIPART_ABORTED: &str = "corelink.failover.multipart_aborted.v1";

/// One in-flight multipart session aborted by the failover drain step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbortedSessionRow {
    /// The aborted upload id.
    pub upload_id: String,
    /// The tenant the upload was bound to (drives the on-call comms template
    /// per-tenant grouping in [`MultipartFailoverReport::by_tenant`]).
    pub tenant_id: Uuid,
    /// The canonical R2 object key (tenant-prefix scoped).
    pub object_key: String,
    /// Wall-clock initiation time (lets the comms template surface "your
    /// upload that started at T was aborted at T+drain").
    pub initiated_at: SystemTime,
    /// Aggregate bytes already uploaded across all parts at drain time
    /// (informational — surfaced in the comms template so enterprise tenants
    /// can estimate the restart cost).
    pub bytes_uploaded: u64,
}

/// CloudEvent payload for one aborted session.
///
/// Emitted via the audit chain (NOT Prometheus) so the per-tenant per-upload
/// identity stays out of metric labels (INV-OBS-CARDINALITY-BUDGET).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartAbortedEvent {
    /// CloudEvents `type` (`corelink.failover.multipart_aborted.v1`).
    pub event_type: &'static str,
    /// The aborted upload id.
    pub upload_id: String,
    /// The tenant the upload was bound to.
    pub tenant_id: Uuid,
    /// The canonical R2 object key.
    pub object_key: String,
    /// Wall-clock initiation time.
    pub initiated_at: SystemTime,
    /// Bytes uploaded at drain time.
    pub bytes_uploaded: u64,
    /// Wall-clock failover-drain time (CloudEvent `time`).
    pub aborted_at: SystemTime,
}

impl MultipartAbortedEvent {
    /// Construct from an [`AbortedSessionRow`] + the wall-clock drain time.
    #[must_use]
    pub fn from_row(row: &AbortedSessionRow, aborted_at: SystemTime) -> Self {
        Self {
            event_type: CLOUDEVENT_TYPE_MULTIPART_ABORTED,
            upload_id: row.upload_id.clone(),
            tenant_id: row.tenant_id,
            object_key: row.object_key.clone(),
            initiated_at: row.initiated_at,
            bytes_uploaded: row.bytes_uploaded,
            aborted_at,
        }
    }
}

/// Aggregate report returned by [`MultipartFailoverInventory::drain_and_inventory`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultipartFailoverReport {
    /// One row per in-flight session aborted by the drain.
    pub rows: Vec<AbortedSessionRow>,
}

impl MultipartFailoverReport {
    /// Group aborted sessions by `tenant_id` — drives the on-call comms
    /// template ("you have N aborted uploads; restart at your earliest
    /// convenience").
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_r2_multipart::failover::{AbortedSessionRow, MultipartFailoverReport};
    /// use std::time::SystemTime;
    /// use uuid::Uuid;
    ///
    /// let t1 = Uuid::new_v4();
    /// let report = MultipartFailoverReport {
    ///     rows: vec![
    ///         AbortedSessionRow {
    ///             upload_id: "u1".into(), tenant_id: t1,
    ///             object_key: "k1".into(), initiated_at: SystemTime::UNIX_EPOCH,
    ///             bytes_uploaded: 100,
    ///         },
    ///         AbortedSessionRow {
    ///             upload_id: "u2".into(), tenant_id: t1,
    ///             object_key: "k2".into(), initiated_at: SystemTime::UNIX_EPOCH,
    ///             bytes_uploaded: 200,
    ///         },
    ///     ],
    /// };
    /// let by_tenant = report.by_tenant();
    /// assert_eq!(by_tenant.get(&t1).map(Vec::len), Some(2));
    /// ```
    #[must_use]
    pub fn by_tenant(&self) -> std::collections::BTreeMap<Uuid, Vec<AbortedSessionRow>> {
        let mut out: std::collections::BTreeMap<Uuid, Vec<AbortedSessionRow>> =
            std::collections::BTreeMap::new();
        for r in &self.rows {
            out.entry(r.tenant_id).or_default().push(r.clone());
        }
        out
    }

    /// Total bytes of in-flight work aborted by the drain. Informational
    /// signal for the post-failover review ("we cost customers N GiB of
    /// re-upload bandwidth").
    #[must_use]
    pub fn total_bytes_aborted(&self) -> u64 {
        self.rows.iter().map(|r| r.bytes_uploaded).sum()
    }

    /// Number of sessions aborted (== rows.len(); explicit helper for the
    /// success-metric assertion "100% of in-flight sessions accounted for").
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.rows.len()
    }
}

/// Per-emit failure taxonomy for the inventory call.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MultipartFailoverError {
    /// At least one per-session CloudEvent emit failed; drain step MUST
    /// fail-CLOSED (SEV-2 + manual reconciliation).
    AuditEmitFailed(String),
    /// The underlying session store could not be enumerated.
    InventoryQueryFailed(String),
}

impl std::fmt::Display for MultipartFailoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipartFailoverError::AuditEmitFailed(m) => {
                write!(f, "multipart_aborted audit emit failed: {m}")
            }
            MultipartFailoverError::InventoryQueryFailed(m) => {
                write!(f, "multipart inventory query failed: {m}")
            }
        }
    }
}

impl std::error::Error for MultipartFailoverError {}

/// CloudEvent emit trait — production wires the audit chain bus; tests use
/// [`InMemoryMultipartAuditSink`].
///
/// **Audit fail-CLOSED**: if `emit` returns `Err`, the inventory call MUST
/// propagate it as [`MultipartFailoverError::AuditEmitFailed`] (caller
/// fail-CLOSED — never silently proceed).
pub trait MultipartAuditSink: std::fmt::Debug + Send + Sync {
    /// Emit one `multipart_aborted` CloudEvent.
    ///
    /// # Errors
    /// Returns `Err(String)` on backend failure. The inventory call must
    /// wrap this in [`MultipartFailoverError::AuditEmitFailed`] and refuse
    /// to return a successful report.
    fn emit(&self, event: MultipartAbortedEvent) -> Result<(), String>;
}

/// In-memory audit sink for tests.
#[derive(Debug, Default)]
pub struct InMemoryMultipartAuditSink {
    events: Mutex<Vec<MultipartAbortedEvent>>,
}

impl InMemoryMultipartAuditSink {
    /// Create an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all emitted events so far.
    pub fn events(&self) -> Vec<MultipartAbortedEvent> {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl MultipartAuditSink for InMemoryMultipartAuditSink {
    fn emit(&self, event: MultipartAbortedEvent) -> Result<(), String> {
        self.events.lock().map_err(|e| e.to_string())?.push(event);
        Ok(())
    }
}

/// Adversarial audit sink that always fails — proves fail-CLOSED.
#[derive(Debug)]
pub struct FailingMultipartAuditSink {
    message: String,
}

impl FailingMultipartAuditSink {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl MultipartAuditSink for FailingMultipartAuditSink {
    fn emit(&self, _event: MultipartAbortedEvent) -> Result<(), String> {
        Err(self.message.clone())
    }
}

/// Multipart in-flight inventory + drain trait.
///
/// Production impl queries the D1 `multipart_sessions` table
/// (`WHERE state = 'in_progress' AND region = <draining_region>`), emits one
/// `multipart_aborted.v1` CloudEvent per row, then marks the rows as aborted
/// in D1 and aborts the R2 sessions via `AbortMultipartUpload`.
///
/// Per the autonomous execution charter, the in-memory impl here proves the
/// boundary semantics + the per-session audit ordering; the CF Worker wiring
/// is `trait-abstraction-defer`.
pub trait MultipartFailoverInventory: std::fmt::Debug + Send + Sync {
    /// Enumerate all `InProgress` multipart sessions, emit one CloudEvent per
    /// row via `audit_sink`, and return the aggregate report.
    ///
    /// # Audit ordering
    ///
    /// `enumerate → emit_audit (per row) → return_report`. If any emit fails,
    /// the inventory call returns `Err(AuditEmitFailed)` — the caller MUST
    /// treat this as fail-CLOSED.
    ///
    /// # Errors
    ///
    /// Returns [`MultipartFailoverError::InventoryQueryFailed`] if the
    /// underlying enumeration fails, or [`MultipartFailoverError::AuditEmitFailed`]
    /// if any per-session emit returns `Err`.
    fn drain_and_inventory(
        &self,
        audit_sink: &dyn MultipartAuditSink,
        aborted_at: SystemTime,
    ) -> Result<MultipartFailoverReport, MultipartFailoverError>;
}

/// In-memory implementation of [`MultipartFailoverInventory`].
///
/// Seeded via [`InMemoryMultipartFailoverInventory::seed_in_flight`]. Per-instance
/// `Mutex<>` F-001 closure.
#[derive(Debug, Default)]
pub struct InMemoryMultipartFailoverInventory {
    sessions: Mutex<Vec<(AbortedSessionRow, SessionState)>>,
    fail_query: Mutex<Option<String>>,
}

impl InMemoryMultipartFailoverInventory {
    /// Construct an empty inventory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed one in-flight session. The drain will pick it up and abort it.
    pub fn seed_in_flight(&self, row: AbortedSessionRow) {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((row, SessionState::InProgress));
    }

    /// Seed a non-`InProgress` session — drain MUST ignore it (only
    /// `InProgress` sessions are aborted).
    pub fn seed_terminal(&self, row: AbortedSessionRow, state: SessionState) {
        debug_assert!(state != SessionState::InProgress);
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((row, state));
    }

    /// Force the next [`MultipartFailoverInventory::drain_and_inventory`] call to return an
    /// `InventoryQueryFailed` error with `msg`.
    pub fn force_query_failure(&self, msg: impl Into<String>) {
        *self.fail_query.lock().unwrap_or_else(|p| p.into_inner()) = Some(msg.into());
    }

    /// Snapshot of seeded sessions in `InProgress` (test helper — count of
    /// rows the next drain WOULD emit).
    pub fn in_flight_count(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|(_, s)| *s == SessionState::InProgress)
            .count()
    }
}

impl MultipartFailoverInventory for InMemoryMultipartFailoverInventory {
    fn drain_and_inventory(
        &self,
        audit_sink: &dyn MultipartAuditSink,
        aborted_at: SystemTime,
    ) -> Result<MultipartFailoverReport, MultipartFailoverError> {
        if let Some(msg) = self
            .fail_query
            .lock()
            .map_err(|e| MultipartFailoverError::InventoryQueryFailed(e.to_string()))?
            .clone()
        {
            return Err(MultipartFailoverError::InventoryQueryFailed(msg));
        }
        let snap: Vec<AbortedSessionRow> = self
            .sessions
            .lock()
            .map_err(|e| MultipartFailoverError::InventoryQueryFailed(e.to_string()))?
            .iter()
            .filter(|(_, s)| *s == SessionState::InProgress)
            .map(|(r, _)| r.clone())
            .collect();

        // Audit-emit BEFORE returning report (fail-CLOSED).
        for row in &snap {
            let ev = MultipartAbortedEvent::from_row(row, aborted_at);
            audit_sink
                .emit(ev)
                .map_err(MultipartFailoverError::AuditEmitFailed)?;
        }

        Ok(MultipartFailoverReport { rows: snap })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    fn make_row(upload: &str, tenant: Uuid, bytes: u64) -> AbortedSessionRow {
        AbortedSessionRow {
            upload_id: upload.to_string(),
            tenant_id: tenant,
            object_key: format!("tenants/{tenant}/blobs/{upload}"),
            initiated_at: SystemTime::UNIX_EPOCH,
            bytes_uploaded: bytes,
        }
    }

    #[test]
    fn metric_and_event_names_are_canonical() {
        assert_eq!(
            METRIC_FAILOVER_MULTIPART_ABORTED_TOTAL,
            "corelink_failover_multipart_aborted_total"
        );
        assert_eq!(
            CLOUDEVENT_TYPE_MULTIPART_ABORTED,
            "corelink.failover.multipart_aborted.v1"
        );
    }

    #[test]
    fn aborted_event_carries_session_fields() {
        let t = Uuid::new_v4();
        let row = make_row("u1", t, 1024);
        let ev = MultipartAbortedEvent::from_row(&row, SystemTime::UNIX_EPOCH);
        assert_eq!(ev.event_type, CLOUDEVENT_TYPE_MULTIPART_ABORTED);
        assert_eq!(ev.upload_id, "u1");
        assert_eq!(ev.tenant_id, t);
        assert_eq!(ev.bytes_uploaded, 1024);
    }

    #[test]
    fn report_by_tenant_groups_correctly() {
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let r = MultipartFailoverReport {
            rows: vec![
                make_row("u1", t1, 100),
                make_row("u2", t1, 200),
                make_row("u3", t2, 50),
            ],
        };
        let by = r.by_tenant();
        assert_eq!(by.get(&t1).map(Vec::len), Some(2));
        assert_eq!(by.get(&t2).map(Vec::len), Some(1));
        assert_eq!(r.total_bytes_aborted(), 350);
        assert_eq!(r.session_count(), 3);
    }

    #[test]
    fn empty_inventory_yields_empty_report_with_no_events() {
        let inv = InMemoryMultipartFailoverInventory::new();
        let sink = InMemoryMultipartAuditSink::new();
        let r = inv
            .drain_and_inventory(&sink, SystemTime::UNIX_EPOCH)
            .expect("drain ok");
        assert_eq!(r.session_count(), 0);
        assert!(sink.events().is_empty());
    }

    #[test]
    fn drain_emits_one_event_per_in_flight_session() {
        let inv = InMemoryMultipartFailoverInventory::new();
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        inv.seed_in_flight(make_row("u1", t1, 1_000_000));
        inv.seed_in_flight(make_row("u2", t1, 500_000));
        inv.seed_in_flight(make_row("u3", t2, 250_000));
        let sink = InMemoryMultipartAuditSink::new();
        let r = inv
            .drain_and_inventory(&sink, SystemTime::UNIX_EPOCH)
            .expect("drain ok");
        assert_eq!(r.session_count(), 3, "100% inventoried");
        assert_eq!(sink.events().len(), 3, "one event per row");
        // Tenant grouping is the on-call comms template input.
        let by = r.by_tenant();
        assert_eq!(by.get(&t1).map(Vec::len), Some(2));
        assert_eq!(by.get(&t2).map(Vec::len), Some(1));
        // Every event carries the canonical type.
        for ev in sink.events() {
            assert_eq!(ev.event_type, CLOUDEVENT_TYPE_MULTIPART_ABORTED);
        }
    }

    #[test]
    fn drain_ignores_completed_and_aborted_sessions() {
        // Only `InProgress` sessions are picked up — finished sessions must
        // be left alone (idempotent re-drain semantics for the orchestrator).
        let inv = InMemoryMultipartFailoverInventory::new();
        let t = Uuid::new_v4();
        inv.seed_in_flight(make_row("u-live", t, 100));
        inv.seed_terminal(make_row("u-done", t, 200), SessionState::Completed);
        inv.seed_terminal(make_row("u-abrt", t, 50), SessionState::Aborted);
        let sink = InMemoryMultipartAuditSink::new();
        let r = inv
            .drain_and_inventory(&sink, SystemTime::UNIX_EPOCH)
            .expect("drain ok");
        assert_eq!(r.session_count(), 1);
        assert_eq!(r.rows[0].upload_id, "u-live");
        assert_eq!(sink.events().len(), 1);
    }

    #[test]
    fn drain_fails_closed_when_audit_emit_fails() {
        // Audit fail-CLOSED contract: any per-session emit error means the
        // drain call MUST return AuditEmitFailed, never a partial report.
        let inv = InMemoryMultipartFailoverInventory::new();
        let t = Uuid::new_v4();
        inv.seed_in_flight(make_row("u1", t, 1));
        inv.seed_in_flight(make_row("u2", t, 1));
        let sink = FailingMultipartAuditSink::new("audit bus down");
        let r = inv.drain_and_inventory(&sink, SystemTime::UNIX_EPOCH);
        assert!(matches!(r, Err(MultipartFailoverError::AuditEmitFailed(_))));
    }

    #[test]
    fn drain_propagates_inventory_query_failure() {
        let inv = InMemoryMultipartFailoverInventory::new();
        inv.force_query_failure("D1 outage");
        let sink = InMemoryMultipartAuditSink::new();
        let r = inv.drain_and_inventory(&sink, SystemTime::UNIX_EPOCH);
        assert!(matches!(
            r,
            Err(MultipartFailoverError::InventoryQueryFailed(_))
        ));
        assert!(sink.events().is_empty(), "no audit when query failed");
    }

    #[test]
    fn in_flight_count_reflects_seeded_state() {
        let inv = InMemoryMultipartFailoverInventory::new();
        assert_eq!(inv.in_flight_count(), 0);
        inv.seed_in_flight(make_row("u1", Uuid::new_v4(), 1));
        inv.seed_in_flight(make_row("u2", Uuid::new_v4(), 1));
        inv.seed_terminal(make_row("u3", Uuid::new_v4(), 1), SessionState::Completed);
        assert_eq!(inv.in_flight_count(), 2);
    }

    #[test]
    fn inventory_trait_object_safe() {
        let inv: Box<dyn MultipartFailoverInventory> =
            Box::new(InMemoryMultipartFailoverInventory::new());
        let sink: Box<dyn MultipartAuditSink> = Box::new(InMemoryMultipartAuditSink::new());
        let r = inv
            .drain_and_inventory(sink.as_ref(), SystemTime::UNIX_EPOCH)
            .expect("drain ok");
        assert_eq!(r.session_count(), 0);
    }

    #[test]
    fn multipart_failover_error_display_is_actionable() {
        let e = MultipartFailoverError::AuditEmitFailed("bus closed".to_string());
        assert!(e.to_string().contains("audit emit failed"));
        let e2 = MultipartFailoverError::InventoryQueryFailed("D1".to_string());
        assert!(e2.to_string().contains("inventory query failed"));
    }
}
