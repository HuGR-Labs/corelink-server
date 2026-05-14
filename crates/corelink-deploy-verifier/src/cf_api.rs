//! Cloudflare API client trait for deploy propagation.
//!
//! In production this calls the CF Workers API to deploy a pinned-digest
//! version of the Worker script.  During S-12 testing the
//! [`crate::verifier::InMemoryCfApiClient`] is used.
//!
//! # TOCTOU mitigation
//!
//! The `pinned_digest` parameter is the SHA-256 image digest resolved by the
//! Cosign verify step.  Passing the digest (not the tag) to the CF API call
//! ensures that an attacker cannot swap the OCI image between verify and deploy
//! (WI-S12-003 §2 threat 10 + §9.6).

use crate::error::DeployVerifyError;

/// Result returned by a successful [`CfApiClient::propagate`] call.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PropagateResult {
    /// Cloudflare deployment ID returned by the CF API.
    pub deployment_id: String,
}

/// Cloudflare API client trait.
///
/// Implementations must use the `pinned_digest` (not a floating tag) when
/// calling the CF API to prevent TOCTOU attacks.
///
/// # Token IAM scoping
///
/// The CF API token used by implementations must be scoped to
/// `Workers Scripts:Edit` only for the `corelink-deploy-verifier` Worker
/// identity (CTRL-AUTH-014).  Manual `wrangler deploy` must be blocked via
/// IAM per §6.1.4.
pub trait CfApiClient: Send + Sync {
    /// Propagate the deploy to the Cloudflare Workers API.
    ///
    /// Uses the pinned `pinned_digest` (SHA-256) to prevent TOCTOU.
    ///
    /// # Errors
    ///
    /// Returns [`DeployVerifyError::CfApiFailed`] on any CF API error.
    fn propagate(
        &self,
        script_name: &str,
        pinned_digest: &str,
    ) -> Result<PropagateResult, DeployVerifyError>;
}
