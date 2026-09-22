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
    /// The provider rejected creation of a Checkout session. The reservation
    /// remains open so a later retry can reconcile the provider first.
    #[error("runner checkout session creation failed: {0}")]
    ProviderCreateFailed(String),
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

/// Provider operation that creates a session using the attempt's exact
/// idempotency key. Implementations must pass that key unchanged to Stripe.
pub trait RunnerCheckoutSessionCreator {
    /// Create one hosted session for the reserved attempt.
    fn create(&self, attempt: &RunnerCheckoutAttempt)
        -> Result<String, RunnerCheckoutAttemptError>;
}

/// Provider lookup used after a process crashed before recording Stripe's
/// response. `Some` means Stripe accepted the original request; `None` means
/// no session exists and the reservation may be abandoned.
pub trait RunnerCheckoutSessionReconciler {
    /// Find the provider session by the stored idempotency key.
    fn reconcile(
        &self,
        attempt: &RunnerCheckoutAttempt,
    ) -> Result<Option<String>, RunnerCheckoutAttemptError>;
}

/// Complete provider seam used by [`RunnerCheckoutOrchestrator`].
pub trait RunnerCheckoutProvider:
    RunnerCheckoutSessionCreator
    + RunnerCheckoutSessionExpiry
    + RunnerCheckoutSessionReconciler
    + core::fmt::Debug
    + Send
    + Sync
{
}

impl<T> RunnerCheckoutProvider for T where
    T: RunnerCheckoutSessionCreator
        + RunnerCheckoutSessionExpiry
        + RunnerCheckoutSessionReconciler
        + core::fmt::Debug
        + Send
        + Sync
{
}

/// Coordinates the durable ledger with Stripe's create, expire, and lookup
/// calls. It never creates a replacement while an open attempt is present.
#[derive(Clone, Debug)]
pub struct RunnerCheckoutOrchestrator<P> {
    ledger: InMemoryRunnerCheckoutAttemptLedger,
    provider: std::sync::Arc<P>,
}

impl<P: RunnerCheckoutProvider> RunnerCheckoutOrchestrator<P> {
    /// Construct an orchestrator over a ledger and provider adapter.
    #[must_use]
    pub fn new(ledger: InMemoryRunnerCheckoutAttemptLedger, provider: std::sync::Arc<P>) -> Self {
        Self { ledger, provider }
    }

    /// Return the payable attempt, creating it only after any prior attempt
    /// has been reconciled or confirmed expired by the provider.
    pub fn checkout(
        &self,
        tenant_id: TenantId,
        tier: TierKind,
        price_id: impl Into<String>,
        customer_id: impl Into<String>,
    ) -> Result<RunnerCheckoutAttempt, RunnerCheckoutAttemptError> {
        let price_id = price_id.into();
        let customer_id = customer_id.into();
        let decision = self
            .ledger
            .begin(tenant_id, tier, price_id.clone(), customer_id.clone())?;
        match decision {
            RunnerCheckoutAttemptDecision::Replay(attempt)
                if attempt.state == RunnerCheckoutAttemptState::Reserved =>
            {
                // A concurrent click can observe the reservation while its
                // owner is between D1 and Stripe. First recover a provider
                // response already committed under that key; if none exists,
                // reuse the same key rather than allocating a generation.
                let session_id = match self.provider.reconcile(&attempt)? {
                    Some(id) => id,
                    None => self
                        .provider
                        .create(&attempt)
                        .map_err(|error| match error {
                            RunnerCheckoutAttemptError::ProviderCreateFailed(_) => error,
                            other => {
                                RunnerCheckoutAttemptError::ProviderCreateFailed(other.to_string())
                            }
                        })?,
                };
                self.ledger
                    .record_session(&attempt.tenant_id, attempt.generation, &session_id)?;
                self.ledger
                    .current(&attempt.tenant_id, attempt.generation)
                    .ok_or(RunnerCheckoutAttemptError::GenerationMismatch)
            }
            RunnerCheckoutAttemptDecision::Replay(attempt) => Ok(attempt),
            RunnerCheckoutAttemptDecision::Create(attempt) => {
                let session_id = self
                    .provider
                    .create(&attempt)
                    .map_err(|error| match error {
                        RunnerCheckoutAttemptError::ProviderCreateFailed(_) => error,
                        other => {
                            RunnerCheckoutAttemptError::ProviderCreateFailed(other.to_string())
                        }
                    })?;
                self.ledger
                    .record_session(&attempt.tenant_id, attempt.generation, &session_id)?;
                self.ledger
                    .current(&attempt.tenant_id, attempt.generation)
                    .ok_or(RunnerCheckoutAttemptError::GenerationMismatch)
            }
            RunnerCheckoutAttemptDecision::ExpiryRequired(attempt) => {
                self.provider.expire(&attempt)?;
                let session_id = attempt
                    .session_id
                    .as_deref()
                    .ok_or(RunnerCheckoutAttemptError::InvalidTransition)?;
                self.ledger
                    .mark_expired(&attempt.tenant_id, attempt.generation, session_id)?;
                self.checkout(
                    attempt.tenant_id,
                    tier,
                    price_id.clone(),
                    customer_id.clone(),
                )
            }
            RunnerCheckoutAttemptDecision::RecoveryRequired(attempt) => {
                match self.provider.reconcile(&attempt)? {
                    Some(session_id) => {
                        self.ledger.record_session(
                            &attempt.tenant_id,
                            attempt.generation,
                            &session_id,
                        )?;
                        self.ledger
                            .current(&attempt.tenant_id, attempt.generation)
                            .ok_or(RunnerCheckoutAttemptError::GenerationMismatch)
                    }
                    None => {
                        self.ledger
                            .mark_provider_absent(&attempt.tenant_id, attempt.generation)?;
                        self.checkout(attempt.tenant_id, tier, price_id, customer_id)
                    }
                }
            }
        }
    }

    /// Expose the underlying ledger for webhook and reconciliation wiring.
    #[must_use]
    pub fn ledger(&self) -> &InMemoryRunnerCheckoutAttemptLedger {
        &self.ledger
    }
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
        if rows.values().flatten().any(|other| {
            (other.tenant_id != *tenant_id || other.generation != generation)
                && other.session_id.as_deref() == Some(session_id)
        }) {
            return Err(RunnerCheckoutAttemptError::InvalidTransition);
        }
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

    /// Snapshot the current generation after a provider mutation is recorded.
    #[must_use]
    pub fn current(&self, tenant_id: &TenantId, generation: u64) -> Option<RunnerCheckoutAttempt> {
        self.attempts
            .lock()
            .ok()
            .and_then(|rows| rows.get(tenant_id)?.last().cloned())
            .filter(|attempt| attempt.generation == generation)
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
    use std::collections::HashMap;

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

    #[derive(Debug, Default)]
    struct FakeProvider {
        sessions: Mutex<HashMap<String, String>>,
        create_calls: Mutex<Vec<String>>,
        expire_calls: Mutex<Vec<String>>,
        fail_expire: Mutex<bool>,
        lose_next_ack: Mutex<bool>,
    }

    impl RunnerCheckoutSessionCreator for FakeProvider {
        fn create(
            &self,
            attempt: &RunnerCheckoutAttempt,
        ) -> Result<String, RunnerCheckoutAttemptError> {
            let mut calls = self.create_calls.lock().unwrap();
            calls.push(attempt.idempotency_key.clone());
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(id) = sessions.get(&attempt.idempotency_key).cloned() {
                return Ok(id);
            }
            let id = format!("cs_{}", sessions.len() + 1);
            sessions.insert(attempt.idempotency_key.clone(), id.clone());
            if *self.lose_next_ack.lock().unwrap() {
                *self.lose_next_ack.lock().unwrap() = false;
                return Err(RunnerCheckoutAttemptError::ProviderCreateFailed(
                    "lost response after provider commit".to_string(),
                ));
            }
            Ok(id)
        }
    }

    impl RunnerCheckoutSessionExpiry for FakeProvider {
        fn expire(
            &self,
            attempt: &RunnerCheckoutAttempt,
        ) -> Result<(), RunnerCheckoutAttemptError> {
            self.expire_calls
                .lock()
                .unwrap()
                .push(attempt.session_id.clone().unwrap());
            if *self.fail_expire.lock().unwrap() {
                return Err(RunnerCheckoutAttemptError::ProviderExpiryFailed(
                    "provider refused expiry".to_string(),
                ));
            }
            Ok(())
        }
    }

    impl RunnerCheckoutSessionReconciler for FakeProvider {
        fn reconcile(
            &self,
            attempt: &RunnerCheckoutAttempt,
        ) -> Result<Option<String>, RunnerCheckoutAttemptError> {
            Ok(self
                .sessions
                .lock()
                .unwrap()
                .get(&attempt.idempotency_key)
                .cloned())
        }
    }

    fn orchestrator() -> (RunnerCheckoutOrchestrator<FakeProvider>, Arc<FakeProvider>) {
        let provider = Arc::new(FakeProvider::default());
        (
            RunnerCheckoutOrchestrator::new(
                InMemoryRunnerCheckoutAttemptLedger::default(),
                Arc::clone(&provider),
            ),
            provider,
        )
    }

    #[test]
    fn plan_change_expires_before_allocating_replacement() {
        let (orchestrator, provider) = orchestrator();
        let old = orchestrator
            .checkout(
                TenantId::new("tenant-a"),
                TierKind::RunnerStarter,
                "price_a",
                "cus_a",
            )
            .unwrap();
        let replacement = orchestrator
            .checkout(
                TenantId::new("tenant-a"),
                TierKind::RunnerPro,
                "price_b",
                "cus_a",
            )
            .unwrap();
        assert_eq!(replacement.generation, old.generation + 1);
        assert_eq!(*provider.expire_calls.lock().unwrap(), vec!["cs_1"]);
        assert_eq!(provider.create_calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn failed_expiry_leaves_old_session_and_blocks_replacement() {
        let (orchestrator, provider) = orchestrator();
        let old = orchestrator
            .checkout(
                TenantId::new("tenant-a"),
                TierKind::RunnerStarter,
                "price_a",
                "cus_a",
            )
            .unwrap();
        *provider.fail_expire.lock().unwrap() = true;
        assert!(matches!(
            orchestrator.checkout(
                TenantId::new("tenant-a"),
                TierKind::RunnerPro,
                "price_b",
                "cus_a"
            ),
            Err(RunnerCheckoutAttemptError::ProviderExpiryFailed(_))
        ));
        assert_eq!(
            orchestrator
                .ledger()
                .current(&old.tenant_id, old.generation)
                .unwrap()
                .state,
            RunnerCheckoutAttemptState::SessionCreated
        );
        assert_eq!(provider.create_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn lost_create_ack_reconciles_without_a_second_session() {
        let (orchestrator, provider) = orchestrator();
        *provider.lose_next_ack.lock().unwrap() = true;
        assert!(orchestrator
            .checkout(
                TenantId::new("tenant-a"),
                TierKind::RunnerStarter,
                "price_a",
                "cus_a",
            )
            .is_err());
        let recovered = orchestrator
            .checkout(
                TenantId::new("tenant-a"),
                TierKind::RunnerStarter,
                "price_a",
                "cus_a",
            )
            .unwrap();
        assert_eq!(recovered.session_id.as_deref(), Some("cs_1"));
        assert_eq!(provider.create_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_orchestrator_clicks_have_one_payable_identity() {
        let (orchestrator, provider) = orchestrator();
        let shared = Arc::new(orchestrator);
        let mut joins = Vec::new();
        for _ in 0..8 {
            let shared = Arc::clone(&shared);
            joins.push(std::thread::spawn(move || {
                shared.checkout(
                    TenantId::new("tenant-a"),
                    TierKind::RunnerStarter,
                    "price_a",
                    "cus_a",
                )
            }));
        }
        let attempts: Vec<_> = joins
            .into_iter()
            .map(|join| join.join().unwrap().unwrap())
            .collect();
        assert!(attempts
            .iter()
            .all(|attempt| attempt.session_id == Some("cs_1".to_string())));
        assert_eq!(provider.sessions.lock().unwrap().len(), 1);
    }
}
