//! GC-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a thin trait surface
//!
//! Mirrors `corelink-worker::reapi::cas::audit` + `reapi::ac::audit`
//! patterns: a small AC-flavoured sink trait the production wiring
//! composes on top of the `audit_outbox` (WI-S01-004) row insert. The
//! S-09 audit chain processor will lift these records into the
//! canonical `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S06-001 §6.1.8 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.gc.run_started`
//! - `corelink.gc.run_completed`
//! - `corelink.gc.run_aborted` (degrade-mode)
//! - `corelink.gc.phase_transitioned`

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::region::GcRegion;
use crate::run::{GcPhase, GcStatus, RunId};

/// Canonical GC audit taxonomy. The `#[non_exhaustive]` marker reserves
/// additive growth for follow-on WIs (mark / sweep / physical-delete
/// sub-events lift into S-09 with the chain processor).
///
/// WI-S06-001 §6.1.8 froze the original 4-event run-lifecycle taxonomy
/// (`RunStarted` / `RunCompleted` / `RunAborted` / `PhaseTransitioned`).
/// WI-S06-001 polish added `RunFailed`. **WI-S06-003** extends the
/// taxonomy with sweep-decision events `SweepSoftDeleted` +
/// `SweepProtectedReRef` per §6.1.7 — sweep emits ONE event per
/// candidate decision so the S-09 audit chain processor can reconstruct
/// the soft-delete forensic trail (and the INV-GC-004 protect-if-`>=`
/// catches) from the durable outbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum GcEventType {
    /// `corelink.gc.run_started` — `Pending → Running` transition.
    RunStarted,
    /// `corelink.gc.run_completed` — terminal `Running → Succeeded`.
    RunCompleted,
    /// `corelink.gc.run_aborted` — terminal `Running → Aborted` via
    /// degrade-mode `gc-pause`.
    RunAborted,
    /// `corelink.gc.phase_transitioned` — phase moves within
    /// `Running` (e.g. `Mark → Sweep`).
    PhaseTransitioned,
    /// `corelink.gc.run_failed` — terminal `Running → Failed`
    /// (PhaseBudgetExceeded / non-recoverable PhaseFailure).
    RunFailed,
    /// `corelink.gc.sweep.soft_deleted` (WI-S06-003) — sweep phase
    /// soft-deleted a confirmed orphan blob.
    SweepSoftDeleted,
    /// `corelink.gc.sweep.protected_re_ref` (WI-S06-003) — sweep phase
    /// detected a re-reference fired during mark (canonical TLA
    /// `gc_correctness.tla` L152-154 protect-if-`>=`); did NOT delete.
    SweepProtectedReRef,
    /// `corelink.gc.physical_deleted` (WI-S06-004) — physical-delete
    /// phase removed a soft-deleted blob from R2 + D1 after the grace
    /// window expired. Emitted ONCE per confirmed purge (the
    /// `Purged` decision arm); skipped/race-protected/idempotent
    /// arms do NOT emit. NOT SEV-1 — alerts fire at the metric layer
    /// (>5% sustained `r2_failed_total` per WI §6.1.11). Deletion is
    /// irreversible, so the forensic trail per row is the load-bearing
    /// observability invariant.
    PhysicalDeleted,
    /// `corelink.gc.reconcile.refcount_reconciled` (WI-S06-005) —
    /// reconcile phase observed `expected_refcount == stored_refcount`
    /// for a `(tenant, digest)` pair (no drift). Emitted on the
    /// `NoDrift` decision arm. Forensic trail per row gives operators
    /// the canonical signal sustained drift < 0.1% (sprint contract DoD
    /// §10.s06.5).
    RefcountReconciled,
    /// `corelink.gc.reconcile.refcount_auto_fixed` (WI-S06-005) —
    /// reconcile phase detected a small drift (`drift_count ≤ 5 AND
    /// drift_percent ≤ 0.01%` per Lote 10.6bis P0-6 dual-condition
    /// gate) and auto-corrected `blob_meta.refcount` via UPDATE.
    /// Emitted BEFORE the UPDATE (fail-closed envelope; production
    /// wiring rolls back the D1 batch on emit failure). NOT SEV-1 —
    /// alerts fire at the metric layer (>0.1% global drift sustained
    /// per WI §6.1.8).
    RefcountAutoFixed,
    /// `corelink.gc.reconcile.refcount_manual_review_required`
    /// (WI-S06-005) — reconcile phase detected a drift exceeding the
    /// auto-fix gate (`drift_count > 5 OR drift_percent > 0.01%`).
    /// Auto-fix is paused; the row is flagged for SRE review via the
    /// audit chain. SEV-1 because per-tenant drift > 1% indicates a
    /// systemic refcount-write bug (UpdateAR / DeleteAR / Sweep) and
    /// reachable computation is at risk.
    RefcountManualReviewRequired,
}

impl GcEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStarted => "corelink.gc.run_started",
            Self::RunCompleted => "corelink.gc.run_completed",
            Self::RunAborted => "corelink.gc.run_aborted",
            Self::PhaseTransitioned => "corelink.gc.phase_transitioned",
            Self::RunFailed => "corelink.gc.run_failed",
            Self::SweepSoftDeleted => "corelink.gc.sweep.soft_deleted",
            Self::SweepProtectedReRef => "corelink.gc.sweep.protected_re_ref",
            Self::PhysicalDeleted => "corelink.gc.physical_deleted",
            Self::RefcountReconciled => "corelink.gc.reconcile.refcount_reconciled",
            Self::RefcountAutoFixed => "corelink.gc.reconcile.refcount_auto_fixed",
            Self::RefcountManualReviewRequired => {
                "corelink.gc.reconcile.refcount_manual_review_required"
            }
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter`
    /// fan-out). `RunAborted` + `RunFailed` are SEV-1 because a
    /// degrade-mode abort or terminal failure both warrant operator
    /// pager wake-up. `RefcountManualReviewRequired` (WI-S06-005) is
    /// SEV-1 because per-tenant drift > 1% indicates a systemic
    /// refcount-write bug at risk of cascading to INV-GC-001
    /// (mark-phase reachable computation reads stale refcount). Sweep
    /// events + `RefcountReconciled` + `RefcountAutoFixed` are NOT
    /// SEV-1 — alerts fire at the metric layer (sustained per-tenant
    /// drift > 0.1% per WI §6.1.8 / §10.s06.5).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(
            self,
            Self::RunAborted | Self::RunFailed | Self::RefcountManualReviewRequired
        )
    }
}

impl core::fmt::Display for GcEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list (11 entries) for cross-component
/// regression tests + dashboard widget configuration. WI-S06-003
/// extended the list from 5 → 7 with the sweep-decision events;
/// WI-S06-004 → 8 with `physical_deleted`; WI-S06-005 → 11 with the
/// reconcile decision events.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 11] {
    &[
        "corelink.gc.run_started",
        "corelink.gc.run_completed",
        "corelink.gc.run_aborted",
        "corelink.gc.phase_transitioned",
        "corelink.gc.run_failed",
        "corelink.gc.sweep.soft_deleted",
        "corelink.gc.sweep.protected_re_ref",
        "corelink.gc.physical_deleted",
        "corelink.gc.reconcile.refcount_reconciled",
        "corelink.gc.reconcile.refcount_auto_fixed",
        "corelink.gc.reconcile.refcount_manual_review_required",
    ]
}

/// Typed GC audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the
/// envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcAuditRecord {
    /// Canonical event type.
    pub event_type: GcEventType,
    /// Run identity.
    pub run_id: RunId,
    /// Verified tenant id.
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: GcRegion,
    /// Status at the moment of emission.
    pub status: GcStatus,
    /// Phase-from (None for `RunStarted`; canonical for
    /// `PhaseTransitioned`).
    pub from_phase: Option<GcPhase>,
    /// Phase-to (None for `RunStarted`; canonical for
    /// `PhaseTransitioned` / terminal events).
    pub to_phase: Option<GcPhase>,
    /// Source attribution: `"cron"` for scheduled tick OR admin PAT
    /// id hex prefix.
    pub created_by_request_id: String,
    /// Short canonical reason code (e.g. `"degrade_mode_gc_pause"` /
    /// `"phase_budget_exceeded"`). Empty when no extra context.
    pub reason: &'static str,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`GcAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GcAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout).
    #[error("gc audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the gc_run row mutation (S-01 audit_outbox table).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (forward; SEV-1 variants `RunAborted` +
///   `RunFailed` are wired here).
pub trait GcAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to
    /// 503 so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`GcAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: GcAuditRecord) -> Result<(), GcAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryGcAuditSink {
    inner: std::sync::Arc<Mutex<Vec<GcAuditRecord>>>,
}

impl InMemoryGcAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<GcAuditRecord> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of records captured.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Filter snapshot down to records of a single event type.
    #[must_use]
    pub fn snapshot_of(&self, event_type: GcEventType) -> Vec<GcAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl GcAuditSink for InMemoryGcAuditSink {
    fn emit(&self, record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| GcAuditSinkError::Store("audit sink mutex poisoned".to_string()))?;
        guard.push(record);
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

    fn rec(t: GcEventType) -> GcAuditRecord {
        GcAuditRecord {
            event_type: t,
            run_id: RunId(Uuid::nil()),
            tenant_id: Uuid::nil(),
            region: GcRegion::Sam,
            status: GcStatus::Pending,
            from_phase: None,
            to_phase: None,
            created_by_request_id: "cron".into(),
            reason: "",
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            GcEventType::RunStarted,
            GcEventType::RunCompleted,
            GcEventType::RunAborted,
            GcEventType::PhaseTransitioned,
            GcEventType::RunFailed,
            GcEventType::SweepSoftDeleted,
            GcEventType::SweepProtectedReRef,
            GcEventType::PhysicalDeleted,
            GcEventType::RefcountReconciled,
            GcEventType::RefcountAutoFixed,
            GcEventType::RefcountManualReviewRequired,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.gc."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {}", t);
        }
        assert_eq!(set.len(), 11);
    }

    #[test]
    fn sev1_taxonomy_is_subset() {
        assert!(GcEventType::RunAborted.is_sev1());
        assert!(GcEventType::RunFailed.is_sev1());
        assert!(GcEventType::RefcountManualReviewRequired.is_sev1());
        assert!(!GcEventType::RunStarted.is_sev1());
        assert!(!GcEventType::RunCompleted.is_sev1());
        assert!(!GcEventType::PhaseTransitioned.is_sev1());
        assert!(!GcEventType::RefcountReconciled.is_sev1());
        assert!(!GcEventType::RefcountAutoFixed.is_sev1());
    }

    #[test]
    fn sink_round_trip() {
        let sink = InMemoryGcAuditSink::new();
        sink.emit(rec(GcEventType::RunStarted)).unwrap();
        sink.emit(rec(GcEventType::RunCompleted)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(GcEventType::RunStarted).len(), 1);
        assert_eq!(sink.snapshot_of(GcEventType::RunCompleted).len(), 1);
    }

    #[test]
    fn canonical_event_strings_cover_taxonomy() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 11);
        for s in canonical {
            assert!(s.starts_with("corelink.gc."));
        }
    }

    #[test]
    fn reconcile_event_strings_match_taxonomy() {
        assert_eq!(
            GcEventType::RefcountReconciled.as_str(),
            "corelink.gc.reconcile.refcount_reconciled"
        );
        assert_eq!(
            GcEventType::RefcountAutoFixed.as_str(),
            "corelink.gc.reconcile.refcount_auto_fixed"
        );
        assert_eq!(
            GcEventType::RefcountManualReviewRequired.as_str(),
            "corelink.gc.reconcile.refcount_manual_review_required"
        );
        // RefcountManualReviewRequired IS SEV-1; the auto-fix +
        // reconciled events are NOT — alerts fire at the metric layer.
        assert!(!GcEventType::RefcountReconciled.is_sev1());
        assert!(!GcEventType::RefcountAutoFixed.is_sev1());
        assert!(GcEventType::RefcountManualReviewRequired.is_sev1());
    }

    #[test]
    fn sweep_event_strings_match_taxonomy() {
        assert_eq!(
            GcEventType::SweepSoftDeleted.as_str(),
            "corelink.gc.sweep.soft_deleted"
        );
        assert_eq!(
            GcEventType::SweepProtectedReRef.as_str(),
            "corelink.gc.sweep.protected_re_ref"
        );
        // Sweep events are NOT SEV-1 — alerts fire at the metric layer.
        assert!(!GcEventType::SweepSoftDeleted.is_sev1());
        assert!(!GcEventType::SweepProtectedReRef.is_sev1());
    }
}
