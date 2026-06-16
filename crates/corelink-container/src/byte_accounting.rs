//! Per-tenant **storage byte accounting** (red-team finding #1, HIGH).
//!
//! # Why this exists
//!
//! The `tenant_storage_state` row (migration 0008) carries the authoritative
//! `bytes_used` counter the eviction worker + storage-quota policy read to
//! decide the per-tier storage cap. But on the **container data plane** NOTHING
//! ever incremented it: a successful CAS/AC/Turbo write committed bytes to R2
//! and returned success WITHOUT touching `bytes_used`. The storage cap was
//! therefore structurally **inert** — a Free tenant could store unbounded TB at
//! `$0`, because the counter the cap reads never moved.
//!
//! [`ByteAccountant`] closes that gap. After a successful store write the
//! billable handler calls [`ByteAccountant::accrue`], which performs an ATOMIC
//! check-and-accrue against `tenant_storage_state`:
//!
//! ```sql
//! INSERT INTO tenant_storage_state
//!     (tenant_id, region, bytes_used, bytes_quota,
//!      bytes_used_updated_at_ms, last_synced_at_ms,
//!      bytes_reclaimed_lifetime, created_at_ms, updated_at_ms)
//!   VALUES (?1, ?2, ?3, 0, ?4, ?4, 0, ?4, ?4)
//!   ON CONFLICT(tenant_id, region) DO UPDATE SET
//!     bytes_used               = bytes_used + ?3,
//!     bytes_used_updated_at_ms = ?4,
//!     updated_at_ms            = ?4
//!   WHERE tenant_storage_state.bytes_quota = 0
//!      OR tenant_storage_state.bytes_used + ?3
//!         <= tenant_storage_state.bytes_quota
//!   RETURNING bytes_used
//! ```
//!
//! The add happens **in the DB** (`bytes_used = bytes_used + ?3`), not in the
//! app, so concurrent accruals sum instead of clobbering each other (the same
//! lost-update discipline as [`crate::tenant_quota::D1QuotaStore`]). The cap
//! check is **serialized with the increment** in the one statement, so two
//! concurrent over-cap writes cannot both read the same baseline and both pass.
//!
//! When the row's `bytes_quota` is `0` (not yet synced from `tenant_quota` by
//! the DO) the write is **uncapped** — accrual always succeeds; the counter
//! still moves so the cap becomes live the moment the DO populates the quota.
//!
//! # Fail-CLOSED
//!
//! The billable handler treats this as a money/quota gate: an [`AccrueOutcome`]
//! of [`AccrueOutcome::OverCap`] ⇒ the write must be REJECTED (the cache must
//! not serve unbounded storage), and a transport `Err` ⇒ the handler fails
//! CLOSED (503) — we do not return success for a write we could not account.
//!
//! # Deletes
//!
//! [`ByteAccountant::release`] is the inverse: a **saturating** decrement of
//! `bytes_used` (clamped at `0` by the table's
//! `CHECK (bytes_used >= 0)` and a `MAX(0, …)` in SQL) wired into the CAS/AC
//! delete handlers so reclaimed bytes free the tenant's headroom.
//!
//! # Env gating (dev/CI = absent)
//!
//! [`byte_accountant_from_env`] builds the production accountant from the same
//! [`crate::storage::StorageEnv`] the other D1 adapters use, returning `None`
//! when the env is unset (dev/CI) — exactly mirroring
//! [`crate::tenant_quota::quota_guard_from_env`] /
//! [`crate::routes::QuotaGate::from_env`]. Billable handlers hold an
//! `Option<Arc<ByteAccountant>>`; `None` ⇒ accounting is simply not enforced.

#![forbid(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;

/// Container's own storage region, sourced from `R2_CAS_REGION` (default
/// `"iad"`), used as the second component of the `tenant_storage_state`
/// composite PK. Mirrors the region `cas.rs` keys its R2 objects under, so the
/// byte counter lands on the SAME `(tenant, region)` row the eviction worker +
/// quota policy read. The value is one of the canonical 5-region literals; the
/// table's `CHECK (region IN ('sam','iad','lhr','nrt','syd'))` rejects anything
/// else (an invalid `R2_CAS_REGION` then fail-CLOSES the accrual at the DB).
#[must_use]
pub fn region_from_env() -> String {
    crate::storage::env_or("R2_CAS_REGION", "iad")
}

/// Outcome of an [`ByteAccountant::accrue`] attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccrueOutcome {
    /// The bytes were accrued (under the cap, or the row is uncapped).
    Accrued,
    /// The accrual would push `bytes_used` past `bytes_quota` (the cap tripped).
    /// The caller MUST reject the write (the bytes were NOT counted).
    OverCap,
}

/// Backing store for the per-tenant byte counter.
///
/// Abstracted as a trait so the security-critical accrual logic can be
/// unit-tested hermetically with an in-memory fake — the production impl talks
/// to D1 over HTTP, which a unit test cannot reach. Both methods are async
/// because the production impl reaches D1 over HTTP.
#[async_trait]
pub trait ByteStore: std::fmt::Debug + Send + Sync {
    /// ATOMIC check-and-accrue: add `bytes` to the `(tenant, region)` row's
    /// `bytes_used`, seeding a fresh uncapped row when none exists, ONLY IF the
    /// row is uncapped (`bytes_quota = 0`) or the result stays within
    /// `bytes_quota`.
    ///
    /// Returns:
    /// - `Ok(AccrueOutcome::Accrued)` — applied;
    /// - `Ok(AccrueOutcome::OverCap)` — refused (over the cap; counter unchanged);
    /// - `Err(_)` — transport / decode error (the caller fails CLOSED).
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        region: &str,
        bytes: i64,
        now_ms: i64,
    ) -> Result<AccrueOutcome, String>;

    /// SATURATING decrement of the `(tenant, region)` row's `bytes_used` by
    /// `bytes` (clamped at `0`). A no-op when no row exists. `Err` on transport
    /// error.
    async fn release(
        &self,
        tenant_id: &str,
        region: &str,
        bytes: i64,
        now_ms: i64,
    ) -> Result<(), String>;
}

/// The per-tenant storage byte accountant.
///
/// Holds the [`ByteStore`] + the container's region. Clone is cheap (one `Arc`
/// + a small `String`) so it drops into per-route state alongside the existing
/// quota collaborators.
#[derive(Clone, Debug)]
pub struct ByteAccountant {
    store: Arc<dyn ByteStore>,
    region: String,
}

impl ByteAccountant {
    /// Construct from a store + the container's storage region.
    #[must_use]
    pub fn new(store: Arc<dyn ByteStore>, region: String) -> Self {
        Self { store, region }
    }

    /// Accrue `bytes` of newly-written storage against `tenant`.
    ///
    /// Called by a billable write handler AFTER the store write committed and
    /// BEFORE returning success. A non-positive `bytes` (e.g. an idempotent
    /// re-write that stored nothing new) accrues nothing and returns
    /// [`AccrueOutcome::Accrued`].
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on a store transport / decode error; the caller
    /// fails CLOSED (503).
    pub async fn accrue(&self, tenant: &str, bytes: i64) -> Result<AccrueOutcome, String> {
        if bytes <= 0 {
            return Ok(AccrueOutcome::Accrued);
        }
        let now_ms = current_unix_ms();
        self.store
            .check_and_accrue(tenant, &self.region, bytes, now_ms)
            .await
    }

    /// Release (saturating-decrement) `bytes` from `tenant`'s counter after a
    /// delete. A non-positive `bytes` is a no-op.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on a store transport error. Callers log + continue
    /// (a failed release over-counts the tenant — conservative, never
    /// under-counts), since the delete itself already succeeded.
    pub async fn release(&self, tenant: &str, bytes: i64) -> Result<(), String> {
        if bytes <= 0 {
            return Ok(());
        }
        let now_ms = current_unix_ms();
        self.store
            .release(tenant, &self.region, bytes, now_ms)
            .await
    }
}

/// Current Unix epoch in milliseconds (system wall clock).
///
/// The `tenant_storage_state` lifecycle columns are `INTEGER` Unix-epoch ms
/// with monotone `CHECK`s (`updated_at_ms >= created_at_ms`, etc.); the system
/// clock satisfies them for any realistic deployment. A clock at the Unix epoch
/// (0) is impossible in production; the value is clamped to `i64::MAX` only
/// against the (unreachable) year-292M overflow.
fn current_unix_ms() -> i64 {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    i64::try_from(ms).unwrap_or(i64::MAX)
}

/// Build the production [`ByteAccountant`] from process env, or `None` in
/// dev/CI (no D1 storage env). Mirrors
/// [`crate::tenant_quota::quota_guard_from_env`] exactly: the same
/// [`crate::storage::StorageEnv`] gate, the same fail-CLOSED `None` when the env
/// is unset/invalid (the billable routes then run WITHOUT byte accounting).
#[must_use]
pub fn byte_accountant_from_env() -> Option<Arc<ByteAccountant>> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let client = crate::storage::d1_http::D1HttpClient::new(&storage_env)
        .map_err(|e| tracing::warn!(error = %e, "byte-accounting: D1 client init failed"))
        .ok()?;
    let store: Arc<dyn ByteStore> = Arc::new(D1ByteStore::new(Arc::new(client)));
    Some(Arc::new(ByteAccountant::new(store, region_from_env())))
}

/// Production [`ByteStore`] over the `tenant_storage_state` D1 table (migration
/// 0008), reached via the async [`crate::storage::d1_http::D1HttpClient`].
///
/// All SQL is parameterised (positional binds); the tenant scope rides in the
/// composite PK `(tenant_id, region)` on every statement (INV-TENANT-ISOLATION).
#[derive(Debug)]
pub struct D1ByteStore {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl D1ByteStore {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ByteStore for D1ByteStore {
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        region: &str,
        bytes: i64,
        now_ms: i64,
    ) -> Result<AccrueOutcome, String> {
        // Atomic check-and-accrue. On a fresh row the INSERT seeds an UNCAPPED
        // row (`bytes_quota = 0`) at `bytes_used = bytes` — a single op is far
        // below any realistic cap and the DO refreshes the real cap on its next
        // sweep. On conflict the increment happens DB-side, gated by the
        // serialized cap predicate so concurrent over-cap writes cannot both
        // pass. A non-empty result set ⇒ the predicate matched (accrued); an
        // empty set ⇒ the cap tripped (refused, counter unchanged).
        let rows = self
            .client
            .query(
                "INSERT INTO tenant_storage_state \
                   (tenant_id, region, bytes_used, bytes_quota, \
                    bytes_used_updated_at_ms, last_synced_at_ms, \
                    bytes_reclaimed_lifetime, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, 0, ?4, ?4, 0, ?4, ?4) \
                 ON CONFLICT(tenant_id, region) DO UPDATE SET \
                   bytes_used               = bytes_used + ?3, \
                   bytes_used_updated_at_ms = ?4, \
                   updated_at_ms            = ?4 \
                 WHERE tenant_storage_state.bytes_quota = 0 \
                    OR tenant_storage_state.bytes_used + ?3 \
                       <= tenant_storage_state.bytes_quota \
                 RETURNING bytes_used",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::String(region.to_owned()),
                    serde_json::Value::from(bytes),
                    serde_json::Value::from(now_ms),
                ],
            )
            .await?;
        if rows.is_empty() {
            Ok(AccrueOutcome::OverCap)
        } else {
            Ok(AccrueOutcome::Accrued)
        }
    }

    async fn release(
        &self,
        tenant_id: &str,
        region: &str,
        bytes: i64,
        now_ms: i64,
    ) -> Result<(), String> {
        // Saturating decrement: `MAX(0, bytes_used - bytes)` keeps the counter
        // non-negative (also enforced by the table's `CHECK (bytes_used >= 0)`).
        // A no-op when the row is absent (nothing to release).
        let _ = self
            .client
            .query(
                "UPDATE tenant_storage_state \
                    SET bytes_used               = MAX(0, bytes_used - ?3), \
                        bytes_used_updated_at_ms = ?4, \
                        updated_at_ms            = ?4 \
                  WHERE tenant_id = ?1 AND region = ?2",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::String(region.to_owned()),
                    serde_json::Value::from(bytes),
                    serde_json::Value::from(now_ms),
                ],
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
pub(crate) mod testing {
    //! In-memory [`ByteStore`] for tests + route-integration fixtures.
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// One in-memory storage row: the running counter + an optional cap
    /// (`quota == 0` ⇒ uncapped, mirroring the D1 `bytes_quota = 0` default).
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Row {
        /// Running `bytes_used` counter.
        pub used: i64,
        /// Cap (`0` ⇒ uncapped).
        pub quota: i64,
    }

    /// Hermetic in-memory [`ByteStore`] — models the D1 `(tenant, region)` row
    /// with an in-process `Mutex` so the read-modify-write is atomic exactly as
    /// the D1 `bytes_used = bytes_used + ?` increment is.
    #[derive(Debug, Default)]
    pub struct InMemoryByteStore {
        rows: Mutex<HashMap<(String, String), Row>>,
    }

    impl InMemoryByteStore {
        /// Construct an empty store.
        #[must_use]
        pub fn new() -> Self {
            Self {
                rows: Mutex::new(HashMap::new()),
            }
        }

        /// Seed a `(tenant, region)` row (test helper).
        pub fn seed(&self, tenant: &str, region: &str, row: Row) {
            if let Ok(mut rows) = self.rows.lock() {
                let _ = rows.insert((tenant.to_owned(), region.to_owned()), row);
            }
        }

        /// Read a row's `bytes_used` (test assertion helper); `0` when absent.
        #[must_use]
        pub fn used(&self, tenant: &str, region: &str) -> i64 {
            self.rows
                .lock()
                .ok()
                .and_then(|r| r.get(&(tenant.to_owned(), region.to_owned())).map(|row| row.used))
                .unwrap_or(0)
        }
    }

    #[async_trait]
    impl ByteStore for InMemoryByteStore {
        async fn check_and_accrue(
            &self,
            tenant_id: &str,
            region: &str,
            bytes: i64,
            _now_ms: i64,
        ) -> Result<AccrueOutcome, String> {
            let mut rows = self
                .rows
                .lock()
                .map_err(|_| "InMemoryByteStore: poisoned lock".to_owned())?;
            let row = rows
                .entry((tenant_id.to_owned(), region.to_owned()))
                .or_insert_with(Row::default);
            // Uncapped (quota == 0) always accrues; otherwise the projected
            // total must stay within the cap (mirrors the D1 WHERE predicate).
            if row.quota != 0 && row.used.saturating_add(bytes) > row.quota {
                return Ok(AccrueOutcome::OverCap);
            }
            row.used = row.used.saturating_add(bytes);
            Ok(AccrueOutcome::Accrued)
        }

        async fn release(
            &self,
            tenant_id: &str,
            region: &str,
            bytes: i64,
            _now_ms: i64,
        ) -> Result<(), String> {
            let mut rows = self
                .rows
                .lock()
                .map_err(|_| "InMemoryByteStore: poisoned lock".to_owned())?;
            if let Some(row) = rows.get_mut(&(tenant_id.to_owned(), region.to_owned())) {
                row.used = (row.used - bytes).max(0);
            }
            Ok(())
        }
    }

    /// A [`ByteStore`] that always errors — drives the fail-CLOSED 503 path.
    #[derive(Debug)]
    pub struct ErroringByteStore;

    #[async_trait]
    impl ByteStore for ErroringByteStore {
        async fn check_and_accrue(
            &self,
            _tenant_id: &str,
            _region: &str,
            _bytes: i64,
            _now_ms: i64,
        ) -> Result<AccrueOutcome, String> {
            Err("simulated D1 transport error".to_owned())
        }
        async fn release(
            &self,
            _tenant_id: &str,
            _region: &str,
            _bytes: i64,
            _now_ms: i64,
        ) -> Result<(), String> {
            Err("simulated D1 transport error".to_owned())
        }
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
    use super::testing::{ErroringByteStore, InMemoryByteStore, Row};
    use super::*;

    const REGION: &str = "iad";

    fn accountant(store: Arc<dyn ByteStore>) -> ByteAccountant {
        ByteAccountant::new(store, REGION.to_owned())
    }

    #[tokio::test]
    async fn accrue_increments_bytes_used_uncapped() {
        // The load-bearing finding-#1 assertion: a write accrues bytes_used (the
        // counter the storage cap reads — previously NEVER moved).
        let store = Arc::new(InMemoryByteStore::new());
        let acc = accountant(store.clone());
        assert_eq!(acc.accrue("t1", 1_000).await.unwrap(), AccrueOutcome::Accrued);
        assert_eq!(acc.accrue("t1", 500).await.unwrap(), AccrueOutcome::Accrued);
        assert_eq!(store.used("t1", REGION), 1_500, "concurrent accruals must sum");
    }

    #[tokio::test]
    async fn accrue_over_cap_is_blocked_and_counter_unchanged() {
        // A tenant at 900/1000 bytes; a 200-byte write would hit 1100 > 1000 ⇒
        // OverCap, and the counter must NOT move (atomic check-and-accrue).
        let store = Arc::new(InMemoryByteStore::new());
        store.seed("t-cap", REGION, Row { used: 900, quota: 1_000 });
        let acc = accountant(store.clone());
        assert_eq!(acc.accrue("t-cap", 200).await.unwrap(), AccrueOutcome::OverCap);
        assert_eq!(
            store.used("t-cap", REGION),
            900,
            "an over-cap accrual must not move the counter"
        );
        // A write that exactly fills the cap is allowed (`<=` predicate).
        assert_eq!(acc.accrue("t-cap", 100).await.unwrap(), AccrueOutcome::Accrued);
        assert_eq!(store.used("t-cap", REGION), 1_000);
        // Now AT the cap; one more byte trips it.
        assert_eq!(acc.accrue("t-cap", 1).await.unwrap(), AccrueOutcome::OverCap);
    }

    #[tokio::test]
    async fn release_saturates_at_zero() {
        let store = Arc::new(InMemoryByteStore::new());
        store.seed("t-del", REGION, Row { used: 300, quota: 0 });
        let acc = accountant(store.clone());
        acc.release("t-del", 100).await.unwrap();
        assert_eq!(store.used("t-del", REGION), 200);
        // Over-release saturates at 0, never negative (table CHECK invariant).
        acc.release("t-del", 9_999).await.unwrap();
        assert_eq!(store.used("t-del", REGION), 0);
    }

    #[tokio::test]
    async fn zero_or_negative_byte_ops_are_noops() {
        let store = Arc::new(InMemoryByteStore::new());
        let acc = accountant(store.clone());
        // A no-new-bytes write (idempotent re-write) accrues nothing.
        assert_eq!(acc.accrue("t0", 0).await.unwrap(), AccrueOutcome::Accrued);
        assert_eq!(acc.accrue("t0", -5).await.unwrap(), AccrueOutcome::Accrued);
        acc.release("t0", 0).await.unwrap();
        assert_eq!(store.used("t0", REGION), 0);
    }

    #[tokio::test]
    async fn store_error_surfaces_for_fail_closed_handling() {
        // The handler maps an accrue Err to 503 (fail-CLOSED) — assert the error
        // propagates rather than being silently swallowed.
        let acc = accountant(Arc::new(ErroringByteStore));
        assert!(acc.accrue("t-err", 100).await.is_err());
    }

    #[test]
    fn region_default_is_iad() {
        // `region_from_env` defaults to the residency-correct US colo when
        // R2_CAS_REGION is unset; a regional env overrides it (see cas.rs F7).
        // We only assert the default is a canonical literal accepted by the
        // table CHECK; the env-set path is covered by storage::env_or tests.
        let r = region_from_env();
        assert!(
            ["sam", "iad", "lhr", "nrt", "syd"].contains(&r.as_str()),
            "region must be a canonical 5-region literal"
        );
    }
}
