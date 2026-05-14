//! `SignupOutcome` + `OrchestrationStep` + `BillingIntent` canonical
//! taxonomies.
//!
//! Per spec contract S-19 §5.1 R-S19-1..R-S19-3 + Lote 10.19 codex P0
//! canonical scope clarification on `INV-ONBOARD-ATOMIC-PROVISIONING`:
//!
//! - `SignupOutcome::Provisioned` — atomic D1 tx committed
//!   (tenant + DPA + first PAT inserted) **and** Stripe customer linked
//!   synchronously (rare; eventually-consistent saga normally drives
//!   the link asynchronously).
//! - `SignupOutcome::Deferred` — atomic D1 tx committed; Stripe link
//!   pending (chaos Stripe outage path; tenant degrades to
//!   `pending_billing_link` state until saga compensation succeeds).
//!   Returns the `BillingIntent::Deferred` flag to the caller as a
//!   202-equivalent + queued-signup marker.
//! - `SignupOutcome::Duplicate` — same `IdempotencyKey` previously
//!   resolved to a `SignupId`; idempotency cache hits return the
//!   original record without re-running the orchestrator.
//! - `SignupOutcome::Rejected` — input validation failed (e.g. empty
//!   `IdempotencyKey`, invalid Clerk event id). The orchestrator emits
//!   `corelink.signup.failed` and does NOT open a D1 transaction.

use crate::billing::StripeCustomerId;
use crate::pat::{PatHash, ShownOnceToken};
use crate::region::PrimaryRegion;
use crate::tenant::{SignupId, TenantId};

/// Atomic-tx staging marker for rollback debugging. Reported as the
/// `step` label on `corelink_signup_atomicity_rollback_total` (Prometheus
/// counter; cardinality bounded per INV-OBS-CARDINALITY-BUDGET).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum OrchestrationStep {
    /// Pre-tx: validated input, opened idempotency-cache lookup.
    Begin,
    /// Inside tx: inserted `tenant` row.
    InsertTenant,
    /// Inside tx: inserted `dpa_acceptance_pending` row.
    InsertDpa,
    /// Inside tx: inserted first `pat` row.
    InsertFirstPat,
    /// Inside tx: inserted `usage_counter` row + committed.
    InsertUsageCounterAndCommit,
}

impl OrchestrationStep {
    /// Canonical lowercase label for Prometheus + audit fields.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::InsertTenant => "insert_tenant",
            Self::InsertDpa => "insert_dpa",
            Self::InsertFirstPat => "insert_first_pat",
            Self::InsertUsageCounterAndCommit => "insert_usage_counter_and_commit",
        }
    }
}

impl core::fmt::Display for OrchestrationStep {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 5-element list of [`OrchestrationStep`] for surface
/// stability regression tests.
#[must_use]
pub const fn canonical_orchestration_steps() -> &'static [&'static str; 5] {
    &[
        "begin",
        "insert_tenant",
        "insert_dpa",
        "insert_first_pat",
        "insert_usage_counter_and_commit",
    ]
}

/// Stripe customer link state at the moment the orchestrator returns.
/// Per Lote 10.19 codex P0 canonical scope: Stripe link is
/// **eventually-consistent** via saga compensation; the atomic boundary
/// covers tenant + DPA + first PAT only.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BillingIntent {
    /// Stripe customer linked synchronously inside the orchestrator
    /// (rare; happy-path when Stripe is healthy).
    Linked {
        /// Stripe customer id assigned to the tenant.
        stripe_customer_id: StripeCustomerId,
    },
    /// Atomic D1 tx committed; Stripe link pending. Tenant degrades to
    /// `pending_billing_link` (read-only mode per WI §9.2 + PAT-DEGRADE-
    /// 001) until the saga compensation flow succeeds.
    PendingBillingLink,
    /// Chaos Stripe outage path: orchestrator returns 503-equivalent +
    /// `Deferred` flag without leaking partial state. The atomic D1 tx
    /// is **still committed** before the billing call (per Lote 10.19
    /// codex P0 canonical scope: Stripe is NOT inside the atomic
    /// boundary); the queued-signup marker triggers retry with
    /// exponential backoff.
    Deferred,
}

/// Signup outcome taxonomy (`#[non_exhaustive]` reserves additive
/// growth for follow-on WIs).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SignupOutcome {
    /// Tenant provisioned end-to-end with Stripe customer linked
    /// synchronously.
    Provisioned {
        /// Tenant id assigned by the orchestrator.
        tenant_id: TenantId,
        /// Signup id (idempotency cache key target).
        signup_id: SignupId,
        /// Primary region the tenant was pinned to.
        primary_region: PrimaryRegion,
        /// First PAT hash recorded in the `pat` row.
        first_pat_hash: PatHash,
        /// Shown-once reveal token; WI-S19-006 surfaces the raw PAT
        /// exactly once when this token is GET'd.
        shown_once_token: ShownOnceToken,
        /// Stripe customer id assigned to the tenant.
        stripe_customer_id: StripeCustomerId,
    },
    /// Tenant provisioned; Stripe link deferred via saga (chaos Stripe
    /// outage path). Tenant exists in `pending_billing_link` state.
    Deferred {
        /// Tenant id assigned by the orchestrator.
        tenant_id: TenantId,
        /// Signup id (idempotency cache key target).
        signup_id: SignupId,
        /// Primary region the tenant was pinned to.
        primary_region: PrimaryRegion,
        /// First PAT hash recorded in the `pat` row.
        first_pat_hash: PatHash,
        /// Shown-once reveal token; WI-S19-006 surfaces the raw PAT.
        shown_once_token: ShownOnceToken,
        /// Billing intent flag (always
        /// [`BillingIntent::PendingBillingLink`] or
        /// [`BillingIntent::Deferred`]).
        billing: BillingIntent,
    },
    /// Idempotency cache hit; the original `SignupId` is returned
    /// unchanged. The orchestrator did NOT open a new D1 transaction.
    Duplicate {
        /// Original signup id (cache hit target).
        signup_id: SignupId,
        /// Original tenant id.
        tenant_id: TenantId,
    },
    /// Input validation failed; orchestrator did NOT open a D1
    /// transaction. The audit event `corelink.signup.failed` is emitted.
    Rejected {
        /// Reason label (terse, audit-emit-safe; the
        /// `OrchestrationError` cause carries the detail).
        reason: String,
    },
}

/// Canonical 4-element list of [`SignupOutcome`] discriminant labels
/// for surface-stability regression tests.
#[must_use]
pub const fn canonical_signup_outcomes() -> &'static [&'static str; 4] {
    &["provisioned", "deferred", "duplicate", "rejected"]
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn orchestration_steps_canonical() {
        let expected = canonical_orchestration_steps();
        let actual: Vec<&str> = [
            OrchestrationStep::Begin,
            OrchestrationStep::InsertTenant,
            OrchestrationStep::InsertDpa,
            OrchestrationStep::InsertFirstPat,
            OrchestrationStep::InsertUsageCounterAndCommit,
        ]
        .iter()
        .map(|s| s.as_str())
        .collect();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn outcomes_canonical() {
        assert_eq!(
            canonical_signup_outcomes(),
            &["provisioned", "deferred", "duplicate", "rejected"]
        );
    }

    #[test]
    fn billing_intent_pending_is_distinct_from_deferred() {
        let pbl = BillingIntent::PendingBillingLink;
        let def = BillingIntent::Deferred;
        assert_ne!(pbl, def);
    }
}
