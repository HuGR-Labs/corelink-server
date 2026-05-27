//! Atomic CAS-versioned tenant storage state — the canonical
//! `tenant_storage_state.bytes_used` mirror with an explicit monotone
//! `cas_version` counter (mirrors DO actor model serialisation).
//!
//! ## Why a dedicated state surface
//!
//! `corelink-eviction::TenantStorageStateStore` (S-07 canonical) does
//! not surface a CAS version (it is a passive read-mostly store backed
//! by D1). The canonical S-08 hard-block path requires an explicit
//! version counter so the orchestrator can detect mid-flight bumps
//! (concurrent commit_reservation / eviction_reclaim) and retry without
//! over-counting.
//!
//! The trait surface here is intentionally narrow: snapshot-read +
//! conditional-write keyed on `(tenant, region, cas_version)`. Production
//! wiring at WI-S08-006 implements this against a CF DO singleton's
//! durable storage; the in-memory fake mirrors the actor semantic
//! byte-for-byte.

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use corelink_eviction::EvictionRegion;

/// Snapshot of a tenant's atomic CAS-versioned storage state.
///
/// Mirrors the canonical `tenant_storage_state` row at WI-S07-002 with
/// an explicit `cas_version` field driving the optimistic CAS write
/// path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtomicTenantBytesState {
    /// Tenant scope.
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: EvictionRegion,
    /// Current bytes used (mutable telemetry; running counter).
    pub bytes_used: u64,
    /// Quota ceiling (POLICY; immutable per period).
    pub bytes_quota: u64,
    /// Monotone CAS version. Bumps on every successful mutation
    /// (try_acquire_commit / eviction_reclaim / period_reset). Reads
    /// snapshot the version; conditional writes succeed only if the
    /// observed version matches.
    pub cas_version: u64,
}

/// Errors surfaced by [`AtomicCasState`] backends.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AtomicCasStateError {
    /// Tenant + region row missing.
    #[error("atomic cas state row missing for tenant={tenant_id} region={region}")]
    Missing {
        /// Tenant scope.
        tenant_id: Uuid,
        /// Region (canonical 5-region literal).
        region: &'static str,
    },
    /// CAS version mismatch — write would clobber a mid-flight bump.
    /// Caller MUST surface this as a retry signal (NOT a hard error);
    /// the orchestrator handles the retry-loop.
    #[error(
        "atomic cas version mismatch: observed={observed} actual={actual}"
    )]
    VersionMismatch {
        /// Version the caller observed (stale).
        observed: u64,
        /// Current actual version (live).
        actual: u64,
    },
    /// Bytes-used overflow (would clobber u64::MAX on commit).
    #[error("bytes_used overflow: current={current} delta={delta}")]
    Overflow {
        /// Current value.
        current: u64,
        /// Delta requested.
        delta: u64,
    },
    /// Backend transport failure (DO storage / D1 / fake mutex
    /// poisoning).
    #[error("atomic cas state backend error: {0}")]
    Backend(String),
}

/// Trait surface for the atomic CAS-versioned storage state. Production
/// wiring at WI-S08-006 implements this against a CF DO singleton's
/// durable storage; the in-memory fake mirrors the actor semantic
/// byte-for-byte.
pub trait AtomicCasState: Send + Sync + core::fmt::Debug {
    /// Snapshot-read the tenant's state. Returns `Ok(None)` if the row
    /// is missing (caller should surface as 5xx — programmer wiring
    /// error / post-deploy backfill pending).
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn lookup(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<Option<AtomicTenantBytesState>, AtomicCasStateError>;

    /// Conditional-write: attempt to commit `delta_bytes` to
    /// `bytes_used`, succeeding only if `expected_version` matches the
    /// current version. Returns the new state on success.
    ///
    /// # Errors
    ///
    /// - [`AtomicCasStateError::VersionMismatch`] on stale-version
    ///   write (caller's retry signal).
    /// - [`AtomicCasStateError::Overflow`] on bytes_used overflow.
    /// - [`AtomicCasStateError::Missing`] on missing row.
    /// - Backend transport failures.
    fn try_commit_delta(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        expected_version: u64,
        delta_bytes: u64,
    ) -> Result<AtomicTenantBytesState, AtomicCasStateError>;

    /// Seed a fresh row (admin / test wiring only). Production wiring
    /// uses a separate admin-plane endpoint; the trait surface allows
    /// the in-memory fake to seed test fixtures inline.
    ///
    /// Returns the seeded state.
    ///
    /// # Errors
    ///
    /// Backend transport failures.
    fn seed_row(
        &self,
        state: AtomicTenantBytesState,
    ) -> Result<(), AtomicCasStateError>;
}

/// In-memory atomic CAS state. Mirrors the canonical DO actor model
/// serialisation byte-for-byte: a per-instance `Mutex` envelopes the
/// `(tenant, region) → AtomicTenantBytesState` `HashMap`; concurrent
/// `try_commit_delta` calls observing the same `cas_version` cannot
/// both succeed.
#[derive(Default, Debug)]
pub struct InMemoryAtomicCasState {
    inner: Mutex<HashMap<(Uuid, EvictionRegion), AtomicTenantBytesState>>,
}

impl InMemoryAtomicCasState {
    /// Construct a fresh in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every row (for test fixtures + cardinality assertions).
    #[must_use]
    pub fn snapshot(&self) -> Vec<AtomicTenantBytesState> {
        match self.inner.lock() {
            Ok(g) => g.values().copied().collect(),
            Err(p) => p.into_inner().values().copied().collect(),
        }
    }
}

impl AtomicCasState for InMemoryAtomicCasState {
    fn lookup(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<Option<AtomicTenantBytesState>, AtomicCasStateError> {
        let g = self.inner.lock().map_err(|_| {
            AtomicCasStateError::Backend(
                "atomic cas state mutex poisoned".to_string(),
            )
        })?;
        Ok(g.get(&(tenant_id, region)).copied())
    }

    fn try_commit_delta(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        expected_version: u64,
        delta_bytes: u64,
    ) -> Result<AtomicTenantBytesState, AtomicCasStateError> {
        let mut g = self.inner.lock().map_err(|_| {
            AtomicCasStateError::Backend(
                "atomic cas state mutex poisoned".to_string(),
            )
        })?;
        let cur_opt = g.get(&(tenant_id, region)).copied();
        let cur = match cur_opt {
            Some(s) => s,
            None => {
                return Err(AtomicCasStateError::Missing {
                    tenant_id,
                    region: region.as_str(),
                });
            }
        };
        if cur.cas_version != expected_version {
            return Err(AtomicCasStateError::VersionMismatch {
                observed: expected_version,
                actual: cur.cas_version,
            });
        }
        let new_used = cur.bytes_used.checked_add(delta_bytes).ok_or(
            AtomicCasStateError::Overflow {
                current: cur.bytes_used,
                delta: delta_bytes,
            },
        )?;
        let new_version = cur.cas_version.saturating_add(1);
        let new_state = AtomicTenantBytesState {
            tenant_id: cur.tenant_id,
            region: cur.region,
            bytes_used: new_used,
            bytes_quota: cur.bytes_quota,
            cas_version: new_version,
        };
        g.insert((tenant_id, region), new_state);
        Ok(new_state)
    }

    fn seed_row(
        &self,
        state: AtomicTenantBytesState,
    ) -> Result<(), AtomicCasStateError> {
        let mut g = self.inner.lock().map_err(|_| {
            AtomicCasStateError::Backend(
                "atomic cas state mutex poisoned".to_string(),
            )
        })?;
        g.insert((state.tenant_id, state.region), state);
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

    fn fresh_state(
        tenant: Uuid,
        used: u64,
        quota: u64,
        ver: u64,
    ) -> AtomicTenantBytesState {
        AtomicTenantBytesState {
            tenant_id: tenant,
            region: EvictionRegion::Sam,
            bytes_used: used,
            bytes_quota: quota,
            cas_version: ver,
        }
    }

    #[test]
    fn lookup_empty_returns_none() {
        let s = InMemoryAtomicCasState::new();
        assert_eq!(
            s.lookup(Uuid::nil(), EvictionRegion::Sam).unwrap(),
            None
        );
    }

    #[test]
    fn seed_then_lookup_returns_seeded() {
        let s = InMemoryAtomicCasState::new();
        let t = Uuid::from_u128(0xa);
        let st = fresh_state(t, 50, 100, 0);
        s.seed_row(st).unwrap();
        let got = s.lookup(t, EvictionRegion::Sam).unwrap().unwrap();
        assert_eq!(got, st);
    }

    #[test]
    fn try_commit_delta_succeeds_with_matching_version() {
        let s = InMemoryAtomicCasState::new();
        let t = Uuid::from_u128(0xa);
        s.seed_row(fresh_state(t, 50, 100, 0)).unwrap();
        let new_state = s
            .try_commit_delta(t, EvictionRegion::Sam, 0, 10)
            .unwrap();
        assert_eq!(new_state.bytes_used, 60);
        assert_eq!(new_state.cas_version, 1);
    }

    #[test]
    fn try_commit_delta_fails_with_stale_version() {
        let s = InMemoryAtomicCasState::new();
        let t = Uuid::from_u128(0xa);
        s.seed_row(fresh_state(t, 50, 100, 5)).unwrap();
        let err = s
            .try_commit_delta(t, EvictionRegion::Sam, 4, 10)
            .unwrap_err();
        assert!(matches!(
            err,
            AtomicCasStateError::VersionMismatch {
                observed: 4,
                actual: 5
            }
        ));
    }

    #[test]
    fn try_commit_delta_returns_overflow_on_max_used() {
        let s = InMemoryAtomicCasState::new();
        let t = Uuid::from_u128(0xa);
        s.seed_row(fresh_state(t, u64::MAX, u64::MAX, 0)).unwrap();
        let err = s
            .try_commit_delta(t, EvictionRegion::Sam, 0, 1)
            .unwrap_err();
        assert!(matches!(err, AtomicCasStateError::Overflow { .. }));
    }

    #[test]
    fn try_commit_delta_returns_missing_on_unknown_row() {
        let s = InMemoryAtomicCasState::new();
        let t = Uuid::from_u128(0xa);
        let err = s
            .try_commit_delta(t, EvictionRegion::Sam, 0, 10)
            .unwrap_err();
        assert!(matches!(err, AtomicCasStateError::Missing { .. }));
    }

    #[test]
    fn snapshot_returns_seeded_rows() {
        let s = InMemoryAtomicCasState::new();
        s.seed_row(fresh_state(Uuid::from_u128(0xa), 1, 100, 0))
            .unwrap();
        s.seed_row(fresh_state(Uuid::from_u128(0xb), 2, 200, 0))
            .unwrap();
        assert_eq!(s.snapshot().len(), 2);
    }

    #[test]
    fn version_increments_monotonically() {
        let s = InMemoryAtomicCasState::new();
        let t = Uuid::from_u128(0xa);
        s.seed_row(fresh_state(t, 0, 1_000, 0)).unwrap();
        for i in 0u64..5 {
            let st = s
                .try_commit_delta(t, EvictionRegion::Sam, i, 1)
                .unwrap();
            assert_eq!(st.cas_version, i + 1);
            assert_eq!(st.bytes_used, i + 1);
        }
    }
}
