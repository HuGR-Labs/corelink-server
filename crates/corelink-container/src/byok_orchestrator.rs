//! BYOK orchestrator — feature-gated production dispatch over
//! [`corelink_byok::KmsProvider`] trait objects.
//!
//! # Responsibility
//!
//! Construct the single `Arc<dyn KmsProvider>` that the server boot
//! path threads through every BYOK-aware code path (envelope encrypt /
//! decrypt, kill-switch poller, erasure attestation). Exactly one of
//! the four production providers is wired per binary; multiple-provider
//! deployments are out of scope and rejected at compile time.
//!
//! # Feature-flag dispatch
//!
//! | Flag set | Concrete type | Audit target |
//! |---|---|---|
//! | (none) | no provider (fail-closed) | `corelink.byok.orchestrator.audit` |
//! | `byok-aws-real` | `corelink_byok::aws::AwsKmsRealProvider` | `corelink.byok.aws.audit` |
//! | `byok-gcp-real` | `corelink_byok::gcp::GcpKmsRealProvider` | `corelink.byok.gcp.audit` |
//! | `byok-azure-real` | `corelink_byok::azure::AzureKeyVaultRealProvider` | `corelink.byok.azure.audit` |
//! | `byok-vault-real` | `corelink_byok::vault::VaultRealProvider` | `corelink.byok.vault.audit` |
//!
//! Setting two or more `byok-*-real` flags simultaneously is a HARD
//! compile error — only one production provider may be linked into the
//! singleton trait-object surface (see the `compile_error!` block at
//! the bottom of this file).
//!
//! # Audit wiring
//!
//! Each concrete provider emits structured `tracing` events with
//! `target = "corelink.byok.<provider>.audit"` and `audit = true` at
//! every error site (audit fail-CLOSED). The orchestrator does NOT
//! transform those events — they flow through the server's global
//! `tracing` subscriber to the audit sink. See
//! `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §5` for the
//! closed `reason` vocabulary.
//!
//! # Configuration via environment
//!
//! Provider constructors read the following environment variables when
//! the matching feature is enabled. Missing vars fail CLOSED with
//! [`BYOKError::Provider`].
//!
//! | Feature | Variable | Purpose |
//! |---|---|---|
//! | `byok-aws-real` | `AWS_REGION` | KMS region (default `us-east-1`) |
//! | `byok-gcp-real` | `GCP_REGION` | Cloud KMS region (default `us-east1`) |
//! | `byok-azure-real` | `CORELINK_BYOK_AZURE_VAULT_URL` | Premium / Managed HSM base URL |
//! | `byok-azure-real` | `CORELINK_BYOK_AZURE_REGION` | Azure region (default `eastus2`) |
//! | `byok-vault-real` | `VAULT_ADDR` | Vault cluster URL (consumed by `VaultRealProvider::from_env`) |
//! | `byok-vault-real` | `CORELINK_BYOK_VAULT_REGION` | Logical region label (default `customer-hosted`) |
//!
//! AWS credentials are explicit `CORELINK_BYOK_KMS_*` (or standard AWS)
//! environment variables, while GCP ADC, Entra ID, and Vault auth are
//! resolved by their provider-specific constructors. Missing owner
//! credentials fail closed; see the provider crate docs.
//!
//! # Pattern reference
//!
//! `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7`.

#![forbid(unsafe_code)]

use std::sync::Arc;

use corelink_byok::{BYOKError, KmsProvider};
use tracing::{info, warn};

// ── Multi-flag guard ─────────────────────────────────────────────────
//
// Only one production provider may be linked into the trait-object
// singleton. We expand a `compile_error!` for every pairwise overlap so
// the diagnostic names the exact two flags in conflict — easier to
// triage than a generic "more than one set" message.

#[cfg(all(feature = "byok-aws-real", feature = "byok-gcp-real"))]
compile_error!(
    "BYOK orchestrator: features `byok-aws-real` AND `byok-gcp-real` are \
     mutually exclusive — only one BYOK real provider may be enabled at \
     a time (the orchestrator is a singleton trait object). \
     See specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7."
);

#[cfg(all(feature = "byok-aws-real", feature = "byok-azure-real"))]
compile_error!(
    "BYOK orchestrator: features `byok-aws-real` AND `byok-azure-real` are \
     mutually exclusive — only one BYOK real provider may be enabled at \
     a time. See specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7."
);

#[cfg(all(feature = "byok-aws-real", feature = "byok-vault-real"))]
compile_error!(
    "BYOK orchestrator: features `byok-aws-real` AND `byok-vault-real` are \
     mutually exclusive — only one BYOK real provider may be enabled at \
     a time. See specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7."
);

#[cfg(all(feature = "byok-gcp-real", feature = "byok-azure-real"))]
compile_error!(
    "BYOK orchestrator: features `byok-gcp-real` AND `byok-azure-real` are \
     mutually exclusive — only one BYOK real provider may be enabled at \
     a time. See specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7."
);

#[cfg(all(feature = "byok-gcp-real", feature = "byok-vault-real"))]
compile_error!(
    "BYOK orchestrator: features `byok-gcp-real` AND `byok-vault-real` are \
     mutually exclusive — only one BYOK real provider may be enabled at \
     a time. See specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7."
);

#[cfg(all(feature = "byok-azure-real", feature = "byok-vault-real"))]
compile_error!(
    "BYOK orchestrator: features `byok-azure-real` AND `byok-vault-real` are \
     mutually exclusive — only one BYOK real provider may be enabled at \
     a time. See specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7."
);

// ── Active-provider label (used for telemetry / readiness probes) ────

/// Compile-time discriminator for the active orchestrator dispatch.
///
/// Surfaced via [`active_provider`] so readiness / `/healthz` reflect
/// the binary's real BYOK wiring rather than a runtime guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActiveProvider {
    /// No real provider compiled in. BYOK operations are unavailable and
    /// fail closed; plaintext fallback is never permitted.
    Unavailable,
    /// `byok-aws-real` enabled.
    AwsKms,
    /// `byok-gcp-real` enabled.
    GcpKms,
    /// `byok-azure-real` enabled.
    AzureKeyVault,
    /// `byok-vault-real` enabled.
    HashicorpVault,
}

impl ActiveProvider {
    /// Canonical lowercase label (for logs / audit / metrics).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::AwsKms => "aws",
            Self::GcpKms => "gcp",
            Self::AzureKeyVault => "azure",
            Self::HashicorpVault => "vault",
        }
    }
}

/// Compile-time-constant active-provider label.
#[must_use]
pub const fn active_provider() -> ActiveProvider {
    #[cfg(feature = "byok-aws-real")]
    {
        ActiveProvider::AwsKms
    }
    #[cfg(feature = "byok-gcp-real")]
    {
        ActiveProvider::GcpKms
    }
    #[cfg(feature = "byok-azure-real")]
    {
        ActiveProvider::AzureKeyVault
    }
    #[cfg(feature = "byok-vault-real")]
    {
        ActiveProvider::HashicorpVault
    }
    #[cfg(not(any(
        feature = "byok-aws-real",
        feature = "byok-gcp-real",
        feature = "byok-azure-real",
        feature = "byok-vault-real",
    )))]
    {
        ActiveProvider::Unavailable
    }
}

// ── Dispatch ─────────────────────────────────────────────────────────

/// Construct the singleton BYOK provider for this binary.
///
/// Returns an `Arc<dyn KmsProvider>` whose concrete type is selected at
/// compile time by which (single) `byok-*-real` feature flag is set.
/// With no flag set, construction fails closed. There is deliberately no
/// in-process crypto fallback: a binary without a real KMS provider cannot
/// claim BYOK protection or persist a tenant as active.
///
/// # Errors
///
/// - [`BYOKError::Provider`] if the active provider's constructor fails
///   (missing required env var, SDK init failure, FIPS endpoint
///   resolution failure, etc.).
///
/// # Audit
///
/// On the success path, emits one `info` event at
/// `target = "corelink.byok.orchestrator.audit"` with `audit = true` and
/// `provider = <label>` so the audit chain records the binary's active
/// BYOK wiring at boot. Per-operation audit events are emitted by the
/// concrete provider crates with `target = "corelink.byok.<provider>.audit"`.
pub async fn make_provider() -> Result<Arc<dyn KmsProvider>, BYOKError> {
    let label = active_provider().as_str();
    let provider: Arc<dyn KmsProvider> = build_active().await?;
    info!(
        target: "corelink.byok.orchestrator.audit",
        audit = true,
        op = "boot",
        provider = label,
        "BYOK orchestrator: active provider selected"
    );
    Ok(provider)
}

/// Compile-time dispatch helper — exactly one of the cfg branches is
/// active per build. Returns the trait object directly.
async fn build_active() -> Result<Arc<dyn KmsProvider>, BYOKError> {
    #[cfg(feature = "byok-aws-real")]
    {
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let p = corelink_byok::aws::AwsKmsRealProvider::new(&region).await?;
        Ok(Arc::new(p))
    }
    #[cfg(feature = "byok-gcp-real")]
    {
        let region = std::env::var("GCP_REGION").unwrap_or_else(|_| "us-east1".to_string());
        let p = corelink_byok::gcp::GcpKmsRealProvider::new(&region).await?;
        Ok(Arc::new(p))
    }
    #[cfg(feature = "byok-azure-real")]
    {
        let vault_url = std::env::var("CORELINK_BYOK_AZURE_VAULT_URL").map_err(|_| {
            warn!(
                target: "corelink.byok.orchestrator.audit",
                audit = true,
                op = "boot",
                provider = "azure",
                reason = "azure_vault_url_unset",
                "BYOK orchestrator: CORELINK_BYOK_AZURE_VAULT_URL unset"
            );
            BYOKError::Provider(
                "CORELINK_BYOK_AZURE_VAULT_URL unset (required for byok-azure-real)".to_string(),
            )
        })?;
        let region =
            std::env::var("CORELINK_BYOK_AZURE_REGION").unwrap_or_else(|_| "eastus2".to_string());
        let p = corelink_byok::azure::AzureKeyVaultRealProvider::new(&region, &vault_url)?;
        Ok(Arc::new(p))
    }
    #[cfg(feature = "byok-vault-real")]
    {
        let region = std::env::var("CORELINK_BYOK_VAULT_REGION")
            .unwrap_or_else(|_| "customer-hosted".to_string());
        let p = corelink_byok::vault::VaultRealProvider::from_env(&region)?;
        Ok(Arc::new(p))
    }
    #[cfg(not(any(
        feature = "byok-aws-real",
        feature = "byok-gcp-real",
        feature = "byok-azure-real",
        feature = "byok-vault-real",
    )))]
    {
        warn!(
            target: "corelink.byok.orchestrator.audit",
            audit = true,
            op = "boot",
            provider = "unavailable",
            reason = "no_real_provider_compiled",
            "BYOK orchestrator: no real KMS provider compiled; refusing crypto operations"
        );
        Err(BYOKError::Provider(
            "no real KMS provider compiled (enable exactly one byok-*-real feature)".to_string(),
        ))
    }
}
