//! Reservation lifecycle: trait + InMemory fake mirroring the SQL
//! `quota_reservations` table byte-for-byte.
//!
//! ## Lifecycle
//!
//! 1. **Reserve** — middleware calls `insert(tenant, region, request_bytes)`;
//!    tracker generates a `ReservationId`, computes `expires_at_ms` via
//!    [`corelink_eviction::reservation_ttl_ms`] (size-proportional formula
//!    `max(60s, request_bytes/1MB/s × 2x), capped 7d` per Lote 10.7bis
//!    R5 P0-2; reused canonical), inserts the row.
//! 2. **Commit** — handler calls `commit(tenant, reservation_id)` post-write;
//!    tracker removes the row + signals the caller the reserved bytes
//!    should roll into `tenant_storage_state.bytes_used`.
//! 3. **Release** — handler calls `release(tenant, reservation_id)` on
//!    write failure; tracker removes the row WITHOUT rolling into
//!    `bytes_used` (idempotent — release of a TTL-expired reservation
//!    is a no-op `Ok(false)`).
//! 4. **TTL sweep** — `sweep_expired(now_ms)` removes every row whose
//!    `expires_at_ms < now_ms` (DO alarm cleanup mirror; periodic D1
//!    sweeper as the durable backstop).
//!
//! ## Invariants enforced (mirror SQL CHECK envelope)
//!
//! - **Tenant-leftmost** (CTRL-ISO-005): the in-memory `BTreeMap` key
//!   is `(tenant_id, reservation_id)`; tenant A `lookup` for a
//!   reservation owned by tenant B returns `Ok(None)`.
//! - **Monotone immutable `requested_bytes`**: once a row is inserted,
//!   `requested_bytes` is never mutated. Tests pin this via the
//!   `requested_bytes_immutable_after_insert` regression.
//! - **TTL strictly positive**: `expires_at_ms > created_at_ms`
//!   enforced at insert time (matches SQL CHECK).
//! - **F-001 closure**: per-instance `Mutex` (NOT process-global
//!   `static LazyLock<Mutex<>>`).

use std::collections::BTreeMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use corelink_eviction::{reservation_ttl_ms, EvictionRegion};

/// Newtype for reservation identity.
///
/// Production wiring uses UUIDv7 (`Uuid::now_v7`); tests pass an
/// explicit `Uuid` via `ReservationId::from_uuid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReservationId(Uuid);

impl ReservationId {
    /// Construct from a raw UUID (test wiring).
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Inner UUID.
    #[must_use]
    pub const fn into_uuid(self) -> Uuid {
        self.0
    }

    /// Borrow the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl core::fmt::Display for ReservationId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

/// Materialised projection of a `quota_reservations` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReservationRow {
    /// Tenant scope.
    pub tenant_id: Uuid,
    /// Reservation identity.
    pub reservation_id: ReservationId,
    /// Region scope.
    pub region: EvictionRegion,
    /// Bytes reserved (immutable post-insert).
    pub requested_bytes: u64,
    /// Expiration watermark (Unix ms; immutable).
    pub expires_at_ms: u64,
    /// Creation watermark (Unix ms; immutable).
    pub created_at_ms: u64,
}

/// Errors surfaced by [`ReservationTracker`] backends.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReservationTrackerError {
    /// Backend transport failure (DO storage / D1 / fake mutex
    /// poisoning).
    #[error("reservation tracker backend error: {0}")]
    Backend(String),
    /// CHECK violation — `expires_at_ms <= created_at_ms` would
    /// surface in the SQL CHECK envelope; mirror at API surface.
    #[error("reservation TTL CHECK violation: expires_at_ms={expires_at_ms} <= created_at_ms={created_at_ms}")]
    InvalidTtl {
        /// Created watermark.
        created_at_ms: u64,
        /// Expires watermark.
        expires_at_ms: u64,
    },
}

/// Trait surfaced by every reservation backend (production CF DO
/// storage / fake / D1 mirror).
pub trait ReservationTracker: Send + Sync + core::fmt::Debug {
    /// Insert a fresh reservation. Returns the new [`ReservationRow`].
    /// `reservation_id` is supplied by the caller (in production, the
    /// DO generates UUIDv7; in tests, the harness picks a deterministic
    /// id).
    ///
    /// `expires_at_ms` is computed as
    /// `created_at_ms + reservation_ttl_ms(requested_bytes)` — the
    /// size-proportional canonical formula reused from
    /// `corelink-eviction`.
    ///
    /// # Errors
    ///
    /// Returns [`ReservationTrackerError::InvalidTtl`] if the resulting
    /// `expires_at_ms <= created_at_ms` (saturating-add overflow);
    /// [`ReservationTrackerError::Backend`] on transport failure.
    fn insert(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
        region: EvictionRegion,
        requested_bytes: u64,
        created_at_ms: u64,
    ) -> Result<ReservationRow, ReservationTrackerError>;

    /// Remove the reservation. Returns `Ok(Some(row))` if the row
    /// existed; `Ok(None)` if not found (TTL-expired or already
    /// removed — idempotent).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn remove(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
    ) -> Result<Option<ReservationRow>, ReservationTrackerError>;

    /// Lookup the row. Returns `Ok(None)` if not found OR if the
    /// caller's tenant does not own the reservation.
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn lookup(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
    ) -> Result<Option<ReservationRow>, ReservationTrackerError>;

    /// Sum the active reservations (rows with `expires_at_ms > now_ms`)
    /// for `(tenant_id, region)`. Drives the `bytes_used +
    /// active_reservations + request_bytes <= bytes_quota` boundary
    /// check.
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn sum_active_bytes(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        now_ms: u64,
    ) -> Result<u64, ReservationTrackerError>;

    /// Sweep TTL-expired rows. Returns the count of rows removed.
    /// Production wiring schedules this via DO alarm every 60s;
    /// integration tests call directly.
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn sweep_expired(&self, now_ms: u64) -> Result<u64, ReservationTrackerError>;

    /// Total active row count (cross-tenant; for cardinality assertions
    /// in tests + DASH-DEDUP "active reservations" widget).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn active_row_count(&self) -> Result<usize, ReservationTrackerError>;
}

/// In-memory reservation tracker. Mirrors the canonical
/// `(tenant_id, reservation_id)` PK + CHECK envelope. F-001 closure
/// preserved (per-instance `Mutex`; no process-global state).
#[derive(Debug, Default)]
pub struct InMemoryReservationTracker {
    inner: Mutex<BTreeMap<(Uuid, ReservationId), ReservationRow>>,
}

impl InMemoryReservationTracker {
    /// Construct an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every row (diagnostic; for property tests).
    #[must_use]
    pub fn snapshot_rows(&self) -> Vec<ReservationRow> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.values().cloned().collect()
    }
}

impl ReservationTracker for InMemoryReservationTracker {
    fn insert(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
        region: EvictionRegion,
        requested_bytes: u64,
        created_at_ms: u64,
    ) -> Result<ReservationRow, ReservationTrackerError> {
        let ttl_ms = reservation_ttl_ms(requested_bytes);
        let expires_at_ms = created_at_ms.saturating_add(ttl_ms);
        if expires_at_ms <= created_at_ms {
            return Err(ReservationTrackerError::InvalidTtl {
                created_at_ms,
                expires_at_ms,
            });
        }
        let row = ReservationRow {
            tenant_id,
            reservation_id,
            region,
            requested_bytes,
            expires_at_ms,
            created_at_ms,
        };
        let mut g = self.inner.lock().map_err(|_| {
            ReservationTrackerError::Backend("reservation tracker mutex poisoned".to_string())
        })?;
        g.insert((tenant_id, reservation_id), row.clone());
        Ok(row)
    }

    fn remove(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
    ) -> Result<Option<ReservationRow>, ReservationTrackerError> {
        let mut g = self.inner.lock().map_err(|_| {
            ReservationTrackerError::Backend("reservation tracker mutex poisoned".to_string())
        })?;
        Ok(g.remove(&(tenant_id, reservation_id)))
    }

    fn lookup(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
    ) -> Result<Option<ReservationRow>, ReservationTrackerError> {
        let g = self.inner.lock().map_err(|_| {
            ReservationTrackerError::Backend("reservation tracker mutex poisoned".to_string())
        })?;
        Ok(g.get(&(tenant_id, reservation_id)).cloned())
    }

    fn sum_active_bytes(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        now_ms: u64,
    ) -> Result<u64, ReservationTrackerError> {
        let g = self.inner.lock().map_err(|_| {
            ReservationTrackerError::Backend("reservation tracker mutex poisoned".to_string())
        })?;
        let mut total: u64 = 0;
        for ((t, _), row) in g.iter() {
            if *t != tenant_id {
                continue;
            }
            if row.region != region {
                continue;
            }
            if row.expires_at_ms <= now_ms {
                continue;
            }
            total = total.saturating_add(row.requested_bytes);
        }
        Ok(total)
    }

    fn sweep_expired(&self, now_ms: u64) -> Result<u64, ReservationTrackerError> {
        let mut g = self.inner.lock().map_err(|_| {
            ReservationTrackerError::Backend("reservation tracker mutex poisoned".to_string())
        })?;
        let before = g.len();
        g.retain(|_, row| row.expires_at_ms > now_ms);
        let after = g.len();
        Ok((before - after) as u64)
    }

    fn active_row_count(&self) -> Result<usize, ReservationTrackerError> {
        let g = self.inner.lock().map_err(|_| {
            ReservationTrackerError::Backend("reservation tracker mutex poisoned".to_string())
        })?;
        Ok(g.len())
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

    fn rid(seed: u128) -> ReservationId {
        ReservationId::from_uuid(Uuid::from_u128(seed))
    }

    #[test]
    fn insert_round_trips_canonical_row() {
        let t = InMemoryReservationTracker::new();
        let row = t
            .insert(ten_a(), rid(1), EvictionRegion::Sam, 1024, 100)
            .unwrap();
        assert_eq!(row.tenant_id, ten_a());
        assert_eq!(row.reservation_id, rid(1));
        assert_eq!(row.region, EvictionRegion::Sam);
        assert_eq!(row.requested_bytes, 1024);
        assert_eq!(row.created_at_ms, 100);
        // 1024 bytes → floor (60s = 60_000 ms); expires_at = 100 + 60_000.
        assert_eq!(row.expires_at_ms, 60_100);
    }

    #[test]
    fn insert_uses_canonical_size_proportional_ttl() {
        let t = InMemoryReservationTracker::new();
        // 100 MiB → above floor.
        let row = t
            .insert(ten_a(), rid(1), EvictionRegion::Sam, 100 * 1024 * 1024, 100)
            .unwrap();
        // 100 MiB / 1024 × 2 = 204_800 ms.
        assert_eq!(row.expires_at_ms, 100 + 204_800);
    }

    #[test]
    fn insert_invalid_ttl_when_overflow() {
        // saturating_add would saturate at u64::MAX − so created_at_ms
        // = u64::MAX yields expires_at_ms = u64::MAX (which is > created
        // by 0, not > by ttl_ms). The check should fire because
        // saturating_add cannot produce a strictly greater value.
        let t = InMemoryReservationTracker::new();
        let err = t
            .insert(ten_a(), rid(1), EvictionRegion::Sam, 1024, u64::MAX)
            .unwrap_err();
        assert!(matches!(err, ReservationTrackerError::InvalidTtl { .. }));
    }

    #[test]
    fn lookup_round_trips_inserted() {
        let t = InMemoryReservationTracker::new();
        let row = t
            .insert(ten_a(), rid(1), EvictionRegion::Sam, 4096, 100)
            .unwrap();
        let got = t.lookup(ten_a(), rid(1)).unwrap().unwrap();
        assert_eq!(got, row);
    }

    #[test]
    fn lookup_cross_tenant_returns_none() {
        let t = InMemoryReservationTracker::new();
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 4096, 100)
            .unwrap();
        let got = t.lookup(ten_b(), rid(1)).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn remove_returns_row_idempotent() {
        let t = InMemoryReservationTracker::new();
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 4096, 100)
            .unwrap();
        let r1 = t.remove(ten_a(), rid(1)).unwrap();
        assert!(r1.is_some());
        // Second remove is idempotent.
        let r2 = t.remove(ten_a(), rid(1)).unwrap();
        assert!(r2.is_none());
    }

    #[test]
    fn remove_cross_tenant_does_not_remove() {
        let t = InMemoryReservationTracker::new();
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 4096, 100)
            .unwrap();
        // Tenant B tries to remove tenant A's reservation.
        let r = t.remove(ten_b(), rid(1)).unwrap();
        assert!(r.is_none());
        // Tenant A's row still exists.
        assert!(t.lookup(ten_a(), rid(1)).unwrap().is_some());
    }

    #[test]
    fn sum_active_bytes_excludes_expired() {
        let t = InMemoryReservationTracker::new();
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 1024, 100)
            .unwrap(); // expires_at = 60_100.
        t.insert(ten_a(), rid(2), EvictionRegion::Sam, 2048, 100)
            .unwrap();
        // Now=10 → both active.
        let sum = t
            .sum_active_bytes(ten_a(), EvictionRegion::Sam, 10)
            .unwrap();
        assert_eq!(sum, 1024 + 2048);
        // Now=70_000 → both expired (60_100 < 70_000).
        let sum = t
            .sum_active_bytes(ten_a(), EvictionRegion::Sam, 70_000)
            .unwrap();
        assert_eq!(sum, 0);
    }

    #[test]
    fn sum_active_bytes_excludes_other_tenant() {
        let t = InMemoryReservationTracker::new();
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 5000, 100)
            .unwrap();
        t.insert(ten_b(), rid(2), EvictionRegion::Sam, 7000, 100)
            .unwrap();
        let sum = t
            .sum_active_bytes(ten_a(), EvictionRegion::Sam, 10)
            .unwrap();
        assert_eq!(sum, 5000);
    }

    #[test]
    fn sum_active_bytes_excludes_other_region() {
        let t = InMemoryReservationTracker::new();
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 5000, 100)
            .unwrap();
        t.insert(ten_a(), rid(2), EvictionRegion::Iad, 7000, 100)
            .unwrap();
        let sum = t
            .sum_active_bytes(ten_a(), EvictionRegion::Sam, 10)
            .unwrap();
        assert_eq!(sum, 5000);
    }

    #[test]
    fn sweep_expired_removes_only_expired() {
        let t = InMemoryReservationTracker::new();
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 1024, 100)
            .unwrap(); // expires 60_100.
        t.insert(ten_a(), rid(2), EvictionRegion::Sam, 1024, 70_000)
            .unwrap(); // expires 130_000.
        let removed = t.sweep_expired(70_000).unwrap();
        // Only rid(1) expired.
        assert_eq!(removed, 1);
        let active = t.active_row_count().unwrap();
        assert_eq!(active, 1);
    }

    #[test]
    fn requested_bytes_immutable_after_insert() {
        // Property: a row's `requested_bytes` field is set at insert
        // and never mutated. The trait surface has no API to mutate it.
        let t = InMemoryReservationTracker::new();
        let row1 = t
            .insert(ten_a(), rid(1), EvictionRegion::Sam, 1024, 100)
            .unwrap();
        let row2 = t.lookup(ten_a(), rid(1)).unwrap().unwrap();
        assert_eq!(row1.requested_bytes, row2.requested_bytes);
    }

    #[test]
    fn active_row_count_tracks_inserts() {
        let t = InMemoryReservationTracker::new();
        assert_eq!(t.active_row_count().unwrap(), 0);
        t.insert(ten_a(), rid(1), EvictionRegion::Sam, 1024, 100)
            .unwrap();
        t.insert(ten_b(), rid(2), EvictionRegion::Iad, 2048, 100)
            .unwrap();
        assert_eq!(t.active_row_count().unwrap(), 2);
        t.remove(ten_a(), rid(1)).unwrap();
        assert_eq!(t.active_row_count().unwrap(), 1);
    }

    #[test]
    fn reservation_id_display_matches_inner_uuid() {
        let id = rid(0xdead_beef);
        let s = format!("{id}");
        let inner = format!("{}", Uuid::from_u128(0xdead_beef));
        assert_eq!(s, inner);
    }
}
