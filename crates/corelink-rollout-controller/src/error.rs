//! Canonical error taxonomy for `corelink-rollout-controller`
//! (WI-S13-005).
//!
//! All error enums are `#[non_exhaustive]` per Lote 10.6bis discipline.
//!
//! ## Fail policy
//!
//! The rollout controller is **fail-CLOSED** at the state-machine layer:
//! any error in audit emission, storage, or gate evaluation aborts the
//! transition. A partial stage advance or rollback would violate
//! INV-ROLLOUT-SINGLE-ACTIVE and INV-AUDIT-APPEND-ONLY. The production
//! DO wiring surfaces SEV-2 alerts on error; operators use
//! `RB-ROLLOUT-STUCK.md` for manual override.

use thiserror::Error;
use uuid::Uuid;

/// Canonical error surface for [`crate::controller::RolloutController`]
/// methods.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RolloutError {
    /// Deploy artifact missing Cosign signature or Rekor log index.
    /// Enforces INV-SUPPLY-SIGNED-DEPLOY + INV-SUPPLY-PROVENANCE-IN-REKOR
    /// (S-12). Rollout blocked at `start()`.
    #[error("deploy artifact missing Cosign signature (INV-SUPPLY-SIGNED-DEPLOY)")]
    UnsignedDeploy,

    /// Monthly rollback budget exceeded (>30% of monthly error budget
    /// consumed via auto-rollbacks in the rolling 30d window); all further
    /// rollouts frozen until manual override (Architect + Security lead
    /// approval + ADR waiver) or natural 30d window expiry.
    #[error(
        "monthly rollback budget exceeded ({consumed:.4} > 30%); deploys frozen — \
         Architect + Security lead override required"
    )]
    BudgetExceeded {
        /// Fraction of monthly budget consumed (0.0–1.0+; >1.0 = overrun).
        consumed: f64,
    },

    /// A rollout is already in-flight for this environment. Enforces
    /// INV-ROLLOUT-SINGLE-ACTIVE (D1 UNIQUE active constraint + DO
    /// singleton per env). Concurrent start returns the in-flight handle.
    #[error(
        "rollout in-flight (handle {0}); concurrent start blocked \
         (INV-ROLLOUT-SINGLE-ACTIVE)"
    )]
    RolloutInFlight(Uuid),

    /// Cloudflare gradual deploy API returned an error. Production
    /// wiring: stage stuck > 1h → SEV-3 alert + `RB-ROLLOUT-STUCK.md`
    /// manual override path.
    #[error("Cloudflare gradual deploy API error: {0}")]
    CloudflareApi(String),

    /// Storage (D1) error during state read or write. Fail-CLOSED:
    /// transition aborted; state remains at last committed value.
    #[error("storage error: {0}")]
    Storage(String),

    /// Audit emission failed (fail-CLOSED: INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
    /// Transition aborted; D1 state unchanged.
    #[error("audit emission failed (state unchanged): {0}")]
    Audit(String),

    /// Attempted to bypass progressive stage progression (direct 100%
    /// deploy). Returns 403 + audit `corelink.admin.rollout.bypass_attempted`.
    /// INV-ROLLOUT-NO-STAGE-SKIP.
    #[error(
        "stage bypass rejected — progressive stage progression enforced \
         (INV-ROLLOUT-NO-STAGE-SKIP)"
    )]
    StageBypassed,

    /// Internal state invariant violation (e.g. handle not found,
    /// state-machine inconsistency). Fail-CLOSED: abort + SEV-2 alert.
    #[error("rollout controller internal invariant violated: {0}")]
    Internal(String),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests use panics as assertions"
)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn unsigned_deploy_displays() {
        let e = RolloutError::UnsignedDeploy;
        assert!(format!("{e}").contains("Cosign"));
    }

    #[test]
    fn budget_exceeded_displays_consumed() {
        let e = RolloutError::BudgetExceeded { consumed: 0.32 };
        let s = format!("{e}");
        assert!(s.contains("frozen"));
        assert!(s.contains("0.3200"));
    }

    #[test]
    fn rollout_in_flight_displays_uuid() {
        let id = Uuid::now_v7();
        let e = RolloutError::RolloutInFlight(id);
        let s = format!("{e}");
        assert!(s.contains(&id.to_string()));
        assert!(s.contains("concurrent start blocked"));
    }

    #[test]
    fn stage_bypassed_displays() {
        let e = RolloutError::StageBypassed;
        assert!(format!("{e}").contains("bypass rejected"));
    }

    #[test]
    fn cloudflare_api_displays() {
        let e = RolloutError::CloudflareApi("503 Service Unavailable".to_string());
        assert!(format!("{e}").contains("503"));
    }
}
