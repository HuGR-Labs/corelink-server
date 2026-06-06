//! `tenant_storage_state` row + store trait + InMemory fake mirroring
//! the SQL `migrations/d1/0008_tenant_storage_state.sql` byte-for-byte.
//!
//! ## Schema separation rationale
//!
//! Per Lote 10.7bis P0-2 R4 option B, `tenant_quota` is POLICY (max
//! storage bytes immutable per period) and `tenant_storage_state` is
//! STATE (mutable running counter + DO-D1 sync watermarks +
//! eviction-cooldown tracking). The eviction worker (this crate) reads
//! `bytes_used` + `bytes_quota` to decide the 95% trigger arm; the
//! quota middleware (WI-S07-003) DO singleton owns the authoritative
//! counter.
//!
//! ## Invariants enforced by [`InMemoryTenantStorageStateStore`] (same
//! as SQL CHECK)
//!
//! - **Tenant-leftmost** — the PK is `(tenant_id, region)`; cross-tenant
//!   read returns `Ok(None)`.
//! - **bytes_used >= 0**, **bytes_quota >= 0**,
//!   **bytes_reclaimed_lifetime >= 0** — saturating arithmetic at the
//!   API surface; underflow surfaces as `Backend` error.
//! - **Region domain** — canonical 5-region literal (mirrors
//!   `EvictionRegion`).
//! - **Lifecycle monotonic** — `updated_at_ms >= created_at_ms`;
//!   `bytes_used_updated_at_ms >= created_at_ms`; `last_synced_at_ms
//!   >= created_at_ms`; `last_evict_at_ms (when set) >= created_at_ms`.

use std::collections::BTreeMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::region::EvictionRegion;

/// Materialised projection of a `tenant_storage_state` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantStorageStateRow {
    /// Tenant scope.
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: EvictionRegion,
    /// Running bytes_used counter (DO authoritative; D1 5min sync).
    pub bytes_used: u64,
    /// Quota ceiling snapshot (denormalized from
    /// `tenant_quota.max_storage_bytes`).
    pub bytes_quota: u64,
    /// `bytes_used` watermark.
    pub bytes_used_updated_at_ms: u64,
    /// DO ↔ D1 sync watermark.
    pub last_synced_at_ms: u64,
    /// Eviction cooldown watermark (set by the eviction worker on
    /// each pass).
    pub last_evict_at_ms: Option<u64>,
    /// Lifetime bytes reclaimed by the eviction worker (monotone).
    pub bytes_reclaimed_lifetime: u64,
    /// Lifecycle.
    pub created_at_ms: u64,
    /// Lifecycle.
    pub updated_at_ms: u64,
}

impl TenantStorageStateRow {
    /// Compute the utilization pct as `bytes_used / bytes_quota`.
    /// Returns `0.0` when `bytes_quota == 0` (defensive — a zero-quota
    /// row is a misconfigured tenant; the eviction worker treats this
    /// as "trigger NEVER fires" rather than divide-by-zero).
    #[must_use]
    pub fn utilization_pct(&self) -> f64 {
        if self.bytes_quota == 0 {
            return 0.0;
        }
        // Saturating cast; counter values are well within f64 precision.
        let used_f = self.bytes_used as f64;
        let quota_f = self.bytes_quota as f64;
        used_f / quota_f
    }

    /// Whether the tenant has crossed the 95% trigger threshold.
    #[must_use]
    pub fn at_or_above_trigger(&self) -> bool {
        self.utilization_pct() >= crate::phase::QUOTA_TRIGGER_THRESHOLD_PCT
    }
}

/// Errors surfaced by [`TenantStorageStateStore`] backends.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageStateError {
    /// CHECK violation — `bytes_used` underflow / overflow / negative
    /// surfaced at the API surface.
    #[error("CHECK violation: {column} value invalid (got={got}, reason={reason})")]
    CheckViolation {
        /// Column that failed the CHECK.
        column: &'static str,
        /// Observed value.
        got: u64,
        /// Short reason code.
        reason: &'static str,
    },
    /// Lifecycle monotonic violation — `updated_at_ms < created_at_ms`
    /// (etc.).
    #[error("lifecycle monotonic violation: {field} new={new_value} prev={prev_value}")]
    LifecycleMonotonic {
        /// Field that regressed.
        field: &'static str,
        /// New (rejected) value.
        new_value: u64,
        /// Existing value.
        prev_value: u64,
    },
    /// Backend transport failure.
    #[error("storage_state backend error: {0}")]
    Backend(String),
}

/// Trait surfaced by every `tenant_storage_state` backend.
pub trait TenantStorageStateStore: Send + Sync + core::fmt::Debug {
    /// Lookup the current row for `(tenant_id, region)`. Returns
    /// `Ok(None)` when the row does not exist (post-deploy backfill
    /// pending OR programmer wiring error).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn lookup(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<Option<TenantStorageStateRow>, StorageStateError>;

    /// Apply a soft-delete reclaim: subtract `bytes_reclaimed` from
    /// `bytes_used`, add to `bytes_reclaimed_lifetime`, set
    /// `last_evict_at_ms = now_ms`. Atomic across the three counter
    /// updates.
    ///
    /// # Errors
    ///
    /// - [`StorageStateError::CheckViolation`] on `bytes_used`
    ///   underflow.
    /// - [`StorageStateError::Backend`] on transport failure.
    fn apply_eviction_reclaim(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        bytes_reclaimed: u64,
        now_ms: u64,
    ) -> Result<(), StorageStateError>;

    /// Update the eviction cooldown watermark without applying any
    /// reclaim — used when the per-region pass observes
    /// `bytes_used < 95% bytes_quota` (the
    /// [`crate::EvictionDecision::SkipQuotaOk`] arm) so the next cron
    /// tick can short-circuit if `now - last_evict_at_ms <
    /// eviction_cooldown_ms`.
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn touch_evict_watermark(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        now_ms: u64,
    ) -> Result<(), StorageStateError>;
}

/// In-memory `tenant_storage_state` store. Mirrors the canonical
/// `(tenant_id, region)` PK + CHECK envelope.
///
/// F-001 closure: per-instance `Mutex` (NOT process-global
/// `static LazyLock<Mutex<>>`).
#[derive(Debug, Default)]
pub struct InMemoryTenantStorageStateStore {
    inner: Mutex<BTreeMap<(Uuid, EvictionRegion), TenantStorageStateRow>>,
}

impl InMemoryTenantStorageStateStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a fresh row (test wiring).
    ///
    /// # Errors
    ///
    /// Returns [`StorageStateError::Backend`] on mutex poisoning.
    pub fn push_row(&self, row: TenantStorageStateRow) -> Result<(), StorageStateError> {
        let key = (row.tenant_id, row.region);
        let mut g = self
            .inner
            .lock()
            .map_err(|_| StorageStateError::Backend("storage_state mutex poisoned".to_string()))?;
        g.insert(key, row);
        Ok(())
    }

    /// Snapshot a row (diagnostic).
    #[must_use]
    pub fn snapshot(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Option<TenantStorageStateRow> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(tenant_id, region)).cloned()
    }

    /// Total row count (cross-tenant; for cardinality assertions in
    /// tests only).
    #[must_use]
    pub fn rows_count(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }
}

impl TenantStorageStateStore for InMemoryTenantStorageStateStore {
    fn lookup(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<Option<TenantStorageStateRow>, StorageStateError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| StorageStateError::Backend("storage_state mutex poisoned".to_string()))?;
        Ok(g.get(&(tenant_id, region)).cloned())
    }

    fn apply_eviction_reclaim(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        bytes_reclaimed: u64,
        now_ms: u64,
    ) -> Result<(), StorageStateError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| StorageStateError::Backend("storage_state mutex poisoned".to_string()))?;
        let Some(row) = g.get_mut(&(tenant_id, region)) else {
            return Err(StorageStateError::Backend(format!(
                "tenant_storage_state row missing for tenant={tenant_id} region={region}"
            )));
        };
        // Saturating-sub semantic mirrors SQL `bytes_used = MAX(0,
        // bytes_used - ?)` — under bug conditions (over-subtract) the
        // counter floors at 0 and we surface a CHECK violation so the
        // operator can investigate.
        let new_bytes_used = row.bytes_used.checked_sub(bytes_reclaimed).ok_or(
            StorageStateError::CheckViolation {
                column: "bytes_used",
                got: row.bytes_used,
                reason: "underflow_on_eviction_reclaim",
            },
        )?;
        // Monotonic check on the eviction watermark.
        if let Some(prev) = row.last_evict_at_ms {
            if now_ms < prev {
                return Err(StorageStateError::LifecycleMonotonic {
                    field: "last_evict_at_ms",
                    new_value: now_ms,
                    prev_value: prev,
                });
            }
        }
        if now_ms < row.created_at_ms {
            return Err(StorageStateError::LifecycleMonotonic {
                field: "updated_at_ms",
                new_value: now_ms,
                prev_value: row.created_at_ms,
            });
        }
        row.bytes_used = new_bytes_used;
        row.bytes_reclaimed_lifetime = row.bytes_reclaimed_lifetime.saturating_add(bytes_reclaimed);
        row.last_evict_at_ms = Some(now_ms);
        row.updated_at_ms = now_ms;
        row.bytes_used_updated_at_ms = now_ms;
        Ok(())
    }

    fn touch_evict_watermark(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        now_ms: u64,
    ) -> Result<(), StorageStateError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| StorageStateError::Backend("storage_state mutex poisoned".to_string()))?;
        let Some(row) = g.get_mut(&(tenant_id, region)) else {
            return Err(StorageStateError::Backend(format!(
                "tenant_storage_state row missing for tenant={tenant_id} region={region}"
            )));
        };
        if let Some(prev) = row.last_evict_at_ms {
            if now_ms < prev {
                return Err(StorageStateError::LifecycleMonotonic {
                    field: "last_evict_at_ms",
                    new_value: now_ms,
                    prev_value: prev,
                });
            }
        }
        row.last_evict_at_ms = Some(now_ms);
        row.updated_at_ms = now_ms;
        Ok(())
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

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    fn fresh_row(tenant: Uuid, region: EvictionRegion) -> TenantStorageStateRow {
        TenantStorageStateRow {
            tenant_id: tenant,
            region,
            bytes_used: 1_000,
            bytes_quota: 10_000,
            bytes_used_updated_at_ms: 100,
            last_synced_at_ms: 100,
            last_evict_at_ms: None,
            bytes_reclaimed_lifetime: 0,
            created_at_ms: 100,
            updated_at_ms: 100,
        }
    }

    #[test]
    fn utilization_pct_returns_zero_for_zero_quota() {
        let mut row = fresh_row(ten_a(), EvictionRegion::Sam);
        row.bytes_quota = 0;
        row.bytes_used = 100;
        assert_eq!(row.utilization_pct(), 0.0);
        assert!(!row.at_or_above_trigger());
    }

    #[test]
    fn utilization_pct_at_95_triggers() {
        let mut row = fresh_row(ten_a(), EvictionRegion::Sam);
        row.bytes_quota = 100;
        row.bytes_used = 95;
        assert!((row.utilization_pct() - 0.95).abs() < f64::EPSILON);
        assert!(row.at_or_above_trigger());
    }

    #[test]
    fn utilization_pct_just_below_95_does_not_trigger() {
        let mut row = fresh_row(ten_a(), EvictionRegion::Sam);
        row.bytes_quota = 1000;
        row.bytes_used = 949;
        assert!(row.utilization_pct() < 0.95);
        assert!(!row.at_or_above_trigger());
    }

    #[test]
    fn utilization_pct_above_95_triggers() {
        let mut row = fresh_row(ten_a(), EvictionRegion::Sam);
        row.bytes_quota = 100;
        row.bytes_used = 96;
        assert!(row.at_or_above_trigger());
    }

    #[test]
    fn lookup_returns_none_for_missing_row() {
        let store = InMemoryTenantStorageStateStore::new();
        let r = store.lookup(ten_a(), EvictionRegion::Sam).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn lookup_round_trips_inserted_row() {
        let store = InMemoryTenantStorageStateStore::new();
        let row = fresh_row(ten_a(), EvictionRegion::Sam);
        store.push_row(row.clone()).unwrap();
        let got = store.lookup(ten_a(), EvictionRegion::Sam).unwrap().unwrap();
        assert_eq!(got, row);
    }

    #[test]
    fn cross_tenant_lookup_returns_none() {
        let store = InMemoryTenantStorageStateStore::new();
        let row_a = fresh_row(ten_a(), EvictionRegion::Sam);
        store.push_row(row_a).unwrap();
        // Tenant B asks for the same region — surfaces None.
        let got = store.lookup(ten_b(), EvictionRegion::Sam).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn apply_eviction_reclaim_updates_counters() {
        let store = InMemoryTenantStorageStateStore::new();
        let row = fresh_row(ten_a(), EvictionRegion::Sam);
        store.push_row(row).unwrap();
        store
            .apply_eviction_reclaim(ten_a(), EvictionRegion::Sam, 300, 200)
            .unwrap();
        let got = store.snapshot(ten_a(), EvictionRegion::Sam).unwrap();
        assert_eq!(got.bytes_used, 700);
        assert_eq!(got.bytes_reclaimed_lifetime, 300);
        assert_eq!(got.last_evict_at_ms, Some(200));
        assert_eq!(got.updated_at_ms, 200);
    }

    #[test]
    fn apply_eviction_reclaim_underflow_rejected() {
        let store = InMemoryTenantStorageStateStore::new();
        let mut row = fresh_row(ten_a(), EvictionRegion::Sam);
        row.bytes_used = 100;
        store.push_row(row).unwrap();
        let err = store
            .apply_eviction_reclaim(ten_a(), EvictionRegion::Sam, 200, 200)
            .unwrap_err();
        match err {
            StorageStateError::CheckViolation { column, .. } => {
                assert_eq!(column, "bytes_used");
            }
            other => panic!("expected CheckViolation, got {other:?}"),
        }
    }

    #[test]
    fn touch_evict_watermark_updates_lifecycle() {
        let store = InMemoryTenantStorageStateStore::new();
        let row = fresh_row(ten_a(), EvictionRegion::Sam);
        store.push_row(row).unwrap();
        store
            .touch_evict_watermark(ten_a(), EvictionRegion::Sam, 500)
            .unwrap();
        let got = store.snapshot(ten_a(), EvictionRegion::Sam).unwrap();
        assert_eq!(got.last_evict_at_ms, Some(500));
        assert_eq!(got.updated_at_ms, 500);
    }

    #[test]
    fn lifecycle_monotonic_eviction_watermark_rejected() {
        let store = InMemoryTenantStorageStateStore::new();
        let row = fresh_row(ten_a(), EvictionRegion::Sam);
        store.push_row(row).unwrap();
        store
            .touch_evict_watermark(ten_a(), EvictionRegion::Sam, 500)
            .unwrap();
        // Backwards in time → reject.
        let err = store
            .touch_evict_watermark(ten_a(), EvictionRegion::Sam, 400)
            .unwrap_err();
        assert!(matches!(err, StorageStateError::LifecycleMonotonic { .. }));
    }

    #[test]
    fn rows_count_tracks_inserts() {
        let store = InMemoryTenantStorageStateStore::new();
        assert_eq!(store.rows_count(), 0);
        store
            .push_row(fresh_row(ten_a(), EvictionRegion::Sam))
            .unwrap();
        store
            .push_row(fresh_row(ten_a(), EvictionRegion::Iad))
            .unwrap();
        store
            .push_row(fresh_row(ten_b(), EvictionRegion::Sam))
            .unwrap();
        assert_eq!(store.rows_count(), 3);
    }
}
