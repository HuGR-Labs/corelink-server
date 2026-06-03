//! Tenant-offboarding durable store trait + InMemory test store.
//!
//! Production wiring at PRR ship gate binds this to the canonical D1
//! `tenant_offboarding_state` table per
//! `migrations/d1/0046_tenant_offboarding_state.sql`. The trait
//! surface here pins the load-bearing semantics: per-tenant single
//! current state + canonical T+0 anchor + canonical operator id +
//! canonical reason / notes for the audit trail.

use std::collections::HashMap;
use std::sync::Mutex;

use super::error::TenantOffboardingStoreError;
use super::state::TenantOffboardingState;

/// Canonical durable record stored per tenant. Production wiring at
/// PRR ship gate binds this to a 1:1 D1 row in
/// `tenant_offboarding_state` (PK on `tenant_id`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantOffboardingRecord {
    /// Tenant identifier (opaque). Production uses ULID.
    pub tenant_id: String,
    /// Current state at the time the row was last persisted.
    pub state: TenantOffboardingState,
    /// T+0 anchor: wall-clock instant (ms since Unix epoch) at which
    /// the canonical [`super::state::TransitionTrigger::CustomerInitiated`]
    /// row was committed. Used by the daily-cron tick to compute
    /// T+30 / T+45 / T+90 deadlines without re-reading the audit
    /// chain.
    pub cancel_requested_at_ms: i64,
    /// Optional initiator user id (for audit forensics on the
    /// T+0 click).
    pub initiator_user_id: Option<String>,
    /// Optional free-form reason captured at T+0 (customer survey).
    /// Production wiring caps this at 4 KB.
    pub reason: Option<String>,
}

impl TenantOffboardingRecord {
    /// Fresh record at T+0 with state = CancelRequested.
    #[must_use]
    pub fn at_cancel(
        tenant_id: String,
        cancel_requested_at_ms: i64,
        initiator_user_id: Option<String>,
        reason: Option<String>,
    ) -> Self {
        Self {
            tenant_id,
            state: TenantOffboardingState::CancelRequested,
            cancel_requested_at_ms,
            initiator_user_id,
            reason,
        }
    }
}

/// Durable store trait. Production wiring at PRR ship gate binds
/// this to the canonical D1 `tenant_offboarding_state` table.
pub trait TenantOffboardingStore: Send + Sync + core::fmt::Debug {
    /// Read the current record for `tenant_id`, or `Ok(None)` if no
    /// offboarding has been initiated.
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingStoreError::Backend`] if the
    /// underlying durable backend (D1) rejects the read.
    fn get(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantOffboardingRecord>, TenantOffboardingStoreError>;

    /// Insert a fresh offboarding record at T+0. The canonical
    /// orchestrator pre-validates that no record exists for the
    /// tenant; the store SHOULD enforce a UNIQUE constraint on
    /// `tenant_id` as a defence-in-depth check.
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingStoreError::Backend`] on transport
    /// failure or unique-violation collision.
    fn insert(&self, record: TenantOffboardingRecord) -> Result<(), TenantOffboardingStoreError>;

    /// Advance the state for an existing record. The canonical
    /// orchestrator pre-validates that the `(from, trigger) → to`
    /// pair is a legal transition; the store MUST refuse the
    /// mutation if no record exists ([`TenantOffboardingStoreError::NotFound`]).
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingStoreError::NotFound`] if no
    /// record exists for `tenant_id`, or
    /// [`TenantOffboardingStoreError::Backend`] on transport failure.
    fn advance_state(
        &self,
        tenant_id: &str,
        new_state: TenantOffboardingState,
    ) -> Result<(), TenantOffboardingStoreError>;
}

/// In-memory store for tests. Per-instance `Mutex<HashMap>` closure
/// (F-001 isolation pattern).
#[derive(Debug, Default)]
pub struct InMemoryTenantOffboardingStore {
    inner: Mutex<HashMap<String, TenantOffboardingRecord>>,
}

impl InMemoryTenantOffboardingStore {
    /// Fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of records held by the store.
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingStoreError::Backend`] if the
    /// per-instance mutex is poisoned.
    pub fn len(&self) -> Result<usize, TenantOffboardingStoreError> {
        let guard = self.inner.lock().map_err(|_| {
            TenantOffboardingStoreError::Backend("store mutex poisoned".to_string())
        })?;
        Ok(guard.len())
    }

    /// Whether no records are held.
    ///
    /// # Errors
    ///
    /// Returns [`TenantOffboardingStoreError::Backend`] if the
    /// per-instance mutex is poisoned.
    pub fn is_empty(&self) -> Result<bool, TenantOffboardingStoreError> {
        Ok(self.len()? == 0)
    }
}

impl TenantOffboardingStore for InMemoryTenantOffboardingStore {
    fn get(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantOffboardingRecord>, TenantOffboardingStoreError> {
        let guard = self.inner.lock().map_err(|_| {
            TenantOffboardingStoreError::Backend("store mutex poisoned".to_string())
        })?;
        Ok(guard.get(tenant_id).cloned())
    }

    fn insert(&self, record: TenantOffboardingRecord) -> Result<(), TenantOffboardingStoreError> {
        let mut guard = self.inner.lock().map_err(|_| {
            TenantOffboardingStoreError::Backend("store mutex poisoned".to_string())
        })?;
        if guard.contains_key(&record.tenant_id) {
            return Err(TenantOffboardingStoreError::Backend(format!(
                "duplicate offboarding record for tenant_id={}",
                record.tenant_id
            )));
        }
        guard.insert(record.tenant_id.clone(), record);
        Ok(())
    }

    fn advance_state(
        &self,
        tenant_id: &str,
        new_state: TenantOffboardingState,
    ) -> Result<(), TenantOffboardingStoreError> {
        let mut guard = self.inner.lock().map_err(|_| {
            TenantOffboardingStoreError::Backend("store mutex poisoned".to_string())
        })?;
        match guard.get_mut(tenant_id) {
            Some(rec) => {
                rec.state = new_state;
                Ok(())
            }
            None => Err(TenantOffboardingStoreError::NotFound(tenant_id.to_string())),
        }
    }
}

/// Always-failing store. Used by tests to force the fail-CLOSED
/// envelope on store backend failure.
#[derive(Debug, Default)]
pub struct FailingTenantOffboardingStore;

impl FailingTenantOffboardingStore {
    /// Fresh sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TenantOffboardingStore for FailingTenantOffboardingStore {
    fn get(
        &self,
        _tenant_id: &str,
    ) -> Result<Option<TenantOffboardingRecord>, TenantOffboardingStoreError> {
        Err(TenantOffboardingStoreError::Backend(
            "induced store get failure".to_string(),
        ))
    }

    fn insert(&self, _record: TenantOffboardingRecord) -> Result<(), TenantOffboardingStoreError> {
        Err(TenantOffboardingStoreError::Backend(
            "induced store insert failure".to_string(),
        ))
    }

    fn advance_state(
        &self,
        _tenant_id: &str,
        _new_state: TenantOffboardingState,
    ) -> Result<(), TenantOffboardingStoreError> {
        Err(TenantOffboardingStoreError::Backend(
            "induced store advance failure".to_string(),
        ))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_get_round_trips() {
        let store = InMemoryTenantOffboardingStore::new();
        let rec = TenantOffboardingRecord::at_cancel("t".to_string(), 100, None, None);
        store.insert(rec.clone()).unwrap();
        let back = store.get("t").unwrap().unwrap();
        assert_eq!(back, rec);
    }

    #[test]
    fn insert_duplicate_rejected() {
        let store = InMemoryTenantOffboardingStore::new();
        let rec = TenantOffboardingRecord::at_cancel("t".to_string(), 100, None, None);
        store.insert(rec.clone()).unwrap();
        let r = store.insert(rec);
        assert!(r.is_err());
    }

    #[test]
    fn advance_state_updates() {
        let store = InMemoryTenantOffboardingStore::new();
        let rec = TenantOffboardingRecord::at_cancel("t".to_string(), 100, None, None);
        store.insert(rec).unwrap();
        store
            .advance_state("t", TenantOffboardingState::GracePeriod)
            .unwrap();
        let back = store.get("t").unwrap().unwrap();
        assert_eq!(back.state, TenantOffboardingState::GracePeriod);
    }

    #[test]
    fn advance_state_missing_returns_not_found() {
        let store = InMemoryTenantOffboardingStore::new();
        let r = store.advance_state("absent", TenantOffboardingState::GracePeriod);
        assert!(matches!(r, Err(TenantOffboardingStoreError::NotFound(_))));
    }

    #[test]
    fn failing_store_errors_all_paths() {
        let store = FailingTenantOffboardingStore::new();
        assert!(store.get("t").is_err());
        assert!(store
            .insert(TenantOffboardingRecord::at_cancel(
                "t".to_string(),
                0,
                None,
                None,
            ))
            .is_err());
        assert!(store
            .advance_state("t", TenantOffboardingState::Erased)
            .is_err());
    }
}
