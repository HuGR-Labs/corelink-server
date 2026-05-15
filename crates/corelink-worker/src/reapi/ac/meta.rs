//! `ac_meta` row store trait + InMemory fake (WI-S04-001 §6.1.5–§6.1.10).
//!
//! Mirrors the trait-abstraction-defer pattern used by S-01 / S-02 /
//! S-03 (per charter): the AC handler depends on the [`AcMetaStore`]
//! trait surface; this crate ships [`InMemoryAcMetaStore`] that
//! preserves every documented semantic; the real Cloudflare D1 binding
//! shim is wired in WI-S04-006 alongside the `corelink-reapi` host-
//! server REAPI mounting (ADR-0036 governs the migration).
//!
//! ## Tenant isolation seam
//!
//! [`AcKey::new`] is the **only** way to address a row; it takes the
//! tenant_id by value. The trait surface has no method that accepts a
//! "raw" `(tenant_id, action_digest)` pair without going through this
//! constructor, so cross-tenant lookups are unreachable through the
//! public API (mirrors `corelink_meta::BlobMetaKey`).
//!
//! ## Idempotent UPDATE contract (`INV-AC-IDEMPOTENT`)
//!
//! [`AcMetaStore::upsert`] resolves to one of three outcomes:
//!
//! - [`AcMetaUpsertOutcome::Inserted`] — first-time write for the
//!   `(tenant_id, action_digest)` PK; row materialized with all
//!   payload columns. Audit emits `ac.update.ok`.
//! - [`AcMetaUpsertOutcome::IdempotentRefresh`] — row exists with the
//!   **same** `result_hash`; the only mutation is
//!   `last_hit_at = now_ms` (and `expires_at = now_ms + ttl_ms` if
//!   provided). Audit emits `ac.update.ok` (REAPI conformance: client
//!   receives 200 OK echo regardless of insert vs idempotent path).
//! - [`AcMetaUpsertOutcome::ResultHashMismatch`] — row exists with a
//!   **different** `result_hash` than the new payload; the upsert is
//!   **rejected** (no row mutation; existing row preserved per
//!   `INV-AC-RESULT-HASH-IMMUTABLE`). Handler maps to 409
//!   `COR_AC_RESULT_HASH_MISMATCH` + audit emits `ac.update.result_mismatch`.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker R2Backend canonical pattern"
)]

use core::fmt;
use core::future::Future;
use std::collections::HashMap;
// DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` (infallible lock).
// `AcMetaError::MutexPoisoned` is now structurally unreachable on
// this in-memory backend; the variant is retained on the enum for
// API stability and may be returned by non-in-memory implementations.
use parking_lot::Mutex;

use corelink_hash::Digest;
use corelink_tenant_path::TenantPrefix;
use thiserror::Error;
use uuid::Uuid;

use super::types::{ActionDigest, ResultHash};
use crate::region::Region;

/// Composite primary key for the `ac_meta` row store: `(tenant_id,
/// action_digest)`. Constructing one is the **only** legal way to
/// address a row through the [`AcMetaStore`] trait surface — cross-
/// tenant lookups are structurally unreachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AcKey {
    tenant_id: Uuid,
    action_digest: Digest,
}

impl AcKey {
    /// Build a fresh [`AcKey`] from the canonical pair.
    #[must_use]
    pub const fn new(tenant_id: Uuid, action_digest: Digest) -> Self {
        Self {
            tenant_id,
            action_digest,
        }
    }

    /// Borrow the tenant id.
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// Borrow the action digest hash.
    #[must_use]
    pub const fn action_digest(&self) -> &Digest {
        &self.action_digest
    }
}

/// A row in the `ac_meta` table.
///
/// Columns mirror `data_model.md §4.2` + WI-S04-002 schema fix:
/// `(tenant_id, action_digest)` PK, `tenant_prefix` materialized
/// BLOB(16) (per ADR-0035 H-3), `region`, `result_hash`, `expires_at`
/// nullable, `created_at`, `last_hit_at`, `path_key_id` for TDK
/// rotation forward-compat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcMetaRow {
    /// PK component — verified tenant id.
    pub tenant_id: Uuid,
    /// PK component — REAPI v2 `Digest.hash` of the canonical Action proto.
    pub action_digest: ActionDigest,
    /// Tenant prefix materialized at INSERT time. Read on GET; never
    /// recomputed per ADR-0035 H-3.
    pub tenant_prefix: TenantPrefix,
    /// Region the row was minted under (residency anchor).
    pub region: Region,
    /// BLAKE3-256 over the canonical REAPI ActionResult proto bytes.
    pub result_hash: ResultHash,
    /// Path-derivation TDK version — forward-compat S-14 cross-region
    /// rotation. Cron worker (WI-S04-005) reads the column directly
    /// without TDK access.
    pub path_key_id: u32,
    /// Sig key id minted alongside the envelope (WI-S04-004).
    pub sig_key_id: u32,
    /// Unix epoch ms; sticky on first INSERT.
    pub created_at_ms: u64,
    /// Unix epoch ms; refreshed on every GET hit + every idempotent
    /// UPDATE.
    pub last_hit_at_ms: u64,
    /// Optional TTL — `None` ⇒ no expiry; `Some(ms)` ⇒ wall-clock
    /// expiry per S-07 / ADR-0019.
    pub expires_at_ms: Option<u64>,
}

/// Argument bundle for [`AcMetaStore::upsert`].
#[derive(Clone, Debug)]
pub struct AcUpsertRequest {
    /// PK.
    pub key: AcKey,
    /// Action proto digest (PK + size_bytes round-trip).
    pub action_digest: ActionDigest,
    /// Materialized tenant prefix (handler computes via
    /// `corelink_tenant_path::derive_prefix(tdk, tenant_id)` once at
    /// INSERT time per ADR-0035 H-3).
    pub tenant_prefix: TenantPrefix,
    /// Region this UPDATE was issued under.
    pub region: Region,
    /// BLAKE3 over canonical proto bytes.
    pub result_hash: ResultHash,
    /// Path-derivation TDK version.
    pub path_key_id: u32,
    /// Sig key id (WI-S04-004 binding).
    pub sig_key_id: u32,
    /// Unix epoch ms — handler clock.
    pub now_ms: u64,
    /// Optional TTL — `Some(delta_ms)` ⇒ `expires_at = now_ms +
    /// delta_ms`; `None` ⇒ no expiry write (the existing row keeps its
    /// `expires_at`, the new row writes NULL).
    pub ttl_ms: Option<u64>,
}

/// Argument bundle for [`AcMetaStore::refresh_on_hit`].
#[derive(Clone, Debug)]
pub struct AcRefreshRequest {
    /// PK.
    pub key: AcKey,
    /// Unix epoch ms — handler clock.
    pub now_ms: u64,
    /// Optional TTL extension — `Some(delta_ms)` ⇒ `expires_at =
    /// now_ms + delta_ms`; `None` ⇒ leave unchanged.
    pub ttl_extend_ms: Option<u64>,
}

/// Outcome of [`AcMetaStore::upsert`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AcMetaUpsertOutcome {
    /// First-time INSERT (no prior `(tenant_id, action_digest)` row).
    Inserted,
    /// Row pre-existed with the **same** `result_hash`; refreshed
    /// `last_hit_at` (+ optional `expires_at`).
    IdempotentRefresh,
    /// Row pre-existed with a **different** `result_hash`. The upsert
    /// is rejected; existing row is preserved per
    /// `INV-AC-RESULT-HASH-IMMUTABLE`. Handler maps to 409.
    ResultHashMismatch {
        /// `result_hash` already on the row.
        existing: ResultHash,
        /// `result_hash` the client attempted to write.
        attempted: ResultHash,
    },
}

/// Errors surfaced by [`AcMetaStore`] methods.
#[derive(Debug, Error)]
pub enum AcMetaError {
    /// Backend transport failure (D1 RPC error / fake outage).
    #[error("ac_meta backend error: {0}")]
    Backend(String),
    /// Test-only mutex poisoning. Maps to a backend-class error in the
    /// trait surface so the handler treats it as a transient failure.
    #[error("ac_meta in-memory mutex poisoned")]
    MutexPoisoned,
}

/// Tenant-scoped descriptor of an expired row identified by the TTL
/// worker (WI-S04-005). Carries the bare minimum the cron tick needs
/// to issue a tenant-scoped R2 DELETE + D1 DELETE — the TDK is **not**
/// dereferenced on the cron path (`tenant_prefix` is materialized at
/// INSERT time per ADR-0035 H-3 + WI-S04-005 Lote 10.4bis P0 fix).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcExpiredCandidate {
    /// PK component 1.
    pub tenant_id: Uuid,
    /// PK component 2.
    pub action_digest: Digest,
    /// Materialized tenant prefix BLOB(16) — read off the column.
    pub tenant_prefix: TenantPrefix,
    /// Region this row was minted under. The TTL worker MUST refuse
    /// candidates whose region differs from its own (per-region shard).
    pub region: Region,
    /// `expires_at` value the worker observed at SELECT time. The
    /// production binding's `WHERE expires_at < ? AND region = ?`
    /// filter is honored at the trait surface; the value is echoed
    /// back so audit emission carries the canonical timestamp.
    pub expires_at_ms: u64,
}

/// `ac_meta` row store contract.
///
/// Production wiring (WI-S04-006) implements this against a Cloudflare
/// D1 batch + `INSERT … ON CONFLICT (tenant_id, action_digest)` SQL.
/// The in-memory fake [`InMemoryAcMetaStore`] preserves every
/// documented semantic.
pub trait AcMetaStore: Send + Sync {
    /// Atomic upsert: INSERT new row OR refresh `last_hit_at` /
    /// `expires_at` on idempotent re-update.
    ///
    /// On `result_hash` mismatch the existing row is preserved; the
    /// handler responds 409 + audit `ac.update.result_mismatch` per
    /// WI §8 Gherkin scenario.
    fn upsert<'a>(
        &'a self,
        request: AcUpsertRequest,
    ) -> impl Future<Output = Result<AcMetaUpsertOutcome, AcMetaError>> + Send + 'a;

    /// Read-only PK lookup. Returns `None` when the row is absent OR
    /// when the row's `expires_at <= now_ms` (stale row is reported as
    /// missing — caller emits `COR_AC_TTL_EXPIRED` 410 if it has the
    /// row in hand and observes `expires_at <= now_ms` via
    /// [`Self::get_with_expiry`]).
    fn get<'a>(
        &'a self,
        key: &'a AcKey,
    ) -> impl Future<Output = Result<Option<AcMetaRow>, AcMetaError>> + Send + 'a;

    /// Read-only PK lookup that returns the row regardless of
    /// `expires_at`. Used by the GET handler to distinguish 404 (row
    /// absent) from 410 (row expired).
    fn get_with_expiry<'a>(
        &'a self,
        key: &'a AcKey,
    ) -> impl Future<Output = Result<Option<AcMetaRow>, AcMetaError>> + Send + 'a;

    /// Refresh `last_hit_at` (+ optional `expires_at`) on a row.
    /// Idempotent no-op when the row is absent (caller already checked
    /// existence pre-refresh in the canonical handler flow). Surfaces
    /// `Ok(false)` when the row is absent so tests can assert the
    /// handler ordering.
    fn refresh_on_hit<'a>(
        &'a self,
        request: AcRefreshRequest,
    ) -> impl Future<Output = Result<bool, AcMetaError>> + Send + 'a;

    /// Tenant-scoped batched SELECT of expired rows for one region —
    /// the TTL worker (WI-S04-005) cron tick.
    ///
    /// **Tenant scoping**: the production binding emits
    /// `SELECT … WHERE region = ?1 AND tenant_id = ?2 AND expires_at < ?3
    /// ORDER BY expires_at ASC LIMIT ?4` — `tenant_id` is a mandatory
    /// argument; cross-tenant batches are structurally impossible per
    /// `INV-AC-EVICT-TENANT-SCOPED`.
    ///
    /// **Bounded batch**: callers MUST pass `limit <= 250` per
    /// WI-S04-005 §6.1.5 (D1 100 KB batch ceiling). The TTL worker
    /// caps at the canonical batch size; the trait surface accepts any
    /// `usize` and trusts the caller (the cron worker is the only
    /// production caller).
    ///
    /// Returns the candidate descriptors in `expires_at` ASC order so
    /// the worker drains the oldest entries first.
    fn select_expired_for_region<'a>(
        &'a self,
        region: Region,
        tenant_id: Uuid,
        now_ms: u64,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<AcExpiredCandidate>, AcMetaError>> + Send + 'a;

    /// Tenant-scoped enumeration of every tenant_id that has at least
    /// one expired row in `region` at `now_ms`. The TTL worker calls
    /// this once per tick to discover which tenants need work; per-
    /// tenant batches are then drained via [`Self::select_expired_for_region`]
    /// + [`Self::delete_tenant_scoped`].
    ///
    /// Production binding: `SELECT DISTINCT tenant_id FROM ac_meta
    /// WHERE region = ?1 AND expires_at < ?2`.
    fn tenants_with_expired<'a>(
        &'a self,
        region: Region,
        now_ms: u64,
    ) -> impl Future<Output = Result<Vec<Uuid>, AcMetaError>> + Send + 'a;

    /// Tenant-scoped DELETE of a single `(tenant_id, action_digest)`
    /// row. Production binding: `DELETE FROM ac_meta WHERE tenant_id =
    /// ?1 AND action_digest = ?2 AND region = ?3` — the `region`
    /// predicate is a defense-in-depth WHERE clause that re-asserts
    /// the per-region cron worker's pinning even if the SELECT-time
    /// region matched (WI-S04-005 §6.1.5).
    ///
    /// Returns `Ok(true)` when a row was deleted, `Ok(false)` when no
    /// row matched (idempotent — re-running the cron tick after a
    /// crash mid-batch is safe). The handler / cron worker MUST treat
    /// `Ok(false)` as a no-op, **not** an error.
    ///
    /// **Anti-scope**: the trait surface intentionally does **not**
    /// expose a "DELETE WHERE expires_at < ?" bulk method. Any future
    /// scaling improvement that wants bulk DELETE MUST pin the
    /// `tenant_id` + `region` predicates by construction (e.g., via a
    /// `BulkTenantDelete { tenant_id, region, expires_before_ms }`
    /// argument bundle that re-asserts the canonical scoping). Direct
    /// expression of `WHERE expires_at < ?` alone is the FM-303
    /// catastrophic anti-pattern.
    fn delete_tenant_scoped<'a>(
        &'a self,
        tenant_id: Uuid,
        action_digest: &'a Digest,
        region: Region,
    ) -> impl Future<Output = Result<bool, AcMetaError>> + Send + 'a;
}

/// In-memory `ac_meta` store. Preserves:
///
/// - PK uniqueness on `(tenant_id, action_digest)`.
/// - `INV-AC-RESULT-HASH-IMMUTABLE` (mismatched re-update is rejected).
/// - `INV-AC-IDEMPOTENT` (matched re-update updates `last_hit_at` only).
/// - Tenant isolation via PK shape — Tenant B querying a digest
///   inserted under Tenant A's PK gets `Ok(None)`.
pub struct InMemoryAcMetaStore {
    inner: Mutex<HashMap<AcKey, AcMetaRow>>,
}

impl Default for InMemoryAcMetaStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InMemoryAcMetaStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryAcMetaStore")
            .finish_non_exhaustive()
    }
}

impl InMemoryAcMetaStore {
    /// Construct a fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Snapshot every row (test diagnostics).
    pub fn snapshot(&self) -> Result<Vec<AcMetaRow>, AcMetaError> {
        let guard = self.inner.lock();
        Ok(guard.values().cloned().collect())
    }

    /// Test-only: count rows currently materialized.
    pub fn len(&self) -> Result<usize, AcMetaError> {
        let guard = self.inner.lock();
        Ok(guard.len())
    }

    /// Test-only: assert empty.
    pub fn is_empty(&self) -> Result<bool, AcMetaError> {
        Ok(self.len()? == 0)
    }
}

impl AcMetaStore for InMemoryAcMetaStore {
    fn upsert<'a>(
        &'a self,
        request: AcUpsertRequest,
    ) -> impl Future<Output = Result<AcMetaUpsertOutcome, AcMetaError>> + Send + 'a {
        async move {
            let mut guard = self.inner.lock();
            match guard.get(&request.key).cloned() {
                None => {
                    let row = AcMetaRow {
                        tenant_id: request.key.tenant_id,
                        action_digest: request.action_digest,
                        tenant_prefix: request.tenant_prefix,
                        region: request.region,
                        result_hash: request.result_hash,
                        path_key_id: request.path_key_id,
                        sig_key_id: request.sig_key_id,
                        created_at_ms: request.now_ms,
                        last_hit_at_ms: request.now_ms,
                        expires_at_ms: request.ttl_ms.map(|delta| request.now_ms + delta),
                    };
                    guard.insert(request.key, row);
                    Ok(AcMetaUpsertOutcome::Inserted)
                }
                Some(existing) => {
                    if existing.result_hash != request.result_hash {
                        return Ok(AcMetaUpsertOutcome::ResultHashMismatch {
                            existing: existing.result_hash,
                            attempted: request.result_hash,
                        });
                    }
                    // Idempotent path — refresh last_hit_at and
                    // optional expires_at; preserve every other field.
                    let mut refreshed = existing.clone();
                    refreshed.last_hit_at_ms = request.now_ms;
                    if let Some(delta) = request.ttl_ms {
                        refreshed.expires_at_ms = Some(request.now_ms + delta);
                    }
                    guard.insert(request.key, refreshed);
                    Ok(AcMetaUpsertOutcome::IdempotentRefresh)
                }
            }
        }
    }

    fn get<'a>(
        &'a self,
        key: &'a AcKey,
    ) -> impl Future<Output = Result<Option<AcMetaRow>, AcMetaError>> + Send + 'a {
        async move {
            let guard = self.inner.lock();
            Ok(guard.get(key).cloned())
        }
    }

    fn get_with_expiry<'a>(
        &'a self,
        key: &'a AcKey,
    ) -> impl Future<Output = Result<Option<AcMetaRow>, AcMetaError>> + Send + 'a {
        // The fake makes no distinction (the production binding will
        // emit a different SQL — `WHERE …` without expiry filter — to
        // avoid the second round-trip, but the trait surface keeps
        // the two methods symmetric for clarity).
        self.get(key)
    }

    fn refresh_on_hit<'a>(
        &'a self,
        request: AcRefreshRequest,
    ) -> impl Future<Output = Result<bool, AcMetaError>> + Send + 'a {
        async move {
            let mut guard = self.inner.lock();
            let Some(row) = guard.get_mut(&request.key) else {
                return Ok(false);
            };
            // INV-AC-TTL-REFRESH-MONOTONIC: last_hit_at can only ever
            // move forward. The handler clock is wall-clock-driven; we
            // pin the field to the max(prev, now) to absorb minor NTP
            // skew on the production cron worker.
            row.last_hit_at_ms = row.last_hit_at_ms.max(request.now_ms);
            if let Some(delta) = request.ttl_extend_ms {
                row.expires_at_ms = Some(request.now_ms + delta);
            }
            Ok(true)
        }
    }

    fn select_expired_for_region<'a>(
        &'a self,
        region: Region,
        tenant_id: Uuid,
        now_ms: u64,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<AcExpiredCandidate>, AcMetaError>> + Send + 'a {
        async move {
            let guard = self.inner.lock();
            // Tenant-scoped + region-scoped filter; mirrors the SQL
            // `WHERE region = ? AND tenant_id = ? AND expires_at < ?`.
            // Sorted ASC by expires_at so the oldest rows drain first.
            let mut hits: Vec<&AcMetaRow> = guard
                .values()
                .filter(|row| {
                    row.region == region
                        && row.tenant_id == tenant_id
                        && row
                            .expires_at_ms
                            .is_some_and(|exp| exp < now_ms)
                })
                .collect();
            hits.sort_by_key(|row| row.expires_at_ms.unwrap_or(u64::MAX));
            Ok(hits
                .into_iter()
                .take(limit)
                .map(|row| AcExpiredCandidate {
                    tenant_id: row.tenant_id,
                    action_digest: row.action_digest.hash,
                    tenant_prefix: row.tenant_prefix,
                    region: row.region,
                    expires_at_ms: row.expires_at_ms.unwrap_or(0),
                })
                .collect())
        }
    }

    fn tenants_with_expired<'a>(
        &'a self,
        region: Region,
        now_ms: u64,
    ) -> impl Future<Output = Result<Vec<Uuid>, AcMetaError>> + Send + 'a {
        async move {
            let guard = self.inner.lock();
            let mut tenants: std::collections::BTreeSet<Uuid> = std::collections::BTreeSet::new();
            for row in guard.values() {
                if row.region == region
                    && row.expires_at_ms.is_some_and(|exp| exp < now_ms)
                {
                    tenants.insert(row.tenant_id);
                }
            }
            Ok(tenants.into_iter().collect())
        }
    }

    fn delete_tenant_scoped<'a>(
        &'a self,
        tenant_id: Uuid,
        action_digest: &'a Digest,
        region: Region,
    ) -> impl Future<Output = Result<bool, AcMetaError>> + Send + 'a {
        async move {
            let mut guard = self.inner.lock();
            let key = AcKey::new(tenant_id, *action_digest);
            // Defense-in-depth: re-assert region match before deletion
            // (mirrors the production `AND region = ?` clause). A
            // mis-routed delete (cron worker pinned to wnam handed a
            // `weur` candidate) is a programmer error; surface as
            // no-op rather than corrupt other regions.
            let region_match = guard
                .get(&key)
                .map(|row| row.region == region)
                .unwrap_or(false);
            if !region_match {
                return Ok(false);
            }
            Ok(guard.remove(&key).is_some())
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use zeroize::Zeroizing;

    fn fixed_tdk() -> TenantDerivationKey {
        TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
    }

    fn fixed_tenant_a() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    fn fixed_tenant_b() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap()
    }

    fn upsert_req(tid: Uuid, ad_seed: &[u8], rh_seed: &[u8], now_ms: u64) -> AcUpsertRequest {
        let action_hash = Digest::compute(ad_seed);
        let key = AcKey::new(tid, action_hash);
        let prefix = derive_prefix(&fixed_tdk(), tid);
        let raw = rh_seed.to_vec();
        let result_hash = ResultHash::compute(&super::super::types::ActionResult::new(
            Vec::new(),
            Vec::new(),
            0,
            raw,
        ));
        AcUpsertRequest {
            key,
            action_digest: ActionDigest::new(action_hash, ad_seed.len() as i64),
            tenant_prefix: prefix,
            region: Region::Wnam,
            result_hash,
            path_key_id: 1,
            sig_key_id: 1,
            now_ms,
            ttl_ms: Some(60_000),
        }
    }

    #[tokio::test]
    async fn upsert_inserted_then_idempotent_refresh() {
        let store = InMemoryAcMetaStore::new();
        let req = upsert_req(fixed_tenant_a(), b"action1", b"result1", 1_000);
        let out = store.upsert(req.clone()).await.unwrap();
        assert_eq!(out, AcMetaUpsertOutcome::Inserted);
        // Same payload at later timestamp ⇒ idempotent.
        let mut req2 = req.clone();
        req2.now_ms = 5_000;
        let out2 = store.upsert(req2).await.unwrap();
        assert_eq!(out2, AcMetaUpsertOutcome::IdempotentRefresh);
        let row = store.get(&req.key).await.unwrap().unwrap();
        assert_eq!(row.last_hit_at_ms, 5_000);
        assert_eq!(row.created_at_ms, 1_000);
    }

    #[tokio::test]
    async fn result_hash_mismatch_rejects_upsert() {
        let store = InMemoryAcMetaStore::new();
        let req1 = upsert_req(fixed_tenant_a(), b"action1", b"result1", 1_000);
        store.upsert(req1.clone()).await.unwrap();
        let req2 = upsert_req(fixed_tenant_a(), b"action1", b"result2", 2_000);
        let out = store.upsert(req2.clone()).await.unwrap();
        match out {
            AcMetaUpsertOutcome::ResultHashMismatch {
                existing,
                attempted,
            } => {
                assert_eq!(existing, req1.result_hash);
                assert_eq!(attempted, req2.result_hash);
            }
            other => panic!("expected mismatch, got {other:?}"),
        }
        // Existing row preserved.
        let row = store.get(&req1.key).await.unwrap().unwrap();
        assert_eq!(row.result_hash, req1.result_hash);
        assert_eq!(row.last_hit_at_ms, 1_000); // not refreshed
    }

    #[tokio::test]
    async fn cross_tenant_get_returns_none() {
        let store = InMemoryAcMetaStore::new();
        let req_a = upsert_req(fixed_tenant_a(), b"shared-action", b"r", 1_000);
        store.upsert(req_a.clone()).await.unwrap();
        // Tenant B asks for the same action_digest under their own PK.
        let key_b = AcKey::new(fixed_tenant_b(), *req_a.key.action_digest());
        let row = store.get(&key_b).await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn refresh_on_hit_is_no_op_when_absent() {
        let store = InMemoryAcMetaStore::new();
        let key = AcKey::new(fixed_tenant_a(), Digest::compute(b"absent"));
        let updated = store
            .refresh_on_hit(AcRefreshRequest {
                key,
                now_ms: 9_999,
                ttl_extend_ms: Some(60_000),
            })
            .await
            .unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn refresh_on_hit_updates_last_hit_and_ttl() {
        let store = InMemoryAcMetaStore::new();
        let req = upsert_req(fixed_tenant_a(), b"action1", b"result1", 1_000);
        store.upsert(req.clone()).await.unwrap();
        let key = req.key;
        let updated = store
            .refresh_on_hit(AcRefreshRequest {
                key,
                now_ms: 7_000,
                ttl_extend_ms: Some(60_000),
            })
            .await
            .unwrap();
        assert!(updated);
        let row = store.get(&key).await.unwrap().unwrap();
        assert_eq!(row.last_hit_at_ms, 7_000);
        assert_eq!(row.expires_at_ms, Some(67_000));
    }

    #[tokio::test]
    async fn refresh_monotonic_clamps_to_max() {
        let store = InMemoryAcMetaStore::new();
        let req = upsert_req(fixed_tenant_a(), b"action1", b"result1", 5_000);
        store.upsert(req.clone()).await.unwrap();
        // Earlier timestamp must NOT roll back last_hit_at.
        let _ = store
            .refresh_on_hit(AcRefreshRequest {
                key: req.key,
                now_ms: 1_000,
                ttl_extend_ms: None,
            })
            .await
            .unwrap();
        let row = store.get(&req.key).await.unwrap().unwrap();
        assert_eq!(row.last_hit_at_ms, 5_000); // unchanged
    }

    fn upsert_req_with_ttl(
        tid: Uuid,
        ad_seed: &[u8],
        rh_seed: &[u8],
        now_ms: u64,
        ttl_ms: Option<u64>,
        region: Region,
    ) -> AcUpsertRequest {
        let mut req = upsert_req(tid, ad_seed, rh_seed, now_ms);
        req.ttl_ms = ttl_ms;
        req.region = region;
        req
    }

    #[tokio::test]
    async fn select_expired_filters_by_tenant_and_region() {
        let store = InMemoryAcMetaStore::new();
        // Tenant A — expired in wnam.
        store
            .upsert(upsert_req_with_ttl(
                fixed_tenant_a(),
                b"a1",
                b"r1",
                1_000,
                Some(1_000),
                Region::Wnam,
            ))
            .await
            .unwrap();
        // Tenant A — fresh in wnam.
        store
            .upsert(upsert_req_with_ttl(
                fixed_tenant_a(),
                b"a2",
                b"r2",
                1_000,
                Some(60_000_000),
                Region::Wnam,
            ))
            .await
            .unwrap();
        // Tenant B — expired in wnam.
        store
            .upsert(upsert_req_with_ttl(
                fixed_tenant_b(),
                b"b1",
                b"r3",
                1_000,
                Some(2_000),
                Region::Wnam,
            ))
            .await
            .unwrap();
        // Tenant A — expired in weur (different region).
        store
            .upsert(upsert_req_with_ttl(
                fixed_tenant_a(),
                b"a3",
                b"r4",
                1_000,
                Some(500),
                Region::Weur,
            ))
            .await
            .unwrap();

        // Tenant A's expired wnam rows only.
        let exp = store
            .select_expired_for_region(Region::Wnam, fixed_tenant_a(), 5_000_000, 100)
            .await
            .unwrap();
        assert_eq!(exp.len(), 1);
        assert_eq!(exp[0].tenant_id, fixed_tenant_a());
        assert_eq!(exp[0].region, Region::Wnam);
    }

    #[tokio::test]
    async fn select_expired_respects_limit_and_orders_asc() {
        let store = InMemoryAcMetaStore::new();
        for (i, ttl) in [3_000_u64, 1_000, 2_000].iter().enumerate() {
            let mut req = upsert_req_with_ttl(
                fixed_tenant_a(),
                &[b'a', i as u8],
                &[b'r', i as u8],
                1_000,
                Some(*ttl),
                Region::Wnam,
            );
            req.now_ms = 1_000;
            store.upsert(req).await.unwrap();
        }
        let exp = store
            .select_expired_for_region(Region::Wnam, fixed_tenant_a(), 5_000_000, 2)
            .await
            .unwrap();
        assert_eq!(exp.len(), 2);
        // Oldest (1_000 + 1_000 = 2_000) first, then 3_000.
        assert!(exp[0].expires_at_ms <= exp[1].expires_at_ms);
    }

    #[tokio::test]
    async fn tenants_with_expired_returns_unique_tenants() {
        let store = InMemoryAcMetaStore::new();
        store
            .upsert(upsert_req_with_ttl(
                fixed_tenant_a(),
                b"a1",
                b"r",
                1_000,
                Some(1_000),
                Region::Wnam,
            ))
            .await
            .unwrap();
        store
            .upsert(upsert_req_with_ttl(
                fixed_tenant_a(),
                b"a2",
                b"r",
                1_000,
                Some(2_000),
                Region::Wnam,
            ))
            .await
            .unwrap();
        store
            .upsert(upsert_req_with_ttl(
                fixed_tenant_b(),
                b"b1",
                b"r",
                1_000,
                Some(1_000),
                Region::Wnam,
            ))
            .await
            .unwrap();
        let tenants = store
            .tenants_with_expired(Region::Wnam, 5_000_000)
            .await
            .unwrap();
        assert_eq!(tenants.len(), 2);
        assert!(tenants.contains(&fixed_tenant_a()));
        assert!(tenants.contains(&fixed_tenant_b()));
    }

    #[tokio::test]
    async fn delete_tenant_scoped_removes_only_that_tenants_row() {
        let store = InMemoryAcMetaStore::new();
        let req_a = upsert_req(fixed_tenant_a(), b"shared", b"r1", 1_000);
        let req_b = upsert_req(fixed_tenant_b(), b"shared", b"r2", 1_000);
        store.upsert(req_a.clone()).await.unwrap();
        store.upsert(req_b.clone()).await.unwrap();
        let removed = store
            .delete_tenant_scoped(
                fixed_tenant_a(),
                req_a.key.action_digest(),
                Region::Wnam,
            )
            .await
            .unwrap();
        assert!(removed);
        // Tenant A row gone.
        assert!(store.get(&req_a.key).await.unwrap().is_none());
        // Tenant B row preserved.
        assert!(store.get(&req_b.key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn delete_tenant_scoped_no_op_on_missing_row() {
        let store = InMemoryAcMetaStore::new();
        let absent = Digest::compute(b"absent");
        let removed = store
            .delete_tenant_scoped(fixed_tenant_a(), &absent, Region::Wnam)
            .await
            .unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn delete_tenant_scoped_refuses_region_mismatch() {
        // Defense-in-depth: a cron worker pinned to weur asking to
        // delete a wnam row is a programmer wiring error; the trait
        // surface refuses (no-op) rather than corrupting the row.
        let store = InMemoryAcMetaStore::new();
        let mut req = upsert_req(fixed_tenant_a(), b"a", b"r", 1_000);
        req.region = Region::Wnam;
        store.upsert(req.clone()).await.unwrap();
        let removed = store
            .delete_tenant_scoped(fixed_tenant_a(), req.key.action_digest(), Region::Weur)
            .await
            .unwrap();
        assert!(!removed);
        // Row preserved.
        assert!(store.get(&req.key).await.unwrap().is_some());
    }
}
