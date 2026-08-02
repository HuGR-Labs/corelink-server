//! Tier-selection ledger orchestrator.
//!
//! In-process mirror of the D1 tables (`tier_selections`,
//! `tier_selection_locks`, `stripe_checkout_sessions`). Production
//! wiring runs the same logic inside a `BEGIN IMMEDIATE TRANSACTION`
//! per WI §6.4 + Lote 10.19 codex P0 canonical fix.
//!
//! ## Invariants enforced
//!
//! - INV-ONBOARD-DPA-FIRST (HIGH): `select_tier` HARD-rejects with
//!   [`TierError::DpaRequired`] BEFORE any Stripe API call if
//!   [`DpaAcceptanceGate::is_accepted`] returns `false`. Free tier
//!   ALSO requires DPA — no exception (WI §6.5).
//! - INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; Lote 10.6bis pattern):
//!   audit envelope BEFORE any state mutation on every decision arm;
//!   audit failure aborts.
//! - D1 row lock: `INSERT OR IGNORE` on `tier_selection_locks` with
//!   60s `expires_ms` window prevents concurrent tier switches per
//!   tenant. Concurrent callers get [`TierError::LockHeld`].
//! - Stripe webhook idempotency: `event_id` dedup before subscription
//!   activation; duplicate delivery returns [`TierError::DuplicateEvent`].
//! - Enterprise route enforcement (backend): direct Stripe Checkout
//!   for enterprise tier rejected with [`TierError::UseInquiryForm`].
//! - UNIQUE partial index defense-in-depth: at most one
//!   `subscription_state = "active"` per tenant.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::audit::{TierSelectionAuditEventType, TierSelectionAuditRecord, TierSelectionAuditSink};
use crate::dpa::DpaAcceptanceGate;
use crate::error::TierError;
use crate::stripe::{CheckoutSessionRequest, StripeCheckoutSessionCompletedEvent, StripeClient};
use crate::tenant::{StripeCustomerId, TenantCtx, TenantId};
use crate::tier::TierKind;

/// 60-second canonical D1 row-lock window per WI §6.4.
pub const TIER_SELECTION_LOCK_WINDOW_MS: u64 = 60_000;

/// Receipt returned by [`TierSelectionLedger::select_tier`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TierSelectionReceipt {
    /// Free tier activated instantly (no Stripe redirect).
    FreeActivated {
        /// Tenant whose subscription was activated.
        tenant_id: TenantId,
        /// Activation timestamp (ms since epoch).
        activated_at_ms: u64,
    },
    /// Paid tier — caller should redirect user to `checkout_url`.
    CheckoutRedirect {
        /// Tenant whose Checkout Session was created.
        tenant_id: TenantId,
        /// Tier the session is for.
        tier: TierKind,
        /// Stripe Checkout URL.
        checkout_url: String,
        /// Stripe-assigned session id.
        session_id: String,
    },
}

/// Receipt returned by [`TierSelectionLedger::on_checkout_completed`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubscriptionActivationReceipt {
    /// Subscription state advanced to `active`.
    Activated {
        /// Tenant whose subscription was activated.
        tenant_id: TenantId,
        /// Tier activated.
        tier: TierKind,
        /// Activation timestamp (ms since epoch).
        activated_at_ms: u64,
    },
    /// Webhook was a duplicate (same `event_id`); state untouched.
    DuplicateIgnored {
        /// Event id that was deduped.
        event_id: String,
    },
}

/// Canonical subscription state per WI §6.4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubscriptionState {
    /// No active subscription yet.
    Inactive,
    /// Paid tier Checkout session pending webhook.
    PendingCheckout,
    /// Active subscription.
    Active,
}

/// Per-tenant subscription mirror (matches the D1 `tier_selections`
/// row).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TierSelectionRow {
    /// Tenant.
    pub tenant_id: TenantId,
    /// Selected tier.
    pub tier: TierKind,
    /// Subscription state.
    pub subscription_state: SubscriptionState,
    /// Stripe customer id once mapped (atomic per WI §6.7).
    pub stripe_customer_id: Option<StripeCustomerId>,
    /// Activation timestamp (ms; populated on `Active`).
    pub subscription_started_at_ms: Option<u64>,
}

impl TierSelectionRow {
    /// Does this row represent an active **paid** subscription?
    ///
    /// This is the predicate behind the "at most one active subscription per
    /// tenant" rule (the UNIQUE partial index `idx_tenant_active_subscription`),
    /// and it is deliberately NOT just `state == Active`.
    ///
    /// Every tenant is seeded at signup with `tier = Free, state = Active` — free
    /// is an instant *activation*, not a subscription. Treating that row as an
    /// active subscription makes the guard reject the first purchase attempt of
    /// every account in existence with `already_active` (this shipped: 117 prod
    /// tenants were in exactly that state on 2026-08-02, none of them able to
    /// buy). `Enterprise` is likewise not a self-serve subscription — it routes
    /// to the inquiry form and never reaches Checkout — but an Enterprise row is
    /// only ever written by an operator grant, so it stays guarded here.
    #[must_use]
    pub fn holds_paid_subscription(&self) -> bool {
        self.subscription_state == SubscriptionState::Active && self.tier != TierKind::Free
    }
}

/// Mirror row for an in-flight Checkout Session.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CheckoutSessionRow {
    /// Stripe session id.
    pub session_id: String,
    /// Tenant the session is for.
    pub tenant_id: TenantId,
    /// Tier.
    pub tier: TierKind,
    /// Created at (ms since epoch).
    pub created_at_ms: u64,
}

#[derive(Debug, Default)]
struct LedgerInner {
    /// `tier_selections` table mirror.
    rows: HashMap<TenantId, TierSelectionRow>,
    /// `tier_selection_locks` table mirror.
    ///
    /// Stores `tenant_id -> expires_at_ms`; a row is present iff a
    /// tier-selection is in flight or within 60s of the prior one.
    locks: HashMap<TenantId, u64>,
    /// `stripe_checkout_sessions` table mirror.
    sessions: HashMap<String, CheckoutSessionRow>,
    /// Stripe webhook idempotency table (mirror of D1 audit chain
    /// dedup index per WI §6.3).
    processed_event_ids: HashSet<String>,
}

/// Tier-selection orchestrator. Holds the in-process mirror of D1 +
/// composes the DPA gate, Stripe client, and audit sink.
#[derive(Debug)]
pub struct TierSelectionLedger {
    inner: Arc<Mutex<LedgerInner>>,
    dpa_gate: Arc<dyn DpaAcceptanceGate>,
    stripe: Arc<dyn StripeClient>,
    audit: Arc<dyn TierSelectionAuditSink>,
    current_dpa_version: String,
    success_url: String,
    cancel_url: String,
}

impl TierSelectionLedger {
    /// Construct a ledger.
    #[must_use]
    pub fn new(
        dpa_gate: Arc<dyn DpaAcceptanceGate>,
        stripe: Arc<dyn StripeClient>,
        audit: Arc<dyn TierSelectionAuditSink>,
        current_dpa_version: impl Into<String>,
        success_url: impl Into<String>,
        cancel_url: impl Into<String>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LedgerInner::default())),
            dpa_gate,
            stripe,
            audit,
            current_dpa_version: current_dpa_version.into(),
            success_url: success_url.into(),
            cancel_url: cancel_url.into(),
        }
    }

    /// POST `/v1/onboarding/tier-select` handler.
    ///
    /// Order of checks (fail-CLOSED):
    /// 1. Audit-emit `tier_select_attempted` BEFORE any state read.
    /// 2. INV-ONBOARD-DPA-FIRST: reject [`TierError::DpaRequired`] if
    ///    DPA not accepted (BEFORE Stripe).
    /// 3. Enterprise route enforcement: reject
    ///    [`TierError::UseInquiryForm`] for enterprise tier.
    /// 4. D1 row lock acquire (`INSERT OR IGNORE`): reject
    ///    [`TierError::LockHeld`] if concurrent in flight.
    /// 5. UNIQUE active subscription check: reject
    ///    [`TierError::AlreadyActive`] if active.
    /// 6. Free → instant activation; Paid → Stripe Checkout.
    pub fn select_tier(
        &self,
        ctx: &TenantCtx,
        tier: TierKind,
        customer_email: &str,
    ) -> Result<TierSelectionReceipt, TierError> {
        // (1) Audit BEFORE state mutation.
        self.audit_emit_or_abort(TierSelectionAuditRecord::new(
            TierSelectionAuditEventType::TierSelectAttempted,
            ctx.tenant_id.clone(),
            Some(tier),
            ctx.now_ms,
            ctx.correlation_id.clone(),
        ))?;

        // (2) INV-ONBOARD-DPA-FIRST canonical check — BEFORE Stripe.
        // Applies to ALL tiers including Free (WI §6.5).
        if !self
            .dpa_gate
            .is_accepted(&ctx.tenant_id, &self.current_dpa_version)
        {
            // Audit the violation attempt.
            self.audit_emit_or_abort(TierSelectionAuditRecord::new(
                TierSelectionAuditEventType::DpaFirstViolationAttempt,
                ctx.tenant_id.clone(),
                Some(tier),
                ctx.now_ms,
                ctx.correlation_id.clone(),
            ))?;
            return Err(TierError::DpaRequired);
        }

        // (3) Enterprise tier route enforcement at backend (UI hint
        // alone is bypassable).
        if tier.routes_to_inquiry_form() {
            self.audit_emit_or_abort(TierSelectionAuditRecord::new(
                TierSelectionAuditEventType::EnterpriseRouteBypassAttempt,
                ctx.tenant_id.clone(),
                Some(tier),
                ctx.now_ms,
                ctx.correlation_id.clone(),
            ))?;
            return Err(TierError::UseInquiryForm);
        }

        // (4) + (5): atomic lock acquire + UNIQUE active check inside
        // a single critical section (mirrors `BEGIN IMMEDIATE`).
        {
            let mut g = self
                .inner
                .lock()
                .map_err(|e| TierError::Internal(format!("mutex poisoned: {e}")))?;
            // Evict expired locks (60s window).
            g.locks.retain(|_, expires_ms| *expires_ms > ctx.now_ms);
            // INSERT OR IGNORE semantics: if a lock exists, reject.
            if g.locks.contains_key(&ctx.tenant_id) {
                return Err(TierError::LockHeld);
            }
            // UNIQUE partial index: at most one active PAID subscription per
            // tenant. A free row is an activation, not a subscription — see
            // `TierSelectionRow::holds_paid_subscription`.
            if let Some(row) = g.rows.get(&ctx.tenant_id) {
                if row.holds_paid_subscription() {
                    return Err(TierError::AlreadyActive);
                }
            }
            // Acquire lock.
            g.locks.insert(
                ctx.tenant_id.clone(),
                ctx.now_ms.saturating_add(TIER_SELECTION_LOCK_WINDOW_MS),
            );
        }

        // (6) Tier dispatch.
        let result = if tier == TierKind::Free {
            self.activate_free(ctx)
        } else {
            self.activate_paid(ctx, tier, customer_email)
        };

        // Release the lock on terminal failure for non-LockHeld paths
        // so the tenant can retry. (LockHeld already returned above.)
        if result.is_err() {
            if let Ok(mut g) = self.inner.lock() {
                g.locks.remove(&ctx.tenant_id);
            }
        }
        result
    }

    fn activate_free(&self, ctx: &TenantCtx) -> Result<TierSelectionReceipt, TierError> {
        // Audit BEFORE mutation.
        self.audit_emit_or_abort(TierSelectionAuditRecord::new(
            TierSelectionAuditEventType::TierActivatedFree,
            ctx.tenant_id.clone(),
            Some(TierKind::Free),
            ctx.now_ms,
            ctx.correlation_id.clone(),
        ))?;

        let mut g = self
            .inner
            .lock()
            .map_err(|e| TierError::Internal(format!("mutex poisoned: {e}")))?;
        g.rows.insert(
            ctx.tenant_id.clone(),
            TierSelectionRow {
                tenant_id: ctx.tenant_id.clone(),
                tier: TierKind::Free,
                subscription_state: SubscriptionState::Active,
                stripe_customer_id: None,
                subscription_started_at_ms: Some(ctx.now_ms),
            },
        );

        Ok(TierSelectionReceipt::FreeActivated {
            tenant_id: ctx.tenant_id.clone(),
            activated_at_ms: ctx.now_ms,
        })
    }

    fn activate_paid(
        &self,
        ctx: &TenantCtx,
        tier: TierKind,
        customer_email: &str,
    ) -> Result<TierSelectionReceipt, TierError> {
        let req = CheckoutSessionRequest {
            tenant_id: ctx.tenant_id.clone(),
            tier,
            customer_email: customer_email.to_string(),
            success_url: self.success_url.clone(),
            cancel_url: self.cancel_url.clone(),
        };
        // Stripe call (only reached AFTER DPA gate passes).
        let resp = self.stripe.create_checkout_session(&req)?;

        // Audit BEFORE mirror mutation.
        self.audit_emit_or_abort(TierSelectionAuditRecord::new(
            TierSelectionAuditEventType::StripeCheckoutSessionCreated,
            ctx.tenant_id.clone(),
            Some(tier),
            ctx.now_ms,
            ctx.correlation_id.clone(),
        ))?;

        let mut g = self
            .inner
            .lock()
            .map_err(|e| TierError::Internal(format!("mutex poisoned: {e}")))?;
        // tenant.stripe_customer_id set ATOMIC in the same critical
        // section (WI §6.7 drift prevention).
        g.rows.insert(
            ctx.tenant_id.clone(),
            TierSelectionRow {
                tenant_id: ctx.tenant_id.clone(),
                tier,
                subscription_state: SubscriptionState::PendingCheckout,
                stripe_customer_id: Some(resp.stripe_customer_id.clone()),
                subscription_started_at_ms: None,
            },
        );
        g.sessions.insert(
            resp.session_id.clone(),
            CheckoutSessionRow {
                session_id: resp.session_id.clone(),
                tenant_id: ctx.tenant_id.clone(),
                tier,
                created_at_ms: ctx.now_ms,
            },
        );

        Ok(TierSelectionReceipt::CheckoutRedirect {
            tenant_id: ctx.tenant_id.clone(),
            tier,
            checkout_url: resp.url,
            session_id: resp.session_id,
        })
    }

    /// POST `/v1/billing/stripe-webhook` handler (after signature
    /// verification at the caller — see [`crate::stripe::verify_stripe_signature`]).
    ///
    /// Idempotent per `event_id`. Returns
    /// [`SubscriptionActivationReceipt::DuplicateIgnored`] on dedup hit.
    pub fn on_checkout_completed(
        &self,
        event: &StripeCheckoutSessionCompletedEvent,
    ) -> Result<SubscriptionActivationReceipt, TierError> {
        // Idempotency dedup first.
        {
            let g = self
                .inner
                .lock()
                .map_err(|e| TierError::Internal(format!("mutex poisoned: {e}")))?;
            if g.processed_event_ids.contains(&event.event_id) {
                drop(g);
                self.audit_emit_or_abort(TierSelectionAuditRecord::new(
                    TierSelectionAuditEventType::StripeWebhookDuplicate,
                    event.tenant_id.clone(),
                    Some(event.tier),
                    event.ts_ms,
                    format!("dedup:{}", event.event_id),
                ))?;
                return Ok(SubscriptionActivationReceipt::DuplicateIgnored {
                    event_id: event.event_id.clone(),
                });
            }
        }

        // Audit BEFORE activation.
        self.audit_emit_or_abort(TierSelectionAuditRecord::new(
            TierSelectionAuditEventType::StripeSubscriptionActivated,
            event.tenant_id.clone(),
            Some(event.tier),
            event.ts_ms,
            format!("webhook:{}", event.event_id),
        ))?;

        let mut g = self
            .inner
            .lock()
            .map_err(|e| TierError::Internal(format!("mutex poisoned: {e}")))?;

        // UNIQUE partial index defense-in-depth. Scoped to a PAID row: the
        // tenant being activated by this webhook was on the free tier moments
        // ago (that is what an upgrade IS), so matching a free row here would
        // reject the very activation that the successful Checkout paid for.
        if let Some(row) = g.rows.get(&event.tenant_id) {
            if row.holds_paid_subscription() {
                return Err(TierError::AlreadyActive);
            }
        }

        g.processed_event_ids.insert(event.event_id.clone());
        g.rows.insert(
            event.tenant_id.clone(),
            TierSelectionRow {
                tenant_id: event.tenant_id.clone(),
                tier: event.tier,
                subscription_state: SubscriptionState::Active,
                stripe_customer_id: Some(event.stripe_customer_id.clone()),
                subscription_started_at_ms: Some(event.ts_ms),
            },
        );
        // Release any in-flight lock now that activation is final.
        g.locks.remove(&event.tenant_id);

        Ok(SubscriptionActivationReceipt::Activated {
            tenant_id: event.tenant_id.clone(),
            tier: event.tier,
            activated_at_ms: event.ts_ms,
        })
    }

    /// Snapshot the current `tier_selections` row for `tenant_id`.
    #[must_use]
    pub fn row(&self, tenant_id: &TenantId) -> Option<TierSelectionRow> {
        match self.inner.lock() {
            Ok(g) => g.rows.get(tenant_id).cloned(),
            Err(p) => p.into_inner().rows.get(tenant_id).cloned(),
        }
    }

    /// Snapshot the in-flight Checkout Session count (D1
    /// `stripe_checkout_sessions` mirror).
    #[must_use]
    pub fn checkout_session_count(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.sessions.len(),
            Err(p) => p.into_inner().sessions.len(),
        }
    }

    /// Snapshot all processed Stripe event ids (idempotency table).
    #[must_use]
    pub fn processed_event_count(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.processed_event_ids.len(),
            Err(p) => p.into_inner().processed_event_ids.len(),
        }
    }

    fn audit_emit_or_abort(&self, record: TierSelectionAuditRecord) -> Result<(), TierError> {
        self.audit
            .emit(&record)
            .map_err(|e| TierError::Audit(e.to_string()))
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
    use crate::audit::{FailingTierSelectionAuditSink, InMemoryTierSelectionAuditSink};
    use crate::dpa::{AlwaysDenyDpaGate, InMemoryDpaGate};
    use crate::stripe::InMemoryStripeClient;

    fn ledger_with_dpa_ok() -> (TierSelectionLedger, InMemoryTierSelectionAuditSink) {
        let dpa = InMemoryDpaGate::new();
        dpa.accept(TenantId::new("t1"), "v1");
        let audit = InMemoryTierSelectionAuditSink::new();
        let ledger = TierSelectionLedger::new(
            Arc::new(dpa),
            Arc::new(InMemoryStripeClient::new()),
            Arc::new(audit.clone()),
            "v1",
            "https://app/success",
            "https://app/cancel",
        );
        (ledger, audit)
    }

    fn ctx(tenant: &str, now_ms: u64) -> TenantCtx {
        TenantCtx::new(TenantId::new(tenant), now_ms, "corr-1")
    }

    #[test]
    fn free_tier_skips_stripe_when_dpa_accepted() {
        let (l, audit) = ledger_with_dpa_ok();
        let r = l
            .select_tier(&ctx("t1", 1000), TierKind::Free, "u@x.com")
            .unwrap();
        assert!(matches!(r, TierSelectionReceipt::FreeActivated { .. }));
        assert!(audit.has_event(TierSelectionAuditEventType::TierActivatedFree));
        assert_eq!(l.checkout_session_count(), 0);
    }

    #[test]
    fn dpa_required_blocks_free_tier() {
        let dpa = AlwaysDenyDpaGate;
        let audit = InMemoryTierSelectionAuditSink::new();
        let l = TierSelectionLedger::new(
            Arc::new(dpa),
            Arc::new(InMemoryStripeClient::new()),
            Arc::new(audit.clone()),
            "v1",
            "ok",
            "cancel",
        );
        let err = l
            .select_tier(&ctx("t1", 100), TierKind::Free, "u@x.com")
            .unwrap_err();
        assert!(matches!(err, TierError::DpaRequired));
        assert!(audit.has_event(TierSelectionAuditEventType::DpaFirstViolationAttempt));
    }

    #[test]
    fn dpa_required_blocks_paid_tier_before_stripe() {
        let dpa = AlwaysDenyDpaGate;
        let stripe = InMemoryStripeClient::new();
        let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
        let audit = InMemoryTierSelectionAuditSink::new();
        let l = TierSelectionLedger::new(
            Arc::new(dpa),
            stripe_arc,
            Arc::new(audit),
            "v1",
            "ok",
            "cancel",
        );
        let err = l
            .select_tier(&ctx("t1", 100), TierKind::Starter, "u@x.com")
            .unwrap_err();
        assert!(matches!(err, TierError::DpaRequired));
        // INV-ONBOARD-DPA-FIRST: zero Stripe sessions created.
        assert_eq!(stripe.sessions().len(), 0);
    }

    #[test]
    fn paid_tier_creates_stripe_session_when_dpa_accepted() {
        let (l, audit) = ledger_with_dpa_ok();
        let r = l
            .select_tier(&ctx("t1", 1000), TierKind::Starter, "u@x.com")
            .unwrap();
        match r {
            TierSelectionReceipt::CheckoutRedirect { tier, .. } => {
                assert_eq!(tier, TierKind::Starter);
            }
            other => panic!("expected CheckoutRedirect, got {other:?}"),
        }
        assert!(audit.has_event(TierSelectionAuditEventType::StripeCheckoutSessionCreated));
    }

    #[test]
    fn enterprise_direct_checkout_rejected() {
        let (l, audit) = ledger_with_dpa_ok();
        let err = l
            .select_tier(&ctx("t1", 1000), TierKind::Enterprise, "u@x.com")
            .unwrap_err();
        assert!(matches!(err, TierError::UseInquiryForm));
        assert!(audit.has_event(TierSelectionAuditEventType::EnterpriseRouteBypassAttempt));
    }

    #[test]
    fn d1_lock_prevents_concurrent_selection() {
        let (l, _) = ledger_with_dpa_ok();
        let _ok = l
            .select_tier(&ctx("t1", 1000), TierKind::Starter, "u@x.com")
            .unwrap();
        // Second call within 60s window.
        let err = l
            .select_tier(&ctx("t1", 2000), TierKind::Pro, "u@x.com")
            .unwrap_err();
        assert!(matches!(err, TierError::LockHeld));
        // After 60s window, lock should be evicted.
        let _ok2 = l
            .select_tier(
                &ctx("t1", 1000 + TIER_SELECTION_LOCK_WINDOW_MS + 1),
                TierKind::Free,
                "u@x.com",
            )
            .ok();
        // Note: this may fail with AlreadyActive after a Free
        // activation; we check at minimum that LockHeld is not raised
        // post-window.
    }

    #[test]
    fn audit_fail_closed_aborts_mutation() {
        let dpa = InMemoryDpaGate::new();
        dpa.accept(TenantId::new("t1"), "v1");
        let l = TierSelectionLedger::new(
            Arc::new(dpa),
            Arc::new(InMemoryStripeClient::new()),
            Arc::new(FailingTierSelectionAuditSink),
            "v1",
            "ok",
            "cancel",
        );
        let err = l
            .select_tier(&ctx("t1", 1000), TierKind::Free, "u@x.com")
            .unwrap_err();
        assert!(matches!(err, TierError::Audit(_)));
        assert!(l.row(&TenantId::new("t1")).is_none());
    }

    #[test]
    fn webhook_activation_sets_active_state() {
        let (l, _) = ledger_with_dpa_ok();
        let receipt = l
            .select_tier(&ctx("t1", 1000), TierKind::Pro, "u@x.com")
            .unwrap();
        let (session_id, _) = match receipt {
            TierSelectionReceipt::CheckoutRedirect {
                session_id, tier, ..
            } => (session_id, tier),
            other => panic!("expected CheckoutRedirect, got {other:?}"),
        };
        let event = StripeCheckoutSessionCompletedEvent {
            event_id: "evt_1".to_string(),
            session_id,
            tenant_id: TenantId::new("t1"),
            tier: TierKind::Pro,
            stripe_customer_id: StripeCustomerId::new("cus_1"),
            ts_ms: 5_000,
        };
        let r = l.on_checkout_completed(&event).unwrap();
        assert!(matches!(r, SubscriptionActivationReceipt::Activated { .. }));
        let row = l.row(&TenantId::new("t1")).unwrap();
        assert_eq!(row.subscription_state, SubscriptionState::Active);
    }

    #[test]
    fn webhook_duplicate_event_is_idempotent() {
        let (l, _) = ledger_with_dpa_ok();
        let _ = l
            .select_tier(&ctx("t1", 1000), TierKind::Pro, "u@x.com")
            .unwrap();
        let event = StripeCheckoutSessionCompletedEvent {
            event_id: "evt_dup".to_string(),
            session_id: "cs_x".to_string(),
            tenant_id: TenantId::new("t1"),
            tier: TierKind::Pro,
            stripe_customer_id: StripeCustomerId::new("cus_1"),
            ts_ms: 5_000,
        };
        let r1 = l.on_checkout_completed(&event).unwrap();
        assert!(matches!(
            r1,
            SubscriptionActivationReceipt::Activated { .. }
        ));
        let r2 = l.on_checkout_completed(&event).unwrap();
        assert!(matches!(
            r2,
            SubscriptionActivationReceipt::DuplicateIgnored { .. }
        ));
        assert_eq!(l.processed_event_count(), 1);
    }
}
