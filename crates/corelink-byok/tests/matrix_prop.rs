#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, dead_code)]
//! Property tests: 4 props × all providers (10k iter PR gate; 100k nightly).
//!
//! Properties tested:
//! 1. `prop_wrap_unwrap_roundtrip_per_provider` — wrap+unwrap preserves DEK bytes.
//! 2. `prop_fips_level_const_per_provider` — fips_level() returns same value each call.
//! 3. `prop_aad_binding_per_provider` — tampered encryption_context is rejected.
//! 4. `prop_check_access_ok_mock` — check_access returns Ok in mock mode.

use corelink_byok_core::{BYOKError, Dek, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek};
use corelink_byok_gcp::GcpKmsProvider;
use corelink_byok_azure::AzureKeyVaultProvider;
use corelink_byok_vault::VaultProvider;
use async_trait::async_trait;
use proptest::prelude::*;
use corelink_byok_core::{FipsLevel, KmsAccessStatus};

// ──────────────────────────────────────────────────────────────────────────────
// AWS mock (fills 4th provider slot)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct AwsMockProvider;

#[async_trait]
impl KmsProvider for AwsMockProvider {
    fn provider_kind(&self) -> KmsProviderKind { KmsProviderKind::AwsKms }
    fn region(&self) -> &str { "us-east-1" }
    fn fips_level(&self) -> FipsLevel { FipsLevel::Fips140_3_L1 }

    async fn wrap_dek(&self, dek: &Dek, key_id: &KmsKeyId, encryption_context: Option<&serde_json::Value>) -> Result<WrappedDek, BYOKError> {
        let mut ct = dek.bytes.to_vec();
        for b in &mut ct { *b ^= 0xBB; }
        Ok(WrappedDek { provider: KmsProviderKind::AwsKms, key_id: key_id.clone(), ciphertext: ct, encryption_context: encryption_context.cloned() })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::AwsKms { return Err(BYOKError::EnvelopeError("wrong provider".to_string())); }
        if wrapped.ciphertext.len() != 32 { return Err(BYOKError::EnvelopeError("bad len".to_string())); }
        let mut bytes = [0u8; 32];
        for (i, &b) in wrapped.ciphertext.iter().enumerate() { bytes[i] = b ^ 0xBB; }
        Ok(Dek { bytes })
    }

    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> { Ok(KmsAccessStatus::Ok) }
}

// ──────────────────────────────────────────────────────────────────────────────
// Proptest strategy: random 32-byte DEK bytes
// ──────────────────────────────────────────────────────────────────────────────

fn arb_dek_bytes() -> impl Strategy<Value = [u8; 32]> {
    prop::array::uniform32(any::<u8>())
}

fn arb_tenant_id() -> impl Strategy<Value = String> {
    "[A-Za-z0-9]{4,16}".prop_map(|s| format!("T-{s}"))
}

// ──────────────────────────────────────────────────────────────────────────────
// Prop 1: wrap+unwrap roundtrip per provider
// ──────────────────────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_wrap_unwrap_roundtrip_gcp(
        tenant_id in arb_tenant_id(),
        dek_bytes in arb_dek_bytes(),
    ) {
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let provider = GcpKmsProvider::new_mock("us-east1");
            let key_id = KmsKeyId {
                provider: KmsProviderKind::GcpKms,
                key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(),
                region: "us-east1".to_string(),
            };
            let dek = Dek { bytes: dek_bytes };
            let ctx = serde_json::json!({"tenant_id": tenant_id});
            let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx)).await
                .expect("gcp wrap");
            let unwrapped = provider.unwrap_dek(&wrapped).await
                .expect("gcp unwrap");
            prop_assert_eq!(dek_bytes, unwrapped.bytes);
            Ok(())
        }).expect("async prop");
    }

    #[test]
    fn prop_wrap_unwrap_roundtrip_azure(
        tenant_id in arb_tenant_id(),
        dek_bytes in arb_dek_bytes(),
    ) {
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let provider = AzureKeyVaultProvider::new_mock("eastus");
            let key_id = KmsKeyId {
                provider: KmsProviderKind::AzureKeyVault,
                key_arn_or_id: "https://v.vault.azure.net/keys/k".to_string(),
                region: "eastus".to_string(),
            };
            let dek = Dek { bytes: dek_bytes };
            let ctx = serde_json::json!({"tenant_id": tenant_id});
            let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx)).await
                .expect("azure wrap");
            let unwrapped = provider.unwrap_dek(&wrapped).await
                .expect("azure unwrap");
            prop_assert_eq!(dek_bytes, unwrapped.bytes);
            Ok(())
        }).expect("async prop");
    }

    #[test]
    fn prop_wrap_unwrap_roundtrip_vault(
        tenant_id in arb_tenant_id(),
        dek_bytes in arb_dek_bytes(),
    ) {
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let provider = VaultProvider::new_mock("us-east-1");
            let key_id = KmsKeyId {
                provider: KmsProviderKind::HashicorpVault,
                key_arn_or_id: "transit/keys/k".to_string(),
                region: "us-east-1".to_string(),
            };
            let dek = Dek { bytes: dek_bytes };
            let ctx = serde_json::json!({"tenant_id": tenant_id});
            let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx)).await
                .expect("vault wrap");
            let unwrapped = provider.unwrap_dek(&wrapped).await
                .expect("vault unwrap");
            prop_assert_eq!(dek_bytes, unwrapped.bytes);
            Ok(())
        }).expect("async prop");
    }

    #[test]
    fn prop_wrap_unwrap_roundtrip_aws(
        dek_bytes in arb_dek_bytes(),
    ) {
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let provider = AwsMockProvider;
            let key_id = KmsKeyId {
                provider: KmsProviderKind::AwsKms,
                key_arn_or_id: "arn:aws:kms:us-east-1:123:key/k".to_string(),
                region: "us-east-1".to_string(),
            };
            let dek = Dek { bytes: dek_bytes };
            let wrapped = provider.wrap_dek(&dek, &key_id, None).await
                .expect("aws wrap");
            let unwrapped = provider.unwrap_dek(&wrapped).await
                .expect("aws unwrap");
            prop_assert_eq!(dek_bytes, unwrapped.bytes);
            Ok(())
        }).expect("async prop");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Prop 2: fips_level() returns const value per provider
// ──────────────────────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_fips_level_const_per_provider(
        _seed in 0u32..10000,
    ) {
        // Called N times; must always return the same value.
        let gcp = GcpKmsProvider::new_mock("us-east1");
        let azure = AzureKeyVaultProvider::new_mock("eastus");
        let vault = VaultProvider::new_mock("us-east-1");
        let aws = AwsMockProvider;

        prop_assert_eq!(gcp.fips_level(), FipsLevel::Fips140_2_L1);
        prop_assert_eq!(azure.fips_level(), FipsLevel::Fips140_2_L2);
        prop_assert_eq!(vault.fips_level(), FipsLevel::Fips140_3_L1);
        prop_assert_eq!(aws.fips_level(), FipsLevel::Fips140_3_L1);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Prop 3: AAD binding per provider — tampered context rejected
// ──────────────────────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_aad_binding_gcp(
        tenant_id in arb_tenant_id(),
        attacker_id in "[A-Za-z0-9]{4,16}".prop_map(|s| format!("ATTACKER-{s}")),
        dek_bytes in arb_dek_bytes(),
    ) {
        // Ensure tenant_id != attacker_id to guarantee a different context.
        prop_assume!(tenant_id != attacker_id);
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let provider = GcpKmsProvider::new_mock("us-east1");
            let key_id = KmsKeyId {
                provider: KmsProviderKind::GcpKms,
                key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(),
                region: "us-east1".to_string(),
            };
            let dek = Dek { bytes: dek_bytes };
            let ctx_orig = serde_json::json!({"tenant_id": tenant_id});
            let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx_orig)).await
                .expect("wrap");
            let tampered = WrappedDek {
                encryption_context: Some(serde_json::json!({"tenant_id": attacker_id})),
                ..wrapped
            };
            let result = provider.unwrap_dek(&tampered).await;
            prop_assert!(result.is_err(), "GCP AAD binding must reject tampered context");
            Ok(())
        }).expect("async prop");
    }

    #[test]
    fn prop_aad_binding_azure(
        tenant_id in arb_tenant_id(),
        attacker_id in "[A-Za-z0-9]{4,16}".prop_map(|s| format!("ATTACKER-{s}")),
        dek_bytes in arb_dek_bytes(),
    ) {
        prop_assume!(tenant_id != attacker_id);
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let provider = AzureKeyVaultProvider::new_mock("eastus");
            let key_id = KmsKeyId {
                provider: KmsProviderKind::AzureKeyVault,
                key_arn_or_id: "https://v.vault.azure.net/keys/k".to_string(),
                region: "eastus".to_string(),
            };
            let dek = Dek { bytes: dek_bytes };
            let ctx_orig = serde_json::json!({"tenant_id": tenant_id});
            let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx_orig)).await
                .expect("wrap");
            let tampered = WrappedDek {
                encryption_context: Some(serde_json::json!({"tenant_id": attacker_id})),
                ..wrapped
            };
            let result = provider.unwrap_dek(&tampered).await;
            prop_assert!(result.is_err(), "Azure AAD binding (GCM tag) must reject tampered context");
            Ok(())
        }).expect("async prop");
    }

    #[test]
    fn prop_aad_binding_vault(
        tenant_id in arb_tenant_id(),
        attacker_id in "[A-Za-z0-9]{4,16}".prop_map(|s| format!("ATTACKER-{s}")),
        dek_bytes in arb_dek_bytes(),
    ) {
        prop_assume!(tenant_id != attacker_id);
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let provider = VaultProvider::new_mock("us-east-1");
            let key_id = KmsKeyId {
                provider: KmsProviderKind::HashicorpVault,
                key_arn_or_id: "transit/keys/k".to_string(),
                region: "us-east-1".to_string(),
            };
            let dek = Dek { bytes: dek_bytes };
            let ctx_orig = serde_json::json!({"tenant_id": tenant_id});
            let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx_orig)).await
                .expect("wrap");
            let tampered = WrappedDek {
                encryption_context: Some(serde_json::json!({"tenant_id": attacker_id})),
                ..wrapped
            };
            let result = provider.unwrap_dek(&tampered).await;
            prop_assert!(result.is_err(), "Vault context binding must reject tampered context");
            Ok(())
        }).expect("async prop");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Prop 4: check_access returns Ok in mock mode per provider
// ──────────────────────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_check_access_ok_mock_all_providers(
        _seed in 0u32..10000,
    ) {
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let gcp = GcpKmsProvider::new_mock("us-east1");
            let gcp_key = KmsKeyId { provider: KmsProviderKind::GcpKms, key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(), region: "us-east1".to_string() };
            prop_assert_eq!(gcp.check_access(&gcp_key).await.expect("gcp check"), KmsAccessStatus::Ok);

            let azure = AzureKeyVaultProvider::new_mock("eastus");
            let azure_key = KmsKeyId { provider: KmsProviderKind::AzureKeyVault, key_arn_or_id: "https://v.vault.azure.net/keys/k".to_string(), region: "eastus".to_string() };
            prop_assert_eq!(azure.check_access(&azure_key).await.expect("azure check"), KmsAccessStatus::Ok);

            let vault = VaultProvider::new_mock("us-east-1");
            let vault_key = KmsKeyId { provider: KmsProviderKind::HashicorpVault, key_arn_or_id: "transit/keys/k".to_string(), region: "us-east-1".to_string() };
            prop_assert_eq!(vault.check_access(&vault_key).await.expect("vault check"), KmsAccessStatus::Ok);

            let aws = AwsMockProvider;
            let aws_key = KmsKeyId { provider: KmsProviderKind::AwsKms, key_arn_or_id: "arn:aws:kms:us-east-1:123:key/k".to_string(), region: "us-east-1".to_string() };
            prop_assert_eq!(aws.check_access(&aws_key).await.expect("aws check"), KmsAccessStatus::Ok);

            Ok(())
        }).expect("async prop");
    }
}
