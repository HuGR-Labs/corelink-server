//! D1 writer trait + native in-memory mirror.
//!
//! The [`BillingD1Writer`] trait is the production seam: the wasm32
//! binder wraps `corelink-cf-bindings::CfD1DatabaseReal` and forwards
//! every method to a tenant-scoped prepared statement. Native CI / dev
//! use [`InMemoryBillingD1`] which mirrors the canonical
//! `INSERT … ON CONFLICT DO NOTHING` semantics so the materializer
//! contract tests pin the **behaviour** without standing up a D1
//! emulator (per `trait-abstraction-defer` charter).
//!
//! Every row carries the Stripe `event_id` plus the canonical tenant
//! id so the writer can refuse cross-tenant writes at the bind layer
//! (production: `CfD1DatabaseReal` ct-eq compares the first bound
//! parameter against the database's tenant-id).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

type RunnerFence = (String, u64, u64, String, bool);

/// Error category surfaced by [`BillingD1Writer`].
///
/// `Transient` flows back to the dispatcher as
/// [`corelink_stripe_real::webhook_dispatch::MaterializerError::Transient`]
/// (→ HTTP 500, Stripe retries). `InvalidPayload` flows back as
/// `MaterializerError::InvalidPayload` (→ HTTP 422, Stripe stops).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BillingD1Error {
    /// Transient backend error (D1 unreachable, mutex poisoned, etc.).
    Transient(String),
    /// Permanent shape error (required field missing / malformed).
    InvalidPayload(String),
}

impl fmt::Display for BillingD1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(s) => write!(f, "billing-d1 transient: {s}"),
            Self::InvalidPayload(s) => write!(f, "billing-d1 invalid payload: {s}"),
        }
    }
}

impl std::error::Error for BillingD1Error {}

/// Snapshot of a materialized D1 row. The materializer test suite
/// inspects this; the production binder discards it (the row lives in
/// D1 only).
///
/// The struct is `#[non_exhaustive]` so future column additions can
/// extend the canonical shape without breaking out-of-crate consumers.
/// Use [`MaterializedRow::new`] for constructing instances outside the
/// canonical materializer (e.g. wasm32 binder integration tests).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MaterializedRow {
    /// Canonical D1 table name (e.g. `stripe_subscriptions`).
    pub table: String,
    /// Tenant scope (matches the `tenant_id` column).
    pub tenant_id: String,
    /// Stripe-side primary identifier (e.g. `sub_…`, `in_…`, `dp_…`).
    pub stripe_id: String,
    /// Stripe event id this row was derived from (for forensics).
    pub stripe_event_id: String,
    /// Free-form JSON payload of the canonical column set. Production
    /// stores these as discrete columns; the in-memory mirror stores
    /// them as a single JSON blob so the materializer-shape tests can
    /// inspect the full column set without baking the schema into the
    /// trait surface.
    pub payload: serde_json::Value,
    /// Unix-ms when this row was materialized (writer-side clock).
    pub materialized_at_ms: u64,
}

impl MaterializedRow {
    /// Construct a row from its canonical column set. The constructor
    /// is the out-of-crate seam for [`MaterializedRow`] (the struct is
    /// `#[non_exhaustive]`, so consumers can't use brace-init).
    #[must_use]
    pub fn new(
        table: impl Into<String>,
        tenant_id: impl Into<String>,
        stripe_id: impl Into<String>,
        stripe_event_id: impl Into<String>,
        payload: serde_json::Value,
        materialized_at_ms: u64,
    ) -> Self {
        Self {
            table: table.into(),
            tenant_id: tenant_id.into(),
            stripe_id: stripe_id.into(),
            stripe_event_id: stripe_event_id.into(),
            payload,
            materialized_at_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical billing SQL literals — SINGLE SOURCE OF TRUTH.
//
// Reconciled COLUMN-BY-COLUMN against the DEPLOYED schema
// (`migrations/d1/0048_stripe_billing_materializer.sql` +
// `0044_stripe_webhook_events_processed.sql`). BOTH production writers
// transcribe these EXACT strings so the two targets can never drift:
//
//   * wasm32 CF Worker — `crate::wasm32_binders::CfD1BillingWriter`
//     (validates the shape via `CfD1DatabaseReal::scoped_query`; the
//     actual `worker::D1Database` call runs one frame above).
//   * native container — `corelink-container::billing_d1_http::D1HttpBillingWriter`
//     (executes them over the CF D1 REST API via `D1HttpClient::query`).
//
// Invariants baked in here (do NOT regress — the deployed schema is the
// authority):
//   * the JSON payload column is `payload_json` (NOT `payload`);
//   * the `ON CONFLICT` target is each table's single natural PRIMARY
//     KEY (the per-table Stripe id), not a composite `(tenant_id, …)`;
//   * `tenant_id` is the FIRST bound parameter on every TENANTED table
//     so the wasm32 `verify_first_bind` ct-eq probe and the native
//     positional bind agree;
//   * NOT-NULL-no-default columns are bound explicitly
//     (`stripe_subscriptions.status`, `stripe_invoices.outcome`);
//     DEFAULTed `severity` / `schema_version` are omitted.
//
// Native positional binds, in `?1..?n` order (kept in lockstep with the
// native writer):
//   CUSTOMER:     tenant_id, stripe_id, stripe_event_id, materialized_at_ms, payload_json
//   SUBSCRIPTION: tenant_id, stripe_id, stripe_event_id, status, materialized_at_ms, payload_json
//   CANCEL:       payload_json, materialized_at_ms, tenant_id, stripe_id
//   INVOICE:      tenant_id, stripe_id, stripe_event_id, outcome, materialized_at_ms, payload_json
//   DISPUTE:      tenant_id, stripe_id, stripe_event_id, materialized_at_ms, payload_json
//   REFUND:       tenant_id, stripe_charge_id, stripe_event_id, materialized_at_ms, payload_json
//   WEBHOOK:      event_id, event_type, processed_at_ms, ('dispatched' literal), correlation_id
// ---------------------------------------------------------------------------

/// `customer.created` → `stripe_customers` UPSERT (PK `stripe_customer_id`).
pub const SQL_UPSERT_CUSTOMER: &str = "INSERT INTO stripe_customers (tenant_id, stripe_customer_id, stripe_event_id, materialized_at_ms, payload_json) VALUES (?, ?, ?, ?, ?) ON CONFLICT(stripe_customer_id) DO UPDATE SET payload_json = excluded.payload_json, materialized_at_ms = excluded.materialized_at_ms";
/// `customer.subscription.created|updated` → `stripe_subscriptions`
/// UPSERT (PK `stripe_subscription_id`; binds the NOT-NULL `status`).
pub const SQL_UPSERT_SUBSCRIPTION: &str = "INSERT INTO stripe_subscriptions (tenant_id, stripe_subscription_id, stripe_event_id, status, materialized_at_ms, payload_json) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(stripe_subscription_id) DO UPDATE SET status = excluded.status, payload_json = excluded.payload_json, materialized_at_ms = excluded.materialized_at_ms";
/// `customer.subscription.deleted` → set `status='canceled'` on the
/// existing subscription row (tenant-scoped UPDATE).
pub const SQL_MARK_SUBSCRIPTION_CANCELED: &str = "UPDATE stripe_subscriptions SET status = 'canceled', payload_json = ?, materialized_at_ms = ? WHERE tenant_id = ? AND stripe_subscription_id = ?";
/// `invoice.paid|payment_failed` → `stripe_invoices` UPSERT
/// (PK `stripe_invoice_id`; binds the NOT-NULL CHECKed `outcome`).
pub const SQL_UPSERT_INVOICE: &str = "INSERT INTO stripe_invoices (tenant_id, stripe_invoice_id, stripe_event_id, outcome, materialized_at_ms, payload_json) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(stripe_invoice_id) DO UPDATE SET outcome = excluded.outcome, payload_json = excluded.payload_json, materialized_at_ms = excluded.materialized_at_ms";
/// `charge.dispute.created` → `stripe_disputes` INSERT (PK
/// `stripe_dispute_id`; DEFAULTed `severity`/`schema_version` omitted).
pub const SQL_INSERT_DISPUTE: &str = "INSERT INTO stripe_disputes (tenant_id, stripe_dispute_id, stripe_event_id, materialized_at_ms, payload_json) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING";
/// `charge.refunded` → `stripe_refunds` INSERT. The PRIMARY KEY is the
/// parent `stripe_charge_id` (refunds are addressed via the charge in
/// webhook deliveries) — there is NO `stripe_refund_id` column.
pub const SQL_INSERT_REFUND: &str = "INSERT INTO stripe_refunds (tenant_id, stripe_charge_id, stripe_event_id, materialized_at_ms, payload_json) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING";
/// Resolve the subscription recorded on an already-materialized invoice.  A
/// charge refund only carries the invoice id, while the invoice is the durable
/// Stripe object that carries the subscription relationship.
pub const SQL_FIND_INVOICE_SUBSCRIPTION: &str = "SELECT json_extract(payload_json, '$.stripe_subscription_id') AS stripe_subscription_id FROM stripe_invoices WHERE tenant_id = ? AND stripe_invoice_id = ? LIMIT 1";
/// Resolve a subscription against the two durable purchase maps.  Returning
/// more than one row is deliberately treated as ambiguous by the writer.
pub const SQL_RESOLVE_REFUND_PRODUCT: &str = "SELECT 'runners' AS product_axis FROM runner_billing WHERE tenant_id = ? AND runner_subscription_id = ? UNION ALL SELECT 'cache' AS product_axis FROM tenant_billing WHERE tenant_id = ? AND stripe_subscription_id = ? LIMIT 2";
/// Idempotency dedup INSERT into `stripe_webhook_events_processed`
/// (migration 0044). This table is UN-tenanted — its PRIMARY KEY is the
/// globally-unique Stripe `event_id`; there is NO `tenant_id` column.
/// `outcome` is a BOUND parameter (see [`WebhookOutcome`] — the CHECK
/// admits `dispatched`/`acknowledged_unknown`, and the caller supplies
/// whichever actually happened) and the Stripe `event_id` doubles as
/// the NOT-NULL `correlation_id` (the sync trait carries neither).
/// `RETURNING event_id` lets the native writer detect insert-vs-ignore
/// (mirrors the tier-select lock probe).
pub const SQL_INSERT_WEBHOOK_EVENT_PROCESSED: &str = "INSERT INTO stripe_webhook_events_processed (event_id, event_type, processed_at_ms, outcome, correlation_id) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING RETURNING event_id";

/// Outcome bucket for a webhook dedup row (`stripe_webhook_events_processed.outcome`,
/// migration `0044`). The CHECK constraint admits exactly these two
/// strings — [`Self::as_str`] MUST keep returning them verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WebhookOutcome {
    /// The dispatcher acted on the event (a known, handled canonical
    /// event type).
    Dispatched,
    /// The event type was outside the canonical 10-element taxonomy
    /// ([`corelink_billing_stripe_traits::CanonicalWebhookEventType::Unknown`]) —
    /// acknowledged for forward-compat but not acted on.
    AcknowledgedUnknown,
}

/// The product axis durably identified for a refunded subscription.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RefundedProduct {
    /// Cache subscription recorded in `tenant_billing`.
    Cache,
    /// Runners subscription recorded in `runner_billing`.
    Runners,
}

impl RefundedProduct {
    /// Stable audit payload value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Runners => "runners",
        }
    }
}

/// A product-axis decision paired with the durable Stripe subscription id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefundedPurchase {
    /// The product axis selected from the purchase maps.
    pub product: RefundedProduct,
    /// The Stripe subscription whose invoice was refunded.
    pub stripe_subscription_id: String,
}

impl WebhookOutcome {
    /// The exact CHECK-admitted SQL literal for this outcome.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dispatched => "dispatched",
            Self::AcknowledgedUnknown => "acknowledged_unknown",
        }
    }
}

/// Result of a durable entitlement CAS operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntitlementCasOutcome {
    /// The provider key advanced the fence and the requested state was applied.
    Applied,
    /// The same provider key was replayed; the requested state is already
    /// owned by this key and may be safely repeated.
    Duplicate,
    /// The provider key was older than the durable fence and no mutation ran.
    Stale,
}
/// Read the current tier for a tenant (`tier_selections`, migration
/// 0039). Bind `?1` = `tenant_id`. Already schema-correct (the #172 fix).
pub const SQL_READ_TIER: &str = "SELECT tier FROM tier_selections WHERE tenant_id = ?";
/// UPSERT a tenant's tier → `subscription_state='active'` with the
/// activation timestamp bound to `subscription_started_at_ms` (the
/// `subscription_started_when_active` CHECK requires it NOT NULL when
/// active). Binds `?1..?4` = (tenant_id, tier, subscription_started_at_ms,
/// correlation_id). Already schema-correct (the #172 fix).
///
/// **SQL-level cancel guard (mirrors the signup-worker authority).** The DO
/// UPDATE carries a `WHERE NOT EXISTS` guard on `tenant_billing.status =
/// 'canceled'`, matching the PRIMARY writer's
/// `apps/signup-worker/src/webhooks/stripe.ts`
/// (`"must NOT resurrect a canceled subscription"`,
/// `AND status != 'canceled'`). Without it, a STALE
/// `customer.subscription.updated(status=active)` redelivery landing AFTER
/// the `customer.subscription.deleted` was processed would re-grant access
/// to a canceled tenant. The guard is CORRELATED on the conflicting row's
/// own `tier_selections.tenant_id` — it adds NO bind placeholder, keeping
/// the arity pinned at 4 (see `tier_sql_unchanged_and_schema_correct_0039`).
///
/// Semantics preserved by the guard:
/// - no `tenant_billing` row → `NOT EXISTS` true → grant proceeds (first
///   purchase never blocked);
/// - the INSERT branch (no prior `tier_selections` row) is untouched by a
///   `DO UPDATE WHERE`;
/// - [`SQL_DOWNGRADE_TIER`] (cancel path) writes `'inactive'` and never
///   deletes, so "row exists" always holds for tenants that ever had a tier.
///
/// KNOWN + ACCEPTED residual: because the guard cannot apply to the INSERT
/// branch, an INSERT over a tenant whose `tenant_billing.status='canceled'`
/// but which has NO `tier_selections` row still grants. This state is not
/// reachable through the cancel flow (cancellation downgrades to
/// `'inactive'` rather than deleting), so "no row" means "never had a
/// tier", not "was canceled". Pinned behaviorally by
/// `upsert_tier_insert_branch_is_not_gated_known_residual`; do NOT close by
/// rewriting as `INSERT … SELECT … WHERE NOT EXISTS` — that breaks the ON
/// CONFLICT semantics and the 4-bind arity.
pub const SQL_UPSERT_TIER: &str = "INSERT INTO tier_selections (tenant_id, tier, subscription_state, subscription_started_at_ms, correlation_id) VALUES (?, ?, 'active', ?, ?) ON CONFLICT(tenant_id) DO UPDATE SET tier = excluded.tier, subscription_state = 'active', subscription_started_at_ms = excluded.subscription_started_at_ms, correlation_id = excluded.correlation_id WHERE NOT EXISTS (SELECT 1 FROM tenant_billing tb WHERE tb.tenant_id = tier_selections.tenant_id AND tb.status = 'canceled')";
/// DOWNGRADE a tenant's tier → `subscription_state='inactive'` (the
/// `customer.subscription.deleted` cancel path). Binds `?1..?4` =
/// (tenant_id, tier, subscription_started_at_ms, correlation_id).
///
/// The **signup-worker** (`apps/signup-worker/src/webhooks/stripe.ts`) is the
/// PRIMARY, authoritative downgrade authority on cancel — it is the writer that
/// flips the canonical `tier_selections.subscription_state` access gate off.
/// This statement is the container materializer's DEFENSE-IN-DEPTH convergent
/// write: as a SECOND writer of that gate, on cancel it must never write the
/// contradictory `subscription_state='active'` that [`SQL_UPSERT_TIER`] hard-codes
/// — it writes `'inactive'` so a canceled tenant converges to an access-OFF row
/// regardless of which writer wins the race.
///
/// The `subscription_state` CHECK admits `('inactive','pending_checkout',
/// 'active')` and the `subscription_started_when_active` CHECK only requires
/// `subscription_started_at_ms` NOT NULL when state=`active` — so an `'inactive'`
/// row needs no started_at, and the DO UPDATE **deliberately does NOT reset**
/// `subscription_started_at_ms` (a downgrade must preserve the ORIGINAL
/// subscription start; only [`SQL_UPSERT_TIER`]'s grant path refreshes it).
///
/// The DO UPDATE **deliberately does NOT touch `tier` either.** The
/// signup-worker (`apps/signup-worker/src/webhooks/stripe.ts`,
/// `deactivateTierSelectionBySubscription`) is the authoritative writer on
/// cancel and its own statement leaves `tier` untouched, preserving the
/// tenant's historical tier label. This statement must converge to the SAME
/// outcome — overwriting `tier` to `'free'` here would race the authority and
/// non-deterministically stomp the historical label depending on which writer
/// lands last. `tier` is still bound (`?2`) for the INSERT branch, which only
/// fires if no row exists yet (the NOT-NULL column needs a value); on the
/// far-more-common conflict path (a row the signup-worker or a prior
/// tier-select already created) the existing `tier` value survives untouched.
pub const SQL_DOWNGRADE_TIER: &str = "INSERT INTO tier_selections (tenant_id, tier, subscription_state, subscription_started_at_ms, correlation_id) VALUES (?, ?, 'inactive', ?, ?) ON CONFLICT(tenant_id) DO UPDATE SET subscription_state = 'inactive', correlation_id = excluded.correlation_id";

/// UPSERT a tenant's Runners entitlement (`runners_entitlement`, migrations
/// 0070 `max_concurrency` + 0072 `max_vcpu_h`). A SEPARATE axis from the cache
/// tier — seeded when a Runners-tier Stripe subscription activates. Binds
/// `?1..?4` = (tenant_id, max_concurrency, created_at_ms, max_vcpu_h); `plan` is
/// the literal `'runners'` marker. `max_concurrency` has a `CHECK > 0` (the
/// caller only seeds known tiers, all > 0). On re-purchase/upgrade the cap +
/// ceiling are overwritten; `created_at_ms` is preserved on conflict.
pub const SQL_UPSERT_RUNNERS_ENTITLEMENT: &str = "INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h) VALUES (?, ?, 'runners', ?, ?) ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency = excluded.max_concurrency, max_vcpu_h = excluded.max_vcpu_h";

/// REVOKE a tenant's Runners entitlement (`runners_entitlement`). The symmetric
/// twin of [`SQL_UPSERT_RUNNERS_ENTITLEMENT`] — deletes the tenant's row when a
/// Runners-tier Stripe subscription reaches a NON-granting status (`past_due`,
/// `unpaid`, `paused`, `canceled`, …) or is `customer.subscription.deleted`, so
/// the container materializer never leaves a stale entitlement granted. Binds
/// `?1` = `tenant_id`. Idempotent (a DELETE of a non-existent row is a no-op).
/// The **signup-worker** is the PRIMARY authority; this is the container's
/// defense-in-depth convergent revoke (symmetric to how it seeds).
pub const SQL_DELETE_RUNNERS_ENTITLEMENT: &str =
    "DELETE FROM runners_entitlement WHERE tenant_id = ?";

/// Advance the durable Runner entitlement fence for a newer snapshot of the
/// same subscription. A different subscription id is a replacement boundary:
/// only a successor grant may replace a predecessor revoke. This keeps a
/// predecessor cancellation from revoking a successor entitlement when Stripe
/// replaces a subscription within one billing period. The fence row is
/// retained after revoke so an old grant cannot be reinserted after restart.
pub const SQL_ADVANCE_RUNNER_ENTITLEMENT_FENCE: &str = "INSERT INTO runner_entitlement_reconcile_fence (tenant_id, stripe_subscription_id, subscription_created_at_ms, stripe_event_created_at_ms, stripe_event_id, is_granting, applied_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(tenant_id) DO UPDATE SET stripe_subscription_id = excluded.stripe_subscription_id, subscription_created_at_ms = excluded.subscription_created_at_ms, stripe_event_created_at_ms = excluded.stripe_event_created_at_ms, stripe_event_id = excluded.stripe_event_id, is_granting = excluded.is_granting, applied_at_ms = excluded.applied_at_ms WHERE excluded.subscription_created_at_ms > runner_entitlement_reconcile_fence.subscription_created_at_ms OR (excluded.subscription_created_at_ms = runner_entitlement_reconcile_fence.subscription_created_at_ms AND excluded.stripe_subscription_id > runner_entitlement_reconcile_fence.stripe_subscription_id) OR (excluded.subscription_created_at_ms = runner_entitlement_reconcile_fence.subscription_created_at_ms AND excluded.stripe_subscription_id = runner_entitlement_reconcile_fence.stripe_subscription_id AND excluded.stripe_event_created_at_ms > runner_entitlement_reconcile_fence.stripe_event_created_at_ms) OR (excluded.subscription_created_at_ms = runner_entitlement_reconcile_fence.subscription_created_at_ms AND excluded.stripe_subscription_id = runner_entitlement_reconcile_fence.stripe_subscription_id AND excluded.stripe_event_created_at_ms = runner_entitlement_reconcile_fence.stripe_event_created_at_ms AND excluded.stripe_event_id > runner_entitlement_reconcile_fence.stripe_event_id) RETURNING stripe_subscription_id";
/// Apply a grant only when this operation owns the current durable fence row.
pub const SQL_CAS_UPSERT_RUNNERS_ENTITLEMENT: &str = "INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h) SELECT ?, ?, 'runners', ?, ? WHERE EXISTS (SELECT 1 FROM runner_entitlement_reconcile_fence WHERE tenant_id = ? AND stripe_subscription_id = ? AND subscription_created_at_ms = ? AND stripe_event_created_at_ms = ? AND stripe_event_id = ?) ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency = excluded.max_concurrency, max_vcpu_h = excluded.max_vcpu_h";
/// Apply a revoke only when this operation owns the current durable fence row.
pub const SQL_CAS_DELETE_RUNNERS_ENTITLEMENT: &str = "DELETE FROM runners_entitlement WHERE tenant_id = ? AND EXISTS (SELECT 1 FROM runner_entitlement_reconcile_fence WHERE tenant_id = ? AND stripe_subscription_id = ? AND subscription_created_at_ms = ? AND stripe_event_created_at_ms = ? AND stripe_event_id = ?)";
/// Read back the fence in the same D1 transaction to classify equal-key,
/// equal-identity retries (idempotent duplicate) versus a rejected operation.
pub const SQL_READ_RUNNER_ENTITLEMENT_FENCE: &str =
    "SELECT stripe_subscription_id, subscription_created_at_ms, stripe_event_created_at_ms, stripe_event_id FROM runner_entitlement_reconcile_fence WHERE tenant_id = ?";

/// Canonical billing-D1 writer trait.
///
/// All write methods are **idempotent**: calling the same method twice
/// with the same `stripe_event_id` MUST result in at most one row
/// mutation (mirrors the D1 `INSERT … ON CONFLICT DO NOTHING` semantics
/// + the dispatcher's BLAKE3 dedup gate).
pub trait BillingD1Writer: fmt::Debug + Send + Sync {
    /// `customer.created` → `stripe_customers` UPSERT.
    fn upsert_customer(&self, row: MaterializedRow) -> Result<(), BillingD1Error>;
    /// `customer.subscription.created` / `updated` →
    /// `stripe_subscriptions` UPSERT.
    fn upsert_subscription(&self, row: MaterializedRow) -> Result<(), BillingD1Error>;
    /// `customer.subscription.deleted` → mark the row deleted
    /// (canonical state row preserved for audit; row mutates the
    /// `status` column to `canceled`).
    fn mark_subscription_canceled(&self, row: MaterializedRow) -> Result<(), BillingD1Error>;
    /// `invoice.paid` / `invoice.payment_failed` →
    /// `stripe_invoices` UPSERT.
    fn upsert_invoice(&self, row: MaterializedRow) -> Result<(), BillingD1Error>;
    /// `charge.dispute.created` → `stripe_disputes` INSERT.
    fn insert_dispute(&self, row: MaterializedRow) -> Result<(), BillingD1Error>;
    /// `charge.refunded` → `stripe_refunds` INSERT.
    fn insert_refund(&self, row: MaterializedRow) -> Result<(), BillingD1Error>;

    /// Mark a Stripe event id as processed (idempotency dedup table).
    /// Returns `Ok(true)` iff the row was created; `Ok(false)` iff a
    /// row with the same `stripe_event_id` already existed.
    ///
    /// `outcome` is bound into the `outcome` column verbatim (see
    /// [`WebhookOutcome`]) — the caller decides `Dispatched` vs
    /// `AcknowledgedUnknown`, this seam only persists it.
    ///
    /// Mirrors `INSERT OR IGNORE` against
    /// `stripe_webhook_events_processed` from migration `0044`.
    fn try_record_event(
        &self,
        stripe_event_id: &str,
        canonical_event_type: &str,
        now_ms: u64,
        outcome: WebhookOutcome,
    ) -> Result<bool, BillingD1Error>;

    /// Read the current tier for `tenant_id` (from the
    /// `tier_selections` table); `Ok(None)` iff no row exists yet.
    fn read_tier(&self, tenant_id: &str) -> Result<Option<String>, BillingD1Error>;

    /// UPSERT the tier for `tenant_id`. Production writes
    /// `tier_selections.(tier, subscription_state='active',
    /// subscription_started_at_ms=now_ms, correlation_id)`; the native
    /// mirror stores the value verbatim.
    ///
    /// `now_ms` is the activation timestamp (ms since epoch) bound to
    /// `subscription_started_at_ms`. The production statement writes
    /// `subscription_state = 'active'`, and the
    /// `subscription_started_when_active` table CHECK in migration
    /// `0039` requires `subscription_started_at_ms` NOT NULL whenever the
    /// state is `active` — so `now_ms` MUST be supplied.
    fn upsert_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error>;

    /// DOWNGRADE the tier for `tenant_id` on cancel. Production writes
    /// `tier_selections.(subscription_state='inactive', correlation_id)` via
    /// [`SQL_DOWNGRADE_TIER`] — the container's defense-in-depth convergent
    /// write for `customer.subscription.deleted` (the signup-worker is the
    /// primary downgrade authority). UNLIKE [`Self::upsert_tier`], this writes
    /// `subscription_state='inactive'` (the access gate OFF), does NOT reset
    /// `subscription_started_at_ms` on conflict (the original start is
    /// preserved), and — matching the signup-worker's own cancel statement —
    /// does NOT overwrite `tier` on conflict either, preserving the tenant's
    /// historical tier label rather than stomping it to `tier_wire`. `tier_wire`
    /// is only used if the INSERT branch fires (no existing row — the NOT-NULL
    /// column needs a value); `now_ms` is threaded for that same INSERT-branch
    /// started_at bind + trait parity.
    fn downgrade_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error>;

    /// Resolve a subscription id through the durable Cache/Runners purchase
    /// maps. `None` means unknown or ambiguous ownership and MUST NOT select a
    /// product by tenant identity alone.
    fn resolve_refunded_purchase_by_subscription(
        &self,
        _tenant_id: &str,
        _stripe_subscription_id: &str,
    ) -> Result<Option<RefundedPurchase>, BillingD1Error> {
        Ok(None)
    }

    /// Resolve a charge's invoice id through the materialized invoice and then
    /// the durable purchase maps. Historical or out-of-order invoices may have
    /// no usable relationship; those stay pending attribution.
    fn resolve_refunded_purchase_by_invoice(
        &self,
        _tenant_id: &str,
        _stripe_invoice_id: &str,
    ) -> Result<Option<RefundedPurchase>, BillingD1Error> {
        Ok(None)
    }

    /// Atomically compare the provider authority key and mutate the Runner
    /// entitlement. Every production writer implements this transaction; there
    /// is deliberately no unfenced default writer.
    fn cas_runners_entitlement(
        &self,
        tenant_id: &str,
        _subscription_id: &str,
        _subscription_created_at_ms: u64,
        _stripe_event_created_at_ms: u64,
        _stripe_event_id: &str,
        entitlement: Option<(u32, u32)>,
        now_ms: i64,
    ) -> Result<EntitlementCasOutcome, BillingD1Error> {
        let _ = (tenant_id, entitlement, now_ms);
        Err(BillingD1Error::Transient(
            "runner entitlement CAS is required for this writer".to_owned(),
        ))
    }
}

/// Native in-memory mirror. Stores every materialized row + every
/// dedup token. The materializer test suite inspects the snapshot to
/// pin the schema; production binders forward to the wasm32 D1 binding.
#[derive(Clone, Debug, Default)]
pub struct InMemoryBillingD1 {
    rows: Arc<Mutex<Vec<MaterializedRow>>>,
    /// `stripe_event_id` → canonical event type (mirrors
    /// `stripe_webhook_events_processed` PRIMARY KEY).
    events_seen: Arc<Mutex<HashMap<String, (String, u64)>>>,
    /// `stripe_event_id` → recorded [`WebhookOutcome`] (mirrors the
    /// `outcome` column so tests can assert what was actually stored,
    /// not just that a row exists).
    outcomes: Arc<Mutex<HashMap<String, WebhookOutcome>>>,
    /// `tenant_id` → (`tier_wire`, `correlation_id`).
    tiers: Arc<Mutex<HashMap<String, (String, String)>>>,
    /// `tenant_id` → (`max_concurrency`, `max_vcpu_h`) — the Runners
    /// entitlement mirror (`runners_entitlement`).
    runners: Arc<Mutex<HashMap<String, (u32, u32)>>>,
    /// (`tenant_id`, `subscription_id`) → purchased product. Test-only mirror
    /// of `tenant_billing` and `runner_billing` used by refund routing tests.
    refund_purchases: Arc<Mutex<HashMap<(String, String), RefundedProduct>>>,
    /// `tenant_id` → provider identity, replacement generation, revision, and grant state.
    /// mirror used by native concurrency tests.
    runner_fences: Arc<Mutex<HashMap<String, RunnerFence>>>,
    /// If set, every write returns this error (drives fail-CLOSED tests).
    fail_with: Arc<Mutex<Option<BillingD1Error>>>,
}

impl InMemoryBillingD1 {
    /// Construct an empty mirror.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Force every subsequent write to fail with `err` (cleared once
    /// the suite calls `clear_failure`).
    pub fn arm_failure(&self, err: BillingD1Error) {
        if let Ok(mut g) = self.fail_with.lock() {
            *g = Some(err);
        }
    }

    /// Clear the armed failure (returns the in-flight value if any).
    pub fn clear_failure(&self) -> Option<BillingD1Error> {
        match self.fail_with.lock() {
            Ok(mut g) => g.take(),
            Err(p) => p.into_inner().take(),
        }
    }

    /// Snapshot every materialized row in insertion order.
    #[must_use]
    pub fn snapshot(&self) -> Vec<MaterializedRow> {
        match self.rows.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Current Runners entitlement for a tenant, `(max_concurrency, max_vcpu_h)`,
    /// or `None` if none seeded. Test inspection of `upsert_runners_entitlement`.
    #[must_use]
    pub fn runners_entitlement_of(&self, tenant_id: &str) -> Option<(u32, u32)> {
        match self.runners.lock() {
            Ok(g) => g.get(tenant_id).copied(),
            Err(p) => p.into_inner().get(tenant_id).copied(),
        }
    }

    /// Read the tenant's durable entitlement fence for acceptance tests.
    #[must_use]
    pub fn runner_fence_of(&self, tenant_id: &str) -> Option<(String, u64, u64, String, bool)> {
        match self.runner_fences.lock() {
            Ok(g) => g.get(tenant_id).cloned(),
            Err(p) => p.into_inner().get(tenant_id).cloned(),
        }
    }

    /// Count rows whose `table` matches `table_name`.
    #[must_use]
    pub fn count_table(&self, table_name: &str) -> usize {
        self.snapshot()
            .iter()
            .filter(|r| r.table == table_name)
            .count()
    }

    /// Number of distinct Stripe event ids recorded in the dedup mirror.
    #[must_use]
    pub fn distinct_events(&self) -> usize {
        match self.events_seen.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Read the recorded [`WebhookOutcome`] for `stripe_event_id` (None
    /// iff never recorded). Mirrors [`Self::tier_for`]'s naming.
    #[must_use]
    pub fn outcome_for(&self, stripe_event_id: &str) -> Option<WebhookOutcome> {
        match self.outcomes.lock() {
            Ok(g) => g.get(stripe_event_id).copied(),
            Err(p) => p.into_inner().get(stripe_event_id).copied(),
        }
    }

    /// Read the recorded tier for `tenant_id` (None iff never written).
    #[must_use]
    pub fn tier_for(&self, tenant_id: &str) -> Option<String> {
        match self.tiers.lock() {
            Ok(g) => g.get(tenant_id).map(|(tier, _)| tier.clone()),
            Err(p) => p.into_inner().get(tenant_id).map(|(tier, _)| tier.clone()),
        }
    }

    /// Seed a durable Cache purchase mapping for a hermetic refund test.
    pub fn record_cache_purchase(&self, tenant_id: &str, stripe_subscription_id: &str) {
        if let Ok(mut g) = self.refund_purchases.lock() {
            g.insert(
                (tenant_id.to_owned(), stripe_subscription_id.to_owned()),
                RefundedProduct::Cache,
            );
        }
    }

    /// Seed a durable Runners purchase mapping for a hermetic refund test.
    pub fn record_runners_purchase(&self, tenant_id: &str, stripe_subscription_id: &str) {
        if let Ok(mut g) = self.refund_purchases.lock() {
            g.insert(
                (tenant_id.to_owned(), stripe_subscription_id.to_owned()),
                RefundedProduct::Runners,
            );
        }
    }

    /// Test-only fixture setup that bypasses production writer wiring.
    pub fn upsert_runners_entitlement(
        &self,
        tenant_id: &str,
        max_concurrency: u32,
        max_vcpu_h: u32,
        _now_ms: i64,
    ) -> Result<(), BillingD1Error> {
        self.check_armed()?;
        let mut g = self
            .runners
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("runners mutex poisoned: {e}")))?;
        g.insert(tenant_id.to_string(), (max_concurrency, max_vcpu_h));
        Ok(())
    }

    /// Test-only fixture cleanup that bypasses production writer wiring.
    pub fn delete_runners_entitlement(&self, tenant_id: &str) -> Result<(), BillingD1Error> {
        self.check_armed()?;
        let mut g = self
            .runners
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("runners mutex poisoned: {e}")))?;
        g.remove(tenant_id);
        Ok(())
    }

    fn check_armed(&self) -> Result<(), BillingD1Error> {
        let guard = self
            .fail_with
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("mutex poisoned: {e}")))?;
        match guard.as_ref() {
            Some(err) => Err(err.clone()),
            None => Ok(()),
        }
    }

    fn push_row(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.check_armed()?;
        let mut g = self
            .rows
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("rows mutex poisoned: {e}")))?;
        g.push(row);
        Ok(())
    }
}

impl BillingD1Writer for InMemoryBillingD1 {
    fn upsert_customer(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        debug_table(&row, "stripe_customers")?;
        self.push_row(row)
    }
    fn upsert_subscription(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        debug_table(&row, "stripe_subscriptions")?;
        self.push_row(row)
    }
    fn mark_subscription_canceled(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        debug_table(&row, "stripe_subscriptions")?;
        self.push_row(row)
    }
    fn upsert_invoice(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        debug_table(&row, "stripe_invoices")?;
        self.push_row(row)
    }
    fn insert_dispute(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        debug_table(&row, "stripe_disputes")?;
        self.push_row(row)
    }
    fn insert_refund(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        debug_table(&row, "stripe_refunds")?;
        self.push_row(row)
    }

    fn try_record_event(
        &self,
        stripe_event_id: &str,
        canonical_event_type: &str,
        now_ms: u64,
        outcome: WebhookOutcome,
    ) -> Result<bool, BillingD1Error> {
        self.check_armed()?;
        let mut g = self
            .events_seen
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("events mutex poisoned: {e}")))?;
        if g.contains_key(stripe_event_id) {
            Ok(false)
        } else {
            g.insert(
                stripe_event_id.to_string(),
                (canonical_event_type.to_string(), now_ms),
            );
            let mut outcomes = self
                .outcomes
                .lock()
                .map_err(|e| BillingD1Error::Transient(format!("outcomes mutex poisoned: {e}")))?;
            outcomes.insert(stripe_event_id.to_string(), outcome);
            Ok(true)
        }
    }

    fn read_tier(&self, tenant_id: &str) -> Result<Option<String>, BillingD1Error> {
        self.check_armed()?;
        let g = self
            .tiers
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("tiers mutex poisoned: {e}")))?;
        Ok(g.get(tenant_id).map(|(tier, _)| tier.clone()))
    }

    fn upsert_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        // Activation timestamp bound to `subscription_started_at_ms` in
        // the production statement. The native mirror keeps its existing
        // `(tier, correlation_id)` shape (no behavior regression); the
        // arg is threaded for trait parity with the wasm32 binder.
        _now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error> {
        self.check_armed()?;
        let mut g = self
            .tiers
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("tiers mutex poisoned: {e}")))?;
        g.insert(
            tenant_id.to_string(),
            (tier_wire.to_string(), correlation_id.to_string()),
        );
        Ok(())
    }

    fn downgrade_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        // Bound to `subscription_started_at_ms` on the INSERT (new-row) path in
        // the production statement; the DO UPDATE does NOT touch it. The native
        // mirror keeps its `(tier, correlation_id)` shape (parity is enough).
        _now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error> {
        self.check_armed()?;
        let mut g = self
            .tiers
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("tiers mutex poisoned: {e}")))?;
        // Mirror `SQL_DOWNGRADE_TIER`'s ON CONFLICT: an EXISTING row keeps its
        // `tier` untouched (only `correlation_id` refreshes) — `tier_wire` is
        // used ONLY for the INSERT (new-row) branch, matching the production
        // statement not overwriting the historical tier label on cancel.
        match g.get_mut(tenant_id) {
            Some((_existing_tier, existing_correlation_id)) => {
                *existing_correlation_id = correlation_id.to_string();
            }
            None => {
                g.insert(
                    tenant_id.to_string(),
                    (tier_wire.to_string(), correlation_id.to_string()),
                );
            }
        }
        Ok(())
    }

    fn resolve_refunded_purchase_by_subscription(
        &self,
        tenant_id: &str,
        stripe_subscription_id: &str,
    ) -> Result<Option<RefundedPurchase>, BillingD1Error> {
        self.check_armed()?;
        let g = self.refund_purchases.lock().map_err(|e| {
            BillingD1Error::Transient(format!("refund purchases mutex poisoned: {e}"))
        })?;
        Ok(
            g.get(&(tenant_id.to_owned(), stripe_subscription_id.to_owned()))
                .copied()
                .map(|product| RefundedPurchase {
                    product,
                    stripe_subscription_id: stripe_subscription_id.to_owned(),
                }),
        )
    }

    fn resolve_refunded_purchase_by_invoice(
        &self,
        tenant_id: &str,
        stripe_invoice_id: &str,
    ) -> Result<Option<RefundedPurchase>, BillingD1Error> {
        self.check_armed()?;
        let subscription_id = self.snapshot().into_iter().rev().find_map(|row| {
            (row.table == "stripe_invoices"
                && row.tenant_id == tenant_id
                && row.stripe_id == stripe_invoice_id)
                .then(|| {
                    row.payload
                        .get("stripe_subscription_id")?
                        .as_str()
                        .map(str::to_owned)
                })
                .flatten()
        });
        match subscription_id {
            Some(subscription_id) => {
                self.resolve_refunded_purchase_by_subscription(tenant_id, &subscription_id)
            }
            None => Ok(None),
        }
    }

    fn cas_runners_entitlement(
        &self,
        tenant_id: &str,
        subscription_id: &str,
        subscription_created_at_ms: u64,
        stripe_event_created_at_ms: u64,
        stripe_event_id: &str,
        entitlement: Option<(u32, u32)>,
        _now_ms: i64,
    ) -> Result<EntitlementCasOutcome, BillingD1Error> {
        self.check_armed()?;
        if tenant_id.trim().is_empty()
            || subscription_id.trim().is_empty()
            || subscription_created_at_ms == 0
            || stripe_event_created_at_ms == 0
            || stripe_event_id.trim().is_empty()
        {
            return Err(BillingD1Error::InvalidPayload(
                "runner entitlement CAS requires non-empty identity and provider timestamps"
                    .to_owned(),
            ));
        }
        if entitlement.is_some_and(|(max_concurrency, _)| max_concurrency == 0) {
            return Err(BillingD1Error::InvalidPayload(
                "runner entitlement CAS max_concurrency must be > 0".to_owned(),
            ));
        }
        let mut fences = self
            .runner_fences
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("runner fence mutex poisoned: {e}")))?;
        // Provider facts form one total order: creation generation, immutable
        // subscription identity, provider event time, then provider event id.
        // The identity tie-breaker handles Stripe's whole-second creation clock.
        let candidate = (
            subscription_created_at_ms,
            subscription_id,
            stripe_event_created_at_ms,
            stripe_event_id,
        );
        let outcome = match fences.get(tenant_id) {
            Some((current_subscription, current_created, current_event, current_event_id, _)) => {
                let current = (
                    *current_created,
                    current_subscription.as_str(),
                    *current_event,
                    current_event_id.as_str(),
                );
                if current > candidate {
                    EntitlementCasOutcome::Stale
                } else if current == candidate {
                    EntitlementCasOutcome::Duplicate
                } else {
                    fences.insert(
                        tenant_id.to_owned(),
                        (
                            subscription_id.to_owned(),
                            subscription_created_at_ms,
                            stripe_event_created_at_ms,
                            stripe_event_id.to_owned(),
                            entitlement.is_some(),
                        ),
                    );
                    EntitlementCasOutcome::Applied
                }
            }
            None => {
                fences.insert(
                    tenant_id.to_owned(),
                    (
                        subscription_id.to_owned(),
                        subscription_created_at_ms,
                        stripe_event_created_at_ms,
                        stripe_event_id.to_owned(),
                        entitlement.is_some(),
                    ),
                );
                EntitlementCasOutcome::Applied
            }
        };
        if outcome == EntitlementCasOutcome::Stale {
            return Ok(outcome);
        }
        let mut runners = self
            .runners
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("runners mutex poisoned: {e}")))?;
        match entitlement {
            Some((max_concurrency, max_vcpu_h)) => {
                runners.insert(tenant_id.to_owned(), (max_concurrency, max_vcpu_h));
            }
            None => {
                runners.remove(tenant_id);
            }
        }
        Ok(outcome)
    }
}

/// Sanity-check that a materializer passed the right table name to
/// the writer. Production binders enforce this at the prepared-stmt
/// level; the in-memory mirror enforces it eagerly so a regression in
/// the handler dispatch is caught by the e2e tests.
fn debug_table(row: &MaterializedRow, expected: &str) -> Result<(), BillingD1Error> {
    if row.table == expected {
        Ok(())
    } else {
        Err(BillingD1Error::InvalidPayload(format!(
            "table mismatch: expected `{expected}`, got `{}`",
            row.table
        )))
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
#[path = "tests.rs"]
mod tests;
