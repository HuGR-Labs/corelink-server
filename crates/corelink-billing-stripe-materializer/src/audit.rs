//! Audit emitter trait + production binder shim.
//!
//! Two audit surfaces live in this crate:
//!
//! 1. [`BillingAuditEmitter`] — emitted by the materializer **before**
//!    every state mutation. Carries the canonical
//!    `corelink.billing.<event>.materialized.v1` event name plus the
//!    typed mutation context (tenant id, Stripe object id, etc.).
//! 2. [`RealStripeAuditEmitter`] — implements the wave-15
//!    [`corelink_stripe_real::webhook_dispatch::AuditEmitter`] trait
//!    by forwarding the dispatcher's per-delivery row to the same
//!    sink the materializer uses. Production binds one
//!    `Arc<dyn BillingAuditEmitter>` and shares it between the
//!    dispatcher and the materializer so the chain stays single-topology.

use std::fmt;
use std::sync::{Arc, Mutex};

// Wave-36 Trigger A: trait + record surface migrated to the leaf
// `corelink-billing-stripe-traits` crate. The materializer production
// sources MUST NOT depend on `corelink-stripe-real` for the trait
// surface — that path is reserved for concrete HTTPS adapters /
// dispatchers / test fakes (gated to tests / e2e harnesses).
use corelink_billing_stripe_traits::{AuditEmitter, AuditRecord};
use serde::{Deserialize, Serialize};

/// Error returned by [`BillingAuditEmitter::emit_billing`]. The
/// dispatcher wraps audit-emit failures into HTTP 500 (fail-CLOSED).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BillingAuditError {
    /// Audit chain unreachable / transient error.
    Transient(String),
}

impl fmt::Display for BillingAuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(s) => write!(f, "billing-audit transient: {s}"),
        }
    }
}

impl std::error::Error for BillingAuditError {}

/// One canonical audit row for a Stripe-driven state mutation.
///
/// Distinct from `corelink_stripe_real::webhook_dispatch::AuditRecord`
/// (which lives on the dispatcher) — this row is emitted by the
/// materializer **after** the dispatcher's row but **before** the D1
/// write surfaces success. Production stores both in the same chain;
/// the dispatcher row pins delivery latency, this row pins state
/// mutation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BillingAuditRecord {
    /// Canonical event name — one of:
    /// - `corelink.billing.customer.materialized.v1`
    /// - `corelink.billing.subscription.materialized.v1`
    /// - `corelink.billing.subscription_canceled.materialized.v1`
    /// - `corelink.billing.invoice.materialized.v1`
    /// - `corelink.billing.dispute.materialized.v1`
    /// - `corelink.billing.refund.materialized.v1`
    /// - `corelink.tenant.tier_changed.v1`
    /// - `corelink.billing.echo.v1`
    pub event_name: &'static str,
    /// Stripe event id this audit row was derived from.
    pub stripe_event_id: String,
    /// Canonical event-type label (e.g. `invoice.paid`).
    pub stripe_event_type: String,
    /// Tenant id (mirrors the `tier_selections.tenant_id` column).
    pub tenant_id: String,
    /// Optional Stripe object id (`sub_…`, `in_…`, `cus_…`, …).
    pub stripe_object_id: Option<String>,
    /// Severity bucket. `Sev1` is reserved for `charge.dispute.created`
    /// — Finance + customer attention required.
    pub severity: AuditSeverity,
    /// Wall-clock ms when this audit row was assembled.
    pub ts_ms: u64,
    /// Free-form JSON payload (canonical column set per event name).
    pub payload: serde_json::Value,
}

impl BillingAuditRecord {
    /// Construct a record from its canonical column set. The
    /// constructor is the out-of-crate seam (the struct is
    /// `#[non_exhaustive]`, so consumers can't use brace-init).
    /// Eight parameters reflect the canonical audit-record shape — every
    /// field is load-bearing at the audit data model.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        event_name: &'static str,
        stripe_event_id: impl Into<String>,
        stripe_event_type: impl Into<String>,
        tenant_id: impl Into<String>,
        stripe_object_id: Option<String>,
        severity: AuditSeverity,
        ts_ms: u64,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_name,
            stripe_event_id: stripe_event_id.into(),
            stripe_event_type: stripe_event_type.into(),
            tenant_id: tenant_id.into(),
            stripe_object_id,
            severity,
            ts_ms,
            payload,
        }
    }
}

/// Severity bucket emitted alongside each audit row.
///
/// Maps to the operator-facing pager: `Sev1` pages Finance, others
/// flow into the dashboard-only stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AuditSeverity {
    /// Informational (observability echo).
    Info,
    /// Routine state mutation (subscription updated, invoice paid).
    Notice,
    /// Customer attention needed (`charge.dispute.created`).
    Sev1,
}

/// Audit sink driven by the materializer. Production binds to
/// `corelink-audit-chain` (an `ArchiveProducer` plus an `R2AuditSink`);
/// tests use [`InMemoryBillingAuditEmitter`].
pub trait BillingAuditEmitter: fmt::Debug + Send + Sync {
    /// Emit one audit row. Errors propagate as HTTP 500 (Stripe
    /// retries → next delivery hits the dedup row → resolves without
    /// re-dispatch).
    fn emit_billing(&self, record: &BillingAuditRecord) -> Result<(), BillingAuditError>;
}

/// Native in-memory mirror of the audit emitter.
#[derive(Clone, Debug, Default)]
pub struct InMemoryBillingAuditEmitter {
    rows: Arc<Mutex<Vec<BillingAuditRecord>>>,
    fail_with: Arc<Mutex<Option<BillingAuditError>>>,
}

impl InMemoryBillingAuditEmitter {
    /// Construct an empty emitter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Force every emit call to fail with `err`.
    pub fn arm_failure(&self, err: BillingAuditError) {
        if let Ok(mut g) = self.fail_with.lock() {
            *g = Some(err);
        }
    }

    /// Snapshot every emitted row in order.
    #[must_use]
    pub fn snapshot(&self) -> Vec<BillingAuditRecord> {
        match self.rows.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count rows whose `event_name` matches `name`.
    #[must_use]
    pub fn count_event(&self, name: &str) -> usize {
        self.snapshot()
            .iter()
            .filter(|r| r.event_name == name)
            .count()
    }
}

impl BillingAuditEmitter for InMemoryBillingAuditEmitter {
    fn emit_billing(&self, record: &BillingAuditRecord) -> Result<(), BillingAuditError> {
        let armed = {
            let guard = self
                .fail_with
                .lock()
                .map_err(|e| BillingAuditError::Transient(format!("mutex poisoned: {e}")))?;
            guard.clone()
        };
        if let Some(err) = armed {
            return Err(err);
        }
        let mut g = self
            .rows
            .lock()
            .map_err(|e| BillingAuditError::Transient(format!("rows mutex poisoned: {e}")))?;
        g.push(record.clone());
        Ok(())
    }
}

/// Dispatcher-side audit emitter (implements the wave-15
/// [`corelink_stripe_real::webhook_dispatch::AuditEmitter`] trait).
///
/// Forwards every dispatcher row to the shared
/// [`BillingAuditEmitter`] so the materializer + dispatcher audit
/// trails are interleaved in **one** chain (verifier never sees a
/// split topology).
#[derive(Clone, Debug)]
pub struct RealStripeAuditEmitter {
    sink: Arc<dyn BillingAuditEmitter>,
}

impl RealStripeAuditEmitter {
    /// Construct a new dispatcher-side adapter routing through `sink`.
    #[must_use]
    pub fn new(sink: Arc<dyn BillingAuditEmitter>) -> Self {
        Self { sink }
    }
}

impl AuditEmitter for RealStripeAuditEmitter {
    fn emit(&self, record: &AuditRecord) -> Result<(), String> {
        let canonical = BillingAuditRecord {
            event_name: "corelink.billing.stripe_event_processed.v1",
            stripe_event_id: record.stripe_event_id.clone(),
            stripe_event_type: record.canonical_event_type.label().to_string(),
            // The dispatcher row pins delivery — tenant is unknown at
            // this layer (Stripe addresses by customer; the
            // materializer's first action is to look up the tenant).
            // We use a stable sentinel so the audit chain is still
            // tenant-id-scoped without conflating with real tenants.
            tenant_id: "__stripe_dispatcher__".to_string(),
            stripe_object_id: None,
            severity: AuditSeverity::Info,
            ts_ms: record.ts_ms,
            payload: serde_json::json!({
                "outcome": record.outcome.label(),
                "idempotency_token": record.idempotency_token_hex,
                "error_detail": record.error_detail,
            }),
        };
        self.sink
            .emit_billing(&canonical)
            .map_err(|e| format!("{e}"))
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
    #[test]
    fn in_memory_emitter_records_in_order() {
        let e = InMemoryBillingAuditEmitter::new();
        let rec = BillingAuditRecord {
            event_name: "corelink.billing.invoice.materialized.v1",
            stripe_event_id: "evt_1".to_string(),
            stripe_event_type: "invoice.paid".to_string(),
            tenant_id: "ten_1".to_string(),
            stripe_object_id: Some("in_1".to_string()),
            severity: AuditSeverity::Notice,
            ts_ms: 1,
            payload: serde_json::json!({}),
        };
        e.emit_billing(&rec).unwrap();
        assert_eq!(e.count_event("corelink.billing.invoice.materialized.v1"), 1);
    }

    #[test]
    fn fail_with_propagates_error() {
        let e = InMemoryBillingAuditEmitter::new();
        e.arm_failure(BillingAuditError::Transient("audit down".into()));
        let rec = BillingAuditRecord {
            event_name: "corelink.billing.invoice.materialized.v1",
            stripe_event_id: "evt_1".to_string(),
            stripe_event_type: "invoice.paid".to_string(),
            tenant_id: "ten_1".to_string(),
            stripe_object_id: None,
            severity: AuditSeverity::Notice,
            ts_ms: 1,
            payload: serde_json::json!({}),
        };
        let err = e.emit_billing(&rec).unwrap_err();
        assert!(matches!(err, BillingAuditError::Transient(_)));
    }

    // The `RealStripeAuditEmitter` adapter is exercised end-to-end
    // via the integration test in `tests/materializers_e2e.rs` —
    // `AuditRecord` is `#[non_exhaustive]` so cross-crate brace-init
    // is unavailable here; the e2e suite drives the adapter through
    // the canonical `WebhookDispatcher::process` pipeline which is
    // the only way the production code path reaches `emit`.
}
