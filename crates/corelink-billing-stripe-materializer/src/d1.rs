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
/// Idempotency dedup INSERT into `stripe_webhook_events_processed`
/// (migration 0044). This table is UN-tenanted — its PRIMARY KEY is the
/// globally-unique Stripe `event_id`; there is NO `tenant_id` column.
/// `outcome` is the literal `'dispatched'` (try_record is the dispatch
/// path; the CHECK admits `dispatched`/`acknowledged_unknown`) and the
/// Stripe `event_id` doubles as the NOT-NULL `correlation_id` (the sync
/// trait carries neither). `RETURNING event_id` lets the native writer
/// detect insert-vs-ignore (mirrors the tier-select lock probe).
pub const SQL_INSERT_WEBHOOK_EVENT_PROCESSED: &str = "INSERT INTO stripe_webhook_events_processed (event_id, event_type, processed_at_ms, outcome, correlation_id) VALUES (?, ?, ?, 'dispatched', ?) ON CONFLICT DO NOTHING RETURNING event_id";
/// Read the current tier for a tenant (`tier_selections`, migration
/// 0039). Bind `?1` = `tenant_id`. Already schema-correct (the #172 fix).
pub const SQL_READ_TIER: &str = "SELECT tier FROM tier_selections WHERE tenant_id = ?";
/// UPSERT a tenant's tier → `subscription_state='active'` with the
/// activation timestamp bound to `subscription_started_at_ms` (the
/// `subscription_started_when_active` CHECK requires it NOT NULL when
/// active). Binds `?1..?4` = (tenant_id, tier, subscription_started_at_ms,
/// correlation_id). Already schema-correct (the #172 fix).
pub const SQL_UPSERT_TIER: &str = "INSERT INTO tier_selections (tenant_id, tier, subscription_state, subscription_started_at_ms, correlation_id) VALUES (?, ?, 'active', ?, ?) ON CONFLICT(tenant_id) DO UPDATE SET tier = excluded.tier, subscription_state = 'active', subscription_started_at_ms = excluded.subscription_started_at_ms, correlation_id = excluded.correlation_id";

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
    /// Mirrors `INSERT OR IGNORE` against
    /// `stripe_webhook_events_processed` from migration `0044`.
    fn try_record_event(
        &self,
        stripe_event_id: &str,
        canonical_event_type: &str,
        now_ms: u64,
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
    /// `tenant_id` → (`tier_wire`, `correlation_id`).
    tiers: Arc<Mutex<HashMap<String, (String, String)>>>,
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

    /// Read the recorded tier for `tenant_id` (None iff never written).
    #[must_use]
    pub fn tier_for(&self, tenant_id: &str) -> Option<String> {
        match self.tiers.lock() {
            Ok(g) => g.get(tenant_id).map(|(tier, _)| tier.clone()),
            Err(p) => p.into_inner().get(tenant_id).map(|(tier, _)| tier.clone()),
        }
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
mod tests {
    use super::*;

    fn row(table: &str) -> MaterializedRow {
        MaterializedRow {
            table: table.to_string(),
            tenant_id: "ten_1".to_string(),
            stripe_id: "obj_1".to_string(),
            stripe_event_id: "evt_1".to_string(),
            payload: serde_json::json!({}),
            materialized_at_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn customer_upsert_rejects_wrong_table() {
        let d1 = InMemoryBillingD1::new();
        let err = d1.upsert_customer(row("not_stripe_customers")).unwrap_err();
        assert!(matches!(err, BillingD1Error::InvalidPayload(_)));
    }

    #[test]
    fn idempotency_record_returns_false_on_replay() {
        let d1 = InMemoryBillingD1::new();
        assert!(d1.try_record_event("evt_1", "invoice.paid", 1).unwrap());
        assert!(!d1.try_record_event("evt_1", "invoice.paid", 2).unwrap());
        assert_eq!(d1.distinct_events(), 1);
    }

    #[test]
    fn arm_failure_short_circuits_writes() {
        let d1 = InMemoryBillingD1::new();
        d1.arm_failure(BillingD1Error::Transient("d1 down".into()));
        let err = d1.upsert_customer(row("stripe_customers")).unwrap_err();
        assert!(matches!(err, BillingD1Error::Transient(_)));
        // Clear and try again — should now succeed.
        d1.clear_failure();
        d1.upsert_customer(row("stripe_customers")).unwrap();
        assert_eq!(d1.count_table("stripe_customers"), 1);
    }

    #[test]
    fn tier_read_then_upsert_then_read_back() {
        let d1 = InMemoryBillingD1::new();
        assert_eq!(d1.read_tier("ten_1").unwrap(), None);
        d1.upsert_tier("ten_1", "pro", 1_700_000_000_000, "corr_1")
            .unwrap();
        assert_eq!(d1.read_tier("ten_1").unwrap(), Some("pro".to_string()));
    }

    // ───────────────────────────────────────────────────────────────────────
    // Canonical SQL shape pins — reconcile the shared literals against the
    // DEPLOYED schema (`migrations/d1/0048_*` + `0044_*` + `0039_*`). A
    // regression here is a money-path launch-blocker (a wrong column name
    // is a 100% silent write failure in production), so these run under the
    // default `cargo test --lib` (NOT gated behind `cf-billing-real`).
    // ───────────────────────────────────────────────────────────────────────

    /// The JSON column is `payload_json` on EVERY 0048 table — the old
    /// `payload` name does not exist and would fail every INSERT.
    #[test]
    fn no_materializer_sql_references_legacy_payload_column() {
        for sql in [
            SQL_UPSERT_CUSTOMER,
            SQL_UPSERT_SUBSCRIPTION,
            SQL_MARK_SUBSCRIPTION_CANCELED,
            SQL_UPSERT_INVOICE,
            SQL_INSERT_DISPUTE,
            SQL_INSERT_REFUND,
        ] {
            assert!(
                sql.contains("payload_json"),
                "must use the deployed `payload_json` column: {sql}"
            );
            // The bare token `payload ` (with a trailing space / paren)
            // must never appear — only `payload_json`.
            assert!(
                !sql.contains("payload ") && !sql.contains("payload,") && !sql.contains("payload)"),
                "must NOT reference the non-existent `payload` column: {sql}"
            );
        }
    }

    #[test]
    fn customer_upsert_matches_0048() {
        let sql = SQL_UPSERT_CUSTOMER;
        assert_eq!(sql.matches('?').count(), 5, "5 binds: {sql}");
        assert!(sql.contains("INSERT INTO stripe_customers"), "{sql}");
        // Natural PK conflict target — NOT a composite (tenant_id, …).
        assert!(sql.contains("ON CONFLICT(stripe_customer_id)"), "{sql}");
        assert!(!sql.contains("ON CONFLICT(tenant_id"), "{sql}");
        // tenant_id is the first bound column (verify_first_bind contract).
        assert!(
            sql.contains("(tenant_id, stripe_customer_id"),
            "tenant_id must be bind #1: {sql}"
        );
    }

    #[test]
    fn subscription_upsert_binds_status_not_null_0048() {
        let sql = SQL_UPSERT_SUBSCRIPTION;
        assert_eq!(sql.matches('?').count(), 6, "6 binds incl. status: {sql}");
        assert!(sql.contains("INSERT INTO stripe_subscriptions"), "{sql}");
        assert!(sql.contains("ON CONFLICT(stripe_subscription_id)"), "{sql}");
        // `status` is NOT NULL with no default → MUST be bound + upserted.
        assert!(sql.contains("status"), "must bind NOT-NULL status: {sql}");
        assert!(
            sql.contains("status = excluded.status"),
            "must refresh status on conflict: {sql}"
        );
    }

    #[test]
    fn mark_canceled_sets_status_and_is_tenant_scoped() {
        let sql = SQL_MARK_SUBSCRIPTION_CANCELED;
        assert!(
            sql.trim_start().to_ascii_lowercase().starts_with("update"),
            "{sql}"
        );
        assert!(
            sql.contains("status = 'canceled'"),
            "cancel must set status='canceled': {sql}"
        );
        assert!(
            sql.contains("WHERE tenant_id = ?"),
            "UPDATE must be tenant-scoped: {sql}"
        );
        assert!(sql.contains("stripe_subscription_id = ?"), "{sql}");
    }

    #[test]
    fn invoice_upsert_binds_outcome_not_null_0048() {
        let sql = SQL_UPSERT_INVOICE;
        assert_eq!(sql.matches('?').count(), 6, "6 binds incl. outcome: {sql}");
        assert!(sql.contains("INSERT INTO stripe_invoices"), "{sql}");
        assert!(sql.contains("ON CONFLICT(stripe_invoice_id)"), "{sql}");
        // `outcome` is NOT NULL + CHECK IN('paid','payment_failed').
        assert!(sql.contains("outcome"), "must bind NOT-NULL outcome: {sql}");
        assert!(
            sql.contains("outcome = excluded.outcome"),
            "must refresh outcome on conflict: {sql}"
        );
    }

    #[test]
    fn dispute_insert_matches_0048() {
        let sql = SQL_INSERT_DISPUTE;
        assert_eq!(sql.matches('?').count(), 5, "5 binds: {sql}");
        assert!(sql.contains("INSERT INTO stripe_disputes"), "{sql}");
        assert!(sql.contains("stripe_dispute_id"), "{sql}");
        // severity + schema_version have DEFAULTs and are intentionally omitted.
        assert!(
            !sql.contains("severity"),
            "DEFAULTed severity omitted: {sql}"
        );
    }

    #[test]
    fn refund_insert_uses_charge_id_pk_0048() {
        let sql = SQL_INSERT_REFUND;
        assert_eq!(sql.matches('?').count(), 5, "5 binds: {sql}");
        assert!(sql.contains("INSERT INTO stripe_refunds"), "{sql}");
        // PK is the parent charge id; the non-existent stripe_refund_id
        // column must NEVER appear (the latent bug).
        assert!(sql.contains("stripe_charge_id"), "{sql}");
        assert!(
            !sql.contains("stripe_refund_id"),
            "must NOT reference the non-existent stripe_refund_id: {sql}"
        );
    }

    #[test]
    fn webhook_dedup_insert_matches_0044_untenanted() {
        let sql = SQL_INSERT_WEBHOOK_EVENT_PROCESSED;
        assert!(
            sql.contains("INSERT INTO stripe_webhook_events_processed"),
            "{sql}"
        );
        // Deployed 0044 columns — NOT the old (tenant_id, stripe_event_id,
        // canonical_event_type, processed_at_ms) shape.
        for col in [
            "event_id",
            "event_type",
            "processed_at_ms",
            "outcome",
            "correlation_id",
        ] {
            assert!(sql.contains(col), "missing deployed column `{col}`: {sql}");
        }
        // The table is un-tenanted — no tenant_id column exists.
        assert!(!sql.contains("tenant_id"), "0044 has no tenant_id: {sql}");
        assert!(
            !sql.contains("canonical_event_type"),
            "renamed→event_type: {sql}"
        );
        // outcome bound as the dispatch literal (CHECK-admitted).
        assert!(sql.contains("'dispatched'"), "outcome literal: {sql}");
        // RETURNING lets the native writer detect insert-vs-ignore.
        assert!(sql.contains("RETURNING event_id"), "{sql}");
        // 3 binds: event_id, event_type, processed_at_ms, correlation_id
        // (outcome is a literal, not a bind) → 4 placeholders actually.
        assert_eq!(
            sql.matches('?').count(),
            4,
            "4 binds (outcome is literal): {sql}"
        );
    }

    #[test]
    fn tier_sql_unchanged_and_schema_correct_0039() {
        // The #172 fix — left intact, re-pinned here as the shared owner.
        assert_eq!(SQL_UPSERT_TIER.matches('?').count(), 4, "{SQL_UPSERT_TIER}");
        assert!(SQL_UPSERT_TIER.contains("'active'"), "{SQL_UPSERT_TIER}");
        assert!(
            SQL_UPSERT_TIER.contains("subscription_started_at_ms"),
            "{SQL_UPSERT_TIER}"
        );
        assert!(
            !SQL_UPSERT_TIER.contains("materialized_at_ms"),
            "must NOT reference materialized_at_ms: {SQL_UPSERT_TIER}"
        );
        assert!(
            SQL_READ_TIER.contains("WHERE tenant_id = ?"),
            "{SQL_READ_TIER}"
        );
    }
}
