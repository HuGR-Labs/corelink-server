#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, dead_code)]
//! Adversarial regression tests per provider (WI-S14-005 §6.1 item 10).
//!
//! Covers 12+ adversarial scenarios across 3 providers (+ AWS mock):
//!
//! GCP:
//!   1. additional_authenticated_data swap (wrong tenant in ctx)
//!   2. Cross-provider injection (GCP wrapped DEK presented to Azure)
//!   3. Empty context to non-empty-context-wrapped DEK
//!   4. Non-GCP provider key_id on wrap
//!
//! Azure:
//!   5. AES-GCM tag mismatch via ctx swap (custom AAD flow)
//!   6. Ciphertext truncation (too short to parse)
//!   7. Wrong provider on wrapped DEK
//!   8. Managed Identity context rotation (ctx key swap)
//!
//! Vault:
//!   9. transit context base64 tampering (different tenant)
//!  10. mTLS cert expiry ≤ 30d alert
//!  11. Ciphertext too short
//!  12. Wrong provider on wrapped DEK
//!
//! Cross-provider:
//!  13. GCP wrapped DEK presented to Vault → rejected
//!  14. Azure wrapped DEK presented to GCP → rejected
//!  15. Vault wrapped DEK presented to Azure → rejected
//!  16. All providers: wrap with ctx=None, unwrap with ctx=Some → rejected (ctx added)

use corelink_byok::{BYOKError, Dek, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek, FipsLevel};
use corelink_byok_gcp::GcpKmsProvider;
use corelink_byok_azure::AzureKeyVaultProvider;
use corelink_byok_vault::VaultProvider;
use async_trait::async_trait;

// ──────────────────────────────────────────────────────────────────────────────
// AWS mock
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

    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Ok(KmsAccessStatus::Ok)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Helper
// ──────────────────────────────────────────────────────────────────────────────

fn gcp_key() -> KmsKeyId {
    KmsKeyId { provider: KmsProviderKind::GcpKms, key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(), region: "us-east1".to_string() }
}
fn azure_key() -> KmsKeyId {
    KmsKeyId { provider: KmsProviderKind::AzureKeyVault, key_arn_or_id: "https://v.vault.azure.net/keys/k".to_string(), region: "eastus".to_string() }
}
fn vault_key() -> KmsKeyId {
    KmsKeyId { provider: KmsProviderKind::HashicorpVault, key_arn_or_id: "transit/keys/k".to_string(), region: "us-east-1".to_string() }
}

// ──────────────────────────────────────────────────────────────────────────────
// GCP adversarial tests
// ──────────────────────────────────────────────────────────────────────────────

// Scenario 1: AAD swap (different tenant)
#[tokio::test]
async fn gcp_aad_swap_rejected() {
    let p = GcpKmsProvider::new_mock("us-east1");
    let dek = Dek::generate().expect("entropy");
    let ctx_orig = serde_json::json!({"tenant_id": "T1"});
    let wrapped = p.wrap_dek(&dek, &gcp_key(), Some(&ctx_orig)).await.expect("wrap");
    let tampered = WrappedDek { encryption_context: Some(serde_json::json!({"tenant_id": "ATTACKER"})), ..wrapped };
    assert!(p.unwrap_dek(&tampered).await.is_err(), "GCP: AAD swap must be rejected");
}

// Scenario 2: Cross-provider injection GCP → Azure
#[tokio::test]
async fn gcp_wrapped_dek_rejected_by_azure() {
    let gcp = GcpKmsProvider::new_mock("us-east1");
    let dek = Dek::generate().expect("entropy");
    let gcp_wrapped = gcp.wrap_dek(&dek, &gcp_key(), None).await.expect("wrap");
    let azure = AzureKeyVaultProvider::new_mock("eastus");
    // Tamper: set provider to Azure so Azure adapter attempts to parse.
    let as_azure = WrappedDek {
        provider: KmsProviderKind::AzureKeyVault,
        key_id: KmsKeyId { provider: KmsProviderKind::AzureKeyVault, ..gcp_key() },
        ..gcp_wrapped
    };
    // Azure expects a specific ciphertext layout; GCP's layout differs → error.
    let result = azure.unwrap_dek(&as_azure).await;
    // Either provider mismatch error or parsing error — both are acceptable.
    assert!(result.is_err(), "GCP wrapped DEK must be rejected by Azure adapter");
}

// Scenario 3: Empty context on wrapped-with-context DEK
#[tokio::test]
async fn gcp_empty_ctx_on_ctx_wrapped_rejected() {
    let p = GcpKmsProvider::new_mock("us-east1");
    let dek = Dek::generate().expect("entropy");
    let ctx = serde_json::json!({"tenant_id": "T1"});
    let wrapped = p.wrap_dek(&dek, &gcp_key(), Some(&ctx)).await.expect("wrap");
    // Tamper: remove encryption_context.
    let no_ctx = WrappedDek { encryption_context: None, ..wrapped };
    let result = p.unwrap_dek(&no_ctx).await;
    // AAD mismatch: None != Some(ctx) → error.
    assert!(result.is_err(), "GCP: removing ctx from ctx-wrapped DEK must be rejected");
}

// Scenario 4: Non-GCP provider key_id on wrap
#[tokio::test]
async fn gcp_wrong_provider_key_id_rejected_on_wrap() {
    let p = GcpKmsProvider::new_mock("us-east1");
    let dek = Dek::generate().expect("entropy");
    let result = p.wrap_dek(&dek, &azure_key(), None).await;
    assert!(result.is_err(), "GCP: non-GCP key_id must be rejected on wrap");
}

// ──────────────────────────────────────────────────────────────────────────────
// Azure adversarial tests
// ──────────────────────────────────────────────────────────────────────────────

// Scenario 5: AES-GCM tag mismatch via ctx swap
#[tokio::test]
async fn azure_aad_swap_rejected() {
    let p = AzureKeyVaultProvider::new_mock("eastus");
    let dek = Dek::generate().expect("entropy");
    let ctx = serde_json::json!({"tenant_id": "T2"});
    let wrapped = p.wrap_dek(&dek, &azure_key(), Some(&ctx)).await.expect("wrap");
    let tampered = WrappedDek { encryption_context: Some(serde_json::json!({"tenant_id": "ATTACKER"})), ..wrapped };
    assert!(p.unwrap_dek(&tampered).await.is_err(), "Azure: AES-GCM tag mismatch on ctx swap");
}

// Scenario 6: Ciphertext truncation
#[tokio::test]
async fn azure_truncated_ciphertext_rejected() {
    let p = AzureKeyVaultProvider::new_mock("eastus");
    let truncated = WrappedDek {
        provider: KmsProviderKind::AzureKeyVault,
        key_id: azure_key(),
        ciphertext: vec![0u8; 10], // too short
        encryption_context: None,
    };
    assert!(p.unwrap_dek(&truncated).await.is_err(), "Azure: truncated ciphertext must be rejected");
}

// Scenario 7: Wrong provider on wrapped DEK
#[tokio::test]
async fn azure_wrong_provider_on_unwrap_rejected() {
    let p = AzureKeyVaultProvider::new_mock("eastus");
    let wrong_provider = WrappedDek {
        provider: KmsProviderKind::GcpKms,
        key_id: gcp_key(),
        ciphertext: vec![0u8; 60],
        encryption_context: None,
    };
    assert!(p.unwrap_dek(&wrong_provider).await.is_err(), "Azure: wrong provider in WrappedDek must be rejected");
}

// Scenario 8: Managed Identity context rotation (key field swap in ctx)
#[tokio::test]
async fn azure_managed_identity_ctx_key_swap_rejected() {
    let p = AzureKeyVaultProvider::new_mock("eastus");
    let dek = Dek::generate().expect("entropy");
    let ctx_orig = serde_json::json!({"tenant_id": "T2", "resource": "blob-store"});
    let wrapped = p.wrap_dek(&dek, &azure_key(), Some(&ctx_orig)).await.expect("wrap");
    // Swap a value in the context — different JSON = different AAD = GCM reject.
    let tampered = WrappedDek {
        encryption_context: Some(serde_json::json!({"tenant_id": "T2", "resource": "ATTACKER-RESOURCE"})),
        ..wrapped
    };
    assert!(p.unwrap_dek(&tampered).await.is_err(), "Azure: resource field swap in ctx must be rejected");
}

// ──────────────────────────────────────────────────────────────────────────────
// Vault adversarial tests
// ──────────────────────────────────────────────────────────────────────────────

// Scenario 9: Transit context base64 tampering
#[tokio::test]
async fn vault_transit_context_tampering_rejected() {
    let p = VaultProvider::new_mock("us-east-1");
    let dek = Dek::generate().expect("entropy");
    let ctx = serde_json::json!({"tenant_id": "T3"});
    let wrapped = p.wrap_dek(&dek, &vault_key(), Some(&ctx)).await.expect("wrap");
    let tampered = WrappedDek {
        encryption_context: Some(serde_json::json!({"tenant_id": "ATTACKER"})),
        ..wrapped
    };
    assert!(p.unwrap_dek(&tampered).await.is_err(), "Vault: transit context tampering must be rejected");
}

// Scenario 10: mTLS cert expiry alert ≤ 30d
#[tokio::test]
async fn vault_mtls_cert_expiry_alert() {
    let mut p = VaultProvider::new_mock("us-east-1");
    p.set_mock_cert_days_remaining(Some(15));
    let result = p.check_access(&vault_key()).await;
    assert!(
        matches!(result, Err(BYOKError::MtlsCertExpiringSoon { days_remaining: 15 })),
        "Vault: cert expiry 15d must fire alert"
    );
}

// Scenario 11: Ciphertext too short
#[tokio::test]
async fn vault_truncated_ciphertext_rejected() {
    let p = VaultProvider::new_mock("us-east-1");
    let too_short = WrappedDek {
        provider: KmsProviderKind::HashicorpVault,
        key_id: vault_key(),
        ciphertext: vec![0u8; 20], // < 64 required
        encryption_context: None,
    };
    assert!(p.unwrap_dek(&too_short).await.is_err(), "Vault: truncated ciphertext must be rejected");
}

// Scenario 12: Wrong provider on wrapped DEK
#[tokio::test]
async fn vault_wrong_provider_on_unwrap_rejected() {
    let p = VaultProvider::new_mock("us-east-1");
    let wrong = WrappedDek {
        provider: KmsProviderKind::AwsKms,
        key_id: KmsKeyId { provider: KmsProviderKind::AwsKms, key_arn_or_id: "arn:aws:kms:us-east-1:123:key/k".to_string(), region: "us-east-1".to_string() },
        ciphertext: vec![0u8; 64],
        encryption_context: None,
    };
    assert!(p.unwrap_dek(&wrong).await.is_err(), "Vault: wrong provider must be rejected");
}

// ──────────────────────────────────────────────────────────────────────────────
// Cross-provider scenarios
// ──────────────────────────────────────────────────────────────────────────────

// Scenario 13: GCP wrapped DEK presented to Vault
#[tokio::test]
async fn cross_provider_gcp_to_vault_rejected() {
    let gcp = GcpKmsProvider::new_mock("us-east1");
    let dek = Dek::generate().expect("entropy");
    let gcp_wrapped = gcp.wrap_dek(&dek, &gcp_key(), None).await.expect("gcp wrap");
    let vault = VaultProvider::new_mock("us-east-1");
    let as_vault = WrappedDek {
        provider: KmsProviderKind::HashicorpVault,
        key_id: KmsKeyId { provider: KmsProviderKind::HashicorpVault, ..vault_key() },
        ..gcp_wrapped
    };
    assert!(vault.unwrap_dek(&as_vault).await.is_err(), "GCP wrapped DEK must be rejected by Vault");
}

// Scenario 14: Azure wrapped DEK presented to GCP
#[tokio::test]
async fn cross_provider_azure_to_gcp_rejected() {
    let azure = AzureKeyVaultProvider::new_mock("eastus");
    let dek = Dek::generate().expect("entropy");
    let azure_wrapped = azure.wrap_dek(&dek, &azure_key(), None).await.expect("azure wrap");
    let gcp = GcpKmsProvider::new_mock("us-east1");
    let as_gcp = WrappedDek {
        provider: KmsProviderKind::GcpKms,
        key_id: KmsKeyId { provider: KmsProviderKind::GcpKms, ..gcp_key() },
        ..azure_wrapped
    };
    // Azure's ciphertext is 32+12+32+16=92 bytes; GCP expects exactly 32 → error.
    assert!(gcp.unwrap_dek(&as_gcp).await.is_err(), "Azure wrapped DEK must be rejected by GCP");
}

// Scenario 15: Vault wrapped DEK presented to Azure
#[tokio::test]
async fn cross_provider_vault_to_azure_rejected() {
    let vault = VaultProvider::new_mock("us-east-1");
    let dek = Dek::generate().expect("entropy");
    let vault_wrapped = vault.wrap_dek(&dek, &vault_key(), None).await.expect("vault wrap");
    let azure = AzureKeyVaultProvider::new_mock("eastus");
    let as_azure = WrappedDek {
        provider: KmsProviderKind::AzureKeyVault,
        key_id: KmsKeyId { provider: KmsProviderKind::AzureKeyVault, ..azure_key() },
        ..vault_wrapped
    };
    // Vault's ciphertext is 64 bytes (32 ctx marker + 32 XOR). Azure layout:
    // [0..32] = azure_wrapped_key, [32..] = nonce(12) || ct(32) || tag(16) = 60 bytes → total 92.
    // 64 bytes parses: azure_wrapped_key(32) + inner_ct(32). inner_ct too short (< 12+16=28).
    assert!(azure.unwrap_dek(&as_azure).await.is_err(), "Vault wrapped DEK must be rejected by Azure");
}

// Scenario 16: All providers — wrap with ctx=None, add ctx on unwrap → rejected
#[tokio::test]
async fn all_providers_adding_ctx_on_unwrap_rejected() {
    let ctx_added = serde_json::json!({"tenant_id": "INJECTED"});

    // GCP
    let gcp = GcpKmsProvider::new_mock("us-east1");
    let dek = Dek::generate().expect("entropy");
    let gcp_wrapped = gcp.wrap_dek(&dek, &gcp_key(), None).await.expect("gcp wrap");
    let gcp_tampered = WrappedDek { encryption_context: Some(ctx_added.clone()), ..gcp_wrapped };
    assert!(gcp.unwrap_dek(&gcp_tampered).await.is_err(), "GCP: adding ctx on unwrap must be rejected");

    // Azure
    let azure = AzureKeyVaultProvider::new_mock("eastus");
    let dek2 = Dek::generate().expect("entropy");
    let azure_wrapped = azure.wrap_dek(&dek2, &azure_key(), None).await.expect("azure wrap");
    let azure_tampered = WrappedDek { encryption_context: Some(ctx_added.clone()), ..azure_wrapped };
    assert!(azure.unwrap_dek(&azure_tampered).await.is_err(), "Azure: adding ctx on unwrap must be rejected (GCM tag)");

    // Vault
    let vault = VaultProvider::new_mock("us-east-1");
    let dek3 = Dek::generate().expect("entropy");
    let vault_wrapped = vault.wrap_dek(&dek3, &vault_key(), None).await.expect("vault wrap");
    let vault_tampered = WrappedDek { encryption_context: Some(ctx_added.clone()), ..vault_wrapped };
    assert!(vault.unwrap_dek(&vault_tampered).await.is_err(), "Vault: adding ctx on unwrap must be rejected");
}
