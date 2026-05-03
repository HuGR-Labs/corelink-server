//! Per-tenant quota-state store trait + InMemory orchestrator-side fake.
//!
//! Per WI-S10-005 §6.1.2 the canonical durable mirror lives at
//! `migrations/d1/0020_quota_fsm_state.sql` (this WI's additive
//! migration) — `(tenant_id)` UNIQUE PK; `current_state` enum CHECK
//! constraint; `invoice_failure_count` non-negative; updated_at_ms.
//! Production wiring at WI-S10-007 binds this trait to that table; the
//! in-memory fake here pins the same trait surface so the orchestrator's
//! audit-fail-CLOSED envelope holds at both fakes + the production
//! wiring.

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::error::QuotaFsmStoreError;
use crate::event::{InvoiceFailureCount, QuotaState};

/// Canonical per-tenant quota-FSM state row (mirrors the D1 column set
/// landed by `migrations/d1/0020_quota_fsm_state.sql`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaFsmStateRow {
    /// Tenant id.
    pub tenant_id: Uuid,
    /// Current canonical quota state.
    pub current_state: QuotaState,
    /// Invoice-failure counter (cleared by `reinstate()`; bumped by
    /// `record_invoice_failure()`).
    pub invoice_failure_count: InvoiceFailureCount,
    /// Wall-clock instant of the last mutation (Unix epoch ms).
    pub updated_at_ms: u64,
}

impl QuotaFsmStateRow {
    /// Genesis state row — `WithinPlan` + 0 failures.
    #[must_use]
    pub const fn genesis(tenant_id: Uuid, now_ms: u64) -> Self {
        Self {
            tenant_id,
            current_state: QuotaState::WithinPlan,
            invoice_failure_count: InvoiceFailureCount::zero(),
            updated_at_ms: now_ms,
        }
    }
}

/// Per-tenant quota-state store trait. Production wiring binds to the
/// canonical D1 `quota_fsm_state` table; the trait surface is the
/// abstraction the orchestrator depends on.
pub trait QuotaFsmStore: Send + Sync + core::fmt::Debug {
    /// Look up the per-tenant row. Returns `None` when the tenant has
    /// never had a transition recorded (the orchestrator treats this as
    /// the canonical genesis `WithinPlan` + 0 invoice failures).
    ///
    /// # Errors
    ///
    /// [`QuotaFsmStoreError::Backend`] on backend transport failure.
    fn lookup(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<QuotaFsmStateRow>, QuotaFsmStoreError>;

    /// UPSERT the per-tenant row (production wiring uses an atomic D1
    /// batch in the same transaction as the audit-outbox INSERT).
    ///
    /// # Errors
    ///
    /// [`QuotaFsmStoreError::Backend`] on backend transport failure.
    fn upsert(&self, row: &QuotaFsmStateRow) -> Result<(), QuotaFsmStoreError>;
}

/// In-memory quota-state store fake. Keyed by `tenant_id` so the
/// canonical (tenant) UNIQUE PK falls out naturally.
#[derive(Debug, Default)]
pub struct InMemoryQuotaFsmStore {
    inner: Mutex<BTreeMap<Uuid, QuotaFsmStateRow>>,
}

impl InMemoryQuotaFsmStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every row (diagnostic).
    #[must_use]
    pub fn snapshot(&self) -> Vec<QuotaFsmStateRow> {
        match self.inner.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(p) => p.into_inner().values().cloned().collect(),
        }
    }

    /// Number of rows captured.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl QuotaFsmStore for InMemoryQuotaFsmStore {
    fn lookup(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<QuotaFsmStateRow>, QuotaFsmStoreError> {
        let guard = self.inner.lock().map_err(|_| {
            QuotaFsmStoreError::Backend(
                "quota-fsm store mutex poisoned".to_string(),
            )
        })?;
        Ok(guard.get(&tenant_id).cloned())
    }

    fn upsert(&self, row: &QuotaFsmStateRow) -> Result<(), QuotaFsmStoreError> {
        let mut guard = self.inner.lock().map_err(|_| {
            QuotaFsmStoreError::Backend(
                "quota-fsm store mutex poisoned".to_string(),
            )
        })?;
        guard.insert(row.tenant_id, row.clone());
        Ok(())
    }
}

/// Always-failing quota-state store for adversarial fail-CLOSED tests.
#[derive(Debug, Default)]
pub struct FailingQuotaFsmStore;

impl FailingQuotaFsmStore {
    /// Construct a fresh always-failing store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl QuotaFsmStore for FailingQuotaFsmStore {
    fn lookup(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<QuotaFsmStateRow>, QuotaFsmStoreError> {
        Err(QuotaFsmStoreError::Backend(
            "induced quota-fsm store failure (test fixture)".to_string(),
        ))
    }

    fn upsert(&self, _row: &QuotaFsmStateRow) -> Result<(), QuotaFsmStoreError> {
        Err(QuotaFsmStoreError::Backend(
            "induced quota-fsm store failure (test fixture)".to_string(),
        ))
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
    fn genesis_row_is_within_plan_zero_failures() {
        let t = Uuid::now_v7();
        let r = QuotaFsmStateRow::genesis(t, 100);
        assert_eq!(r.tenant_id, t);
        assert_eq!(r.current_state, QuotaState::WithinPlan);
        assert_eq!(r.invoice_failure_count.value(), 0);
        assert_eq!(r.updated_at_ms, 100);
    }

    #[test]
    fn lookup_missing_returns_none() {
        let s = InMemoryQuotaFsmStore::new();
        let t = Uuid::now_v7();
        assert!(s.lookup(t).unwrap().is_none());
    }

    #[test]
    fn upsert_then_lookup_returns_row() {
        let s = InMemoryQuotaFsmStore::new();
        let t = Uuid::now_v7();
        let r = QuotaFsmStateRow::genesis(t, 100);
        s.upsert(&r).unwrap();
        let back = s.lookup(t).unwrap().unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn upsert_overwrites_existing_row() {
        let s = InMemoryQuotaFsmStore::new();
        let t = Uuid::now_v7();
        let r1 = QuotaFsmStateRow::genesis(t, 100);
        s.upsert(&r1).unwrap();
        let r2 = QuotaFsmStateRow {
            tenant_id: t,
            current_state: QuotaState::SoftWarning80pct,
            invoice_failure_count: InvoiceFailureCount::zero(),
            updated_at_ms: 200,
        };
        s.upsert(&r2).unwrap();
        let back = s.lookup(t).unwrap().unwrap();
        assert_eq!(back, r2);
    }

    #[test]
    fn lookup_isolates_per_tenant() {
        let s = InMemoryQuotaFsmStore::new();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        let r1 = QuotaFsmStateRow::genesis(t1, 100);
        s.upsert(&r1).unwrap();
        assert!(s.lookup(t1).unwrap().is_some());
        assert!(s.lookup(t2).unwrap().is_none());
    }

    #[test]
    fn snapshot_returns_all_rows() {
        let s = InMemoryQuotaFsmStore::new();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        s.upsert(&QuotaFsmStateRow::genesis(t1, 100)).unwrap();
        s.upsert(&QuotaFsmStateRow::genesis(t2, 200)).unwrap();
        let all = s.snapshot();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn failing_store_returns_backend_error_on_lookup() {
        let s = FailingQuotaFsmStore::new();
        let t = Uuid::now_v7();
        let err = s.lookup(t).unwrap_err();
        let backend = matches!(err, QuotaFsmStoreError::Backend(_));
        assert!(backend);
    }

    #[test]
    fn failing_store_returns_backend_error_on_upsert() {
        let s = FailingQuotaFsmStore::new();
        let t = Uuid::now_v7();
        let err = s.upsert(&QuotaFsmStateRow::genesis(t, 100)).unwrap_err();
        let backend = matches!(err, QuotaFsmStoreError::Backend(_));
        assert!(backend);
    }
}
