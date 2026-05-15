//! R3-2 Adversarial: transient `ApiError` MUST NOT trigger kill switch.
//!
//! Lote 10.14 codex P0 regression: a network timeout / 5xx from the
//! KMS provider must NOT be treated as a permanent revocation. The
//! detector tolerates `sustained_failure_threshold` (default 3) cycles
//! of transient failure before conservatively degrading the tenant. The
//! kill switch fires only on `Revoked` / `NotFound` — never on
//! `Throttled` / `ApiError`.
//!
//! Assertions:
//!
//! - 3 cycles of `ApiError(503)` → audit sink empty, tenant status
//!   unset, cache intact, no customer alert dispatched.
//! - 3 cycles of `Throttled` → same.
//! - Mixed scripted sequence — `[ApiError(504), Throttled,
//!   ApiError(502)]` followed by `Ok` → still no kill switch event,
//!   audit chain empty.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_byok::{Dek, KmsAccessStatus, KmsProviderKind};
use e2e_byok_revoke::helpers::{make_wrapped_for, KillSwitchRunner, KmsBehaviour};
use e2e_byok_revoke::setup_byok_env;

#[tokio::test]
async fn three_cycles_api_error_no_kill_switch() {
    let bundle = setup_byok_env(KmsProviderKind::HashicorpVault);

    // Pre-seed cache so we can prove it stays intact.
    for i in 0..7u32 {
        let wrapped = make_wrapped_for(&bundle.key_id, i);
        let dek = Dek::generate().unwrap();
        bundle.dek_cache.put(&wrapped, dek).await.unwrap();
    }
    let pre_len = bundle.dek_cache.len().await;
    assert_eq!(pre_len, 7);

    // Simulate 3 consecutive ApiError(503) cycles.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysApiError(503));
    for _ in 0..3 {
        let event = KillSwitchRunner::run(&bundle).await.unwrap();
        assert!(
            event.is_none(),
            "ApiError MUST NOT trigger kill switch (Lote 10.14 codex P0)"
        );
    }

    // No audit events, no customer alerts, cache intact, tenant has no
    // status entry yet.
    assert!(
        bundle.audit.is_empty(),
        "audit sink must be empty on transient errors"
    );
    assert_eq!(bundle.alert.alert_count(), 0);
    assert_eq!(bundle.alert.recovery_count(), 0);
    assert_eq!(bundle.dek_cache.len().await, 7, "cache intact");
    assert!(
        bundle
            .tenant_store
            .current(&bundle.key_id.key_arn_or_id)
            .is_none(),
        "tenant status untouched by transient errors"
    );

    // Sanity: 3 check_access calls were observed.
    assert_eq!(bundle.provider.check_access_count(), 3);
}

#[tokio::test]
async fn three_cycles_throttled_no_kill_switch() {
    let bundle = setup_byok_env(KmsProviderKind::GcpKms);

    bundle.provider.set_behaviour(KmsBehaviour::AlwaysThrottled);
    for _ in 0..3 {
        let event = KillSwitchRunner::run(&bundle).await.unwrap();
        assert!(event.is_none(), "Throttled MUST NOT trigger kill switch");
    }
    assert!(bundle.audit.is_empty());
    assert_eq!(bundle.alert.alert_count(), 0);
}

#[tokio::test]
async fn mixed_transient_then_ok_no_audit() {
    let bundle = setup_byok_env(KmsProviderKind::AwsKms);

    bundle.provider.set_behaviour(KmsBehaviour::Scripted(vec![
        KmsAccessStatus::ApiError(504),
        KmsAccessStatus::Throttled,
        KmsAccessStatus::ApiError(502),
        KmsAccessStatus::Ok,
    ]));

    for _ in 0..4 {
        let event = KillSwitchRunner::run(&bundle).await.unwrap();
        assert!(event.is_none(), "no kill switch on transient or unprovoked Ok");
    }

    assert!(bundle.audit.is_empty(), "transient-only chain emits no audit");
    assert_eq!(bundle.alert.alert_count(), 0);
    assert_eq!(bundle.alert.recovery_count(), 0);
}

/// Single `ApiError` followed by a genuine `Revoked` still fires the
/// kill switch — the prior transient must not "consume" the
/// subsequent revocation signal.
#[tokio::test]
async fn transient_then_revoked_does_fire_kill_switch() {
    let bundle = setup_byok_env(KmsProviderKind::AzureKeyVault);

    bundle.provider.set_behaviour(KmsBehaviour::Scripted(vec![
        KmsAccessStatus::ApiError(503),
        KmsAccessStatus::Revoked,
    ]));

    // Cycle 1: transient, no kill switch.
    let event = KillSwitchRunner::run(&bundle).await.unwrap();
    assert!(event.is_none());

    // Cycle 2: genuine revocation, kill switch fires.
    let event = KillSwitchRunner::run(&bundle)
        .await
        .unwrap()
        .expect("revoked → kill switch");
    assert_eq!(
        event.event_type,
        corelink_byok_revocation::event::EVENT_TYPE_CMK_REVOKED
    );
}
