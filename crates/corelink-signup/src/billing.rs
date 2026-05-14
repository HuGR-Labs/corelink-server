//! Billing client trait + in-memory fakes (Stripe customer create
//! abstraction).
//!
//! Per Lote 10.19 codex P0 canonical scope clarification on
//! `INV-ONBOARD-ATOMIC-PROVISIONING`: Stripe customer create is
//! **outside** the atomic D1 transaction boundary (calling Stripe HTTPS
//! inside `BEGIN ... COMMIT` would lock D1 for the duration of the
//! network round-trip and starve concurrent signups). The orchestrator
//! commits the atomic D1 tx FIRST, then invokes the billing client; on
//! transient outage the orchestrator returns
//! `SignupOutcome::Deferred { billing: BillingIntent::Deferred }` and
//! the saga compensation worker (deferred to PRR ship gate) retries
//! with exponential backoff. On persistent failure the tenant remains
//! in `pending_billing_link` state per WI §9.2 + PAT-DEGRADE-001.

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::tenant::TenantId;

/// Stripe customer id newtype — opaque `cus_...` token in production.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StripeCustomerId(String);

impl StripeCustomerId {
    /// Construct a new customer id from any string-like value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for StripeCustomerId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Billing client error taxonomy.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum BillingError {
    /// Transient outage — caller should map to
    /// `SignupOutcome::Deferred { billing: BillingIntent::Deferred }`
    /// and let the saga compensation worker retry.
    #[error("billing transient outage: {0}")]
    Outage(String),
    /// Non-retryable transport / validation error.
    #[error("billing non-retryable: {0}")]
    NonRetryable(String),
}

impl BillingError {
    /// True if the error is transient (mappable to `Deferred` outcome).
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(self, Self::Outage(_))
    }
}

/// Trait every billing client satisfies. Production wiring is a Stripe
/// HTTPS customer-create call via `worker::send_future` fire-and-forget
/// (NEVER `tokio::spawn` per Lote 10.7bis R5 P0-3).
pub trait BillingClient: core::fmt::Debug + Send + Sync {
    /// Create a Stripe customer for the given `tenant_id`. Returns the
    /// assigned `StripeCustomerId` on success. Transient outages MUST
    /// surface as [`BillingError::Outage`] so the orchestrator can map
    /// to the chaos / `Deferred` arm without leaking partial state.
    fn create_customer(&self, tenant_id: &TenantId) -> Result<StripeCustomerId, BillingError>;
}

/// In-memory billing client — succeeds every call with a deterministic
/// `cus_<tenant_id>` token.
#[derive(Clone, Debug, Default)]
pub struct InMemoryBillingClient {
    customers: Arc<Mutex<Vec<(TenantId, StripeCustomerId)>>>,
}

impl InMemoryBillingClient {
    /// Construct an empty in-memory client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded `(tenant, customer)` pairs.
    #[must_use]
    pub fn snapshot(&self) -> Vec<(TenantId, StripeCustomerId)> {
        match self.customers.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl BillingClient for InMemoryBillingClient {
    fn create_customer(&self, tenant_id: &TenantId) -> Result<StripeCustomerId, BillingError> {
        let cid = StripeCustomerId::new(format!("cus_{tenant_id}"));
        let mut g = self
            .customers
            .lock()
            .map_err(|e| BillingError::NonRetryable(format!("mutex poisoned: {e}")))?;
        g.push((tenant_id.clone(), cid.clone()));
        Ok(cid)
    }
}

/// Adversarial fixture — every call returns [`BillingError::Outage`].
/// Used by the chaos-Stripe-outage property test to verify the
/// orchestrator maps to `SignupOutcome::Deferred` without leaking
/// partial state.
#[derive(Clone, Debug, Default)]
pub struct StripeOutageBillingClient;

impl BillingClient for StripeOutageBillingClient {
    fn create_customer(&self, _tenant_id: &TenantId) -> Result<StripeCustomerId, BillingError> {
        Err(BillingError::Outage(
            "adversarial fixture: stripe 503".to_string(),
        ))
    }
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
    fn in_memory_creates_customer() {
        let c = InMemoryBillingClient::new();
        let cid = c.create_customer(&TenantId::new("t-1")).unwrap();
        assert_eq!(cid.as_str(), "cus_t-1");
        assert_eq!(c.snapshot().len(), 1);
    }

    #[test]
    fn outage_client_is_transient() {
        let c = StripeOutageBillingClient;
        let err = c.create_customer(&TenantId::new("t-1")).unwrap_err();
        assert!(err.is_transient());
        assert!(matches!(err, BillingError::Outage(_)));
    }

    #[test]
    fn non_retryable_is_not_transient() {
        let e = BillingError::NonRetryable("bad input".to_string());
        assert!(!e.is_transient());
    }
}
