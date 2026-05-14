//! Signup orchestrator — the load-bearing atomicity + idempotency +
//! chaos integration entry point.
//!
//! ## Decision flow (canonical)
//!
//! 1. **Input validation** — empty `IdempotencyKey` / empty
//!    `clerk_event_id` → `SignupOutcome::Rejected` + audit
//!    `corelink.signup.failed`. Pre-tx; no D1 lock.
//! 2. **Idempotency cache lookup** — same `IdempotencyKey` previously
//!    committed → return `SignupOutcome::Duplicate { signup_id,
//!    tenant_id }`. No audit event re-emitted (the original `started` +
//!    `completed`/`deferred` chain stands).
//! 3. **Audit emit `started`** — fail-CLOSED before any state mutation
//!    (INV-AUDIT-APPEND-ONLY + Lote 10.6bis).
//! 4. **Atomic D1 tx** — `begin` → `insert_tenant` →
//!    `insert_dpa_pending` → `insert_first_pat` → `insert_usage_counter`
//!    → `commit`. ANY step error triggers `rollback` + audit emit
//!    `corelink.signup.failed { step }` → return
//!    `OrchestrationError::Storage`.
//! 5. **Stripe customer create (saga, OUTSIDE atomic boundary)** —
//!    invoke [`BillingClient::create_customer`]. Success → audit emit
//!    `corelink.signup.completed` → return `SignupOutcome::Provisioned
//!    { stripe_customer_id }`. Transient outage (`BillingError::Outage`)
//!    → audit emit `corelink.signup.deferred` → return
//!    `SignupOutcome::Deferred { billing: BillingIntent::Deferred }`.
//!    Non-retryable error → `SignupOutcome::Deferred { billing:
//!    BillingIntent::PendingBillingLink }` (tenant degrades to
//!    `pending_billing_link` until manual reconciliation).
//!
//! ## Invariants enforced
//!
//! - **Audit-emit-BEFORE-mutation** fail-CLOSED — `started` emitted
//!   before `begin`; `failed` emitted before `rollback`-side return;
//!   `completed` / `deferred` emitted before returning the outcome to
//!   the caller.
//! - **Atomic boundary** — tenant + DPA + first PAT + usage counter
//!   committed all-or-nothing per Lote 10.19 codex P0 canonical scope.
//! - **Idempotency** — same `IdempotencyKey` → same `SignupId`;
//!   duplicate path skips the atomic tx entirely.
//! - **Chaos / Stripe outage** — D1 atomic tx commits FIRST, so a
//!   `BillingError::Outage` after commit returns
//!   `SignupOutcome::Deferred` (202-equivalent) with zero partial state
//!   leak.
//! - **PAT-CORRELATION-ID-001** — every audit event carries the inbound
//!   `correlation_id`.

use std::sync::{Arc, Mutex};

use crate::audit::{SignupAuditEventType, SignupAuditRecord, SignupAuditSink};
use crate::billing::{BillingClient, BillingError, StripeCustomerId};
use crate::error::OrchestrationError;
use crate::outcome::{BillingIntent, OrchestrationStep, SignupOutcome};
use crate::pat::{PatHash, ShownOnceToken};
use crate::region::PrimaryRegion;
use crate::request::SignupRequest;
use crate::store::{
    AtomicSignupStore, DpaPendingRow, PatRow, SignupTx, StorageError, TenantRow, UsageCounterRow,
};
use crate::tenant::{SignupId, TenantId};

/// Trait wired by the orchestrator to mint deterministic-but-fresh
/// identifiers + token material per signup. Production wiring uses
/// UUID v7 + a cryptographically-strong RNG for the PAT material; tests
/// inject a monotonic counter via [`InMemoryProvisionRecord`].
///
/// Per CTRL-CRED-001: the raw PAT material NEVER appears on the
/// [`SignupRequest`]; the orchestrator owns generation + hashing +
/// shown-once-token issuance.
pub trait ProvisionRecord: core::fmt::Debug + Send + Sync {
    /// Mint a fresh tenant id (UUID v7 in prod).
    fn mint_tenant_id(&self) -> TenantId;
    /// Mint a fresh signup id (UUID v7 in prod; used as the
    /// idempotency cache value).
    fn mint_signup_id(&self) -> SignupId;
    /// Mint a fresh first-PAT hash (hybrid HMAC + Argon2id of a
    /// freshly-rolled CSPRNG token in prod).
    fn mint_first_pat_hash(&self, tenant_id: &TenantId) -> PatHash;
    /// Mint a fresh shown-once reveal token (UUID v7 in prod).
    fn mint_shown_once_token(&self) -> ShownOnceToken;
}

/// In-memory deterministic provision record for tests.
#[derive(Clone, Debug, Default)]
pub struct InMemoryProvisionRecord {
    counter: Arc<Mutex<u64>>,
}

impl InMemoryProvisionRecord {
    /// Construct a new in-memory record starting at counter 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn next(&self) -> u64 {
        match self.counter.lock() {
            Ok(mut g) => {
                *g = g.saturating_add(1);
                *g
            }
            Err(p) => {
                let mut g = p.into_inner();
                *g = g.saturating_add(1);
                *g
            }
        }
    }
}

impl ProvisionRecord for InMemoryProvisionRecord {
    fn mint_tenant_id(&self) -> TenantId {
        TenantId::new(format!("t-{}", self.next()))
    }

    fn mint_signup_id(&self) -> SignupId {
        SignupId::new(format!("s-{}", self.next()))
    }

    fn mint_first_pat_hash(&self, tenant_id: &TenantId) -> PatHash {
        PatHash::new(format!("h-{}-{}", tenant_id, self.next()))
    }

    fn mint_shown_once_token(&self) -> ShownOnceToken {
        ShownOnceToken::new(format!("tok-{}", self.next()))
    }
}

/// Orchestrator response surface — what the Cloudflare Worker handler
/// turns into the HTTP response body. The handler maps:
///
/// - `SignupOutcome::Provisioned` → `201 Created`.
/// - `SignupOutcome::Deferred` → `202 Accepted` (queued; the response
///   includes `Retry-After`).
/// - `SignupOutcome::Duplicate` → `200 OK` (idempotent replay).
/// - `SignupOutcome::Rejected` → `400 Bad Request`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SignupResponse {
    /// Resulting outcome.
    pub outcome: SignupOutcome,
    /// Correlation id echoed back to the caller (PAT-CORRELATION-ID-001).
    pub correlation_id: crate::correlation::CorrelationId,
}

/// Signup orchestrator — the load-bearing entry point.
#[derive(Clone, Debug)]
pub struct SignupOrchestrator<A, S, B, P>
where
    A: SignupAuditSink,
    S: AtomicSignupStore,
    B: BillingClient,
    P: ProvisionRecord,
{
    audit: A,
    store: S,
    billing: B,
    provision: P,
}

impl<A, S, B, P> SignupOrchestrator<A, S, B, P>
where
    A: SignupAuditSink,
    S: AtomicSignupStore,
    B: BillingClient,
    P: ProvisionRecord,
{
    /// Construct a new orchestrator from its 4 trait collaborators.
    #[must_use]
    pub fn new(audit: A, store: S, billing: B, provision: P) -> Self {
        Self {
            audit,
            store,
            billing,
            provision,
        }
    }

    /// Run the full orchestration pipeline for `request`. Returns a
    /// [`SignupResponse`] on every non-Internal arm.
    pub fn provision(
        &self,
        request: &SignupRequest,
    ) -> Result<SignupResponse, OrchestrationError> {
        // -- Step 1: input validation (pre-tx; no D1 lock). --
        if request.idempotency_key.is_empty() {
            let rec = SignupAuditRecord::new(
                SignupAuditEventType::Failed,
                request.correlation_id.clone(),
                request.idempotency_key.as_str(),
            )
            .with_reason("empty_idempotency_key");
            self.audit.emit(&rec)?;
            return Ok(SignupResponse {
                outcome: SignupOutcome::Rejected {
                    reason: "empty_idempotency_key".to_string(),
                },
                correlation_id: request.correlation_id.clone(),
            });
        }
        if request.clerk_event_id.is_empty() {
            let rec = SignupAuditRecord::new(
                SignupAuditEventType::Failed,
                request.correlation_id.clone(),
                request.idempotency_key.as_str(),
            )
            .with_reason("empty_clerk_event_id");
            self.audit.emit(&rec)?;
            return Ok(SignupResponse {
                outcome: SignupOutcome::Rejected {
                    reason: "empty_clerk_event_id".to_string(),
                },
                correlation_id: request.correlation_id.clone(),
            });
        }

        // -- Step 2: idempotency cache lookup (R-S19-1). --
        // Audit-emit ordering canonical (`lookup → emit → mutate`).
        let cached = self
            .store
            .lookup_idempotent(&request.idempotency_key)
            .map_err(|e| OrchestrationError::Storage {
                step: OrchestrationStep::Begin,
                source: e,
            })?;
        if let Some((signup_id, tenant_id)) = cached {
            return Ok(SignupResponse {
                outcome: SignupOutcome::Duplicate {
                    signup_id,
                    tenant_id,
                },
                correlation_id: request.correlation_id.clone(),
            });
        }

        // -- Step 3: region pin (Lote 10.16 cookie-canonical). --
        let primary_region = PrimaryRegion::from_locale(&request.locale);

        // -- Step 4: emit `started` BEFORE opening the atomic tx. --
        let started_rec = SignupAuditRecord::new(
            SignupAuditEventType::Started,
            request.correlation_id.clone(),
            request.idempotency_key.as_str(),
        )
        .with_region(primary_region);
        self.audit.emit(&started_rec)?;

        // -- Step 5: atomic D1 tx. --
        let tenant_id = self.provision.mint_tenant_id();
        let signup_id = self.provision.mint_signup_id();
        let first_pat_hash = self.provision.mint_first_pat_hash(&tenant_id);
        let shown_once_token = self.provision.mint_shown_once_token();

        let tx = self
            .store
            .begin()
            .map_err(|e| self.fail_pre_tx(request, OrchestrationStep::Begin, e))?;

        match self.run_atomic_tx(
            request,
            tx,
            primary_region,
            &tenant_id,
            &signup_id,
            &first_pat_hash,
            &shown_once_token,
        ) {
            Ok(()) => {}
            Err((step, source)) => {
                let rec = SignupAuditRecord::new(
                    SignupAuditEventType::Failed,
                    request.correlation_id.clone(),
                    request.idempotency_key.as_str(),
                )
                .with_region(primary_region)
                .with_step(step)
                .with_reason(source.to_string());
                self.audit.emit(&rec)?;
                return Err(OrchestrationError::Storage { step, source });
            }
        }

        // -- Step 6: Stripe saga (OUTSIDE atomic boundary). --
        // Per Lote 10.19 codex P0 canonical scope clarification: the
        // atomic D1 tx is ALREADY committed; a transient Stripe outage
        // maps to `SignupOutcome::Deferred` (202) without leaking
        // partial state.
        match self.billing.create_customer(&tenant_id) {
            Ok(stripe_customer_id) => {
                let rec = SignupAuditRecord::new(
                    SignupAuditEventType::Completed,
                    request.correlation_id.clone(),
                    request.idempotency_key.as_str(),
                )
                .with_region(primary_region);
                self.audit.emit(&rec)?;
                Ok(SignupResponse {
                    outcome: SignupOutcome::Provisioned {
                        tenant_id,
                        signup_id,
                        primary_region,
                        first_pat_hash,
                        shown_once_token,
                        stripe_customer_id,
                    },
                    correlation_id: request.correlation_id.clone(),
                })
            }
            Err(billing_err) => self.handle_billing_failure(
                request,
                billing_err,
                primary_region,
                tenant_id,
                signup_id,
                first_pat_hash,
                shown_once_token,
            ),
        }
    }

    /// Walk the 4-step atomic D1 tx. On any step error, rolls back +
    /// reports the failing step.
    #[allow(clippy::too_many_arguments)]
    fn run_atomic_tx(
        &self,
        request: &SignupRequest,
        mut tx: SignupTx,
        primary_region: PrimaryRegion,
        tenant_id: &TenantId,
        signup_id: &SignupId,
        first_pat_hash: &PatHash,
        shown_once_token: &ShownOnceToken,
    ) -> Result<(), (OrchestrationStep, StorageError)> {
        let tenant_row = TenantRow {
            tenant_id: tenant_id.clone(),
            signup_id: signup_id.clone(),
            email_hash: request.email_hash.clone(),
            primary_region,
        };
        if let Err(e) = self.store.insert_tenant(&mut tx, tenant_row) {
            let _ = self.store.rollback(tx);
            return Err((OrchestrationStep::InsertTenant, e));
        }
        let dpa_row = DpaPendingRow {
            tenant_id: tenant_id.clone(),
        };
        if let Err(e) = self.store.insert_dpa_pending(&mut tx, dpa_row) {
            let _ = self.store.rollback(tx);
            return Err((OrchestrationStep::InsertDpa, e));
        }
        let pat_row = PatRow {
            tenant_id: tenant_id.clone(),
            pat_hash: first_pat_hash.clone(),
            shown_once_token: shown_once_token.clone(),
        };
        if let Err(e) = self.store.insert_first_pat(&mut tx, pat_row) {
            let _ = self.store.rollback(tx);
            return Err((OrchestrationStep::InsertFirstPat, e));
        }
        let uc_row = UsageCounterRow {
            tenant_id: tenant_id.clone(),
        };
        if let Err(e) = self.store.insert_usage_counter(&mut tx, uc_row) {
            let _ = self.store.rollback(tx);
            return Err((OrchestrationStep::InsertUsageCounterAndCommit, e));
        }
        if let Err(e) = self.store.commit(tx, &request.idempotency_key) {
            // The store's `commit` is responsible for its own internal
            // rollback on error; the orchestrator only reports the step.
            return Err((OrchestrationStep::InsertUsageCounterAndCommit, e));
        }
        Ok(())
    }

    fn fail_pre_tx(
        &self,
        request: &SignupRequest,
        step: OrchestrationStep,
        source: StorageError,
    ) -> OrchestrationError {
        let rec = SignupAuditRecord::new(
            SignupAuditEventType::Failed,
            request.correlation_id.clone(),
            request.idempotency_key.as_str(),
        )
        .with_step(step)
        .with_reason(source.to_string());
        // Best-effort emit; if the audit sink also fails we surface the
        // Audit error (audit-emit-BEFORE-mutation fail-CLOSED semantics
        // already aborted the mutation).
        if let Err(audit_err) = self.audit.emit(&rec) {
            return OrchestrationError::Audit(audit_err);
        }
        OrchestrationError::Storage { step, source }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_billing_failure(
        &self,
        request: &SignupRequest,
        billing_err: BillingError,
        primary_region: PrimaryRegion,
        tenant_id: TenantId,
        signup_id: SignupId,
        first_pat_hash: PatHash,
        shown_once_token: ShownOnceToken,
    ) -> Result<SignupResponse, OrchestrationError> {
        // Tenant + DPA + first PAT are ALREADY committed (atomic D1 tx
        // is OUTSIDE the billing boundary per Lote 10.19 P0). The
        // failure mode determines which `Deferred` flag the response
        // carries; either way the orchestrator does NOT roll back the
        // committed rows (that would be the leak the WI prohibits).
        let (intent, reason) = match &billing_err {
            BillingError::Outage(msg) => (BillingIntent::Deferred, format!("stripe_outage:{msg}")),
            BillingError::NonRetryable(msg) => (
                BillingIntent::PendingBillingLink,
                format!("stripe_non_retryable:{msg}"),
            ),
        };
        let rec = SignupAuditRecord::new(
            SignupAuditEventType::Deferred,
            request.correlation_id.clone(),
            request.idempotency_key.as_str(),
        )
        .with_region(primary_region)
        .with_reason(reason);
        self.audit.emit(&rec)?;
        Ok(SignupResponse {
            outcome: SignupOutcome::Deferred {
                tenant_id,
                signup_id,
                primary_region,
                first_pat_hash,
                shown_once_token,
                billing: intent,
            },
            correlation_id: request.correlation_id.clone(),
        })
    }
}

// Suppress unused-import lint without enabling dead-code; the
// `StripeCustomerId` symbol is re-exported by the crate root + used in
// outcome::SignupOutcome::Provisioned + downstream consumers.
const _: fn() -> Option<StripeCustomerId> = || None;

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
    use crate::audit::{FailingSignupAuditSink, InMemorySignupAuditSink};
    use crate::billing::{InMemoryBillingClient, StripeOutageBillingClient};
    use crate::correlation::CorrelationId;
    use crate::idempotency::IdempotencyKey;
    use crate::region::Bcp47Locale;
    use crate::store::{FailingAtomicSignupStore, InMemoryAtomicSignupStore};
    use crate::tenant::UserEmailHash;

    fn req(idem: &str, email: &str, locale: &str, cid: &str) -> SignupRequest {
        SignupRequest::new(
            cid,
            UserEmailHash::new(email),
            Bcp47Locale::new(locale),
            IdempotencyKey::new(idem),
            CorrelationId::new(cid),
        )
    }

    fn happy_orchestrator() -> SignupOrchestrator<
        InMemorySignupAuditSink,
        InMemoryAtomicSignupStore,
        InMemoryBillingClient,
        InMemoryProvisionRecord,
    > {
        SignupOrchestrator::new(
            InMemorySignupAuditSink::new(),
            InMemoryAtomicSignupStore::new(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        )
    }

    #[test]
    fn happy_path_provisioned() {
        let o = happy_orchestrator();
        let r = o.provision(&req("idem-1", "a", "en-US", "evt_1")).unwrap();
        match r.outcome {
            SignupOutcome::Provisioned {
                primary_region,
                stripe_customer_id,
                ..
            } => {
                assert_eq!(primary_region, PrimaryRegion::Enam);
                assert!(stripe_customer_id.as_str().starts_with("cus_"));
            }
            other => panic!("expected Provisioned, got {other:?}"),
        }
    }

    #[test]
    fn region_pinned_from_pt_br_cookie() {
        let o = happy_orchestrator();
        let r = o.provision(&req("idem-pt", "a", "pt-BR", "evt_pt")).unwrap();
        match r.outcome {
            SignupOutcome::Provisioned { primary_region, .. } => {
                assert_eq!(primary_region, PrimaryRegion::Sam);
            }
            other => panic!("expected Provisioned, got {other:?}"),
        }
    }

    #[test]
    fn empty_idempotency_key_rejected() {
        let o = happy_orchestrator();
        let r = o.provision(&req("", "a", "en-US", "evt_1")).unwrap();
        assert!(matches!(r.outcome, SignupOutcome::Rejected { .. }));
    }

    #[test]
    fn empty_clerk_event_id_rejected() {
        let audit = InMemorySignupAuditSink::new();
        let store = InMemoryAtomicSignupStore::new();
        let billing = InMemoryBillingClient::new();
        let prov = InMemoryProvisionRecord::new();
        let o = SignupOrchestrator::new(audit, store, billing, prov);
        let req = SignupRequest::new(
            "",
            UserEmailHash::new("a"),
            Bcp47Locale::new("en-US"),
            IdempotencyKey::new("idem-1"),
            CorrelationId::new("cid"),
        );
        let r = o.provision(&req).unwrap();
        assert!(matches!(r.outcome, SignupOutcome::Rejected { .. }));
    }

    #[test]
    fn idempotent_replay_returns_duplicate() {
        let o = happy_orchestrator();
        let r1 = o.provision(&req("idem-1", "a", "en-US", "evt_1")).unwrap();
        let r2 = o.provision(&req("idem-1", "a", "en-US", "evt_1")).unwrap();
        let original_signup = match r1.outcome {
            SignupOutcome::Provisioned { signup_id, .. } => signup_id,
            other => panic!("expected Provisioned, got {other:?}"),
        };
        match r2.outcome {
            SignupOutcome::Duplicate { signup_id, .. } => {
                assert_eq!(signup_id, original_signup);
            }
            other => panic!("expected Duplicate, got {other:?}"),
        }
    }

    #[test]
    fn stripe_outage_deferred_no_partial_state_leak() {
        let audit = InMemorySignupAuditSink::new();
        let store = InMemoryAtomicSignupStore::new();
        let billing = StripeOutageBillingClient;
        let prov = InMemoryProvisionRecord::new();
        let o = SignupOrchestrator::new(audit.clone(), store.clone(), billing, prov);
        let r = o.provision(&req("idem-1", "a", "en-US", "evt_1")).unwrap();
        match r.outcome {
            SignupOutcome::Deferred { billing, .. } => {
                assert_eq!(billing, BillingIntent::Deferred);
            }
            other => panic!("expected Deferred, got {other:?}"),
        }
        // The atomic D1 tx is still committed; the chaos outage does
        // NOT leak partial state — exactly one tenant row exists.
        assert_eq!(store.committed_tenant_count().unwrap(), 1);
        // Audit chain MUST carry both `started` AND `deferred`.
        let snap = audit.snapshot();
        let kinds: Vec<_> = snap.iter().map(|r| r.event_type).collect();
        assert!(kinds.contains(&SignupAuditEventType::Started));
        assert!(kinds.contains(&SignupAuditEventType::Deferred));
    }

    #[test]
    fn atomicity_rollback_on_insert_tenant_failure() {
        let audit = InMemorySignupAuditSink::new();
        let store = InMemoryAtomicSignupStore::new();
        store.inject_failure_at(OrchestrationStep::InsertTenant);
        let billing = InMemoryBillingClient::new();
        let prov = InMemoryProvisionRecord::new();
        let o = SignupOrchestrator::new(audit.clone(), store.clone(), billing, prov);
        let err = o.provision(&req("idem-1", "a", "en-US", "evt_1")).unwrap_err();
        match err {
            OrchestrationError::Storage { step, .. } => {
                assert_eq!(step, OrchestrationStep::InsertTenant);
            }
            other => panic!("expected Storage error, got {other:?}"),
        }
        // Zero partial rows committed.
        assert_eq!(store.committed_tenant_count().unwrap(), 0);
        // Audit chain MUST carry `started` AND `failed`.
        let snap = audit.snapshot();
        let kinds: Vec<_> = snap.iter().map(|r| r.event_type).collect();
        assert!(kinds.contains(&SignupAuditEventType::Started));
        assert!(kinds.contains(&SignupAuditEventType::Failed));
    }

    #[test]
    fn atomicity_rollback_on_insert_first_pat_failure() {
        let store = InMemoryAtomicSignupStore::new();
        store.inject_failure_at(OrchestrationStep::InsertFirstPat);
        let o = SignupOrchestrator::new(
            InMemorySignupAuditSink::new(),
            store.clone(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );
        let err = o.provision(&req("idem-1", "a", "en-US", "evt_1")).unwrap_err();
        assert!(matches!(
            err,
            OrchestrationError::Storage {
                step: OrchestrationStep::InsertFirstPat,
                ..
            }
        ));
        assert_eq!(store.committed_tenant_count().unwrap(), 0);
    }

    #[test]
    fn audit_emit_before_mutate_fail_closed() {
        let o = SignupOrchestrator::new(
            FailingSignupAuditSink,
            InMemoryAtomicSignupStore::new(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );
        // The first audit emit (`failed` for empty key) fails-closed,
        // surfacing OrchestrationError::Audit.
        let err = o.provision(&req("", "a", "en-US", "evt_1")).unwrap_err();
        assert!(matches!(err, OrchestrationError::Audit(_)));
    }

    #[test]
    fn pre_tx_begin_failure_propagates() {
        let o = SignupOrchestrator::new(
            InMemorySignupAuditSink::new(),
            FailingAtomicSignupStore,
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );
        let err = o.provision(&req("idem-1", "a", "en-US", "evt_1")).unwrap_err();
        assert!(matches!(
            err,
            OrchestrationError::Storage {
                step: OrchestrationStep::Begin,
                ..
            }
        ));
    }

    #[test]
    fn correlation_id_propagated_to_audit() {
        let audit = InMemorySignupAuditSink::new();
        let o = SignupOrchestrator::new(
            audit.clone(),
            InMemoryAtomicSignupStore::new(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );
        o.provision(&req("idem-1", "a", "en-US", "evt_xyz")).unwrap();
        let snap = audit.snapshot();
        assert!(!snap.is_empty());
        for r in &snap {
            assert_eq!(r.correlation_id.as_str(), "evt_xyz");
        }
    }
}
