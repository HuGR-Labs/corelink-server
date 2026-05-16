//! Wave-23 pilot onboarding — Test 4: DSR erasure.
//!
//! Exercises `POST /v1/dsr/erasure` → 7-day SLA window →
//! verification that all CAS + audit data was cleared (tombstone
//! audit event retained per CTRL-PRIV-ERASURE). Pins:
//!
//! - SLA window is exactly 7 days from request acceptance.
//! - Finalising BEFORE the SLA elapses fails with
//!   `DsrErasureError::SlaWindowOpen`.
//! - Finalising AFTER the SLA elapses drains every CAS blob and
//!   collapses the audit chain to a single tombstone row whose
//!   `kind` is `DsrErasureCompleted`.
//! - Duplicate erasure requests are rejected.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use e2e_pilot_onboarding::{
    canonical_blob_payloads, canonical_pilot_tenant, AuditEventKind, DsrErasureError, PilotHarness,
    PilotHarnessError, DSR_ERASURE_WINDOW_MS, FIXED_NOW_MS, ONE_DAY_MS,
};

#[test]
fn wave23_pilot_dsr_erasure_full_lifecycle() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-dsr-happy");

    h.complete_signup(&tenant).unwrap();
    let payloads = canonical_blob_payloads(&tenant.slug);
    h.batch_upload_blobs(&tenant, &payloads).unwrap();
    assert_eq!(h.cas_blob_count(&tenant), 100);
    let pre_audit_count = h.audit_row_count(&tenant);
    assert!(pre_audit_count > 100);

    // Submit DSR erasure.
    let receipt = h
        .request_dsr_erasure(&tenant, "dsr-req-pilot-001")
        .unwrap();
    assert_eq!(receipt.request_id, "dsr-req-pilot-001");
    assert_eq!(
        receipt.sla_deadline_ms,
        FIXED_NOW_MS + DSR_ERASURE_WINDOW_MS
    );

    // Premature finalisation — SLA still open.
    let early_err = h
        .finalise_dsr_erasure(&tenant, FIXED_NOW_MS + ONE_DAY_MS)
        .unwrap_err();
    let remaining = match early_err {
        PilotHarnessError::DsrErasure(DsrErasureError::SlaWindowOpen { remaining_ms }) => {
            remaining_ms
        }
        other => panic!("expected SlaWindowOpen, got {other:?}"),
    };
    assert_eq!(remaining, DSR_ERASURE_WINDOW_MS - ONE_DAY_MS);
    // CAS untouched.
    assert_eq!(h.cas_blob_count(&tenant), 100);

    // Finalise after the 7-day window.
    h.finalise_dsr_erasure(&tenant, FIXED_NOW_MS + DSR_ERASURE_WINDOW_MS)
        .unwrap();

    // CAS fully drained.
    assert_eq!(h.cas_blob_count(&tenant), 0);

    // Audit chain collapsed to the single tombstone row.
    let rows = h.audit_snapshot(&tenant);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, AuditEventKind::DsrErasureCompleted);
    // Tombstone is anchored to genesis (zero prev-hash) so the
    // post-erasure chain is internally consistent.
    assert_eq!(rows[0].prev_hash_hex, "0".repeat(64));
    assert_eq!(rows[0].seq, 0);
}

#[test]
fn wave23_pilot_dsr_duplicate_request_rejected() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-dsr-dupe");
    h.complete_signup(&tenant).unwrap();
    h.request_dsr_erasure(&tenant, "dsr-pilot-dupe-001").unwrap();
    let err = h
        .request_dsr_erasure(&tenant, "dsr-pilot-dupe-001")
        .unwrap_err();
    assert!(
        matches!(
            err,
            PilotHarnessError::DsrErasure(DsrErasureError::AlreadyRequested(_))
        ),
        "got: {err:?}"
    );
}

#[test]
fn wave23_pilot_dsr_without_signup_rejected() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-dsr-no-signup");
    let err = h.request_dsr_erasure(&tenant, "dsr-no-signup").unwrap_err();
    assert!(
        matches!(
            err,
            PilotHarnessError::DsrErasure(DsrErasureError::TenantNotProvisioned(_))
        ),
        "got: {err:?}"
    );
}
