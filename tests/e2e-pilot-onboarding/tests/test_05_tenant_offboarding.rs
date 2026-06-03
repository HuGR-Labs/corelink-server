//! Wave-23 pilot onboarding — Test 5: tenant offboarding.
//!
//! Exercises subscription cancellation → 30-day grace window →
//! final hard-delete + erasure verification. Pins:
//!
//! - Subscription cancellation transitions lifecycle to
//!   `CancelledInGrace { grace_ends_at_ms }`.
//! - Completing offboarding BEFORE the grace window elapses fails
//!   with `OffboardingError::GraceNotElapsed`.
//! - Completing offboarding AFTER the grace window drains the CAS
//!   and audit chain (residual = 0; INV-OFFBOARDING-CLEAN).
//! - Final lifecycle is `HardDeleted`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use e2e_pilot_onboarding::{
    canonical_blob_payloads, canonical_pilot_tenant, OffboardingError, PilotHarness,
    PilotHarnessError, SubscriptionState, TenantLifecycleState, FIXED_NOW_MS, OFFBOARDING_GRACE_MS,
    ONE_DAY_MS,
};

#[test]
fn wave23_pilot_offboarding_full_lifecycle() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-offboard-happy");

    h.complete_signup(&tenant).unwrap();
    let payloads = canonical_blob_payloads(&tenant.slug);
    h.batch_upload_blobs(&tenant, &payloads).unwrap();
    assert_eq!(h.cas_blob_count(&tenant), 100);

    // Cancel subscription.
    h.cancel_subscription(&tenant).unwrap();
    assert_eq!(h.subscription_state(&tenant), SubscriptionState::Cancelled);
    match h.lifecycle_state(&tenant) {
        Some(TenantLifecycleState::CancelledInGrace { grace_ends_at_ms }) => {
            assert_eq!(grace_ends_at_ms, FIXED_NOW_MS + OFFBOARDING_GRACE_MS);
        }
        other => panic!("expected CancelledInGrace, got {other:?}"),
    }

    // Premature finalisation — grace window still open.
    let early_err = h
        .complete_offboarding(&tenant, FIXED_NOW_MS + ONE_DAY_MS)
        .unwrap_err();
    let remaining = match early_err {
        PilotHarnessError::Offboarding(OffboardingError::GraceNotElapsed { remaining_ms }) => {
            remaining_ms
        }
        other => panic!("expected GraceNotElapsed, got {other:?}"),
    };
    assert_eq!(remaining, OFFBOARDING_GRACE_MS - ONE_DAY_MS);
    // CAS untouched while in grace.
    assert_eq!(h.cas_blob_count(&tenant), 100);

    // Complete after the 30-day window.
    let receipt = h
        .complete_offboarding(&tenant, FIXED_NOW_MS + OFFBOARDING_GRACE_MS)
        .unwrap();
    assert_eq!(receipt.tenant_id, tenant.tenant_id);
    assert_eq!(receipt.residual_cas_blobs, 0);
    assert_eq!(receipt.residual_audit_rows, 0);

    // INV-OFFBOARDING-CLEAN: zero residual.
    assert_eq!(h.cas_blob_count(&tenant), 0);
    assert_eq!(h.audit_row_count(&tenant), 0);

    // Final lifecycle.
    assert!(matches!(
        h.lifecycle_state(&tenant),
        Some(TenantLifecycleState::HardDeleted)
    ));
}

#[test]
fn wave23_pilot_offboarding_without_cancel_rejected() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-offboard-no-cancel");
    h.complete_signup(&tenant).unwrap();

    let err = h
        .complete_offboarding(&tenant, FIXED_NOW_MS + OFFBOARDING_GRACE_MS)
        .unwrap_err();
    assert!(
        matches!(
            err,
            PilotHarnessError::Offboarding(OffboardingError::SubscriptionNotCancelled(_))
        ),
        "got: {err:?}"
    );
}

#[test]
fn wave23_pilot_offboarding_after_dsr_erasure_already_clean() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-offboard-post-dsr");

    h.complete_signup(&tenant).unwrap();
    let payloads = canonical_blob_payloads(&tenant.slug);
    h.batch_upload_blobs(&tenant, &payloads).unwrap();

    // DSR erasure first.
    h.request_dsr_erasure(&tenant, "dsr-offboard-001").unwrap();
    h.finalise_dsr_erasure(&tenant, FIXED_NOW_MS + 7 * ONE_DAY_MS)
        .unwrap();
    assert_eq!(h.cas_blob_count(&tenant), 0);

    // Then cancel + complete offboarding.
    h.cancel_subscription(&tenant).unwrap();
    let receipt = h
        .complete_offboarding(&tenant, FIXED_NOW_MS + OFFBOARDING_GRACE_MS)
        .unwrap();
    // Erasure already wiped everything — residual stays at 0.
    assert_eq!(receipt.residual_cas_blobs, 0);
    assert_eq!(receipt.residual_audit_rows, 0);
    assert!(matches!(
        h.lifecycle_state(&tenant),
        Some(TenantLifecycleState::HardDeleted)
    ));
}
