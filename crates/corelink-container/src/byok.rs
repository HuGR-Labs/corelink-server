//! BYOK provider factory — feature-gated production wire-up.
//!
//! Activated by `--features byok-aws-real`. Returns a boxed
//! `KmsProvider` trait object so future GCP / Azure / Vault providers
//! drop in behind the same surface.
//!
//! Pattern reference (canonical for next 3 providers):
//! `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md`.

#![forbid(unsafe_code)]

use std::sync::Arc;

use corelink_byok::aws::AwsKmsRealProvider;
use corelink_byok::{BYOKError, KmsProvider};

/// Construct the production AWS KMS provider for the configured region.
///
/// FIPS endpoint is enforced unconditionally — see
/// `AwsKmsRealProvider::new` docs. AWS credentials are resolved via the
/// standard AWS SDK chain (env vars, shared config, IRSA, IMDS, SSO).
///
/// # Errors
///
/// Surfaces SDK initialisation errors as [`BYOKError::Provider`].
pub async fn make_aws_kms_provider(region: &str) -> Result<Arc<dyn KmsProvider>, BYOKError> {
    let p = AwsKmsRealProvider::new(region).await?;
    Ok(Arc::new(p))
}
