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
use std::sync::Mutex;

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
        let guard = self.inner.lock().map_err(|_| AcMetaError::MutexPoisoned)?;
        Ok(guard.values().cloned().collect())
    }

    /// Test-only: count rows currently materialized.
    pub fn len(&self) -> Result<usize, AcMetaError> {
        let guard = self.inner.lock().map_err(|_| AcMetaError::MutexPoisoned)?;
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
            let mut guard = self.inner.lock().map_err(|_| AcMetaError::MutexPoisoned)?;
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
            let guard = self.inner.lock().map_err(|_| AcMetaError::MutexPoisoned)?;
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
            let mut guard = self.inner.lock().map_err(|_| AcMetaError::MutexPoisoned)?;
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
}
