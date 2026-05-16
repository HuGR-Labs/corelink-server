//! Scenario 4: RLS GUC dropped mid-transaction.
//!
//! Hypothesis: if `app.current_tenant` is unset mid-tx, the `WITH CHECK`
//! policy fails CLOSED — no row materializes for any tenant scope.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignRlsTable, RlsInsertOutcome,
};

#[test]
fn dropped_session_guc_rejects_insert_via_with_check() {
    let mut table = CampaignRlsTable::new();
    table.set_session_tenant("tenant-A");

    // Baseline — INSERT lands.
    assert_eq!(
        table.insert("tenant-A", "row-1"),
        RlsInsertOutcome::Inserted
    );
    assert_eq!(table.rows_for("tenant-A"), vec!["row-1"]);

    // Inject GUC dropout mid-tx.
    table.drop_session_tenant();

    // (1) INSERT fails CLOSED.
    assert_eq!(
        table.insert("tenant-A", "row-2"),
        RlsInsertOutcome::PolicyRejected,
        "WITH CHECK must reject when session GUC is unset"
    );

    // Cross-tenant claim with no GUC also rejected (defence in depth).
    assert_eq!(
        table.insert("tenant-B", "row-3"),
        RlsInsertOutcome::PolicyRejected
    );

    // No new rows visible to any tenant.
    assert_eq!(table.rows_for("tenant-A"), vec!["row-1"]);
    assert!(table.rows_for("tenant-B").is_empty());

    // (2) Audit event emitted (at least once across the rejections).
    assert!(
        table
            .audit_events()
            .iter()
            .any(|e| e == "corelink.rls.guc.dropped"),
        "expected RLS dropout audit event, found {:?}",
        table.audit_events()
    );

    // (3) SEV-1 alert fired.
    assert_alert_fired(table.sev1_alerts(), "rls_policy_violation_attempt").unwrap();

    // Restoring the GUC re-enables INSERTs (recovery path).
    table.set_session_tenant("tenant-A");
    assert_eq!(
        table.insert("tenant-A", "row-4"),
        RlsInsertOutcome::Inserted
    );
}

#[test]
fn audit_event_taxonomy_canonical_once_per_violation() {
    let mut table = CampaignRlsTable::new();
    table.set_session_tenant("tenant-A");
    table.drop_session_tenant();

    let _ = table.insert("tenant-A", "row-x");

    assert_audit_emitted_once(table.audit_events(), "corelink.rls.guc.dropped").unwrap();
}
