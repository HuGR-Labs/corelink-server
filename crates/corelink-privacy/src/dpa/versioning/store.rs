//! D1 mirror store surface for `dpa_versions` + `tenants.dpa_*` cols.
//!
//! Production wiring (Cloudflare D1 SQL) is deferred to the PRR ship
//! gate per the `trait-abstraction-defer` charter pattern.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::error::DpaVersioningError;
use super::schema::{DpaVersionRecord, TenantDpaState, UnixSeconds};

/// Mirror surface for the persistent state.
pub trait DpaStore: std::fmt::Debug + Send + Sync {
    /// Append a new version record (idempotent on `version`).
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::InvalidBump`] when the new
    /// version is not strictly greater than the latest known, or
    /// [`DpaVersioningError::Store`] on transport failure.
    fn append_version(&self, record: DpaVersionRecord) -> Result<(), DpaVersioningError>;

    /// Return the latest published version, if any.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::Store`] / [`DpaVersioningError::LockPoisoned`]
    /// on transport failure.
    fn latest_version(&self) -> Result<Option<DpaVersionRecord>, DpaVersioningError>;

    /// Read a tenant's DPA state.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::Store`] on transport failure.
    fn read_tenant(&self, tenant_id: Uuid) -> Result<Option<TenantDpaState>, DpaVersioningError>;

    /// Write (upsert) a tenant's DPA state.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::Store`] on transport failure.
    fn write_tenant(&self, state: TenantDpaState) -> Result<(), DpaVersioningError>;

    /// List every tenant currently with a pending re-acceptance —
    /// the cron's working set.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::Store`] on transport failure.
    fn list_pending_tenants(&self) -> Result<Vec<TenantDpaState>, DpaVersioningError>;
}

/// In-memory implementation backing every unit / property test in
/// this crate.
#[derive(Debug, Default)]
pub struct InMemoryDpaStore {
    versions: Arc<Mutex<Vec<DpaVersionRecord>>>,
    tenants: Arc<Mutex<HashMap<Uuid, TenantDpaState>>>,
}

impl InMemoryDpaStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a tenant. Useful for tests that bypass the
    /// initial-onboarding flow.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::LockPoisoned`] if the inner
    /// mutex is poisoned.
    pub fn seed_tenant(&self, state: TenantDpaState) -> Result<(), DpaVersioningError> {
        let mut guard = self
            .tenants
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        guard.insert(state.tenant_id, state);
        Ok(())
    }

    /// All tenant rows (for tests + cron snapshot).
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::LockPoisoned`] if the inner
    /// mutex is poisoned.
    pub fn all_tenants(&self) -> Result<Vec<TenantDpaState>, DpaVersioningError> {
        let guard = self
            .tenants
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        Ok(guard.values().cloned().collect())
    }

    /// Bulk-flag every tenant for a pending re-acceptance with the
    /// canonical grace deadline.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::LockPoisoned`] if the inner
    /// mutex is poisoned.
    pub fn flag_all_pending(
        &self,
        grace_expires_at: UnixSeconds,
    ) -> Result<u64, DpaVersioningError> {
        let mut guard = self
            .tenants
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        let mut flagged = 0u64;
        for state in guard.values_mut() {
            if !state.re_acceptance_pending {
                state.re_acceptance_pending = true;
                state.grace_expires_at = Some(grace_expires_at);
                flagged = flagged.saturating_add(1);
            }
        }
        Ok(flagged)
    }
}

impl DpaStore for InMemoryDpaStore {
    fn append_version(&self, record: DpaVersionRecord) -> Result<(), DpaVersioningError> {
        let mut guard = self
            .versions
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        if let Some(latest) = guard.last() {
            if record.version <= latest.version {
                return Err(DpaVersioningError::InvalidBump {
                    old: latest.version.render(),
                    new: record.version.render(),
                });
            }
        }
        guard.push(record);
        Ok(())
    }

    fn latest_version(&self) -> Result<Option<DpaVersionRecord>, DpaVersioningError> {
        let guard = self
            .versions
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        Ok(guard.last().cloned())
    }

    fn read_tenant(&self, tenant_id: Uuid) -> Result<Option<TenantDpaState>, DpaVersioningError> {
        let guard = self
            .tenants
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        Ok(guard.get(&tenant_id).cloned())
    }

    fn write_tenant(&self, state: TenantDpaState) -> Result<(), DpaVersioningError> {
        let mut guard = self
            .tenants
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        guard.insert(state.tenant_id, state);
        Ok(())
    }

    fn list_pending_tenants(&self) -> Result<Vec<TenantDpaState>, DpaVersioningError> {
        let guard = self
            .tenants
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        Ok(guard
            .values()
            .filter(|t| t.re_acceptance_pending)
            .cloned()
            .collect())
    }
}

/// Always-failing store for fail-CLOSED unit tests.
#[derive(Debug, Default)]
pub struct FailingDpaStore;

impl DpaStore for FailingDpaStore {
    fn append_version(&self, _record: DpaVersionRecord) -> Result<(), DpaVersioningError> {
        Err(DpaVersioningError::Store("simulated append failure".into()))
    }
    fn latest_version(&self) -> Result<Option<DpaVersionRecord>, DpaVersioningError> {
        Err(DpaVersioningError::Store("simulated read failure".into()))
    }
    fn read_tenant(&self, _tenant_id: Uuid) -> Result<Option<TenantDpaState>, DpaVersioningError> {
        Err(DpaVersioningError::Store("simulated read failure".into()))
    }
    fn write_tenant(&self, _state: TenantDpaState) -> Result<(), DpaVersioningError> {
        Err(DpaVersioningError::Store("simulated write failure".into()))
    }
    fn list_pending_tenants(&self) -> Result<Vec<TenantDpaState>, DpaVersioningError> {
        Err(DpaVersioningError::Store("simulated list failure".into()))
    }
}
