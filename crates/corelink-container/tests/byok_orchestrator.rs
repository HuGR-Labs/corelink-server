//! Integration tests for the server BYOK orchestrator dispatch layer.
//!
//! Covers:
//!
//! 1. Default (no `byok-*-real` feature) → construction fails closed.
//! 2. Each `byok-*-real` feature wires the matching concrete type and
//!    surfaces its `provider_kind()` accessor.
//! 3. No test path substitutes local XOR/identity crypto for production KMS.
//!
//! Pattern reference:
//! `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7`.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::print_stderr)]
#![allow(clippy::panic)]
// Test binaries are exempt from print/panic denies; #[cfg(test)] alone
// is not enough because integration tests live in `tests/` (not the
// `tests` module). These allows match the dev-dependency style used
// elsewhere in the workspace.

#[cfg(any(
    feature = "byok-aws-real",
    feature = "byok-gcp-real",
    feature = "byok-azure-real",
    feature = "byok-vault-real",
))]
use std::sync::Arc;

#[cfg(any(
    feature = "byok-aws-real",
    feature = "byok-gcp-real",
    feature = "byok-azure-real",
    feature = "byok-vault-real",
))]
use corelink_byok::types::{FipsLevel, KmsProviderKind};

use corelink_server::byok_orchestrator::{active_provider, make_provider, ActiveProvider};

// ── 1. Default path fails closed ────────────────────────────────────

#[cfg(not(any(
    feature = "byok-aws-real",
    feature = "byok-gcp-real",
    feature = "byok-azure-real",
    feature = "byok-vault-real",
)))]
#[tokio::test]
async fn no_provider_feature_fails_closed_without_crypto_fallback() {
    let error = match make_provider().await {
        Ok(_) => panic!("orchestrator must refuse a binary with no real provider"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("no real KMS provider compiled"));
    assert_eq!(active_provider(), ActiveProvider::Unavailable);
    assert_eq!(active_provider().as_str(), "unavailable");
}

// ── 2. Real-provider dispatch — one #[cfg]-gated test per flag ───────

#[cfg(feature = "byok-aws-real")]
#[tokio::test]
async fn aws_real_dispatch_returns_aws_kms_provider() {
    // The production constructor requires explicit owner-provided AWS
    // credentials and does not silently fall back to a local test provider.
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
