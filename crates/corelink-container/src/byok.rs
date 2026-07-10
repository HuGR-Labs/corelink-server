//! BYOK provider factory — feature-gated production wire-up for ALL FOUR
//! KMS providers.
//!
//! Compiled when any single `byok-*-real` flag is set. Each provider has a thin
//! convenience constructor returning a boxed [`KmsProvider`] trait object; the
//! canonical selectable path is [`make_active_provider`], which delegates to the
//! orchestrator's compile-time cfg dispatch (exactly one provider per binary —
//! two `byok-*-real` flags is a HARD compile error, enforced in
//! [`crate::byok_orchestrator`]).
//!
//! Only the constructor matching the enabled feature is compiled, so this
//! module never links more than one KMS SDK/stack (ADR-S30-001).
//!
//! Pattern reference:
//! `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7`.

#![forbid(unsafe_code)]

use std::sync::Arc;

use corelink_byok::{BYOKError, KmsProvider};

/// Construct the production AWS KMS provider for the configured region.
///
/// FIPS endpoint is enforced unconditionally — see `AwsKmsRealProvider::new`
/// docs. AWS credentials resolve via the standard AWS SDK chain (env vars,
/// shared config, IRSA, IMDS, SSO).
///
/// # Errors
///
/// Surfaces SDK initialisation errors as [`BYOKError::Provider`].
#[cfg(feature = "byok-aws-real")]
pub async fn make_aws_kms_provider(region: &str) -> Result<Arc<dyn KmsProvider>, BYOKError> {
    let p = corelink_byok::aws::AwsKmsRealProvider::new(region).await?;
    Ok(Arc::new(p))
}

/// Construct the production GCP Cloud KMS provider for the configured region.
///
/// Credentials resolve via Application Default Credentials (ADC).
///
/// # Errors
///
/// Surfaces client initialisation errors as [`BYOKError::Provider`].
#[cfg(feature = "byok-gcp-real")]
pub async fn make_gcp_kms_provider(region: &str) -> Result<Arc<dyn KmsProvider>, BYOKError> {
    let p = corelink_byok::gcp::GcpKmsRealProvider::new(region).await?;
    Ok(Arc::new(p))
}

/// Construct the production Azure Key Vault / Managed HSM provider.
///
/// `vault_url` is the Premium / Managed-HSM base URL; Entra ID auth resolves
/// via the provider's native credential chain.
///
/// # Errors
///
/// Surfaces client initialisation errors as [`BYOKError::Provider`].
#[cfg(feature = "byok-azure-real")]
pub fn make_azure_kms_provider(
    region: &str,
    vault_url: &str,
) -> Result<Arc<dyn KmsProvider>, BYOKError> {
    let p = corelink_byok::azure::AzureKeyVaultRealProvider::new(region, vault_url)?;
    Ok(Arc::new(p))
}

/// Construct the production HashiCorp Vault Transit provider from the
/// environment (`VAULT_ADDR` + auth method resolved by the provider crate).
///
/// # Errors
///
/// Surfaces client initialisation errors as [`BYOKError::Provider`].
#[cfg(feature = "byok-vault-real")]
pub fn make_vault_kms_provider(region: &str) -> Result<Arc<dyn KmsProvider>, BYOKError> {
    let p = corelink_byok::vault::VaultRealProvider::from_env(region)?;
    Ok(Arc::new(p))
}

/// Construct THE active provider for this binary — the canonical selectable
/// factory across all four providers.
///
/// Delegates to [`crate::byok_orchestrator::make_provider`], whose compile-time
/// cfg dispatch selects the concrete provider from the single enabled
/// `byok-*-real` flag (or the `InMemoryFake` when none is set). Prefer this over
/// the per-provider constructors when the caller does not care which provider
/// the binary was built for (e.g. the server boot path that threads a single
/// `Arc<dyn KmsProvider>` through the BYOK-aware code paths).
///
/// # Errors
///
/// Surfaces the active provider's constructor failure as [`BYOKError::Provider`]
/// (missing env var, SDK init failure, FIPS endpoint resolution failure).
pub async fn make_active_provider() -> Result<Arc<dyn KmsProvider>, BYOKError> {
    crate::byok_orchestrator::make_provider().await
}
