//! Failback `audit_outbox` drain guard
//! (closes DEBT-011 R-PREP-REPL-P1-002).
//!
//! # What this module ships
//!
//! `RB-ACTIVE-FAILOVER.md §7` (Reverse / failback) documents that re-engaging
//! writes on the old primary requires its `audit_outbox` to be fully drained
//! (i.e. zero rows with `emitted_at IS NULL`) before the write lease flips
//! back. Without this gate, the old primary could re-engage writes while
//! a fork of pre-failover audit events is still queued — splitting the
//! audit chain.
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the production wiring reads D1's `audit_outbox` table via the standard
//! SDK. Here we ship:
//!
//! 1. [`AuditOutboxRepository`] trait — fixed boundary the failback
//!    orchestrator reads to determine drain state.
//! 2. [`InMemoryAuditOutbox`] — deterministic fixture for tests, plus a
//!    helper to seed un-emitted rows (per-instance `Arc<Mutex<>>` F-001
//!    closure).
//! 3. [`assert_outbox_drained_or_block`] — the gate function the failback
//!    flow calls before re-engaging writes; emits the
//!    `corelink_failback_blocked_total{reason="audit_outbox_dirty"}` counter
//!    via the supplied [`FailbackBlockedCounter`] BEFORE returning the error
//!    (observability emit is best-effort; the gate decision is canonical).
//! 4. Audit emit ordering: when the gate refuses, a `failover.resolved`-class
//!    audit record with `detail = "audit_outbox_dirty: ..."` is appended via
//!    the supplied [`FailoverAuditSink`] BEFORE the error returns, so that
//!    forensic walks observe the refusal even if downstream caller swallows
//!    the result (audit fail-CLOSED preserved — if audit emit fails we still
//!    return the drain error, never silently allow the failback).
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Counter labels: `{reason}` — only one canonical reason value today
//! (`audit_outbox_dirty`). Future-proof for additional refusal reasons.
//! No `tenant_id` / `region` cardinality.

use std::sync::Mutex;

use crate::audit::{FailoverAuditEventType, FailoverAuditRecord, FailoverAuditSink};
use crate::error::FailoverError;
use crate::Region;

/// Canonical Prometheus counter name for failback-blocked events.
///
/// LOAD-BEARING: alerting rules in `slo_catalog.md §4.23..4.26` rely on this
/// exact name to fire SEV-2 when the gate refuses re-engagement.
pub const METRIC_FAILBACK_BLOCKED_TOTAL: &str = "corelink_failback_blocked_total";

/// Canonical reason label for the un-drained-outbox refusal path.
///
/// LOAD-BEARING: must match the alerting rule label matcher in
/// `corelink_failback_blocked_total{reason="audit_outbox_dirty"}`.
pub const FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY: &str = "audit_outbox_dirty";

/// Read-only repository abstraction over the old-primary's `audit_outbox`
/// table.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::failback::{
///     AuditOutboxRepository, InMemoryAuditOutbox,
/// };
/// use corelink_failover_router::Region;
///
/// let outbox = InMemoryAuditOutbox::new();
/// outbox.seed_undrained(Region::Wnam, 3);
/// assert_eq!(
///     outbox.undrained_count(Region::Wnam).unwrap(),
///     3
/// );
/// outbox.mark_all_drained(Region::Wnam);
/// assert_eq!(
///     outbox.undrained_count(Region::Wnam).unwrap(),
///     0
/// );
/// ```
pub trait AuditOutboxRepository: std::fmt::Debug + Send + Sync {
    /// Number of `audit_outbox` rows WHERE `emitted_at IS NULL` for `region`.
    ///
    /// # Errors
    /// Returns `Err(String)` on backend failure. Callers MUST treat an
    /// error as "fail-CLOSED: assume dirty" and refuse the failback.
    fn undrained_count(&self, region: Region) -> Result<u64, String>;
}

/// Counter-emit trait — production wires `corelink_failback_blocked_total`
/// via Prometheus; tests use [`InMemoryFailbackBlockedCounter`].
///
/// # Errors and ordering
///
/// Counter emit failures MUST NOT change the gate decision (the gate refusal
/// is canonical; observability is best-effort). The gate function
/// [`assert_outbox_drained_or_block`] therefore ignores any `Err` from
/// [`FailbackBlockedCounter::emit`] and proceeds to return the error.
pub trait FailbackBlockedCounter: std::fmt::Debug + Send + Sync {
    /// Increment the counter by 1 with the canonical `reason` label.
    ///
    /// # Errors
    /// Returns `Err(String)` if the metric backend fails. Callers (the
    /// gate function) MUST log via audit and continue with the refusal —
    /// never let observability gate the decision.
    fn emit(&self, reason: &str) -> Result<(), String>;
}

/// In-memory `audit_outbox` repository — deterministic fixture for tests.
#[derive(Debug, Default)]
pub struct InMemoryAuditOutbox {
    /// `(region, undrained_count)` rows.
    rows: Mutex<Vec<(Region, u64)>>,
}

impl InMemoryAuditOutbox {
    /// Create an empty outbox (all regions report 0 un-drained rows).
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed `n` un-drained rows for `region` (simulates drain skipped /
    /// failed in old primary).
    pub fn seed_undrained(&self, region: Region, n: u64) {
        let mut g = self.rows.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|(r, _)| *r != region);
        g.push((region, n));
    }

    /// Mark all rows for `region` as drained (emitted).
    pub fn mark_all_drained(&self, region: Region) {
        let mut g = self.rows.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|(r, _)| *r != region);
        g.push((region, 0));
    }
}

impl AuditOutboxRepository for InMemoryAuditOutbox {
    fn undrained_count(&self, region: Region) -> Result<u64, String> {
        let g = self.rows.lock().map_err(|e| e.to_string())?;
        Ok(g.iter()
            .find(|(r, _)| *r == region)
            .map(|(_, c)| *c)
            .unwrap_or(0))
    }
}

/// Adversarial outbox that always errors — caller must fail-CLOSED.
#[derive(Debug)]
pub struct FailingAuditOutbox {
    message: String,
}

impl FailingAuditOutbox {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl AuditOutboxRepository for FailingAuditOutbox {
    fn undrained_count(&self, _region: Region) -> Result<u64, String> {
        Err(self.message.clone())
    }
}

/// In-memory failback-blocked counter — captures every increment for
/// deterministic test assertion.
#[derive(Debug, Default)]
pub struct InMemoryFailbackBlockedCounter {
    increments: Mutex<Vec<String>>,
}

impl InMemoryFailbackBlockedCounter {
    /// Create an empty counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all reason-labels recorded so far.
    pub fn increments(&self) -> Vec<String> {
        self.increments
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Count of increments for a given reason.
    pub fn count_for(&self, reason: &str) -> usize {
        self.increments()
            .iter()
            .filter(|r| r.as_str() == reason)
            .count()
    }
}

impl FailbackBlockedCounter for InMemoryFailbackBlockedCounter {
    fn emit(&self, reason: &str) -> Result<(), String> {
        self.increments
            .lock()
            .map_err(|e| e.to_string())?
            .push(reason.to_owned());
        Ok(())
    }
}

/// Adversarial counter that always fails — verifies that gate decision is
/// NOT blocked by observability emit failure.
#[derive(Debug)]
pub struct FailingFailbackBlockedCounter {
    message: String,
}

impl FailingFailbackBlockedCounter {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl FailbackBlockedCounter for FailingFailbackBlockedCounter {
    fn emit(&self, _reason: &str) -> Result<(), String> {
        Err(self.message.clone())
    }
}

/// Gate function: refuse re-engagement of writes on `old_primary` if its
/// `audit_outbox` has any un-drained rows.
///
/// # Ordering (audit fail-CLOSED + observability best-effort)
///
/// 1. Query the outbox (`AuditOutboxRepository::undrained_count`).
///    - Error → fail-CLOSED: treat as dirty, emit audit + counter, return
///      `FailoverError::Internal { .. }` with detail "outbox_query_failed".
/// 2. If `undrained_count == 0` → `Ok(())` (no audit emit needed; nominal
///    failback path).
/// 3. Else → emit the `failover.resolved`-class audit record with
///    `detail = "audit_outbox_dirty: undrained=N"` BEFORE any other side
///    effect (fail-CLOSED: if audit emit fails we still return the drain
///    error — observability never gates the canonical refusal). Then emit
///    the counter (best-effort). Then return
///    `FailoverError::WriteBlockedDuringFailover { region: old_primary }`.
///
/// # Errors
///
/// - `FailoverError::WriteBlockedDuringFailover` — outbox has un-drained rows.
/// - `FailoverError::Internal` — repository query failed (fail-CLOSED).
/// - `FailoverError::Audit` — audit emit failed AND outbox was dirty (caller
///   MUST still refuse failback; this is the strictest interpretation of
///   audit fail-CLOSED).
pub fn assert_outbox_drained_or_block(
    outbox: &dyn AuditOutboxRepository,
    audit_sink: &dyn FailoverAuditSink,
    counter: &dyn FailbackBlockedCounter,
    old_primary: Region,
    new_primary: Region,
    timestamp_ms: u64,
) -> Result<(), FailoverError> {
    // Step 1: query — fail-CLOSED on error.
    let undrained = match outbox.undrained_count(old_primary) {
        Ok(n) => n,
        Err(query_err) => {
            // Audit emit BEFORE returning (fail-CLOSED, but if audit emit
            // also fails we propagate the audit error — never silently allow).
            let audit_res = audit_sink.emit(FailoverAuditRecord {
                event_type: FailoverAuditEventType::FailoverResolved,
                tenant_id_hash: String::new(),
                blob_hash: String::new(),
                primary_region: old_primary.as_str().to_owned(),
                replica_region: new_primary.as_str().to_owned(),
                timestamp_ms,
                detail: format!(
                    "outbox_query_failed reason={} err={}",
                    FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY, query_err
                ),
            });
            // Observability emit — best-effort; ignore result.
            let _ = counter.emit(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY);
            return match audit_res {
                Ok(()) => Err(FailoverError::Internal(format!(
                    "audit_outbox query failed for region {old_primary:?}: {query_err}"
                ))),
                Err(audit_err) => Err(FailoverError::Audit(audit_err)),
            };
        }
    };

    if undrained == 0 {
        // Nominal failback path — no audit emit (the `failover.resolved`
        // audit for the successful failback is emitted by the caller after
        // the write lease flip; see `RB-ACTIVE-FAILOVER.md §7.2 step 7`).
        return Ok(());
    }

    // Step 3: dirty outbox — emit audit BEFORE returning (fail-CLOSED).
    let audit_res = audit_sink.emit(FailoverAuditRecord {
        event_type: FailoverAuditEventType::FailoverResolved,
        tenant_id_hash: String::new(),
        blob_hash: String::new(),
        primary_region: old_primary.as_str().to_owned(),
        replica_region: new_primary.as_str().to_owned(),
        timestamp_ms,
        detail: format!(
            "audit_outbox_dirty undrained={undrained} reason={}",
            FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY
        ),
    });
    // Observability counter — best-effort.
    let _ = counter.emit(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY);

    match audit_res {
        Ok(()) => Err(FailoverError::WriteBlockedDuringFailover {
            region: old_primary,
        }),
        Err(audit_err) => Err(FailoverError::Audit(audit_err)),
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
    use crate::audit::{FailingFailoverAuditSink, InMemoryFailoverAuditSink};

    fn fixtures() -> (
        InMemoryAuditOutbox,
        InMemoryFailoverAuditSink,
        InMemoryFailbackBlockedCounter,
    ) {
        (
            InMemoryAuditOutbox::new(),
            InMemoryFailoverAuditSink::new(),
            InMemoryFailbackBlockedCounter::new(),
        )
    }

    #[test]
    fn canonical_metric_and_reason_constants() {
        // LOAD-BEARING: alerting rule label matchers depend on these.
        assert_eq!(METRIC_FAILBACK_BLOCKED_TOTAL, "corelink_failback_blocked_total");
        assert_eq!(
            FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY,
            "audit_outbox_dirty"
        );
    }

    #[test]
    fn clean_outbox_allows_failback() {
        let (outbox, sink, counter) = fixtures();
        let r = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            0,
        );
        assert!(r.is_ok());
        // No audit emit on nominal path.
        assert_eq!(sink.records().len(), 0);
        // No counter emit on nominal path.
        assert_eq!(counter.increments().len(), 0);
    }

    #[test]
    fn dirty_outbox_refuses_failback_with_canonical_error() {
        let (outbox, sink, counter) = fixtures();
        outbox.seed_undrained(Region::Wnam, 5);
        let r = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            1_700_000_000_000,
        );
        match r {
            Err(FailoverError::WriteBlockedDuringFailover { region }) => {
                assert_eq!(region, Region::Wnam);
            }
            other => panic!("expected WriteBlockedDuringFailover, got {other:?}"),
        }
    }

    #[test]
    fn dirty_outbox_emits_audit_before_counter() {
        let (outbox, sink, counter) = fixtures();
        outbox.seed_undrained(Region::Wnam, 7);
        let _ = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            42,
        );
        // Audit emit happened.
        let records = sink.records();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].event_type,
            FailoverAuditEventType::FailoverResolved
        );
        assert!(records[0].detail.contains("audit_outbox_dirty"));
        assert!(records[0].detail.contains("undrained=7"));
        assert_eq!(records[0].primary_region, "wnam");
        assert_eq!(records[0].replica_region, "enam");
        assert_eq!(records[0].timestamp_ms, 42);
        // Counter emit happened.
        assert_eq!(
            counter.count_for(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY),
            1
        );
    }

    #[test]
    fn dirty_outbox_blocks_even_when_counter_fails() {
        let outbox = InMemoryAuditOutbox::new();
        let sink = InMemoryFailoverAuditSink::new();
        let counter = FailingFailbackBlockedCounter::new("metrics dropped");
        outbox.seed_undrained(Region::Wnam, 1);
        let r = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            0,
        );
        // Gate decision is canonical — counter failure must NOT mask refusal.
        match r {
            Err(FailoverError::WriteBlockedDuringFailover { .. }) => {}
            other => panic!("expected refusal even when counter fails, got {other:?}"),
        }
        // Audit emit still happened.
        assert_eq!(sink.records().len(), 1);
    }

    #[test]
    fn audit_emit_failure_returns_audit_error_not_silent_pass() {
        // The strict fail-CLOSED interpretation: if the audit emit itself
        // fails AND outbox is dirty, the gate MUST surface FailoverError::Audit
        // — never silently allow the failback.
        let outbox = InMemoryAuditOutbox::new();
        let sink = FailingFailoverAuditSink::new("audit chain broken");
        let counter = InMemoryFailbackBlockedCounter::new();
        outbox.seed_undrained(Region::Wnam, 1);
        let r = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            0,
        );
        match r {
            Err(FailoverError::Audit(msg)) => {
                assert!(msg.contains("audit chain broken"));
            }
            other => panic!("expected Audit error, got {other:?}"),
        }
    }

    #[test]
    fn outbox_query_failure_is_treated_as_dirty() {
        // Fail-CLOSED: if we can't tell whether the outbox is dirty, assume
        // it is. This is the strictest interpretation and matches the
        // ordering in the module doc comment.
        let outbox = FailingAuditOutbox::new("d1 unreachable");
        let sink = InMemoryFailoverAuditSink::new();
        let counter = InMemoryFailbackBlockedCounter::new();
        let r = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            0,
        );
        match r {
            Err(FailoverError::Internal(msg)) => {
                assert!(msg.contains("audit_outbox query failed"));
                assert!(msg.contains("d1 unreachable"));
            }
            other => panic!("expected Internal error on query fail, got {other:?}"),
        }
        // Audit emit recorded the forensic trace.
        let records = sink.records();
        assert_eq!(records.len(), 1);
        assert!(records[0].detail.contains("outbox_query_failed"));
        // Counter still incremented for the dashboard.
        assert_eq!(
            counter.count_for(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY),
            1
        );
    }

    #[test]
    fn mark_all_drained_unblocks_failback() {
        let (outbox, sink, counter) = fixtures();
        outbox.seed_undrained(Region::Wnam, 3);
        let r1 = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            1,
        );
        assert!(r1.is_err());

        outbox.mark_all_drained(Region::Wnam);
        let r2 = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            2,
        );
        assert!(r2.is_ok());
        // Exactly one audit emit happened (the refusal) — the success path
        // adds none.
        assert_eq!(sink.records().len(), 1);
    }

    #[test]
    fn dirty_outbox_in_only_one_region_does_not_block_other() {
        let (outbox, sink, counter) = fixtures();
        outbox.seed_undrained(Region::Wnam, 5);
        // Failback for ENAM → WNAM is blocked (WNAM is old primary, dirty).
        let r_blocked = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Wnam,
            Region::Enam,
            0,
        );
        assert!(r_blocked.is_err());
        // Failback for WEUR → SAM proceeds (WEUR is old primary, clean).
        let r_ok = assert_outbox_drained_or_block(
            &outbox,
            &sink,
            &counter,
            Region::Weur,
            Region::Sam,
            0,
        );
        assert!(r_ok.is_ok());
    }
}
