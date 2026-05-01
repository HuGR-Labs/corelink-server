//! Per-tenant `upload_part` concurrency budget.
//!
//! The adapter holds a [`PerTenantSemaphore`] keyed on
//! `tenant_id`. Every `upload_part` call acquires a permit before
//! issuing the underlying R2 PUT; the permit is released on
//! drop. Default budget is
//! [`crate::bounds::DEFAULT_PER_TENANT_PARALLEL_PARTS`] (8); a
//! tier-aware override lands in S-13.
//!
//! ## Why custom — not just a global tokio Semaphore
//!
//! - **Per-tenant fairness**: a single tenant cannot starve other
//!   tenants by saturating the global budget — the budget is keyed
//!   on `tenant_id`.
//! - **Async-friendly**: the canonical happy path awaits a permit
//!   so back-pressure is observable as latency, not as 429s. The
//!   `try_acquire` API surfaces the budget-tripped path for the
//!   non-blocking handler probe (chaos test §15.5; HTTP 429 +
//!   `Retry-After` if the handler exposes it).
//!
//! ## Permit lifecycle
//!
//! Acquired permits are held in a [`PartPermit`] guard whose `Drop`
//! returns the permit to the per-tenant pool. Forgetting the guard
//! (`mem::forget`) leaks the permit — the canonical handler never
//! does this.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use tokio::sync::{Semaphore, TryAcquireError};
use uuid::Uuid;

use crate::bounds;
use crate::error::MultipartError;

/// Per-tenant `upload_part` concurrency manager.
///
/// Constructing a fresh manager — no global state (F-001 closure
/// 2026-05-01). Every adapter instance carries its own.
#[derive(Debug)]
pub struct PerTenantSemaphore {
    limit: usize,
    inner: Mutex<HashMap<Uuid, Arc<Semaphore>>>,
}

impl Default for PerTenantSemaphore {
    fn default() -> Self {
        Self::new(bounds::DEFAULT_PER_TENANT_PARALLEL_PARTS)
    }
}

impl PerTenantSemaphore {
    /// Construct a fresh per-tenant semaphore with `limit` permits
    /// per tenant.
    ///
    /// Limit must be positive — `0` is a configuration bug. The
    /// constructor clamps `0` to `1` so test code that forgets to
    /// pass a positive number doesn't immediately deadlock.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        let limit = limit.max(1);
        Self {
            limit,
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Configured per-tenant budget.
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    /// Acquire a permit, awaiting if the tenant's budget is
    /// saturated. Returns a [`PartPermit`] guard whose `Drop`
    /// releases the permit.
    ///
    /// # Errors
    ///
    /// Surfaces a [`MultipartError::Backend`] only if the
    /// underlying tokio semaphore is closed — this never happens
    /// in production code paths but the error is plumbed
    /// explicitly so a future refactor cannot silently `unwrap`.
    pub async fn acquire(&self, tenant_id: Uuid) -> Result<PartPermit, MultipartError> {
        let sem = self.semaphore_for(tenant_id);
        let permit = sem
            .acquire_owned()
            .await
            .map_err(|e| MultipartError::Backend(format!("semaphore closed: {e}")))?;
        Ok(PartPermit { permit })
    }

    /// Try to acquire a permit without awaiting. Returns
    /// [`MultipartError::ConcurrencyLimitReached`] if the tenant's
    /// budget is saturated.
    ///
    /// # Errors
    ///
    /// - [`MultipartError::ConcurrencyLimitReached`] when the
    ///   permit pool is empty.
    /// - [`MultipartError::Backend`] when the semaphore is closed
    ///   (programmer-error class — never in production).
    pub fn try_acquire(&self, tenant_id: Uuid) -> Result<PartPermit, MultipartError> {
        let sem = self.semaphore_for(tenant_id);
        let permit = sem.try_acquire_owned().map_err(|e| match e {
            TryAcquireError::NoPermits => MultipartError::ConcurrencyLimitReached {
                tenant_id: tenant_id.to_string(),
                limit: self.limit,
            },
            TryAcquireError::Closed => MultipartError::Backend("semaphore closed".to_string()),
        })?;
        Ok(PartPermit { permit })
    }

    fn semaphore_for(&self, tenant_id: Uuid) -> Arc<Semaphore> {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        Arc::clone(
            g.entry(tenant_id)
                .or_insert_with(|| Arc::new(Semaphore::new(self.limit))),
        )
    }
}

/// RAII guard returned by [`PerTenantSemaphore::acquire`] /
/// [`PerTenantSemaphore::try_acquire`]; releases the permit on
/// drop.
#[derive(Debug)]
pub struct PartPermit {
    // The tokio owned permit drops on guard drop, returning the
    // permit to the inner semaphore.
    #[allow(dead_code, reason = "field exists purely for its Drop side effect")]
    permit: tokio::sync::OwnedSemaphorePermit,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test code; panic on assertion failure is the contract"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn acquire_returns_permit_within_budget() {
        let s = PerTenantSemaphore::new(2);
        let _p1 = s.acquire(Uuid::nil()).await.unwrap();
        let _p2 = s.acquire(Uuid::nil()).await.unwrap();
        // 2nd permit succeeded; we don't await a third (would block).
    }

    #[tokio::test]
    async fn try_acquire_trips_at_budget() {
        let s = PerTenantSemaphore::new(2);
        let _p1 = s.try_acquire(Uuid::nil()).unwrap();
        let _p2 = s.try_acquire(Uuid::nil()).unwrap();
        let r = s.try_acquire(Uuid::nil());
        assert!(matches!(
            r,
            Err(MultipartError::ConcurrencyLimitReached { limit: 2, .. })
        ));
    }

    #[tokio::test]
    async fn try_acquire_per_tenant_isolation() {
        let s = PerTenantSemaphore::new(1);
        let t_a = Uuid::from_u128(1);
        let t_b = Uuid::from_u128(2);
        let _p1 = s.try_acquire(t_a).unwrap();
        // Tenant A is saturated; tenant B is not.
        let _p2 = s.try_acquire(t_b).unwrap();
        let r = s.try_acquire(t_a);
        assert!(matches!(r, Err(MultipartError::ConcurrencyLimitReached { .. })));
    }

    #[tokio::test]
    async fn permit_drop_releases_slot() {
        let s = PerTenantSemaphore::new(1);
        let p = s.try_acquire(Uuid::nil()).unwrap();
        assert!(s.try_acquire(Uuid::nil()).is_err());
        drop(p);
        // Slot returned — try again should succeed.
        let _p2 = s.try_acquire(Uuid::nil()).unwrap();
    }

    #[tokio::test]
    async fn default_limit_is_eight() {
        let s = PerTenantSemaphore::default();
        assert_eq!(s.limit(), bounds::DEFAULT_PER_TENANT_PARALLEL_PARTS);
        assert_eq!(s.limit(), 8);
    }

    #[test]
    fn new_zero_clamps_to_one() {
        let s = PerTenantSemaphore::new(0);
        assert_eq!(s.limit(), 1);
    }
}
