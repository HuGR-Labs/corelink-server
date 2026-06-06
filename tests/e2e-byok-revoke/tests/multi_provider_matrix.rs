//! R3-2 Multi-provider matrix — 4 BYOK providers × revoke + restore.
//!
//! Parameterises the happy revoke + recovery flow across all four
//! providers (AWS / GCP / Azure / Vault) using the in-memory
//! `BoundedKmsProvider`. Each cell asserts:
//!
//! - kill switch fires on revoke, audit event provider field matches
//!   provider kind canonical string (`aws` / `gcp` / `azure` / `vault`),
//!   cache fully evicted, tenant degraded_read_only.
//! - recovery cycle restores tenant + emits `cmk_restored`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_byok::revocation::{
    event::{EVENT_TYPE_CMK_RESTORED, EVENT_TYPE_CMK_REVOKED},
    store::TenantByokStatus,
};
use corelink_byok::{Dek, KmsProviderKind};
use e2e_byok_revoke::helpers::{
    make_wrapped_for, KillSwitchRunner, KmsBehaviour, ALL_PROVIDER_KINDS,
};
use e2e_byok_revoke::setup_byok_env;

#[tokio::test]
async fn matrix_all_four_providers_revoke_and_restore() {
    for kind in ALL_PROVIDER_KINDS {
        let bundle = setup_byok_env(*kind);

        // Seed 4 entries.
        for i in 0..4u32 {
            let wrapped = make_wrapped_for(&bundle.key_id, i);
            let dek = Dek::generate().unwrap();
            bundle.dek_cache.put(&wrapped, dek).await.unwrap();
        }

        // Revoke.
        bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
        let event = KillSwitchRunner::run(&bundle)
            .await
            .unwrap()
            .expect("revoke audit event");
        assert_eq!(event.event_type, EVENT_TYPE_CMK_REVOKED);
        assert_eq!(event.provider, kind.as_str(), "audit provider field");
        assert_eq!(event.evicted_dek_count, 4);
        assert_eq!(bundle.dek_cache.len().await, 0);
        assert_eq!(
            bundle.tenant_store.current(&bundle.key_id.key_arn_or_id),
            Some(TenantByokStatus::DegradedReadOnly)
        );

        // Restore.
        bundle.provider.set_behaviour(KmsBehaviour::AlwaysOk);
        let event = KillSwitchRunner::run(&bundle)
            .await
            .unwrap()
            .expect("restore audit event");
        assert_eq!(event.event_type, EVENT_TYPE_CMK_RESTORED);
        assert_eq!(event.provider, kind.as_str());
        assert_eq!(
            bundle.tenant_store.current(&bundle.key_id.key_arn_or_id),
            Some(TenantByokStatus::Active)
        );

        assert_eq!(bundle.alert.alert_count(), 1, "{kind:?}: 1 revoke alert");
        assert_eq!(
            bundle.alert.recovery_count(),
            1,
            "{kind:?}: 1 recovery alert"
        );
    }
}

/// Sanity: every provider kind reports its canonical FIPS level.
#[tokio::test]
async fn matrix_provider_fips_levels_canonical() {
    use corelink_byok::{FipsLevel, KmsProvider};
    for kind in ALL_PROVIDER_KINDS {
        let bundle = setup_byok_env(*kind);
        let expected = match kind {
            KmsProviderKind::AwsKms | KmsProviderKind::GcpKms | KmsProviderKind::AzureKeyVault => {
                FipsLevel::Fips140_3_L1
            }
            KmsProviderKind::HashicorpVault => FipsLevel::Fips140_2_L1,
        };
        assert_eq!(
            bundle.provider.fips_level(),
            expected,
            "{kind:?} FIPS level"
        );
    }
}
