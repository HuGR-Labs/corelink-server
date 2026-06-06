//! Scenario 3: Neon shadow audit sink silent failure.
//!
//! Hypothesis: when the shadow sink silently drops rows (returns Ok
//! but persists nothing), the dual-write reconcile pass detects the
//! drift, fires SEV-2, and the R2 archive continues to accept writes
//! (the canonical primary remains durable).

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

#[test]
fn shadow_silent_failure_flags_drift_and_keeps_r2_writes() {
    let mut audit = CampaignAuditDualWrite::new();

    // Baseline — both sinks accept rows; reconcile passes.
    assert_eq!(audit.emit("row-1"), SinkPersistResult::Persisted);
    assert!(audit.reconcile());

    // Inject the silent failure on the shadow sink.
    audit.inject_shadow_silent_failure();

    // R2 still writes; shadow silently drops.
    assert_eq!(audit.emit("row-2"), SinkPersistResult::SilentDrop);
    assert_eq!(audit.emit("row-3"), SinkPersistResult::SilentDrop);

    // (1) R2 archive remains durable — primary fail-CLOSED property.
    assert_eq!(
        audit.r2_archive().len(),
        3,
        "R2 archive must keep accepting rows"
    );
    assert_eq!(
        audit.neon_shadow().len(),
        1,
        "shadow sink saw only the pre-chaos row"
    );

    // Reconcile detects drift.
    assert!(!audit.reconcile(), "reconcile must flag drift");

    // (2) Audit event emitted.
    assert_audit_emitted_once(
        audit.audit_events(),
        "corelink.audit.shadow_sink.silent_failure",
    )
    .unwrap();

    // (3) SEV-2 alert fired.
    assert_alert_fired(audit.sev2_alerts(), "audit_shadow_sink_drift").unwrap();
}
