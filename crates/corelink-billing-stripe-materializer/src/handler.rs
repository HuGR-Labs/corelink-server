//! Production [`StateMaterializer`] implementation
//! ([`D1SubscriptionStateHandler`]).
//!
//! Drives [`BillingD1Writer`] + [`BillingAuditEmitter`] +
//! [`TierSelector`] for the five **state-mutating** canonical Stripe
//! events. The five observability-only echoes
//! (`customer.subscription.created`, `customer.subscription.trial_will_end`,
//! `charge.refunded`, `customer.created`, `invoice.created`) are
//! materialised here too via the `echo_*`-style entry points the
//! dispatcher does NOT route — instead the handler exposes a
//! [`D1SubscriptionStateHandler::materialize_echo`] entry that the
//! integration harness drives directly. This keeps the trait surface
//! 1:1 with the dispatcher's `StateMaterializer` (five methods) while
//! still allowing the e2e suite to pin all 10 event types.
//!
//! # Audit ordering (fail-CLOSED)
//!
//! Every state mutation goes through this exact sequence:
//!
//! 1. **Emit billing audit** (`corelink.billing.<event>.materialized.v1`).
//!    If this fails, the materializer returns
//!    `MaterializerError::Transient(...)` — the D1 write is **not**
//!    performed (orphan-state-free).
//! 2. **Perform D1 write.** If this fails, return `Transient` or
//!    `InvalidPayload` per the underlying writer error.
//! 3. **(subscription.updated only)** Recompute tier; if changed,
//!    persist new tier + emit `corelink.tenant.tier_changed.v1`. If
//!    the tier-emit fails after the D1 write succeeded, that is a
//!    transient error → dispatcher returns 500 → Stripe retries →
//!    next delivery hits the dedup row → no re-mutation.

use std::fmt;
use std::sync::Arc;

// Wave-36 Trigger A: trait + type surface migrated to the leaf
// `corelink-billing-stripe-traits` crate (no `corelink-stripe-real`
// dep in production sources of the materializer).
use corelink_billing_stripe_traits::{
    CanonicalWebhookEventType, MaterializerError, StateMaterializer, StripeWebhookEnvelope,
};
use corelink_tier_selection::tier::TierKind;

use crate::audit::{AuditSeverity, BillingAuditEmitter, BillingAuditError, BillingAuditRecord};
use crate::clock::{default_mat_clock, MatClock};
use crate::d1::{BillingD1Error, BillingD1Writer, MaterializedRow};
use crate::runners::RunnersEntitlementResolver;
use crate::tier::{TierSelectError, TierSelector};

/// The canonical 10-event × table × audit-event-name matrix.
///
/// Surface-stable for the audit-doc auto-generation step + the
/// regression test in `materializers_e2e.rs`. Each entry is
/// `(stripe_event_type, d1_table_or_none, audit_event_name)`.
pub const EVENT_MATERIALIZATION_MATRIX: &[(&str, Option<&str>, &str)] = &[
    // 5 state mutators ↓
    (
        "customer.subscription.deleted",
        Some("stripe_subscriptions"),
        "corelink.billing.subscription_canceled.materialized.v1",
    ),
    (
        "customer.subscription.updated",
        Some("stripe_subscriptions"),
        "corelink.billing.subscription.materialized.v1",
    ),
    (
        "invoice.paid",
        Some("stripe_invoices"),
        "corelink.billing.invoice.materialized.v1",
    ),
    (
        "invoice.payment_failed",
        Some("stripe_invoices"),
        "corelink.billing.invoice.materialized.v1",
    ),
    (
        "charge.dispute.created",
        Some("stripe_disputes"),
        "corelink.billing.dispute.materialized.v1",
    ),
    // 5 observability echoes ↓ (3 with table writes, 2 without)
    (
        "customer.subscription.created",
        Some("stripe_subscriptions"),
        "corelink.billing.subscription.materialized.v1",
    ),
    (
        "customer.subscription.trial_will_end",
        None,
        "corelink.billing.echo.v1",
    ),
    (
        "charge.refunded",
        Some("stripe_refunds"),
        "corelink.billing.refund.materialized.v1",
    ),
    (
        "customer.created",
        Some("stripe_customers"),
        "corelink.billing.customer.materialized.v1",
    ),
    ("invoice.created", None, "corelink.billing.echo.v1"),
];

/// Materializer error → dispatcher error conversion.
fn d1_to_mat(e: BillingD1Error) -> MaterializerError {
    match e {
        BillingD1Error::Transient(s) => MaterializerError::Transient(s),
        BillingD1Error::InvalidPayload(s) => MaterializerError::InvalidPayload(s),
    }
}

fn audit_to_mat(e: BillingAuditError) -> MaterializerError {
    MaterializerError::Transient(format!("audit fail-CLOSED: {e}"))
}

fn tier_to_mat(e: TierSelectError) -> MaterializerError {
    match e {
        TierSelectError::UnknownPlan(s) => MaterializerError::InvalidPayload(s),
        TierSelectError::Transient(s) => MaterializerError::Transient(s),
    }
}

/// Whether a Stripe subscription `status` is one this container materializer is
/// allowed to GRANT entitlement on. Mirrors the signup-worker's
/// `subscriptionStatusGrantsAccess` (`apps/signup-worker/src/webhooks/stripe.ts`):
/// ONLY `active` and `trialing` qualify. Fail-safe: an unknown/absent status is
/// treated as NOT grantable.
///
/// IMPORTANT — on a `customer.subscription.updated`, this status-gated path is
/// GRANT-ONLY: a non-granting status (`past_due`, `unpaid`, `incomplete`,
/// `incomplete_expired`, `paused`, `disputed`, unknown/absent) is NOT downgraded
/// here — the entitlement write is simply skipped. Active access DOWNGRADE on
/// those payment statuses is the signup-worker's responsibility — it is the
/// authoritative writer that flips the `subscription_state` gate off (see the
/// inline note in `reconcile_tier`). (Explicit `customer.subscription.deleted`
/// is handled separately and DOES downgrade to Free.) This gate is the
/// defense-in-depth guard that stops the container materializer (a SECOND writer
/// of `subscription_state`) from (re-)granting `subscription_state='active'` on
/// an `updated` event carrying a recognized plan but a non-granting status.
fn subscription_status_grants_access(status: &str) -> bool {
    matches!(status, "active" | "trialing")
}

/// Extract the subscription's price/plan id, tolerant to Stripe API-version
/// shape (F-MP-3, go-live audit): prefer the legacy `data.object.plan.id`, fall
/// back to the modern `data.object.items.data[0].price.id`. Both the cache-tier
/// and Runners reconcile use this so a pinned-API-version change (which can drop
/// the legacy `plan.id`) doesn't 422 every subscription event.
fn extract_plan_id(env: &StripeWebhookEnvelope) -> Option<&str> {
    let obj = env.data.get("object")?;
    obj.get("plan")
        .and_then(|p| p.get("id"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            obj.get("items")
                .and_then(|i| i.get("data"))
                .and_then(|d| d.get(0))
                .and_then(|it| it.get("price"))
                .and_then(|p| p.get("id"))
                .and_then(serde_json::Value::as_str)
        })
}

/// Production [`StateMaterializer`] implementation.
pub struct D1SubscriptionStateHandler {
    d1: Arc<dyn BillingD1Writer>,
    audit: Arc<dyn BillingAuditEmitter>,
    tier_selector: Arc<dyn TierSelector>,
    /// Optional Runners-tier entitlement resolver. `None` (default) ⇒ the
    /// Runners seed path is dormant and every subscription is treated as a
    /// cache-tier event (exact pre-existing behavior). Wired via
    /// [`Self::with_runners_resolver`] once the `STRIPE_PRICE_ID_RUNNER_*`
    /// prices exist — env-gated activation, mirroring the cache tier selector.
    runners_resolver: Option<Arc<dyn RunnersEntitlementResolver>>,
    clock: Arc<dyn MatClock>,
}

impl fmt::Debug for D1SubscriptionStateHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("D1SubscriptionStateHandler")
            .field("d1", &self.d1)
            .field("audit", &self.audit)
            .field("tier_selector", &self.tier_selector)
            .field("runners_resolver", &self.runners_resolver)
            .field("clock", &self.clock)
            .finish()
    }
}

impl D1SubscriptionStateHandler {
    /// Construct a new handler wiring `d1` + `audit` + `tier_selector`
    /// behind the canonical production [`MatClock`]
    /// ([`crate::clock::default_mat_clock`]): native targets get
    /// [`crate::clock::SystemMatClock`]; wasm32 targets get
    /// `crate::clock::WasmWorkerMatClock` which reads
    /// `js_sys::Date::now()` (avoiding the
    /// `wasm32-unknown-unknown` `SystemTime::now()` runtime panic per
    /// wave-22 closure of the wave-20 follow-on caveat).
    #[must_use]
    pub fn new(
        d1: Arc<dyn BillingD1Writer>,
        audit: Arc<dyn BillingAuditEmitter>,
        tier_selector: Arc<dyn TierSelector>,
    ) -> Self {
        Self {
            d1,
            audit,
            tier_selector,
            runners_resolver: None,
            clock: default_mat_clock(),
        }
    }

    /// Wire a [`RunnersEntitlementResolver`] so a Runners-tier subscription
    /// seeds `runners_entitlement` instead of `tier_selections`. Without it,
    /// the Runners path is dormant (every subscription → cache-tier path).
    #[must_use]
    pub fn with_runners_resolver(mut self, resolver: Arc<dyn RunnersEntitlementResolver>) -> Self {
        self.runners_resolver = Some(resolver);
        self
    }

    /// Inject a custom [`MatClock`] (tests use
    /// [`crate::clock::InMemoryFakeMatClock`] for deterministic
    /// timestamps).
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn MatClock>) -> Self {
        self.clock = clock;
        self
    }

    /// Materialize one observability-only echo event. Drives the
    /// same code path the dispatcher takes for state-mutating events
    /// (audit emit + optional D1 write) for the 5 echo arms.
    ///
    /// Returns Ok(()) on the no-table observability arms
    /// (`customer.subscription.trial_will_end`, `invoice.created`)
    /// after emitting only the `corelink.billing.echo.v1` audit row.
    pub fn materialize_echo(
        &self,
        event_type: CanonicalWebhookEventType,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        match event_type {
            CanonicalWebhookEventType::CustomerCreated => self.do_customer_created(env),
            CanonicalWebhookEventType::SubscriptionCreated => {
                self.do_subscription_upsert(env, false /* not canceled */)
            }
            CanonicalWebhookEventType::ChargeRefunded => self.do_refund(env),
            CanonicalWebhookEventType::SubscriptionTrialWillEnd
            | CanonicalWebhookEventType::InvoiceCreated => self.do_pure_echo(event_type, env),
            // The five state mutators have dedicated trait methods —
            // drive the typed materializer methods directly so the
            // dispatcher-level routing stays the only seam.
            other => Err(MaterializerError::InvalidPayload(format!(
                "materialize_echo not valid for {other:?}"
            ))),
        }
    }

    // ===== Per-event execution =====

    fn tenant_id_from(&self, env: &StripeWebhookEnvelope) -> Result<String, MaterializerError> {
        // Stripe attaches the tenant id via `metadata.tenant_id` per
        // the `corelink-tier-selection` checkout-session contract. We
        // tolerate both the canonical location and the legacy top-level
        // `tenant_id` echo (some webhook fixtures include it as a sibling
        // of `data.object` for replay convenience).
        let from_metadata = env
            .data
            .get("object")
            .and_then(|o| o.get("metadata"))
            .and_then(|m| m.get("tenant_id"))
            .and_then(|v| v.as_str());
        let from_envelope = env.data.get("tenant_id").and_then(|v| v.as_str());
        from_metadata
            .or(from_envelope)
            .map(|s| s.to_string())
            .ok_or_else(|| {
                MaterializerError::InvalidPayload(
                    "missing tenant_id (expected at data.object.metadata.tenant_id)".to_string(),
                )
            })
    }

    fn stripe_object_id(&self, env: &StripeWebhookEnvelope) -> Option<String> {
        env.data
            .get("object")
            .and_then(|o| o.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    fn do_subscription_upsert(
        &self,
        env: &StripeWebhookEnvelope,
        canceled: bool,
    ) -> Result<(), MaterializerError> {
        let tenant_id = self.tenant_id_from(env)?;
        let sub_id = self.stripe_object_id(env).ok_or_else(|| {
            MaterializerError::InvalidPayload("missing data.object.id".to_string())
        })?;
        let status = env
            .data
            .get("object")
            .and_then(|o| o.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or(if canceled { "canceled" } else { "active" })
            .to_string();

        let audit_name = if canceled {
            "corelink.billing.subscription_canceled.materialized.v1"
        } else {
            "corelink.billing.subscription.materialized.v1"
        };

        let now_ms = self.clock.now_ms();

        // 1) Audit BEFORE state mutation (fail-CLOSED).
        let payload = serde_json::json!({
            "stripe_event_type": env.event_type,
            "stripe_subscription_id": sub_id,
            "status": status,
        });
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: audit_name,
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.clone(),
                stripe_object_id: Some(sub_id.clone()),
                severity: AuditSeverity::Notice,
                ts_ms: now_ms,
                payload: payload.clone(),
            })
            .map_err(audit_to_mat)?;

        // 2) D1 write.
        let row = MaterializedRow {
            table: "stripe_subscriptions".to_string(),
            tenant_id: tenant_id.clone(),
            stripe_id: sub_id.clone(),
            stripe_event_id: env.id.clone(),
            payload,
            materialized_at_ms: now_ms,
        };
        if canceled {
            self.d1.mark_subscription_canceled(row).map_err(d1_to_mat)?;
        } else {
            self.d1.upsert_subscription(row).map_err(d1_to_mat)?;
        }

        // 3) Entitlement reconciliation (the *.updated and *.deleted arms). Route
        // by PRODUCT: a Runners-tier price seeds/revokes `runners_entitlement`;
        // any other price reconciles the cache `tier_selections`.
        // `reconcile_runners` returns Ok(true) when it HANDLED a Runners price (so
        // we must NOT also run the cache path — a Runners price is not a cache
        // tier and would 422 UnknownPlan there); Ok(false) when the resolver is
        // dormant or the price is not a Runners price ⇒ fall through to the cache
        // path (exact pre-existing behavior).
        //
        // The SAME mutual-exclusion routing applies to BOTH the `updated` and the
        // `deleted` arms: on a runner-price subscription that goes non-granting
        // (`updated` with `past_due`/`unpaid`/`canceled`/…) OR is
        // `customer.subscription.deleted`, `reconcile_runners` REVOKES the
        // entitlement (symmetric to how it seeds) rather than leaving it stale;
        // in either handled case the caller skips the cache reconcile/downgrade.
        if env.event_type == "customer.subscription.updated" {
            if !self.reconcile_runners(env, &tenant_id, &status, now_ms)? {
                self.reconcile_tier(env, &tenant_id, &status, now_ms)?;
            }
        } else if canceled {
            // `customer.subscription.deleted`. If the price is a Runners price,
            // `reconcile_runners` revokes `runners_entitlement` (the `canceled`
            // status is non-granting → revoke branch) and returns Ok(true), so we
            // must NOT then run the cache downgrade (a Runners price is not a
            // cache tier). Otherwise fall through to the cache-tier downgrade.
            if !self.reconcile_runners(env, &tenant_id, &status, now_ms)? {
                // On cancel, downgrade tenant to Free (per dispatcher contract —
                // `customer.subscription.deleted` → "downgrade to Free tier").
                // Emit audit if the downgrade is a real change. This uses the
                // dedicated DOWNGRADE path, which writes
                // `subscription_state='inactive'` (the access gate OFF) — NOT the
                // grant path's 'active' — so a canceled tenant never lands a
                // contradictory active-free row (the signup-worker is the primary
                // downgrade authority; this is the container's defense-in-depth
                // convergent write).
                self.persist_tier_downgrade(&tenant_id, TierKind::Free, env, now_ms)?;
            }
        }

        Ok(())
    }

    /// Reconcile `runners_entitlement` when the subscription's plan is a
    /// Runners-tier price. Returns `Ok(true)` when it WAS a Runners price
    /// (handled — the caller must not also run the cache-tier reconcile/downgrade),
    /// `Ok(false)` when the resolver is dormant or the price is not a Runners price
    /// (caller falls back to the cache path). Status-gated + audit-before-write,
    /// mirroring `reconcile_tier`.
    ///
    /// SYMMETRIC seed/revoke: a granting status (`active`/`trialing`) SEEDS the
    /// entitlement (`corelink.tenant.runners_entitlement_seeded.v1`); a
    /// NON-granting status (`past_due`, `unpaid`, `paused`, `canceled`, unknown —
    /// this includes `customer.subscription.deleted`, whose status is `canceled`)
    /// REVOKES it (`corelink.tenant.runners_entitlement_revoked.v1`) rather than
    /// leaving a permanently-granted stale row. The signup-worker is the primary
    /// authority; this is the container's defense-in-depth convergent write.
    fn reconcile_runners(
        &self,
        env: &StripeWebhookEnvelope,
        tenant_id: &str,
        status: &str,
        now_ms: u64,
    ) -> Result<bool, MaterializerError> {
        let Some(resolver) = self.runners_resolver.as_ref() else {
            return Ok(false); // dormant (no STRIPE_PRICE_ID_RUNNER_* wired)
        };
        let Some(plan_id) = extract_plan_id(env) else {
            return Ok(false); // no plan/price id → let the cache path raise its own error
        };
        let Some(ent) = resolver.resolve(plan_id) else {
            return Ok(false); // not a Runners price → cache-tier path
        };
        // It IS a Runners-tier price ⇒ handled (return Ok(true) either way so the
        // caller never falls through to the cache reconcile, which would 422
        // UnknownPlan on a Runners price).
        if !subscription_status_grants_access(status) {
            // Non-granting status (or a `customer.subscription.deleted`, status
            // 'canceled'): REVOKE the entitlement symmetrically to the seed so the
            // container materializer never leaves a stale grant. Audit BEFORE the
            // state mutation (fail-CLOSED ordering), mirroring the seed emit.
            self.audit
                .emit_billing(&BillingAuditRecord {
                    event_name: "corelink.tenant.runners_entitlement_revoked.v1",
                    stripe_event_id: env.id.clone(),
                    stripe_event_type: env.event_type.clone(),
                    tenant_id: tenant_id.to_string(),
                    stripe_object_id: None,
                    severity: AuditSeverity::Notice,
                    ts_ms: now_ms,
                    payload: serde_json::json!({
                        "status": status,
                    }),
                })
                .map_err(audit_to_mat)?;
            self.d1
                .delete_runners_entitlement(tenant_id)
                .map_err(d1_to_mat)?;
            return Ok(true);
        }
        // Granting status: SEED. Audit BEFORE the state mutation (fail-CLOSED).
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.tenant.runners_entitlement_seeded.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.to_string(),
                stripe_object_id: None,
                severity: AuditSeverity::Notice,
                ts_ms: now_ms,
                payload: serde_json::json!({
                    "max_concurrency": ent.max_concurrency,
                    "max_vcpu_h": ent.max_vcpu_h,
                }),
            })
            .map_err(audit_to_mat)?;
        self.d1
            .upsert_runners_entitlement(
                tenant_id,
                ent.max_concurrency,
                ent.max_vcpu_h,
                now_ms as i64,
            )
            .map_err(d1_to_mat)?;
        Ok(true)
    }

    fn reconcile_tier(
        &self,
        env: &StripeWebhookEnvelope,
        tenant_id: &str,
        status: &str,
        now_ms: u64,
    ) -> Result<(), MaterializerError> {
        let plan_id = extract_plan_id(env).ok_or_else(|| {
            MaterializerError::InvalidPayload(
                "missing data.object.plan.id / items.data[].price.id (required for tier reconciliation)"
                    .to_string(),
            )
        })?;
        let seat_count = env
            .data
            .get("object")
            .and_then(|o| o.get("quantity"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let new_tier = self
            .tier_selector
            .compute_tier(plan_id, seat_count)
            .map_err(tier_to_mat)?;

        // SUBSCRIPTION-STATUS GATE (defense-in-depth, mirrors the
        // signup-worker's `subscriptionStatusGrantsAccess`): this materializer
        // is a SECOND writer of the canonical `tier_selections.subscription_state`
        // gate, and `persist_tier_change` → `upsert_tier` UNCONDITIONALLY writes
        // `subscription_state='active'`. A `customer.subscription.updated`
        // carrying a recognized plan but a NON-granting status (`past_due`,
        // `unpaid`, `incomplete`, `incomplete_expired`, `paused`, `canceled`,
        // unknown) must therefore NOT reach the 'active' upsert — otherwise it
        // (re-)grants a paid entitlement for unpaid/lapsed money. We skip the
        // entitlement write (and its tier_changed audit) for non-granting
        // statuses; the `subscription.materialized` audit + the
        // `stripe_subscriptions` row (with the real status) were already
        // recorded above, so the event remains fully observable. The
        // signup-worker (the authority) is responsible for actively flipping
        // the gate to a non-active state.
        if !subscription_status_grants_access(status) {
            return Ok(());
        }

        self.persist_tier_change(tenant_id, new_tier, env, now_ms)
    }

    fn persist_tier_change(
        &self,
        tenant_id: &str,
        new_tier: TierKind,
        env: &StripeWebhookEnvelope,
        now_ms: u64,
    ) -> Result<(), MaterializerError> {
        let current = self.d1.read_tier(tenant_id).map_err(d1_to_mat)?;
        let new_wire = new_tier.as_str();
        let changed = current.as_deref() != Some(new_wire);
        if !changed {
            return Ok(());
        }

        // Audit BEFORE write.
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.tenant.tier_changed.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.to_string(),
                stripe_object_id: None,
                severity: AuditSeverity::Notice,
                ts_ms: now_ms,
                payload: serde_json::json!({
                    "from": current,
                    "to": new_wire,
                }),
            })
            .map_err(audit_to_mat)?;

        self.d1
            // `now_ms` (u64) → i64 for the `subscription_started_at_ms`
            // bind. The Stripe-event clock is far below i64::MAX (ms
            // since epoch), so the cast is lossless in practice.
            .upsert_tier(tenant_id, new_wire, now_ms as i64, &env.id)
            .map_err(d1_to_mat)?;
        Ok(())
    }

    /// Cancel/downgrade twin of [`Self::persist_tier_change`]: same
    /// read_tier/changed short-circuit + the same
    /// `corelink.tenant.tier_changed.v1` audit-BEFORE-write ordering, but
    /// drives [`BillingD1Writer::downgrade_tier`] (which writes
    /// `subscription_state='inactive'` — the access gate OFF) instead of
    /// `upsert_tier` (which hard-codes `'active'`). Used ONLY on
    /// `customer.subscription.deleted` so a canceled tenant converges to an
    /// access-OFF row rather than the contradictory active-free row the grant
    /// path would leave (defense-in-depth; the signup-worker is the primary
    /// downgrade authority).
    fn persist_tier_downgrade(
        &self,
        tenant_id: &str,
        new_tier: TierKind,
        env: &StripeWebhookEnvelope,
        now_ms: u64,
    ) -> Result<(), MaterializerError> {
        let current = self.d1.read_tier(tenant_id).map_err(d1_to_mat)?;
        let new_wire = new_tier.as_str();
        let changed = current.as_deref() != Some(new_wire);
        if !changed {
            return Ok(());
        }

        // Audit BEFORE write.
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.tenant.tier_changed.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.to_string(),
                stripe_object_id: None,
                severity: AuditSeverity::Notice,
                ts_ms: now_ms,
                payload: serde_json::json!({
                    "from": current,
                    "to": new_wire,
                }),
            })
            .map_err(audit_to_mat)?;

        self.d1
            // `now_ms` (u64) → i64 for the `subscription_started_at_ms`
            // bind (INSERT/new-row path only; the DO UPDATE preserves the
            // original start). Lossless in practice (ms since epoch).
            .downgrade_tier(tenant_id, new_wire, now_ms as i64, &env.id)
            .map_err(d1_to_mat)?;
        Ok(())
    }

    fn do_invoice(
        &self,
        env: &StripeWebhookEnvelope,
        outcome: &'static str,
    ) -> Result<(), MaterializerError> {
        let tenant_id = self.tenant_id_from(env)?;
        let inv_id = self.stripe_object_id(env).ok_or_else(|| {
            MaterializerError::InvalidPayload("missing data.object.id".to_string())
        })?;
        let now_ms = self.clock.now_ms();

        let payload = serde_json::json!({
            "stripe_event_type": env.event_type,
            "invoice_id": inv_id,
            "outcome": outcome,
        });
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.billing.invoice.materialized.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.clone(),
                stripe_object_id: Some(inv_id.clone()),
                severity: AuditSeverity::Notice,
                ts_ms: now_ms,
                payload: payload.clone(),
            })
            .map_err(audit_to_mat)?;

        self.d1
            .upsert_invoice(MaterializedRow {
                table: "stripe_invoices".to_string(),
                tenant_id,
                stripe_id: inv_id,
                stripe_event_id: env.id.clone(),
                payload,
                materialized_at_ms: now_ms,
            })
            .map_err(d1_to_mat)
    }

    fn do_dispute(&self, env: &StripeWebhookEnvelope) -> Result<(), MaterializerError> {
        let tenant_id = self.tenant_id_from(env)?;
        let dispute_id = self.stripe_object_id(env).ok_or_else(|| {
            MaterializerError::InvalidPayload("missing data.object.id".to_string())
        })?;
        let now_ms = self.clock.now_ms();

        let payload = serde_json::json!({
            "stripe_event_type": env.event_type,
            "dispute_id": dispute_id,
        });
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.billing.dispute.materialized.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.clone(),
                stripe_object_id: Some(dispute_id.clone()),
                // Sev1: Finance + customer attention required.
                severity: AuditSeverity::Sev1,
                ts_ms: now_ms,
                payload: payload.clone(),
            })
            .map_err(audit_to_mat)?;

        self.d1
            .insert_dispute(MaterializedRow {
                table: "stripe_disputes".to_string(),
                tenant_id,
                stripe_id: dispute_id,
                stripe_event_id: env.id.clone(),
                payload,
                materialized_at_ms: now_ms,
            })
            .map_err(d1_to_mat)
    }

    fn do_refund(&self, env: &StripeWebhookEnvelope) -> Result<(), MaterializerError> {
        let tenant_id = self.tenant_id_from(env)?;
        let charge_id = self.stripe_object_id(env).ok_or_else(|| {
            MaterializerError::InvalidPayload("missing data.object.id".to_string())
        })?;
        let now_ms = self.clock.now_ms();

        let payload = serde_json::json!({
            "stripe_event_type": env.event_type,
            "charge_id": charge_id,
        });
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.billing.refund.materialized.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.clone(),
                stripe_object_id: Some(charge_id.clone()),
                severity: AuditSeverity::Notice,
                ts_ms: now_ms,
                payload: payload.clone(),
            })
            .map_err(audit_to_mat)?;

        self.d1
            .insert_refund(MaterializedRow {
                table: "stripe_refunds".to_string(),
                tenant_id,
                stripe_id: charge_id,
                stripe_event_id: env.id.clone(),
                payload,
                materialized_at_ms: now_ms,
            })
            .map_err(d1_to_mat)
    }

    fn do_customer_created(&self, env: &StripeWebhookEnvelope) -> Result<(), MaterializerError> {
        let tenant_id = self.tenant_id_from(env)?;
        let cus_id = self.stripe_object_id(env).ok_or_else(|| {
            MaterializerError::InvalidPayload("missing data.object.id".to_string())
        })?;
        let now_ms = self.clock.now_ms();

        let payload = serde_json::json!({
            "stripe_event_type": env.event_type,
            "customer_id": cus_id,
        });
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.billing.customer.materialized.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id: tenant_id.clone(),
                stripe_object_id: Some(cus_id.clone()),
                severity: AuditSeverity::Notice,
                ts_ms: now_ms,
                payload: payload.clone(),
            })
            .map_err(audit_to_mat)?;

        self.d1
            .upsert_customer(MaterializedRow {
                table: "stripe_customers".to_string(),
                tenant_id,
                stripe_id: cus_id,
                stripe_event_id: env.id.clone(),
                payload,
                materialized_at_ms: now_ms,
            })
            .map_err(d1_to_mat)
    }

    fn do_pure_echo(
        &self,
        event_type: CanonicalWebhookEventType,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        // No D1 mutation; just emit the audit echo so the chain still
        // pins the delivery. Tenant id is optional for these arms
        // (`invoice.created` arrives before the tenant linkage is
        // resolved in some Stripe flows). We tolerate its absence.
        let tenant_id = self
            .tenant_id_from(env)
            .unwrap_or_else(|_| "__unknown__".to_string());
        let now_ms = self.clock.now_ms();
        self.audit
            .emit_billing(&BillingAuditRecord {
                event_name: "corelink.billing.echo.v1",
                stripe_event_id: env.id.clone(),
                stripe_event_type: env.event_type.clone(),
                tenant_id,
                stripe_object_id: self.stripe_object_id(env),
                severity: AuditSeverity::Info,
                ts_ms: now_ms,
                payload: serde_json::json!({
                    "canonical_event_type": event_type.label(),
                }),
            })
            .map_err(audit_to_mat)
    }
}

impl StateMaterializer for D1SubscriptionStateHandler {
    fn on_subscription_deleted(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.do_subscription_upsert(env, true)
    }

    fn on_subscription_updated(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.do_subscription_upsert(env, false)
    }

    fn on_invoice_paid(&self, env: &StripeWebhookEnvelope) -> Result<(), MaterializerError> {
        self.do_invoice(env, "paid")
    }

    fn on_invoice_payment_failed(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.do_invoice(env, "payment_failed")
    }

    fn on_charge_dispute_created(
        &self,
        env: &StripeWebhookEnvelope,
    ) -> Result<(), MaterializerError> {
        self.do_dispute(env)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::InMemoryBillingAuditEmitter;
    use crate::clock::InMemoryFakeMatClock;
    use crate::d1::InMemoryBillingD1;
    use crate::tier::InMemoryTierSelector;
    use corelink_tier_selection::tier::TierKind;

    fn fixture() -> (
        D1SubscriptionStateHandler,
        Arc<InMemoryBillingD1>,
        Arc<InMemoryBillingAuditEmitter>,
    ) {
        let d1 = Arc::new(InMemoryBillingD1::new());
        let audit = Arc::new(InMemoryBillingAuditEmitter::new());
        let sel = Arc::new(InMemoryTierSelector::with_mapping(&[
            ("plan_solo", TierKind::Solo),
            ("plan_starter", TierKind::Starter),
            ("plan_pro", TierKind::Pro),
            ("plan_max", TierKind::Max),
        ]));
        let handler = D1SubscriptionStateHandler::new(d1.clone(), audit.clone(), sel).with_clock(
            Arc::new(InMemoryFakeMatClock::at_unix_ms(1_700_000_000_000)),
        );
        (handler, d1, audit)
    }

    fn env(id: &str, kind: &str, body: serde_json::Value) -> StripeWebhookEnvelope {
        let raw = serde_json::json!({
            "id": id,
            "type": kind,
            "created": 1_700_000_000_u64,
            "data": body,
        });
        let bytes = serde_json::to_vec(&raw).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn matrix_has_ten_entries() {
        assert_eq!(EVENT_MATERIALIZATION_MATRIX.len(), 10);
    }

    #[test]
    fn container_webhook_events_json_matches_the_matrix() {
        // The reconcile script (scripts/ops/stripe-reconcile-webhook-events.sh)
        // sets the LIVE container endpoint's `enabled_events` from
        // container-webhook-events.json. This test makes EVENT_MATERIALIZATION_MATRIX
        // the sole authority: the JSON can never drift from the code, so the live
        // endpoint can never be subscribed to more/fewer events than the container
        // actually materializes (over-subscription is harmless-but-untidy;
        // under-subscription would silently drop a grant path).
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../container-webhook-events.json")).unwrap();
        let mut from_json: Vec<String> = json["enabled_events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        from_json.sort();
        let mut from_matrix: Vec<String> = EVENT_MATERIALIZATION_MATRIX
            .iter()
            .map(|(ev, _, _)| (*ev).to_owned())
            .collect();
        from_matrix.sort();
        assert_eq!(
            from_json, from_matrix,
            "container-webhook-events.json enabled_events must equal the \
             EVENT_MATERIALIZATION_MATRIX event-type column exactly — update the JSON \
             when you change the matrix (the reconcile script drives the live endpoint from it)."
        );
    }

    #[test]
    fn subscription_updated_recomputes_tier_and_emits_change_audit() {
        let (handler, d1, audit) = fixture();
        d1.upsert_tier("ten_1", "starter", 1_700_000_000_000, "init")
            .unwrap();
        let e = env(
            "evt_su",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_1",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_1" },
                    "plan": { "id": "plan_pro" },
                    "quantity": 5,
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        assert_eq!(d1.tier_for("ten_1"), Some("pro".to_string()));
        assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
    }

    /// Fixture WITH a Runners resolver wired (price `price_runner_team` →
    /// Team tier: 80 concurrency / 600 vCPU-h).
    fn fixture_with_runners() -> (
        D1SubscriptionStateHandler,
        Arc<InMemoryBillingD1>,
        Arc<InMemoryBillingAuditEmitter>,
    ) {
        let (handler, d1, audit) = fixture();
        let resolver = Arc::new(
            crate::runners::InMemoryRunnersEntitlementResolver::new().with_price(
                "price_runner_team",
                crate::runners::RunnersEntitlement {
                    max_concurrency: 80,
                    max_vcpu_h: 600,
                },
            ),
        );
        (handler.with_runners_resolver(resolver), d1, audit)
    }

    #[test]
    fn runners_subscription_seeds_entitlement_not_tier() {
        let (handler, d1, audit) = fixture_with_runners();
        let e = env(
            "evt_run",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_run",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_run" },
                    "plan": { "id": "price_runner_team" }
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));
        assert_eq!(
            audit.count_event("corelink.tenant.runners_entitlement_seeded.v1"),
            1
        );
        // Separate axis: the cache tier was NOT touched (no UnknownPlan 422).
        assert_eq!(d1.tier_for("ten_run"), None);
    }

    #[test]
    fn runners_price_with_non_granting_status_does_not_seed() {
        for non_granting in ["past_due", "unpaid", "canceled"] {
            let (handler, d1, _audit) = fixture_with_runners();
            let e = env(
                "evt_run",
                "customer.subscription.updated",
                serde_json::json!({
                    "object": {
                        "id": "sub_run",
                        "status": non_granting,
                        "metadata": { "tenant_id": "ten_run" },
                        "plan": { "id": "price_runner_team" }
                    }
                }),
            );
            handler.on_subscription_updated(&e).unwrap();
            assert_eq!(
                d1.runners_entitlement_of("ten_run"),
                None,
                "status {non_granting} must NOT seed runners_entitlement"
            );
        }
    }

    #[test]
    fn runners_updated_non_granting_status_revokes_prior_entitlement() {
        // WP4: a Runners subscription that was granted then goes NON-granting
        // (e.g. `customer.subscription.updated` status=canceled/past_due/unpaid)
        // must REVOKE `runners_entitlement` (→ None) and emit
        // `runners_entitlement_revoked.v1`, symmetric to the seed — never leave a
        // stale grant.
        for non_granting in ["canceled", "past_due", "unpaid"] {
            let (handler, d1, audit) = fixture_with_runners();
            // Seed first via a granting `active` event.
            let seed = env(
                "evt_seed",
                "customer.subscription.updated",
                serde_json::json!({
                    "object": {
                        "id": "sub_run",
                        "status": "active",
                        "metadata": { "tenant_id": "ten_run" },
                        "plan": { "id": "price_runner_team" }
                    }
                }),
            );
            handler.on_subscription_updated(&seed).unwrap();
            assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));

            // Now the subscription goes non-granting → revoke.
            let e = env(
                "evt_revoke",
                "customer.subscription.updated",
                serde_json::json!({
                    "object": {
                        "id": "sub_run",
                        "status": non_granting,
                        "metadata": { "tenant_id": "ten_run" },
                        "plan": { "id": "price_runner_team" }
                    }
                }),
            );
            handler.on_subscription_updated(&e).unwrap();
            assert_eq!(
                d1.runners_entitlement_of("ten_run"),
                None,
                "status {non_granting} must REVOKE the prior runners_entitlement"
            );
            assert_eq!(
                audit.count_event("corelink.tenant.runners_entitlement_revoked.v1"),
                1,
                "status {non_granting} must emit a runners_entitlement_revoked.v1 audit"
            );
        }
    }

    #[test]
    fn runners_subscription_deleted_revokes_entitlement() {
        // WP4: a `customer.subscription.deleted` for a Runners-price sub must
        // route through the runner REVOKE (not the cache-tier downgrade) and
        // remove `runners_entitlement`.
        let (handler, d1, audit) = fixture_with_runners();
        // Seed the entitlement first.
        let seed = env(
            "evt_seed",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_run",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_run" },
                    "plan": { "id": "price_runner_team" }
                }
            }),
        );
        handler.on_subscription_updated(&seed).unwrap();
        assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));

        // Now delete the subscription (a Runners-price sub).
        let del = env(
            "evt_del",
            "customer.subscription.deleted",
            serde_json::json!({
                "object": {
                    "id": "sub_run",
                    "status": "canceled",
                    "metadata": { "tenant_id": "ten_run" },
                    "plan": { "id": "price_runner_team" }
                }
            }),
        );
        handler.on_subscription_deleted(&del).unwrap();
        assert_eq!(
            d1.runners_entitlement_of("ten_run"),
            None,
            "customer.subscription.deleted for a runner price must revoke the entitlement"
        );
        assert_eq!(
            audit.count_event("corelink.tenant.runners_entitlement_revoked.v1"),
            1
        );
        // Runner-handled ⇒ the cache-tier downgrade path was NOT taken (a runner
        // price is not a cache tier); the cache tier stays untouched (None).
        assert_eq!(d1.tier_for("ten_run"), None);
    }

    #[test]
    fn cache_subscription_deleted_does_not_touch_runners_entitlement() {
        // WP4 no-regression: a CACHE-price `customer.subscription.deleted` must
        // still downgrade the cache tier and must NOT touch runners_entitlement
        // (nor emit a runner revoke audit). We independently seed a runner
        // entitlement for the SAME tenant to prove the cache cancel leaves it
        // intact.
        let (handler, d1, audit) = fixture_with_runners();
        d1.upsert_tier("ten_cache", "pro", 1_700_000_000_000, "init")
            .unwrap();
        // An unrelated runner grant exists for this tenant.
        d1.upsert_runners_entitlement("ten_cache", 80, 600, 1_700_000_000_000)
            .unwrap();

        let del = env(
            "evt_cache_del",
            "customer.subscription.deleted",
            serde_json::json!({
                "object": {
                    "id": "sub_cache",
                    "status": "canceled",
                    "metadata": { "tenant_id": "ten_cache" },
                    "plan": { "id": "plan_pro" }
                }
            }),
        );
        handler.on_subscription_deleted(&del).unwrap();
        // Cache tier's `subscription_state` gate flips off, but the historical
        // `tier` label is preserved (the signup-worker is the tier authority
        // on cancel; the container must never overwrite it to 'free').
        assert_eq!(d1.tier_for("ten_cache"), Some("pro".to_string()));
        // Runner entitlement UNTOUCHED (no revoke ran on a cache-price cancel).
        assert_eq!(d1.runners_entitlement_of("ten_cache"), Some((80, 600)));
        assert_eq!(
            audit.count_event("corelink.tenant.runners_entitlement_revoked.v1"),
            0,
            "a cache-price cancel must NOT emit a runner revoke audit"
        );
    }

    #[test]
    fn extract_plan_id_falls_back_to_items_price_id() {
        // F-MP-3: a modern Stripe subscription with NO legacy plan.id but a
        // nested items.data[0].price.id must still resolve (tier + runners).
        let (handler, d1, _audit) = fixture_with_runners();
        let e = env(
            "evt_items",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_items",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_items" },
                    "items": { "data": [ { "price": { "id": "price_runner_team" } } ] }
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        // Resolved via items[].price.id → Runners Team seeded (80/600).
        assert_eq!(d1.runners_entitlement_of("ten_items"), Some((80, 600)));
    }

    #[test]
    fn cache_price_still_routes_to_tier_when_runners_resolver_present() {
        // A wired resolver must NOT divert cache-tier subscriptions: a
        // non-Runners price (plan_pro) still reconciles tier_selections.
        let (handler, d1, _audit) = fixture_with_runners();
        let e = env(
            "evt_cache",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_cache",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_cache" },
                    "plan": { "id": "plan_pro" }
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        assert_eq!(d1.tier_for("ten_cache"), Some("pro".to_string()));
        assert_eq!(d1.runners_entitlement_of("ten_cache"), None);
    }

    #[test]
    fn subscription_updated_non_granting_status_does_not_grant_active() {
        // Regression (money-path webhook status gate): a
        // customer.subscription.updated carrying a RECOGNIZED plan but a
        // NON-granting status (past_due / unpaid) MUST NOT (re-)grant the
        // canonical 'active' entitlement. `active` still does.
        for non_granting in ["past_due", "unpaid"] {
            let (handler, d1, audit) = fixture();
            // Tenant starts on a non-active baseline (free), so a granted
            // 'active' write would be an observable upgrade.
            d1.upsert_tier("ten_1", "free", 1_700_000_000_000, "init")
                .unwrap();
            let e = env(
                "evt_su",
                "customer.subscription.updated",
                serde_json::json!({
                    "object": {
                        "id": "sub_1",
                        "status": non_granting,
                        "metadata": { "tenant_id": "ten_1" },
                        "plan": { "id": "plan_pro" },
                        "quantity": 5,
                    }
                }),
            );
            handler.on_subscription_updated(&e).unwrap();
            // Entitlement gate NOT advanced to the paid 'pro' tier: the
            // materializer skipped the 'active' upsert for the non-granting
            // status (tier stays at the pre-existing baseline).
            assert_eq!(
                d1.tier_for("ten_1"),
                Some("free".to_string()),
                "status={non_granting} must NOT grant the paid tier (subscription_state='active')"
            );
            // No tier-change entitlement audit was emitted (the gate fired
            // before persist_tier_change).
            assert_eq!(
                audit.count_event("corelink.tenant.tier_changed.v1"),
                0,
                "status={non_granting} must not emit a tier_changed entitlement audit"
            );
        }

        // Control: an `active` status with the same recognized plan DOES grant.
        let (handler, d1, audit) = fixture();
        d1.upsert_tier("ten_1", "free", 1_700_000_000_000, "init")
            .unwrap();
        let e = env(
            "evt_su_ok",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_1",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_1" },
                    "plan": { "id": "plan_pro" },
                    "quantity": 5,
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        assert_eq!(d1.tier_for("ten_1"), Some("pro".to_string()));
        assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
    }

    #[test]
    fn subscription_deleted_marks_canceled_and_preserves_historical_tier() {
        // Stripe-cancel tier-race fix: previously named
        // `subscription_deleted_marks_canceled_and_downgrades_to_free` and
        // asserted `tier_for("ten_1") == Some("free")` — that encoded the BUG
        // (the container overwriting `tier_selections.tier` to `'free'` on
        // cancel, racing the signup-worker authority, which deliberately
        // leaves `tier` untouched and only flips `subscription_state`).
        // Renamed + re-asserted: cancel must set `subscription_state` to the
        // access-OFF gate (proven by `mark_subscription_canceled`'s row /
        // `downgrade_tier` being the driven path — see
        // `subscription_deleted_takes_downgrade_path_not_grant_path` below)
        // WITHOUT touching the historical `tier` label — it must stay `pro`.
        let (handler, d1, audit) = fixture();
        d1.upsert_tier("ten_1", "pro", 1_700_000_000_000, "init")
            .unwrap();
        let e = env(
            "evt_sd",
            "customer.subscription.deleted",
            serde_json::json!({
                "object": {
                    "id": "sub_1",
                    "status": "canceled",
                    "metadata": { "tenant_id": "ten_1" },
                }
            }),
        );
        handler.on_subscription_deleted(&e).unwrap();
        assert_eq!(
            d1.tier_for("ten_1"),
            Some("pro".to_string()),
            "cancel must preserve the historical tier label, NOT overwrite it to 'free'"
        );
        assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
        assert_eq!(
            audit.count_event("corelink.billing.subscription_canceled.materialized.v1"),
            1
        );
    }

    /// A [`BillingD1Writer`] decorator that records WHICH tier-write method the
    /// handler invoked (grant `upsert_tier` vs cancel `downgrade_tier`). Every
    /// other method + all state forwards to the wrapped [`InMemoryBillingD1`]
    /// so the existing fixture semantics are preserved.
    #[derive(Debug)]
    struct TierPathRecordingD1 {
        inner: Arc<InMemoryBillingD1>,
        upsert_calls: std::sync::Mutex<Vec<String>>,
        downgrade_calls: std::sync::Mutex<Vec<String>>,
    }

    impl TierPathRecordingD1 {
        fn new(inner: Arc<InMemoryBillingD1>) -> Self {
            Self {
                inner,
                upsert_calls: std::sync::Mutex::new(Vec::new()),
                downgrade_calls: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl BillingD1Writer for TierPathRecordingD1 {
        fn upsert_customer(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
            self.inner.upsert_customer(row)
        }
        fn upsert_subscription(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
            self.inner.upsert_subscription(row)
        }
        fn mark_subscription_canceled(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
            self.inner.mark_subscription_canceled(row)
        }
        fn upsert_invoice(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
            self.inner.upsert_invoice(row)
        }
        fn insert_dispute(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
            self.inner.insert_dispute(row)
        }
        fn insert_refund(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
            self.inner.insert_refund(row)
        }
        fn try_record_event(
            &self,
            stripe_event_id: &str,
            canonical_event_type: &str,
            now_ms: u64,
        ) -> Result<bool, BillingD1Error> {
            self.inner
                .try_record_event(stripe_event_id, canonical_event_type, now_ms)
        }
        fn read_tier(&self, tenant_id: &str) -> Result<Option<String>, BillingD1Error> {
            self.inner.read_tier(tenant_id)
        }
        fn upsert_tier(
            &self,
            tenant_id: &str,
            tier_wire: &str,
            now_ms: i64,
            correlation_id: &str,
        ) -> Result<(), BillingD1Error> {
            self.upsert_calls
                .lock()
                .map_err(|e| BillingD1Error::Transient(format!("rec mutex: {e}")))?
                .push(tier_wire.to_string());
            self.inner
                .upsert_tier(tenant_id, tier_wire, now_ms, correlation_id)
        }
        fn downgrade_tier(
            &self,
            tenant_id: &str,
            tier_wire: &str,
            now_ms: i64,
            correlation_id: &str,
        ) -> Result<(), BillingD1Error> {
            self.downgrade_calls
                .lock()
                .map_err(|e| BillingD1Error::Transient(format!("rec mutex: {e}")))?
                .push(tier_wire.to_string());
            self.inner
                .downgrade_tier(tenant_id, tier_wire, now_ms, correlation_id)
        }
        fn upsert_runners_entitlement(
            &self,
            tenant_id: &str,
            max_concurrency: u32,
            max_vcpu_h: u32,
            now_ms: i64,
        ) -> Result<(), BillingD1Error> {
            self.inner
                .upsert_runners_entitlement(tenant_id, max_concurrency, max_vcpu_h, now_ms)
        }
        fn delete_runners_entitlement(&self, tenant_id: &str) -> Result<(), BillingD1Error> {
            self.inner.delete_runners_entitlement(tenant_id)
        }
    }

    #[test]
    fn subscription_deleted_takes_downgrade_path_not_grant_path() {
        // CAA-360 MEDIUM: a `customer.subscription.deleted` must drive the
        // DOWNGRADE path (which writes subscription_state='inactive'), NOT the
        // grant `upsert_tier` path (which hard-codes 'active' → contradictory
        // active-free row). We seed a starting tier via the inner mirror
        // directly (so that write is NOT counted against the recorder) then
        // assert the handler used downgrade_tier — never upsert_tier.
        let inner = Arc::new(InMemoryBillingD1::new());
        inner
            .upsert_tier("ten_1", "pro", 1_700_000_000_000, "init")
            .unwrap();
        let rec = Arc::new(TierPathRecordingD1::new(inner));
        let audit = Arc::new(InMemoryBillingAuditEmitter::new());
        let sel = Arc::new(InMemoryTierSelector::with_mapping(&[(
            "plan_pro",
            TierKind::Pro,
        )]));
        let handler = D1SubscriptionStateHandler::new(rec.clone(), audit.clone(), sel).with_clock(
            Arc::new(InMemoryFakeMatClock::at_unix_ms(1_700_000_000_000)),
        );

        let e = env(
            "evt_sd2",
            "customer.subscription.deleted",
            serde_json::json!({
                "object": {
                    "id": "sub_1",
                    "status": "canceled",
                    "metadata": { "tenant_id": "ten_1" },
                }
            }),
        );
        handler.on_subscription_deleted(&e).unwrap();

        // The cancel path drove downgrade_tier exactly once (→ free), and
        // NEVER the grant path (upsert_tier) — no 'active'-writing statement
        // was reached.
        let downgrades = rec.downgrade_calls.lock().unwrap().clone();
        let upserts = rec.upsert_calls.lock().unwrap().clone();
        assert_eq!(
            downgrades,
            vec!["free".to_string()],
            "cancel must drive downgrade_tier('free') exactly once"
        );
        assert!(
            upserts.is_empty(),
            "cancel must NOT drive the grant upsert_tier path: {upserts:?}"
        );
        // The handler still DRIVES downgrade_tier with tier_wire="free" (the
        // call-site argument, asserted above), but the tier column itself must
        // stay 'pro' — the underlying writer (mirroring SQL_DOWNGRADE_TIER)
        // preserves the existing row's tier rather than overwriting it.
        assert_eq!(rec.inner.tier_for("ten_1").as_deref(), Some("pro"));
        assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
    }

    #[test]
    fn invoice_paid_writes_audit_and_d1() {
        let (handler, d1, audit) = fixture();
        let e = env(
            "evt_ip",
            "invoice.paid",
            serde_json::json!({
                "object": {
                    "id": "in_1",
                    "metadata": { "tenant_id": "ten_1" },
                }
            }),
        );
        handler.on_invoice_paid(&e).unwrap();
        assert_eq!(d1.count_table("stripe_invoices"), 1);
        assert_eq!(
            audit.count_event("corelink.billing.invoice.materialized.v1"),
            1
        );
    }

    #[test]
    fn dispute_created_emits_sev1_audit() {
        let (handler, _d1, audit) = fixture();
        let e = env(
            "evt_dc",
            "charge.dispute.created",
            serde_json::json!({
                "object": {
                    "id": "dp_1",
                    "metadata": { "tenant_id": "ten_1" },
                }
            }),
        );
        handler.on_charge_dispute_created(&e).unwrap();
        let rows = audit.snapshot();
        assert!(rows.iter().any(|r| r.severity == AuditSeverity::Sev1));
    }

    #[test]
    fn audit_failure_aborts_d1_write_fail_closed() {
        let (handler, d1, audit) = fixture();
        audit.arm_failure(BillingAuditError::Transient("audit down".into()));
        let e = env(
            "evt_ip",
            "invoice.paid",
            serde_json::json!({
                "object": {
                    "id": "in_1",
                    "metadata": { "tenant_id": "ten_1" },
                }
            }),
        );
        let err = handler.on_invoice_paid(&e).unwrap_err();
        assert!(matches!(err, MaterializerError::Transient(_)));
        // No D1 row written.
        assert_eq!(d1.count_table("stripe_invoices"), 0);
    }

    #[test]
    fn missing_tenant_id_surfaces_invalid_payload() {
        let (handler, _d1, _audit) = fixture();
        let e = env(
            "evt_x",
            "invoice.paid",
            serde_json::json!({ "object": { "id": "in_1" } }),
        );
        let err = handler.on_invoice_paid(&e).unwrap_err();
        assert!(matches!(err, MaterializerError::InvalidPayload(_)));
    }

    #[test]
    fn unknown_plan_in_subscription_update_surfaces_invalid_payload() {
        let (handler, d1, _audit) = fixture();
        d1.upsert_tier("ten_1", "starter", 1_700_000_000_000, "init")
            .unwrap();
        let e = env(
            "evt_su",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_1",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_1" },
                    "plan": { "id": "plan_DOESNOTEXIST" },
                }
            }),
        );
        let err = handler.on_subscription_updated(&e).unwrap_err();
        assert!(matches!(err, MaterializerError::InvalidPayload(_)));
    }
}
