//! `BillingHarness` — the shared test fixture for the R3-5 end-to-end
//! billing-flow scenarios.
//!
//! The harness composes the four in-memory ledgers
//! (`corelink-signup`, `corelink-tier-selection`,
//! `corelink-billing-stripe`, `corelink-dsr`) plus a small
//! harness-local audit log that records cross-system gate decisions
//! (e.g. "DSR Erasure blocked because subscription is active") which
//! do not naturally belong to any one upstream audit chain.
//!
//! Every identifier (tenant slug, signup payload, Stripe customer id,
//! DSR request id, data subject id) is a deterministic function of
//! the test tenant name so the harness is byte-reproducible across
//! runs. The harness pins a single `now_ms` clock (the canonical
//! `1_700_000_000_000` instant shared with `e2e-signup-flow`) so
//! signature replay-window math is stable.
//!
//! ## Audit-fail-CLOSED ordering at the harness boundary
//!
//! Each helper method on [`BillingHarness`] records the cross-system
//! gate decision via [`BillingHarness::audit_emit_gate`] BEFORE
//! returning the success arm. The composed ledgers already audit-emit
//! BEFORE their own state mutations (per ADR-S10-001 + ADR-S11-002);
//! the harness-local log is additive observability for the
//! cross-system contract, not a replacement.

use std::sync::{Arc, Mutex};

use corelink_billing_stripe::{
    compute_signature, InMemoryStripeAuditSink, InMemoryStripeWebhookHandler,
    InMemoryStripeWebhookLog, StripeAdapterDecision, StripeError, StripeWebhookHandler,
    WebhookEvent, WebhookEventKind, WebhookHandleRequest,
};
use corelink_dsr::{
    DsrDecision, DsrEndpoint, DsrError, DsrJurisdiction, DsrRequest, DsrRequestKind,
    InMemoryDsrAuditSink, InMemoryDsrEndpoint, InMemoryDsrRequestStore,
    InMemoryJwtReceiptIssuer, InMemoryMfaStepUpVerifier, MfaStepUpToken,
};
use corelink_signup::{
    Bcp47Locale, CorrelationId, IdempotencyKey, InMemoryAtomicSignupStore,
    InMemoryBillingClient, InMemorySignupAuditSink, SignupOrchestrator, SignupOutcome,
    SignupRequest, SignupResponse, TenantId as SignupTenantId, UserEmailHash,
};
use corelink_signup::orchestrator::InMemoryProvisionRecord;
use corelink_tier_selection::{
    InMemoryDpaGate, InMemoryStripeClient, InMemoryTierSelectionAuditSink,
    StripeCheckoutSessionCompletedEvent, StripeClient, StripeCustomerId,
    SubscriptionActivationReceipt, TenantCtx as TierTenantCtx, TenantId as TierTenantId,
    TierKind, TierSelectionLedger, TierSelectionReceipt,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Canonical DPA version pinned across the R3-5 harness (mirrors the
/// `e2e-signup-flow` baseline).
pub const TEST_DPA_VERSION: &str = "1.0.0";

/// Canonical webhook signing secret used by the harness. NOT a real
/// secret — exists solely to drive the HMAC-SHA256 path in the
/// composed `corelink-billing-stripe` webhook handler.
pub const TEST_WEBHOOK_SECRET: &[u8] = b"whsec_test_e2e_billing_flow_harness_r3_5";

/// Canonical pinned wall-clock instant (Unix epoch ms) — shared by
/// every ledger in [`BillingHarness`] so signature replay-window math
/// is deterministic across runs.
pub const FIXED_NOW_MS: u64 = 1_700_000_000_000;

/// Canonical pinned wall-clock instant in seconds — derived from
/// [`FIXED_NOW_MS`] so the Stripe-Signature `t=` header math matches
/// the receiver-side `now_ms` exactly.
pub const FIXED_NOW_SECONDS: u64 = FIXED_NOW_MS / 1_000;

/// Canonical lifecycle state of a subscription at the harness
/// boundary. The granular `corelink-tier-selection` ledger only
/// models the on/off + pending arm; the harness layers
/// cancelled / refunded on top so the test scenarios can pin the
/// post-cancel + post-refund states with a single typed surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubscriptionLifecycleState {
    /// No subscription yet (post-signup, pre-checkout).
    PreCheckout,
    /// Checkout Session created; awaiting Stripe webhook activation.
    PendingActivation,
    /// Subscription is `active` (Stripe `checkout.session.completed`
    /// webhook landed).
    Active,
    /// Customer initiated cancel; subscription remains active until
    /// `period_end_ms` per Stripe spec.
    CancelScheduledAtPeriodEnd {
        /// Wall-clock instant the subscription will flip to
        /// `Cancelled` (Unix epoch ms).
        period_end_ms: u64,
    },
    /// Subscription has been refunded after cancel; final terminal
    /// state.
    Refunded,
}

/// Canonical typed error surface for the harness. The integration
/// tests pattern-match on this enum so the failure arms are pinned.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BillingHarnessError {
    /// Signup orchestrator returned a non-`Provisioned` outcome.
    /// Boxed to keep the enum size bounded (`SignupOutcome` carries
    /// 128+ bytes of payload).
    #[error("signup orchestrator returned non-provisioned outcome: {0:?}")]
    SignupRejected(Box<SignupOutcome>),
    /// Tier selection returned an unexpected receipt arm.
    #[error("tier-selection returned unexpected receipt: {0}")]
    TierUnexpected(String),
    /// Tier-selection error (DPA-not-accepted / lock-held / etc.).
    #[error("tier-selection error: {0}")]
    Tier(#[from] corelink_tier_selection::TierError),
    /// Billing-stripe webhook handler error.
    #[error("billing-stripe webhook error: {0}")]
    Stripe(#[from] StripeError),
    /// DSR endpoint error.
    #[error("dsr endpoint error: {0}")]
    Dsr(#[from] DsrError),
    /// Signup orchestrator internal error (mutex poisoned / store
    /// failure).
    #[error("signup orchestrator error: {0}")]
    Signup(String),
    /// Harness invariant violated (caller drove the harness into an
    /// unreachable transition, e.g. `request_refund` before
    /// `cancel_subscription`).
    #[error("billing-harness invariant: {0}")]
    Invariant(String),
    /// DSR Erasure attempted while a subscription is still active —
    /// the canonical "must cancel first" gate.
    #[error(
        "dsr erasure blocked: subscription is currently {state:?}; \
         customer must cancel + wait for refund settlement first"
    )]
    DsrBlockedSubscriptionActive {
        /// Lifecycle state at the time of the rejection.
        state: SubscriptionLifecycleState,
    },
}

/// Short alias for the canonical Result returned by harness methods.
pub type BillingError<T> = Result<T, BillingHarnessError>;

/// Canonical per-tenant test bundle returned by [`make_test_tenant`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BillingTenant {
    /// Logical tenant slug (also seeded into all derived
    /// identifiers).
    pub name: String,
    /// Deterministic signup payload.
    pub signup_request: SignupRequest,
}

/// Build a [`BillingTenant`] whose identifiers (idempotency key,
/// correlation id, Clerk event id, email hash) are deterministic
/// functions of `name`. Mirrors the `e2e-signup-flow` derivation so
/// the R3-5 harness shares the same canonical seeding contract.
#[must_use]
pub fn make_test_tenant(name: &str) -> BillingTenant {
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    h.update(b":corelink-e2e-billing-flow-r3-5");
    let email_hash = hex::encode(h.finalize());
    let idem = format!("idem-billing-{name}");
    let clerk_evt = format!("evt_e2e_billing_{name}");
    BillingTenant {
        name: name.to_string(),
        signup_request: SignupRequest::new(
            clerk_evt.clone(),
            UserEmailHash::new(email_hash),
            Bcp47Locale::new("en-US"),
            IdempotencyKey::new(idem),
            CorrelationId::new(clerk_evt),
        ),
    }
}

/// Derive a canonical UUIDv4 for `name` by hashing the slug + a
/// per-purpose salt.
fn derive_uuid(name: &str, salt: &str) -> Uuid {
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    h.update(b":");
    h.update(salt.as_bytes());
    let digest = h.finalize();
    // Take first 16 bytes for the canonical 128-bit UUID. `Sha256`
    // always yields 32 bytes so `get(..16)` is structurally `Some`,
    // but we keep the typed fallback so clippy `indexing_slicing` is
    // satisfied at the lint boundary.
    let mut bytes = [0u8; 16];
    if let Some(prefix) = digest.get(..16) {
        bytes.copy_from_slice(prefix);
    }
    // Set the canonical UUIDv4 marker bits (RFC 4122 §4.4) so the
    // resulting Uuid round-trips through the `uuid` crate cleanly.
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Derive the canonical tenant UUID for `name`.
#[must_use]
pub fn derive_tenant_uuid(name: &str) -> Uuid {
    derive_uuid(name, "tenant")
}

/// Derive the canonical data-subject UUID for `name`.
#[must_use]
pub fn derive_data_subject_id(name: &str) -> Uuid {
    derive_uuid(name, "data-subject")
}

/// Derive the canonical DSR request UUID for `name` + `tag` (so the
/// same tenant can submit multiple DSR requests within a scenario by
/// passing distinct tags).
#[must_use]
pub fn derive_request_id(name: &str, tag: &str) -> Uuid {
    derive_uuid(name, &format!("dsr-request:{tag}"))
}

/// A pre-signed Stripe webhook envelope — the (header, payload) pair
/// the receiver consumes.
#[derive(Clone, Debug)]
pub struct SignedWebhook {
    /// Value of the `Stripe-Signature` header (`t=<seconds>,v1=<hex>`).
    pub header: String,
    /// Raw payload bytes (what HMAC was computed over).
    pub payload: Vec<u8>,
    /// Canonical event id (matches `WebhookEvent::stripe_event_id`).
    pub event_id: String,
    /// Canonical event kind.
    pub kind: WebhookEventKind,
    /// Wall-clock instant the signature carries (Unix epoch ms).
    pub event_ts_ms: u64,
}

impl SignedWebhook {
    /// Build the typed [`WebhookEvent`] the receiver dispatches on.
    #[must_use]
    pub fn to_event(&self) -> WebhookEvent {
        WebhookEvent {
            stripe_event_id: self.event_id.clone(),
            kind: self.kind,
            event_ts_ms: self.event_ts_ms,
            payload_redacted: self.payload.clone(),
        }
    }
}

/// Build a valid-by-construction signed webhook envelope: HMAC-SHA256
/// over `t=<seconds>.{payload}` using [`TEST_WEBHOOK_SECRET`].
///
/// # Errors
///
/// Returns [`BillingHarnessError::Stripe`] if the HMAC primitive
/// rejects the secret bytes (structurally unreachable for the canonical
/// secret pinned by this crate, but kept fail-CLOSED).
pub fn build_signed_webhook(
    payload: &[u8],
    event_id: impl Into<String>,
    kind: WebhookEventKind,
    timestamp_seconds: u64,
) -> BillingError<SignedWebhook> {
    let tag = compute_signature(TEST_WEBHOOK_SECRET, timestamp_seconds, payload)
        .map_err(BillingHarnessError::Stripe)?;
    let header = format!("t={timestamp_seconds},v1={}", hex::encode(tag));
    Ok(SignedWebhook {
        header,
        payload: payload.to_vec(),
        event_id: event_id.into(),
        kind,
        event_ts_ms: timestamp_seconds.saturating_mul(1_000),
    })
}

/// Cross-system gate decision recorded by the harness-local audit
/// log. Distinct from the per-ledger audit chains (which already
/// audit-emit BEFORE their own state mutations).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HarnessGateEvent {
    /// Customer requested DSR Erasure but the subscription is still
    /// active — request was blocked at the harness boundary BEFORE
    /// reaching the DSR endpoint.
    DsrErasureBlockedSubscriptionActive {
        /// Tenant slug.
        tenant: String,
        /// Lifecycle state at the time of the gate.
        state: SubscriptionLifecycleState,
        /// Wall-clock instant of the gate decision (Unix epoch ms).
        now_ms: u64,
    },
    /// Customer requested cancel; cancel was recorded at
    /// `period_end_ms` per Stripe spec.
    CancelScheduledAtPeriodEnd {
        /// Tenant slug.
        tenant: String,
        /// Wall-clock instant the subscription will flip to
        /// `Cancelled` (Unix epoch ms).
        period_end_ms: u64,
    },
    /// Customer cancelled + 100% refund webhook processed.
    RefundProcessed {
        /// Tenant slug.
        tenant: String,
        /// Wall-clock instant of the refund (Unix epoch ms).
        now_ms: u64,
    },
}

/// Whether the harness-local audit log contains a [`HarnessGateEvent`]
/// matching `predicate`. Replaces the per-test inline iteration so
/// the assertion is one-liner.
#[must_use]
pub fn audit_chain_contains<F>(log: &[HarnessGateEvent], predicate: F) -> bool
where
    F: Fn(&HarnessGateEvent) -> bool,
{
    log.iter().any(predicate)
}

/// Per-tenant lifecycle snapshot the harness keeps in
/// [`BillingHarness::tenants`].
#[derive(Clone, Debug)]
struct TenantState {
    name: String,
    #[allow(dead_code, reason = "retained for cross-system tenant identity debugging")]
    signup_tenant_id: SignupTenantId,
    tier_tenant_id: TierTenantId,
    lifecycle: SubscriptionLifecycleState,
    stripe_customer_id: Option<StripeCustomerId>,
    pending_session_id: Option<String>,
}

/// In-memory composed harness for the R3-5 billing-flow scenarios.
///
/// Construct via [`BillingHarness::setup`]; each test creates a fresh
/// harness so cross-test contamination is structurally impossible
/// (per the F-001 closure pattern used by every other R3 harness).
#[derive(Debug)]
pub struct BillingHarness {
    signup: SignupOrchestrator<
        InMemorySignupAuditSink,
        InMemoryAtomicSignupStore,
        InMemoryBillingClient,
        InMemoryProvisionRecord,
    >,
    signup_audit: InMemorySignupAuditSink,
    signup_store: InMemoryAtomicSignupStore,
    tier: TierSelectionLedger,
    dpa_gate: Arc<InMemoryDpaGate>,
    stripe_fake: Arc<InMemoryStripeClient>,
    tier_audit: InMemoryTierSelectionAuditSink,
    webhook_handler: InMemoryStripeWebhookHandler<
        InMemoryStripeAuditSink,
        InMemoryStripeWebhookLog,
    >,
    webhook_audit: Arc<InMemoryStripeAuditSink>,
    dsr: InMemoryDsrEndpoint<
        InMemoryDsrAuditSink,
        InMemoryDsrRequestStore,
        InMemoryJwtReceiptIssuer,
        InMemoryMfaStepUpVerifier,
    >,
    dsr_audit: Arc<InMemoryDsrAuditSink>,
    tenants: Mutex<Vec<TenantState>>,
    gate_log: Mutex<Vec<HarnessGateEvent>>,
    now_ms: u64,
}

impl BillingHarness {
    /// Construct a fresh harness. Each integration test should call
    /// this rather than sharing a harness across tests.
    #[must_use]
    pub fn setup() -> Self {
        // ---- Signup orchestrator ----
        let signup_audit = InMemorySignupAuditSink::new();
        let signup_store = InMemoryAtomicSignupStore::new();
        let signup = SignupOrchestrator::new(
            signup_audit.clone(),
            signup_store.clone(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );

        // ---- Tier-selection ledger ----
        let dpa_gate = Arc::new(InMemoryDpaGate::new());
        let stripe_fake = Arc::new(InMemoryStripeClient::new());
        let tier_audit = InMemoryTierSelectionAuditSink::new();
        let tier = TierSelectionLedger::new(
            dpa_gate.clone() as Arc<dyn corelink_tier_selection::DpaAcceptanceGate>,
            stripe_fake.clone() as Arc<dyn StripeClient>,
            Arc::new(tier_audit.clone()),
            TEST_DPA_VERSION,
            "https://app.corelink.example/billing/success",
            "https://app.corelink.example/billing/cancel",
        );

        // ---- Billing-stripe webhook handler ----
        let webhook_audit = Arc::new(InMemoryStripeAuditSink::new());
        let webhook_log = Arc::new(InMemoryStripeWebhookLog::new());
        let webhook_handler =
            InMemoryStripeWebhookHandler::new(webhook_audit.clone(), webhook_log);

        // ---- DSR endpoint ----
        let dsr_audit = Arc::new(InMemoryDsrAuditSink::new());
        let dsr_store = Arc::new(InMemoryDsrRequestStore::new());
        let dsr_receipt = Arc::new(InMemoryJwtReceiptIssuer::new(
            "kid-e2e-billing-r3-5",
            b"e2e-billing-r3-5-canonical-test-key",
        ));
        let dsr_mfa = Arc::new(InMemoryMfaStepUpVerifier::new());
        let dsr = InMemoryDsrEndpoint::new(dsr_audit.clone(), dsr_store, dsr_receipt, dsr_mfa);

        Self {
            signup,
            signup_audit,
            signup_store,
            tier,
            dpa_gate,
            stripe_fake,
            tier_audit,
            webhook_handler,
            webhook_audit,
            dsr,
            dsr_audit,
            tenants: Mutex::new(Vec::new()),
            gate_log: Mutex::new(Vec::new()),
            now_ms: FIXED_NOW_MS,
        }
    }

    /// Borrow the signup audit sink (read-only).
    #[must_use]
    pub fn signup_audit(&self) -> &InMemorySignupAuditSink {
        &self.signup_audit
    }

    /// Borrow the signup atomic store (read-only — exposes the
    /// canonical `committed_tenant_count` accessor).
    #[must_use]
    pub fn signup_store(&self) -> &InMemoryAtomicSignupStore {
        &self.signup_store
    }

    /// Borrow the tier-selection audit sink (read-only).
    #[must_use]
    pub fn tier_audit(&self) -> &InMemoryTierSelectionAuditSink {
        &self.tier_audit
    }

    /// Borrow the billing-stripe webhook audit sink (read-only).
    #[must_use]
    pub fn webhook_audit(&self) -> &InMemoryStripeAuditSink {
        &self.webhook_audit
    }

    /// Borrow the DSR audit sink (read-only).
    #[must_use]
    pub fn dsr_audit(&self) -> &InMemoryDsrAuditSink {
        &self.dsr_audit
    }

    /// Borrow the in-memory Stripe Checkout fake.
    #[must_use]
    pub fn stripe_fake(&self) -> &InMemoryStripeClient {
        &self.stripe_fake
    }

    /// Canonical `now_ms` clock pinned at construction.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// Snapshot the harness-local gate log (cross-system decisions).
    #[must_use]
    pub fn gate_log_snapshot(&self) -> Vec<HarnessGateEvent> {
        match self.gate_log.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Lifecycle state of `tenant_name`. Returns
    /// `SubscriptionLifecycleState::PreCheckout` if the tenant has
    /// not yet been signed up.
    #[must_use]
    pub fn lifecycle_state(&self, tenant_name: &str) -> SubscriptionLifecycleState {
        let g = match self.tenants.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.iter()
            .find(|t| t.name == tenant_name)
            .map_or(SubscriptionLifecycleState::PreCheckout, |t| t.lifecycle)
    }

    fn audit_emit_gate(&self, event: HarnessGateEvent) {
        match self.gate_log.lock() {
            Ok(mut g) => g.push(event),
            Err(p) => p.into_inner().push(event),
        }
    }

    fn with_tenants_mut<F, R>(&self, f: F) -> Result<R, BillingHarnessError>
    where
        F: FnOnce(&mut Vec<TenantState>) -> R,
    {
        match self.tenants.lock() {
            Ok(mut g) => Ok(f(&mut g)),
            Err(_) => Err(BillingHarnessError::Invariant(
                "tenant state mutex poisoned".to_string(),
            )),
        }
    }

    /// Step 1 — signup + DPA-accept the tenant. Returns the
    /// canonical signup response so callers can pin
    /// `SignupOutcome::Provisioned` assertions.
    ///
    /// # Errors
    ///
    /// Returns [`BillingHarnessError::SignupRejected`] if the
    /// orchestrator returned a non-`Provisioned` outcome, or
    /// [`BillingHarnessError::Signup`] on internal failure.
    pub fn signup_and_accept_dpa(&self, tenant: &BillingTenant) -> BillingError<SignupResponse> {
        let resp = self
            .signup
            .provision(&tenant.signup_request)
            .map_err(|e| BillingHarnessError::Signup(e.to_string()))?;
        let signup_tenant_id = match resp.outcome.clone() {
            SignupOutcome::Provisioned { tenant_id, .. } => tenant_id,
            other => return Err(BillingHarnessError::SignupRejected(Box::new(other))),
        };
        let tier_tenant_id = TierTenantId::new(signup_tenant_id.as_str());

        // Flip the DPA gate to accepted — production wiring runs the
        // full DPA service from `e2e-signup-flow`; this harness skips
        // straight to the gate accept since the DPA receipt issuance
        // is already exercised in R3-1.
        self.dpa_gate
            .accept(tier_tenant_id.clone(), TEST_DPA_VERSION);

        self.with_tenants_mut(|tenants| {
            tenants.push(TenantState {
                name: tenant.name.clone(),
                signup_tenant_id,
                tier_tenant_id,
                lifecycle: SubscriptionLifecycleState::PreCheckout,
                stripe_customer_id: None,
                pending_session_id: None,
            });
        })?;

        Ok(resp)
    }

    /// Step 2 — select the Starter tier (canonical Stripe Checkout
    /// path). Returns the canonical receipt arm.
    ///
    /// # Errors
    ///
    /// Returns [`BillingHarnessError::Tier`] on a ledger rejection
    /// (e.g. DPA-not-accepted / lock-held).
    pub fn select_starter_tier(
        &self,
        tenant_name: &str,
    ) -> BillingError<TierSelectionReceipt> {
        let (ctx, customer_email) = self.with_tenants_mut(|tenants| {
            let st = tenants
                .iter()
                .find(|t| t.name == tenant_name)
                .ok_or_else(|| {
                    BillingHarnessError::Invariant(format!(
                        "tenant {tenant_name} not signed up yet"
                    ))
                })?;
            let ctx = TierTenantCtx::new(
                st.tier_tenant_id.clone(),
                FIXED_NOW_MS,
                format!("corr-billing-{tenant_name}"),
            );
            let email = format!("{tenant_name}@e2e-billing-flow.example");
            Ok::<_, BillingHarnessError>((ctx, email))
        })??;

        let receipt = self.tier.select_tier(&ctx, TierKind::Starter, &customer_email)?;
        if let TierSelectionReceipt::CheckoutRedirect {
            session_id,
            tenant_id,
            ..
        } = &receipt
        {
            let session_id = session_id.clone();
            let tenant_id = tenant_id.clone();
            self.with_tenants_mut(|tenants| {
                if let Some(st) = tenants.iter_mut().find(|t| t.tier_tenant_id == tenant_id) {
                    st.lifecycle = SubscriptionLifecycleState::PendingActivation;
                    st.pending_session_id = Some(session_id);
                }
            })?;
        }
        Ok(receipt)
    }

    /// Step 3 — drive the canonical `checkout.session.completed`
    /// event into the tier ledger (mirrors the production webhook
    /// route AFTER signature verification). Returns the canonical
    /// activation receipt.
    ///
    /// # Errors
    ///
    /// Returns [`BillingHarnessError::Tier`] on a ledger rejection
    /// (e.g. `AlreadyActive`); [`BillingHarnessError::Invariant`]
    /// when the tenant has no pending session.
    pub fn deliver_checkout_completed(
        &self,
        tenant_name: &str,
        event_id: impl Into<String>,
    ) -> BillingError<SubscriptionActivationReceipt> {
        let (event, tier_tenant_id) = self.with_tenants_mut(|tenants| {
            let st = tenants
                .iter()
                .find(|t| t.name == tenant_name)
                .ok_or_else(|| {
                    BillingHarnessError::Invariant(format!(
                        "tenant {tenant_name} not signed up yet"
                    ))
                })?;
            let session_id = st.pending_session_id.clone().ok_or_else(|| {
                BillingHarnessError::Invariant(format!(
                    "tenant {tenant_name} has no pending checkout session"
                ))
            })?;
            let stripe_customer_id =
                StripeCustomerId::new(format!("cus_fake_{}", st.tier_tenant_id.as_str()));
            let event = StripeCheckoutSessionCompletedEvent::new(
                event_id.into(),
                session_id,
                st.tier_tenant_id.clone(),
                TierKind::Starter,
                stripe_customer_id.clone(),
                FIXED_NOW_MS,
            );
            Ok::<_, BillingHarnessError>((event, st.tier_tenant_id.clone()))
        })??;

        let receipt = self.tier.on_checkout_completed(&event)?;

        if matches!(receipt, SubscriptionActivationReceipt::Activated { .. }) {
            self.with_tenants_mut(|tenants| {
                if let Some(st) = tenants
                    .iter_mut()
                    .find(|t| t.tier_tenant_id == tier_tenant_id)
                {
                    st.lifecycle = SubscriptionLifecycleState::Active;
                    st.stripe_customer_id = Some(StripeCustomerId::new(format!(
                        "cus_fake_{}",
                        st.tier_tenant_id.as_str()
                    )));
                    st.pending_session_id = None;
                }
            })?;
        }

        Ok(receipt)
    }

    /// Replay an already-delivered webhook. Mirrors the production
    /// "Stripe retried because we 5xx'd the first delivery" path —
    /// the ledger MUST dedup on the event id and return
    /// [`SubscriptionActivationReceipt::DuplicateIgnored`].
    ///
    /// # Errors
    ///
    /// Returns [`BillingHarnessError::Tier`] on an unexpected ledger
    /// failure; [`BillingHarnessError::Invariant`] when the tenant has
    /// never seen a successful checkout.
    pub fn replay_checkout_completed(
        &self,
        tenant_name: &str,
        event_id: impl Into<String>,
    ) -> BillingError<SubscriptionActivationReceipt> {
        let event_id = event_id.into();
        let event = self.with_tenants_mut(|tenants| {
            let st = tenants
                .iter()
                .find(|t| t.name == tenant_name)
                .ok_or_else(|| {
                    BillingHarnessError::Invariant(format!(
                        "tenant {tenant_name} not signed up yet"
                    ))
                })?;
            let stripe_customer_id = st.stripe_customer_id.clone().ok_or_else(|| {
                BillingHarnessError::Invariant(format!(
                    "tenant {tenant_name} not yet activated; cannot replay"
                ))
            })?;
            // The session id is irrelevant on replay because the
            // ledger short-circuits on `event_id`; we pass a stable
            // placeholder so the typed shape is well-formed.
            Ok::<_, BillingHarnessError>(StripeCheckoutSessionCompletedEvent::new(
                event_id.clone(),
                format!("cs_replay_{}", st.tier_tenant_id.as_str()),
                st.tier_tenant_id.clone(),
                TierKind::Starter,
                stripe_customer_id,
                FIXED_NOW_MS,
            ))
        })??;
        Ok(self.tier.on_checkout_completed(&event)?)
    }

    /// Customer-initiated cancel. Mirrors the production "user
    /// pressed Cancel in the billing portal" path — the
    /// subscription remains active until `period_end_ms` per Stripe
    /// spec, then flips to `Refunded` once the refund webhook lands.
    ///
    /// # Errors
    ///
    /// Returns [`BillingHarnessError::Invariant`] if the tenant is
    /// not currently `Active`.
    pub fn cancel_subscription(
        &self,
        tenant_name: &str,
        period_end_ms: u64,
    ) -> BillingError<()> {
        self.with_tenants_mut(|tenants| {
            let st = tenants
                .iter_mut()
                .find(|t| t.name == tenant_name)
                .ok_or_else(|| {
                    BillingHarnessError::Invariant(format!(
                        "tenant {tenant_name} not signed up yet"
                    ))
                })?;
            if !matches!(st.lifecycle, SubscriptionLifecycleState::Active) {
                return Err(BillingHarnessError::Invariant(format!(
                    "tenant {tenant_name} not active; current state = {:?}",
                    st.lifecycle
                )));
            }
            st.lifecycle =
                SubscriptionLifecycleState::CancelScheduledAtPeriodEnd { period_end_ms };
            Ok(())
        })??;
        self.audit_emit_gate(HarnessGateEvent::CancelScheduledAtPeriodEnd {
            tenant: tenant_name.to_string(),
            period_end_ms,
        });
        Ok(())
    }

    /// Whether the customer still has access at `at_ms`. Returns
    /// `true` while the subscription is `Active` OR
    /// `CancelScheduledAtPeriodEnd` and `at_ms < period_end_ms`.
    #[must_use]
    pub fn has_access_at(&self, tenant_name: &str, at_ms: u64) -> bool {
        match self.lifecycle_state(tenant_name) {
            SubscriptionLifecycleState::Active => true,
            SubscriptionLifecycleState::CancelScheduledAtPeriodEnd { period_end_ms } => {
                at_ms < period_end_ms
            }
            SubscriptionLifecycleState::PreCheckout
            | SubscriptionLifecycleState::PendingActivation
            | SubscriptionLifecycleState::Refunded => false,
        }
    }

    /// Deliver an arbitrary Stripe webhook through the canonical
    /// signature-verify path. The harness builds the
    /// [`WebhookHandleRequest`] and dispatches into the composed
    /// `corelink-billing-stripe` handler — production wiring uses the
    /// same trait surface.
    ///
    /// # Errors
    ///
    /// Returns [`BillingHarnessError::Stripe`] on any webhook handler
    /// error (signature reject / skew reject / log insert failure).
    pub fn deliver_signed_webhook(
        &self,
        signed: &SignedWebhook,
    ) -> BillingError<StripeAdapterDecision> {
        let req = WebhookHandleRequest {
            signature_header: &signed.header,
            payload: &signed.payload,
            webhook_secret: TEST_WEBHOOK_SECRET,
            now_ms: self.now_ms,
            event: signed.to_event(),
        };
        let decision = self.webhook_handler.handle(req)?;
        Ok(decision)
    }

    /// Process a refund webhook. Marks the subscription `Refunded`
    /// and emits the canonical harness-local
    /// [`HarnessGateEvent::RefundProcessed`] gate event.
    ///
    /// # Errors
    ///
    /// Returns [`BillingHarnessError::Invariant`] if the tenant is
    /// not in a cancel-pending state; [`BillingHarnessError::Stripe`]
    /// when the webhook handler rejects (signature / log error).
    pub fn process_refund(
        &self,
        tenant_name: &str,
        signed: &SignedWebhook,
    ) -> BillingError<StripeAdapterDecision> {
        // Lifecycle gate: only a cancelled subscription can be
        // refunded at the harness boundary. The production
        // billing-stripe handler does NOT enforce this (it only
        // dispatches the typed webhook event); the gate lives at the
        // billing orchestration layer which this harness models.
        self.with_tenants_mut(|tenants| {
            let st = tenants
                .iter_mut()
                .find(|t| t.name == tenant_name)
                .ok_or_else(|| {
                    BillingHarnessError::Invariant(format!(
                        "tenant {tenant_name} not signed up yet"
                    ))
                })?;
            if !matches!(
                st.lifecycle,
                SubscriptionLifecycleState::CancelScheduledAtPeriodEnd { .. }
            ) {
                return Err(BillingHarnessError::Invariant(format!(
                    "tenant {tenant_name} cannot be refunded; current state = {:?}",
                    st.lifecycle
                )));
            }
            st.lifecycle = SubscriptionLifecycleState::Refunded;
            Ok(())
        })??;

        let decision = self.deliver_signed_webhook(signed)?;
        self.audit_emit_gate(HarnessGateEvent::RefundProcessed {
            tenant: tenant_name.to_string(),
            now_ms: self.now_ms,
        });
        Ok(decision)
    }

    /// Submit a DSR Erasure request. The harness layers the
    /// "must-cancel-first" gate on top of the canonical DSR endpoint
    /// — if the tenant's subscription is still active (or
    /// cancel-pending) the request is blocked at the harness
    /// boundary BEFORE reaching the DSR endpoint, and the canonical
    /// [`HarnessGateEvent::DsrErasureBlockedSubscriptionActive`]
    /// audit event fires. Only `Refunded` (or `PreCheckout` —
    /// covering the never-subscribed arm) lifecycle states pass the
    /// gate.
    ///
    /// `mfa_token_tag` carries the canonical synthetic MFA tag —
    /// passing `Some("ok")` mints a non-empty token (the in-memory
    /// MFA verifier accepts any non-empty token); passing `None`
    /// triggers the canonical `MfaRequired` decision arm.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`BillingHarnessError::DsrBlockedSubscriptionActive`] when the
    /// gate fails closed; [`BillingHarnessError::Dsr`] on any endpoint
    /// error.
    pub fn request_dsr_erasure(
        &self,
        tenant_name: &str,
        request_tag: &str,
        mfa_token_tag: Option<&str>,
    ) -> BillingError<DsrDecision> {
        // Lifecycle gate first — fail-CLOSED BEFORE we reach the DSR
        // endpoint per the cross-system contract "user must cancel
        // first" + "refund must settle".
        let state = self.lifecycle_state(tenant_name);
        let pass = matches!(
            state,
            SubscriptionLifecycleState::PreCheckout
                | SubscriptionLifecycleState::Refunded
        );
        if !pass {
            // Audit-emit the gate BEFORE returning.
            self.audit_emit_gate(HarnessGateEvent::DsrErasureBlockedSubscriptionActive {
                tenant: tenant_name.to_string(),
                state,
                now_ms: self.now_ms,
            });
            return Err(BillingHarnessError::DsrBlockedSubscriptionActive { state });
        }

        // Build the canonical DSR request.
        let tenant_uuid = derive_tenant_uuid(tenant_name);
        let subject_uuid = derive_data_subject_id(tenant_name);
        let request_uuid = derive_request_id(tenant_name, request_tag);
        let mut req = DsrRequest::new(
            request_uuid,
            tenant_uuid,
            subject_uuid,
            DsrRequestKind::Erasure,
            DsrJurisdiction::Gdpr,
            self.now_ms,
        );
        if let Some(tag) = mfa_token_tag {
            req = req.with_mfa(MfaStepUpToken::synthetic_for_test(tag));
        }
        let decision = self.dsr.submit(&req)?;
        Ok(decision)
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
    fn make_test_tenant_is_deterministic() {
        let a = make_test_tenant("foo");
        let b = make_test_tenant("foo");
        assert_eq!(a, b);
    }

    #[test]
    fn derive_uuid_is_deterministic() {
        let u1 = derive_tenant_uuid("foo");
        let u2 = derive_tenant_uuid("foo");
        assert_eq!(u1, u2);
        // Different salt → different UUID.
        assert_ne!(derive_tenant_uuid("foo"), derive_data_subject_id("foo"));
    }

    #[test]
    fn audit_chain_contains_predicate_works() {
        let events = vec![HarnessGateEvent::RefundProcessed {
            tenant: "x".to_string(),
            now_ms: 0,
        }];
        assert!(audit_chain_contains(&events, |e| matches!(
            e,
            HarnessGateEvent::RefundProcessed { .. }
        )));
        assert!(!audit_chain_contains(&events, |e| matches!(
            e,
            HarnessGateEvent::CancelScheduledAtPeriodEnd { .. }
        )));
    }

    #[test]
    fn build_signed_webhook_roundtrips_through_handler() {
        let payload = br#"{"id":"evt_test"}"#;
        let signed = build_signed_webhook(
            payload,
            "evt_test",
            WebhookEventKind::InvoicePaid,
            FIXED_NOW_SECONDS,
        )
        .unwrap();
        let harness = BillingHarness::setup();
        let decision = harness.deliver_signed_webhook(&signed).unwrap();
        assert!(matches!(
            decision,
            StripeAdapterDecision::WebhookProcessed { .. }
        ));
    }

    #[test]
    fn lifecycle_state_starts_at_pre_checkout() {
        let h = BillingHarness::setup();
        assert!(matches!(
            h.lifecycle_state("never-touched"),
            SubscriptionLifecycleState::PreCheckout
        ));
    }
}
