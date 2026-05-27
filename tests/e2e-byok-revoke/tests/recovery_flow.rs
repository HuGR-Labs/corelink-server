//! R3-2 Recovery flow — `corelink.byok.cmk_restored`.
//!
//! Pipeline:
//!
//! 1. Revoke CMK → kill switch fires, tenant degraded_read_only.
//! 2. Customer re-enables CMK in BYOK provider.
//! 3. Next cycle: `KmsAccessStatus::Ok` → `byok_status = 'active'` →
//!    audit `corelink.byok.cmk_restored` emitted → recovery alert
//!    dispatched.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_byok::{Dek, KmsProviderKind};
use corelink_byok::revocation::{
    event::{EVENT_TYPE_CMK_RESTORED, EVENT_TYPE_CMK_REVOKED},
    store::TenantByokStatus,
};
use e2e_byok_revoke::helpers::{make_wrapped_for, KillSwitchRunner, KmsBehaviour};
use e2e_byok_revoke::setup_byok_env;

#[tokio::test]
async fn recovery_after_revoke_emits_cmk_restored() {
    let bundle = setup_byok_env(KmsProviderKind::GcpKms);

    // Seed cache + revoke.
    for i in 0..5u32 {
        let wrapped = make_wrapped_for(&bundle.key_id, i);
        let dek = Dek::generate().unwrap();
        bundle.dek_cache.put(&wrapped, dek).await.unwrap();
    }
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    KillSwitchRunner::run(&bundle)
        .await
        .unwrap()
        .expect("revoke event emitted");
    assert_eq!(
        bundle.tenant_store.current(&bundle.key_id.key_arn_or_id),
        Some(TenantByokStatus::DegradedReadOnly)
    );
    assert_eq!(bundle.alert.alert_count(), 1);
    assert_eq!(bundle.alert.recovery_count(), 0);

    // Customer re-enables CMK.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysOk);

    // Next cycle: restore.
    let event = KillSwitchRunner::run(&bundle)
        .await
        .unwrap()
        .expect("restored event emitted");
    assert_eq!(event.event_type, EVENT_TYPE_CMK_RESTORED);
    assert_eq!(event.provider, "gcp");
    assert_eq!(event.evicted_dek_count, 0);

    // Tenant active again.
    assert_eq!(
        bundle.tenant_store.current(&bundle.key_id.key_arn_or_id),
        Some(TenantByokStatus::Active),
        "byok_status must be restored to active"
    );

    // Audit chain: revoked → restored.
    let types = bundle.audit.snapshot_event_types();
    assert_eq!(
        types,
        vec![
            EVENT_TYPE_CMK_REVOKED.to_string(),
            EVENT_TYPE_CMK_RESTORED.to_string(),
        ]
    );

    // Recovery alert dispatched.
    assert_eq!(bundle.alert.recovery_count(), 1, "recovery alert dispatched");
    assert_eq!(bundle.alert.alert_count(), 1, "revoke alerts unchanged");

    // Subsequent Ok cycle is a no-op (already active).
    let event = KillSwitchRunner::run(&bundle).await.unwrap();
    assert!(event.is_none(), "Ok on already-active tenant is a no-op");
    assert_eq!(bundle.alert.recovery_count(), 1, "no duplicate recovery alert");

    // History records both transitions.
    let history = bundle.tenant_store.history();
    let kinds: Vec<TenantByokStatus> = history.iter().map(|(_, s, _)| *s).collect();
    assert_eq!(
        kinds,
        vec![
            TenantByokStatus::DegradedReadOnly,
            TenantByokStatus::Active,
        ]
    );
}

/// Flap-flap-flap: revoke → recover → revoke → recover. Tenant must
/// converge to whatever the last observed CMK state is, and the audit
/// chain records every transition.
#[tokio::test]
async fn recovery_flap_chain_consistent() {
    let bundle = setup_byok_env(KmsProviderKind::HashicorpVault);

    // Initial revoke.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    KillSwitchRunner::run(&bundle).await.unwrap();

    // Restore.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysOk);
    KillSwitchRunner::run(&bundle).await.unwrap();

    // Second revoke.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    KillSwitchRunner::run(&bundle).await.unwrap();

    // Second restore.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysOk);
    KillSwitchRunner::run(&bundle).await.unwrap();

    let types = bundle.audit.snapshot_event_types();
    assert_eq!(
        types,
        vec![
            EVENT_TYPE_CMK_REVOKED.to_string(),
            EVENT_TYPE_CMK_RESTORED.to_string(),
            EVENT_TYPE_CMK_REVOKED.to_string(),
            EVENT_TYPE_CMK_RESTORED.to_string(),
        ],
        "audit chain must record every revoke/restore transition"
    );

    // Final state: active.
    assert_eq!(
        bundle.tenant_store.current(&bundle.key_id.key_arn_or_id),
        Some(TenantByokStatus::Active),
    );
}
