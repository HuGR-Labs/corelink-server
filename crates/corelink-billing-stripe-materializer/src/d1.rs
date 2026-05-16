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
    /// `tier_selections.tier`; native mirror stores the value verbatim.
    fn upsert_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
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
        d1.upsert_tier("ten_1", "pro", "corr_1").unwrap();
        assert_eq!(d1.read_tier("ten_1").unwrap(), Some("pro".to_string()));
    }
}
