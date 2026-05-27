//! R3-2 Happy revoke flow — INV-BYOK-CRYPTO-SOVEREIGNTY assertion #1.
//!
//! Pipeline:
//!
//! 1. Build BYOK env for AWS KMS (default provider for the happy path).
//! 2. Seed DEK cache with a wrapped DEK (simulates an active customer
//!    blob's cached envelope).
//! 3. Provider returns `KmsAccessStatus::Ok` first cycle — no kill switch.
//! 4. Customer revokes CMK: provider flips to `AlwaysRevoked`.
//! 5. Next cycle: kill switch fires — DEK cache evicted, tenant
//!    `byok_status = 'degraded_read_only'`, audit
//!    `corelink.byok.cmk_revoked` emitted, customer alert dispatched.
//! 6. Kill-switch SLA ≤ 30 s asserted (in-memory wiring runs in <50 ms).
//! 7. Public `RevocationDetector::run_one_cycle` is exercised once to
//!    assert the upstream API surface compiles + runs cleanly with the
//!    harness wiring.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_byok::{Dek, KmsProvider, KmsProviderKind};
use corelink_byok::revocation::{
    event::EVENT_TYPE_CMK_REVOKED, store::TenantByokStatus, testutil::NoopAlerter,
    RevocationConfig, RevocationDetector,
};
use e2e_byok_revoke::helpers::{
    make_wrapped_for, KillSwitchRunner, KmsBehaviour, KILL_SWITCH_SLA_MS,
};
use e2e_byok_revoke::setup_byok_env;

#[tokio::test]
async fn happy_revoke_flow_aws() {
    let bundle = setup_byok_env(KmsProviderKind::AwsKms);

    // (1) Seed cache with 3 active DEKs for this CMK.
    for i in 0..3u32 {
        let wrapped = make_wrapped_for(&bundle.key_id, i);
        let dek = Dek::generate().unwrap();
        bundle.dek_cache.put(&wrapped, dek).await.unwrap();
    }
    assert_eq!(bundle.dek_cache.len().await, 3, "3 entries pre-revoke");

    // (2) First cycle: provider Ok → no kill switch.
    let event = KillSwitchRunner::run(&bundle).await.unwrap();
    assert!(event.is_none(), "no audit event on Ok before any degrade");
    assert_eq!(bundle.audit.len(), 0);
    assert_eq!(bundle.alert.alert_count(), 0);
    assert_eq!(
        bundle.dek_cache.len().await,
        3,
        "cache intact when CMK is healthy"
    );

    // (3) Customer revokes in BYOK provider.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);

    // (4) Next cycle: kill switch fires.
    let event = KillSwitchRunner::run(&bundle)
        .await
        .expect("kill switch executed")
        .expect("audit event emitted");
    assert_eq!(event.event_type, EVENT_TYPE_CMK_REVOKED);
    assert_eq!(event.provider, "aws");
    assert_eq!(event.evicted_dek_count, 3);
    assert!(
        event.kill_switch_duration_ms <= KILL_SWITCH_SLA_MS,
        "kill switch SLA violated: {} ms > {} ms",
        event.kill_switch_duration_ms,
        KILL_SWITCH_SLA_MS
    );

    // (5) DEK cache fully evicted (INV-BYOK-CRYPTO-SOVEREIGNTY).
    assert_eq!(
        bundle.dek_cache.len().await,
        0,
        "DEK cache must be fully evicted post-revoke"
    );

    // (6) Tenant byok_status = degraded_read_only.
    assert_eq!(
        bundle.tenant_store.current(&bundle.key_id.key_arn_or_id),
        Some(TenantByokStatus::DegradedReadOnly),
        "tenant must be degraded_read_only post-revoke"
    );

    // (7) Audit sink saw exactly one cmk_revoked event.
    let types = bundle.audit.snapshot_event_types();
    assert_eq!(types, vec![EVENT_TYPE_CMK_REVOKED.to_string()]);

    // (8) Customer alert dispatched exactly once.
    assert_eq!(bundle.alert.alert_count(), 1);
    let alerts = bundle.alert.alerts();
    assert_eq!(alerts[0].provider, "aws");
    assert_eq!(alerts[0].kms_key_id, bundle.key_id);
    assert!(
        alerts[0].recovery_instructions.contains("aws"),
        "alert payload must include provider name"
    );

    // (9) Re-running the cycle on already-revoked + already-degraded
    // tenant is idempotent — additional alerts are dispatched (each
    // revoke check confirms revocation) but state never regresses.
    let _evt2 = KillSwitchRunner::run(&bundle).await.unwrap();
    assert_eq!(
        bundle.tenant_store.current(&bundle.key_id.key_arn_or_id),
        Some(TenantByokStatus::DegradedReadOnly),
        "state must remain degraded_read_only on repeat revoke check"
    );
    assert_eq!(
        bundle.dek_cache.len().await,
        0,
        "cache remains empty across repeated revoke cycles"
    );
}

/// Smoke-check that `RevocationDetector::run_one_cycle` (the production
/// upstream entry point) wires cleanly with the harness `BoundedKms
/// Provider`. The detector's `list_active_byok_keys` is a stub returning
/// `Vec::new()` so this is a no-op cycle — but it asserts the public
/// API surface compiles + runs without panicking under our wiring.
#[tokio::test]
async fn happy_revoke_detector_run_one_cycle_smoke() {
    let bundle = setup_byok_env(KmsProviderKind::AwsKms);
    let detector = RevocationDetector::new(
        vec![Arc::clone(&bundle.provider) as Arc<dyn KmsProvider>],
        Arc::clone(&bundle.dek_cache),
        Arc::clone(&bundle.tenant_store) as Arc<dyn corelink_byok::revocation::TenantStatusStore>,
        Arc::new(NoopAlerter) as Arc<dyn corelink_byok::revocation::CustomerAlerter>,
        RevocationConfig::default(),
    );
    detector
        .run_one_cycle()
        .await
        .expect("detector cycle clean against in-memory wiring");
}
