//! Durable runner Checkout attempt identity and safe replacement state machine.
//!
//! The D1 schema is `migrations/d1/0136_runner_checkout_attempts.sql`.
//! This in-memory model pins the transaction contract for its later D1
//! adapter: a new selected plan cannot create a second payable session until
//! the prior attempt is proven terminal by the provider.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use thiserror::Error;

use crate::{tenant::TenantId, tier::TierKind};
/// Lifecycle states persisted for a runner Checkout attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerCheckoutAttemptState {
    /// The identity was durably reserved before creating a provider session.
    Reserved,
    /// The provider returned a payable Checkout session.
    SessionCreated,
    /// The provider confirmed that its Checkout session is expired.
    Expired,
    /// The provider confirmed no session exists for a crashed reservation.
    Abandoned,
    /// The Checkout session converted successfully.
    Completed,
}

impl RunnerCheckoutAttemptState {
    /// Whether a new attempt may be allocated after this state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Expired | Self::Abandoned | Self::Completed)
    }
}

/// Immutable identity and exact provider parameters for one logical attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerCheckoutAttempt {
    /// Tenant that owns this runner-axis attempt.
    pub tenant_id: TenantId,
    /// Monotonic, tenant-local logical-attempt generation.
    pub generation: u64,
    /// Selected runner tier.
    pub tier: TierKind,
    /// Exact Stripe price id bound to this attempt.
    pub price_id: String,
    /// Exact Stripe customer id bound to this attempt.
    pub customer_id: String,
    /// Deterministic provider idempotency key for this attempt generation.
    pub idempotency_key: String,
    /// Current durable lifecycle state.
    pub state: RunnerCheckoutAttemptState,
    /// Stripe Checkout session id after creation.
    pub session_id: Option<String>,
}

impl RunnerCheckoutAttempt {
    fn matches(&self, tier: TierKind, price_id: &str, customer_id: &str) -> bool {
        self.tier == tier && self.price_id == price_id && self.customer_id == customer_id
    }
}

/// Atomic result of requesting a runner Checkout attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerCheckoutAttemptDecision {
    /// A new attempt was durably allocated; creating its provider session is allowed.
    Create(RunnerCheckoutAttempt),
    /// The exact existing attempt must be retried with its same provider key.
    Replay(RunnerCheckoutAttempt),
    /// The existing session must be expired by the provider before replacement.
    ExpiryRequired(RunnerCheckoutAttempt),
    /// A crashed reservation needs provider reconciliation before replacement.
    RecoveryRequired(RunnerCheckoutAttempt),
}

/// State-machine errors that fail closed instead of creating a replacement.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RunnerCheckoutAttemptError {
    /// A cache tier attempted to enter the runner-only ledger.
    #[error("runner checkout attempt requires a runner tier")]
    NonRunnerTier,
    /// A late writer tried to mutate an attempt other than the current generation.
    #[error("runner checkout attempt generation is no longer current")]
    GenerationMismatch,
    /// The requested state transition is not allowed.
    #[error("runner checkout attempt transition is invalid")]
    InvalidTransition,
    /// The provider refused to expire an unpaid Checkout session.
    #[error("runner checkout session expiry failed: {0}")]
    ProviderExpiryFailed(String),
}

/// Provider operation required before replacing an unpaid runner Checkout.
///
/// WP2 implements this against Stripe. Its caller must invoke
/// [`InMemoryRunnerCheckoutAttemptLedger::mark_expired`] only after this
/// operation reports success; a failed expiry leaves the durable attempt open.
pub trait RunnerCheckoutSessionExpiry {
    /// Expire the session attached to an [`RunnerCheckoutAttemptDecision::ExpiryRequired`] attempt.
    fn expire(&self, attempt: &RunnerCheckoutAttempt) -> Result<(), RunnerCheckoutAttemptError>;
}

/// Atomic in-memory model of the D1 runner checkout attempt ledger.
#[derive(Clone, Debug, Default)]
pub struct InMemoryRunnerCheckoutAttemptLedger {
    attempts: Arc<Mutex<HashMap<TenantId, Vec<RunnerCheckoutAttempt>>>>,
}

impl InMemoryRunnerCheckoutAttemptLedger {
    /// Allocate a fresh logical attempt, replay an exact retry, or fail closed.
    pub fn begin(
        &self,
        tenant_id: TenantId,
        tier: TierKind,
        price_id: impl Into<String>,
        customer_id: impl Into<String>,
    ) -> Result<RunnerCheckoutAttemptDecision, RunnerCheckoutAttemptError> {
        if !is_runner_tier(tier) {
            return Err(RunnerCheckoutAttemptError::NonRunnerTier);
        }
        let price_id = price_id.into();
        let customer_id = customer_id.into();
        let mut rows = self
            .attempts
            .lock()
            .map_err(|_| RunnerCheckoutAttemptError::InvalidTransition)?;
        let attempts = rows.entry(tenant_id.clone()).or_default();
        if let Some(current) = attempts.last() {
            if !current.state.is_terminal() {
                return Ok(if current.matches(tier, &price_id, &customer_id) {
                    RunnerCheckoutAttemptDecision::Replay(current.clone())
                } else if current.state == RunnerCheckoutAttemptState::SessionCreated {
                    RunnerCheckoutAttemptDecision::ExpiryRequired(current.clone())
                } else {
                    RunnerCheckoutAttemptDecision::RecoveryRequired(current.clone())
                });
            }
        }
        let generation = attempts.last().map_or(1, |attempt| attempt.generation + 1);
        let attempt = RunnerCheckoutAttempt {
            idempotency_key: format!("checkout:v2:runner:{}:{generation}", tenant_id.as_str()),
            tenant_id,
            generation,
            tier,
            price_id,
            customer_id,
            state: RunnerCheckoutAttemptState::Reserved,
            session_id: None,
        };
        attempts.push(attempt.clone());
        Ok(RunnerCheckoutAttemptDecision::Create(attempt))
    }

    /// Record a session only for the current reserved generation; same-session retry is idempotent.
    pub fn record_session(
        &self,
        tenant_id: &TenantId,
        generation: u64,
        session_id: &str,
    ) -> Result<(), RunnerCheckoutAttemptError> {
        let mut rows = self
            .attempts
            .lock()
            .map_err(|_| RunnerCheckoutAttemptError::InvalidTransition)?;
        let attempt = current(&mut rows, tenant_id, generation)?;
        match (&attempt.state, &attempt.session_id) {
            (RunnerCheckoutAttemptState::Reserved, None) => {
                attempt.state = RunnerCheckoutAttemptState::SessionCreated;
                attempt.session_id = Some(session_id.to_owned());
                Ok(())
            }
            (RunnerCheckoutAttemptState::SessionCreated, Some(existing))
                if existing == session_id =>
            {
                Ok(())
            }
            _ => Err(RunnerCheckoutAttemptError::InvalidTransition),
        }
    }

    /// Admit a replacement only after the provider has confirmed expiry.
    pub fn mark_expired(
        &self,
        tenant_id: &TenantId,
        generation: u64,
        session_id: &str,
    ) -> Result<(), RunnerCheckoutAttemptError> {
        let mut rows = self
            .attempts
            .lock()
            .map_err(|_| RunnerCheckoutAttemptError::InvalidTransition)?;
        let attempt = current(&mut rows, tenant_id, generation)?;
        if attempt.state == RunnerCheckoutAttemptState::SessionCreated
            && attempt.session_id.as_deref() == Some(session_id)
        {
            attempt.state = RunnerCheckoutAttemptState::Expired;
            Ok(())
        } else {
            Err(RunnerCheckoutAttemptError::InvalidTransition)
        }
    }

    /// Mark the current session converted after the authenticated webhook confirms it.
    pub fn mark_completed(
        &self,
        tenant_id: &TenantId,
        generation: u64,
        session_id: &str,
    ) -> Result<(), RunnerCheckoutAttemptError> {
        let mut rows = self
            .attempts
            .lock()
            .map_err(|_| RunnerCheckoutAttemptError::InvalidTransition)?;
        let attempt = current(&mut rows, tenant_id, generation)?;
        if attempt.state == RunnerCheckoutAttemptState::SessionCreated
            && attempt.session_id.as_deref() == Some(session_id)
        {
            attempt.state = RunnerCheckoutAttemptState::Completed;
            Ok(())
        } else {
            Err(RunnerCheckoutAttemptError::InvalidTransition)
        }
    }

    /// Close a pre-session reservation only after provider reconciliation proved it absent.
    pub fn mark_provider_absent(
        &self,
        tenant_id: &TenantId,
        generation: u64,
    ) -> Result<(), RunnerCheckoutAttemptError> {
        let mut rows = self
            .attempts
            .lock()
            .map_err(|_| RunnerCheckoutAttemptError::InvalidTransition)?;
        let attempt = current(&mut rows, tenant_id, generation)?;
        if attempt.state == RunnerCheckoutAttemptState::Reserved && attempt.session_id.is_none() {
            attempt.state = RunnerCheckoutAttemptState::Abandoned;
            Ok(())
        } else {
            Err(RunnerCheckoutAttemptError::InvalidTransition)
        }
    }
}

fn current<'a>(
    rows: &'a mut HashMap<TenantId, Vec<RunnerCheckoutAttempt>>,
    tenant_id: &TenantId,
    generation: u64,
) -> Result<&'a mut RunnerCheckoutAttempt, RunnerCheckoutAttemptError> {
    rows.get_mut(tenant_id)
        .and_then(|attempts| attempts.last_mut())
        .filter(|attempt| attempt.generation == generation)
        .ok_or(RunnerCheckoutAttemptError::GenerationMismatch)
}

const fn is_runner_tier(tier: TierKind) -> bool {
    matches!(
        tier,
        TierKind::RunnerStarter
            | TierKind::RunnerPro
            | TierKind::RunnerTeam
            | TierKind::RunnerScale
            | TierKind::RunnerMax
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests assert state-machine outcomes"
)]
mod tests {
    use super::*;

    fn begin(
        ledger: &InMemoryRunnerCheckoutAttemptLedger,
        tier: TierKind,
    ) -> RunnerCheckoutAttemptDecision {
        ledger
            .begin(
                TenantId::new("tenant-a"),
                tier,
                format!("price_{}", tier.as_str()),
                "cus-a",
            )
            .unwrap()
    }

    #[test]
    fn exact_retry_reuses_one_durable_generation() {
        let ledger = InMemoryRunnerCheckoutAttemptLedger::default();
        let RunnerCheckoutAttemptDecision::Create(first) = begin(&ledger, TierKind::RunnerStarter)
        else {
            panic!()
        };
        let RunnerCheckoutAttemptDecision::Replay(retry) = begin(&ledger, TierKind::RunnerStarter)
        else {
            panic!()
        };
        assert_eq!(first.idempotency_key, retry.idempotency_key);
        assert_eq!(first.generation, retry.generation);
        assert_eq!(first.idempotency_key, "checkout:v2:runner:tenant-a:1");
    }

    #[test]
    fn changed_unpaid_plan_needs_expiry_before_new_generation() {
        let ledger = InMemoryRunnerCheckoutAttemptLedger::default();
        let RunnerCheckoutAttemptDecision::Create(first) = begin(&ledger, TierKind::RunnerStarter)
        else {
            panic!()
        };
        ledger
            .record_session(&first.tenant_id, first.generation, "cs_old")
            .unwrap();
        assert!(matches!(
            begin(&ledger, TierKind::RunnerPro),
            RunnerCheckoutAttemptDecision::ExpiryRequired(_)
        ));
        ledger
            .mark_expired(&first.tenant_id, first.generation, "cs_old")
            .unwrap();
        let RunnerCheckoutAttemptDecision::Create(replacement) =
            begin(&ledger, TierKind::RunnerPro)
        else {
            panic!()
        };
        assert_eq!(replacement.generation, 2);
        assert_ne!(replacement.idempotency_key, first.idempotency_key);
    }

    #[test]
    fn stale_reservation_needs_reconciliation_before_replacement() {
        let ledger = InMemoryRunnerCheckoutAttemptLedger::default();
        let RunnerCheckoutAttemptDecision::Create(first) = begin(&ledger, TierKind::RunnerStarter)
        else {
            panic!()
        };
        assert!(matches!(
            begin(&ledger, TierKind::RunnerPro),
            RunnerCheckoutAttemptDecision::RecoveryRequired(_)
        ));
        ledger
            .mark_provider_absent(&first.tenant_id, first.generation)
            .unwrap();
        assert!(matches!(
            begin(&ledger, TierKind::RunnerPro),
            RunnerCheckoutAttemptDecision::Create(_)
        ));
    }

    #[test]
    fn concurrent_clicks_share_one_attempt() {
        let ledger = InMemoryRunnerCheckoutAttemptLedger::default();
        let a = ledger.clone();
        let b = ledger.clone();
        let first = std::thread::spawn(move || begin(&a, TierKind::RunnerStarter));
        let second = std::thread::spawn(move || begin(&b, TierKind::RunnerStarter));
        let key = |decision: RunnerCheckoutAttemptDecision| match decision {
            RunnerCheckoutAttemptDecision::Create(attempt)
            | RunnerCheckoutAttemptDecision::Replay(attempt) => attempt.idempotency_key,
            _ => panic!(),
        };
        assert_eq!(key(first.join().unwrap()), key(second.join().unwrap()));
    }
}
