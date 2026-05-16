//! Combined Scenario C (wave-23): Neon shadow audit sink silent failure
//! while a concurrent audit-export trailer pass is running.
//!
//! Composes Scenario 3 (`CampaignAuditDualWrite`) from the wave-22
//! harness with an in-file model of the audit-export trailer
//! emission. The export trailer is the canonical "this batch of rows
//! has been archived and reconciled" record; it must continue to emit
//! even while the shadow sink is silently dropping rows, because the R2
//! archive — the primary durability surface — remains healthy.
//!
//! Steady-state hypothesis:
//!
//! 1. R2 archive writes continue to land for every audit row, even
//!    while the Neon shadow sink silently drops them.
//! 2. The audit-export trailer for the concurrent export pass emits and
//!    seals — the export does not block on shadow-sink health.
//! 3. The reconciler detects shadow drift and fires
//!    `audit_shadow_sink_drift` SEV-2 exactly once.
//!
//! Wave-22 dress rehearsal noted that a silent shadow-sink failure
//! during a long-running export window risked the export trailer
//! waiting on a sink that would never catch up. This wave-23 scenario
//! pins the contract that the export trailer is gated on R2 archive
//! health only — the shadow sink is best-effort and its drift is a
//! SEV-2 reconcile-loop signal, not an export blocker.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignAuditDualWrite, SinkPersistResult,
};

/// Minimal export-trailer model: collects archived rows from the
/// primary R2 surface and emits a trailer once the batch is sealed.
/// Independent of the shadow sink — that is the cross-failure contract.
#[derive(Debug, Default)]
struct ExportTrailerModel {
    archived: Vec<String>,
    trailer_emitted: bool,
}

impl ExportTrailerModel {
    fn new() -> Self {
        Self::default()
    }

    /// Record an R2-archived row as part of the current export batch.
    fn record_archived(&mut self, row: &str) {
        self.archived.push(row.to_string());
    }

    /// Seal the batch and emit the trailer. Returns the row count.
    fn seal_trailer(&mut self) -> usize {
        self.trailer_emitted = true;
        self.archived.len()
    }

    fn trailer_emitted(&self) -> bool {
        self.trailer_emitted
    }
}

#[test]
fn neon_shadow_failure_during_audit_export_keeps_r2_and_trailer_healthy() {
    let mut dual = CampaignAuditDualWrite::new();
    let mut export = ExportTrailerModel::new();

    // --- Warm-up: a few rows under steady state — both surfaces persist.
    for row in ["row-1", "row-2"] {
        let res = dual.emit(row);
        assert_eq!(res, SinkPersistResult::Persisted);
        export.record_archived(row);
    }

    // --- Inject the shadow-sink silent failure mid-export window.
    dual.inject_shadow_silent_failure();

    // --- Continue emitting audit rows while the export pass is running.
    // R2 must continue to accept every row; shadow returns SilentDrop.
    for row in ["row-3", "row-4", "row-5"] {
        let res = dual.emit(row);
        assert_eq!(
            res,
            SinkPersistResult::SilentDrop,
            "shadow must report silent-drop while injection is active"
        );
        // R2 is the canonical primary — record it on the export trailer.
        export.record_archived(row);
    }

    // --- R2 archive holds every row (primary durability intact).
    assert_eq!(
        dual.r2_archive().len(),
        5,
        "R2 archive must hold every emitted row across the chaos window"
    );
    // --- Shadow holds only the pre-injection rows.
    assert_eq!(
        dual.neon_shadow().len(),
        2,
        "shadow sink must reflect the silent-drop count post-injection"
    );

    // --- The audit export trailer continues to seal — the export does
    // NOT block on shadow-sink health.
    let trailer_count = export.seal_trailer();
    assert_eq!(
        trailer_count, 5,
        "export trailer must seal with the full R2 row count under combined chaos"
    );
    assert!(
        export.trailer_emitted(),
        "export trailer must emit even while shadow sink is silently dropping"
    );

    // --- Reconcile pass detects shadow drift and fires SEV-2.
    let reconciled = dual.reconcile();
    assert!(
        !reconciled,
        "reconcile must report drift while shadow sink lags R2"
    );

    assert_audit_emitted_once(
        dual.audit_events(),
        "corelink.audit.shadow_sink.silent_failure",
    )
    .unwrap();
    assert_alert_fired(dual.sev2_alerts(), "audit_shadow_sink_drift").unwrap();

    // --- Recovery: a fresh model represents the post-replay state after
    // the shadow-sink replay job has caught up. Both surfaces hold the
    // same five rows and reconcile passes.
    let mut healed = CampaignAuditDualWrite::new();
    for row in ["row-1", "row-2", "row-3", "row-4", "row-5"] {
        assert_eq!(healed.emit(row), SinkPersistResult::Persisted);
    }
    assert!(
        healed.reconcile(),
        "post-replay reconcile must pass — both surfaces aligned"
    );
}
