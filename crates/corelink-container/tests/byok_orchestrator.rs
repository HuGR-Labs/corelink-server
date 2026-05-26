//! Integration tests for the server BYOK orchestrator dispatch layer.
//!
//! Covers:
//!
//! 1. Default (no `byok-*-real` feature) → `InMemoryFake` returned.
//! 2. Each `byok-*-real` feature wires the matching concrete type and
//!    surfaces its `provider_kind()` accessor.
//! 3. Audit emission round-trip on `wrap_dek` → `unwrap_dek` for the
//!    default in-memory path (the four real-provider paths require
//!    live cloud credentials and are exercised by each crate's nightly
//!    `#[ignore]` e2e suite).
//!
//! Pattern reference:
//! `specs/_audits/2026-05-15-byok-real-provider-pattern.md §7`.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::print_stderr)]
// Test binaries are exempt from print/panic denies; #[cfg(test)] alone
// is not enough because integration tests live in `tests/` (not the
// `tests` module). These allows match the dev-dependency style used
// elsewhere in the workspace.

use std::sync::Arc;

use corelink_byok::types::{FipsLevel, KmsKeyId, KmsProviderKind};
use corelink_byok::{Dek, KmsProvider};

use corelink_server::byok_orchestrator::{
    active_provider, make_provider, ActiveProvider, InMemoryFake,
};

// ── 1. Default path returns InMemoryFake ─────────────────────────────

#[cfg(not(any(
    feature = "byok-aws-real",
    feature = "byok-gcp-real",
    feature = "byok-azure-real",
    feature = "byok-vault-real",
)))]
#[tokio::test]
async fn default_dispatch_returns_in_memory_fake() {
    let p = make_provider().await.expect("orchestrator must succeed in default mode");
    // The fake reports `AwsKms` for downstream compatibility, but
    // `fips_level()` is `None` — that's the load-bearing assertion
    // that distinguishes the fake from any real provider.
    assert_eq!(p.fips_level(), FipsLevel::None);
    assert_eq!(p.region(), "local-fake");
    assert_eq!(active_provider(), ActiveProvider::InMemoryFake);
    assert_eq!(active_provider().as_str(), "in_memory_fake");
    // Trait-object Arc must be alive and reachable.
    assert!(Arc::strong_count(&p) >= 1);
}

// ── 2. Real-provider dispatch — one #[cfg]-gated test per flag ───────

#[cfg(feature = "byok-aws-real")]
#[tokio::test]
async fn aws_real_dispatch_returns_aws_kms_provider() {
    // `AwsKmsRealProvider::new` only initialises the SDK client; it
    // does not require live AWS credentials at construction time
    // (credentials are resolved lazily on first KMS call).
    let p = make_provider()
        .await
        .expect("byok-aws-real orchestrator must build the AWS provider");
    assert_eq!(p.provider_kind(), KmsProviderKind::AwsKms);
    assert_eq!(p.fips_level(), FipsLevel::Fips140_3_L1);
    assert_eq!(active_provider(), ActiveProvider::AwsKms);
    assert_eq!(active_provider().as_str(), "aws");
    assert!(Arc::strong_count(&p) >= 1);
}

#[cfg(feature = "byok-gcp-real")]
#[tokio::test]
async fn gcp_real_dispatch_returns_gcp_kms_provider() {
    // `GcpKmsRealProvider::new` calls `AdcCredentials::detect` which
    // may fail in CI without ADC. Gate this on the explicit mock
    // env var, otherwise report skip — matches the pattern used in
    // `crates/corelink-byok-gcp/tests/real_unit.rs`.
    if std::env::var("CORELINK_BYOK_GCP_MOCK").ok().as_deref() != Some("1") {
        eprintln!("SKIP: GCP orchestrator dispatch requires CORELINK_BYOK_GCP_MOCK=1 or live ADC");
        return;
    }
    let p = make_provider()
        .await
        .expect("byok-gcp-real orchestrator must build the GCP provider");
    assert_eq!(p.provider_kind(), KmsProviderKind::GcpKms);
    assert_eq!(active_provider(), ActiveProvider::GcpKms);
    assert_eq!(active_provider().as_str(), "gcp");
    assert!(Arc::strong_count(&p) >= 1);
}

#[cfg(feature = "byok-azure-real")]
#[tokio::test]
async fn azure_real_dispatch_returns_azure_provider() {
    // `AzureKeyVaultRealProvider::new` calls `EntraCredentials::detect`
    // which requires env credentials. Set the vault URL + skip if
    // creds are absent.
    std::env::set_var(
        "CORELINK_BYOK_AZURE_VAULT_URL",
        "https://orchestrator-test.vault.azure.net",
    );
    if std::env::var("CORELINK_BYOK_AZURE_MOCK").ok().as_deref() != Some("1")
        && std::env::var("AZURE_CLIENT_ID").is_err()
    {
        eprintln!(
            "SKIP: Azure orchestrator dispatch requires CORELINK_BYOK_AZURE_MOCK=1 \
             or AZURE_CLIENT_ID + workload-identity"
        );
        return;
    }
    let p = make_provider()
        .await
        .expect("byok-azure-real orchestrator must build the Azure provider");
    assert_eq!(p.provider_kind(), KmsProviderKind::AzureKeyVault);
    assert_eq!(active_provider(), ActiveProvider::AzureKeyVault);
    assert_eq!(active_provider().as_str(), "azure");
    assert!(Arc::strong_count(&p) >= 1);
}

#[cfg(feature = "byok-vault-real")]
#[tokio::test]
async fn vault_real_dispatch_returns_vault_provider() {
    // `VaultRealProvider::from_env` requires `VAULT_ADDR` + an auth
    // method. Skip when neither are available (CI lane without
    // staging cluster).
    if std::env::var("VAULT_ADDR").is_err() {
        eprintln!("SKIP: Vault orchestrator dispatch requires VAULT_ADDR");
        return;
    }
    let p = make_provider()
        .await
        .expect("byok-vault-real orchestrator must build the Vault provider");
    assert_eq!(p.provider_kind(), KmsProviderKind::HashicorpVault);
    assert_eq!(active_provider(), ActiveProvider::HashicorpVault);
    assert_eq!(active_provider().as_str(), "vault");
    assert!(Arc::strong_count(&p) >= 1);
}

// ── 3. InMemoryFake round-trip (wrap_dek → unwrap_dek) ───────────────
//
// These tests run on every build configuration: they exercise the
// fake directly so they don't depend on which `byok-*-real` flag is
// active. They cover the audit-emission code path and the AAD-
// mandatory contract that mirrors the real-provider behavior.

fn test_key_id() -> KmsKeyId {
    KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:local-fake:000000000000:key/orchestrator-test".to_string(),
        region: "local-fake".to_string(),
    }
}

fn test_aad() -> serde_json::Value {
    serde_json::json!({
        "tenant_id": "tenant-orchestrator-001",
        "blob_hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
    })
}

#[tokio::test]
async fn in_memory_fake_wrap_unwrap_roundtrip_preserves_bytes() {
    let fake = InMemoryFake::new();
    let dek = Dek::generate().expect("dek generation must succeed");
    let original_bytes = dek.bytes;
    let key = test_key_id();
    let aad = test_aad();

    let wrapped = fake
        .wrap_dek(&dek, &key, Some(&aad))
        .await
        .expect("wrap must succeed with AAD");
    assert_eq!(wrapped.provider, KmsProviderKind::AwsKms);
    // Ciphertext must NOT be byte-identical to plaintext (the XOR
    // mask guards against a regression where the fake stores raw
    // DEK material).
    assert_ne!(wrapped.ciphertext.as_slice(), &original_bytes[..]);
    assert_eq!(wrapped.ciphertext.len(), 32);
    assert_eq!(wrapped.encryption_context, Some(aad.clone()));

    let recovered = fake
        .unwrap_dek(&wrapped)
        .await
        .expect("unwrap must succeed with AAD present");
    assert_eq!(recovered.bytes, original_bytes);
}

#[tokio::test]
async fn in_memory_fake_rejects_wrap_without_aad() {
    let fake = InMemoryFake::new();
    let dek = Dek::generate().unwrap();
    let key = test_key_id();
    let err = fake
        .wrap_dek(&dek, &key, None)
        .await
        .expect_err("wrap without AAD must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("encryption_context"),
        "error must mention encryption_context: {msg}"
    );
}

#[tokio::test]
async fn in_memory_fake_rejects_unwrap_without_aad() {
    let fake = InMemoryFake::new();
    let key = test_key_id();
    let wrapped = corelink_byok::types::WrappedDek {
        provider: KmsProviderKind::AwsKms,
        key_id: key,
        ciphertext: vec![0u8; 32],
        encryption_context: None,
    };
    let err = fake
        .unwrap_dek(&wrapped)
        .await
        .expect_err("unwrap without AAD must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("encryption_context"),
        "error must mention encryption_context: {msg}"
    );
}

#[tokio::test]
async fn in_memory_fake_check_access_returns_ok() {
    let fake = InMemoryFake::with_region("custom-fake-region");
    assert_eq!(fake.region(), "custom-fake-region");
    let key = test_key_id();
    let status = fake.check_access(&key).await.expect("check_access must succeed");
    assert!(matches!(
        status,
        corelink_byok::types::KmsAccessStatus::Ok
    ));
}

#[tokio::test]
async fn in_memory_fake_strong_count_through_arc_dyn() {
    let fake: Arc<dyn KmsProvider> = Arc::new(InMemoryFake::new());
    let clone = Arc::clone(&fake);
    assert!(Arc::strong_count(&fake) >= 2);
    assert_eq!(clone.provider_kind(), KmsProviderKind::AwsKms);
    assert_eq!(clone.fips_level(), FipsLevel::None);
}
