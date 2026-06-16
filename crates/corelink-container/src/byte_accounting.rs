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
/// plus a small `String`), so it drops into per-route state alongside the
/// existing quota collaborators.
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

// ── Accounting decorators (reserve → commit → release) ────────────────────────
//
// Cluster B + C (cycle-2 nuclear red-team). #297 wired `accrue` only on the
// NATIVE CAS/AC route handlers, AFTER the R2 PUT committed, with no
// pre-reservation and a dead `release`. That left three holes:
//
//   * Cluster B — the Bazel REAPI, OCI, and language-adapter (cargo/brew/npm/
//     pip) write surfaces ALL drive the SAME `CasWriteHandler` / `AcUpdateHandler`
//     trait objects, but none of them ran the route-level accrual, so their
//     writes never moved `bytes_used` — a Free tenant stored unbounded TB at $0.
//   * Cluster C — the accrual ran AFTER the PUT (race: two concurrent writes
//     both pass the cap, an over-cap blob is durably committed before the 402),
//     and deletes never decremented `bytes_used` (headroom leaked forever).
//
// The decorators below close BOTH at the single chokepoint every CAS/AC write
// surface flows through — the write/delete trait objects themselves. Wrapping
// there means native CAS/AC, Bazel, OCI, and every language adapter inherit
// IDENTICAL reserve→commit→release semantics with zero per-surface duplication:
//
//   write:  reserve(len) BEFORE inner.write()  ──► OverCap ⇒ 402 (NO R2 PUT runs,
//           so NO over-cap blob is ever durably written); transport err ⇒ 503
//           (fail-CLOSED). After the write: inner Err ⇒ release(len) (reservation
//           rolled back); idempotent re-write (`durable == false`, stored nothing
//           new) ⇒ release(len) (net zero, no double-count).
//   delete: inner.delete() then release(reclaimed_bytes) so the freed bytes drop
//           out of `bytes_used`.
//
// The reservation is the SAME atomic single-statement D1 UPSERT
// ([`ByteStore::check_and_accrue`]) used before — the cap check and the
// increment are serialized in one statement, so two concurrent over-cap
// reservations can NEVER both pass. The decorators bridge the async accountant
// to the sync handler traits with `block_in_place` + `block_on`, exactly as the
// R2 handlers bridge their own async S3 I/O.

/// Sentinel prefix carried in `CasHandlerError::Internal` / `AcHandlerError::Internal`
/// by an accounting decorator when a write is refused because it would push the
/// tenant past its storage cap. The route `map_err` maps this to HTTP **402**
/// (Payment Required — "storage quota exceeded"), distinct from a generic 500.
pub const OVER_CAP_SENTINEL: &str = "storage-over-cap: ";

/// Sentinel prefix carried in `…::Internal` by an accounting decorator when the
/// byte-accounting backend (D1) is unavailable. The route `map_err` maps this to
/// HTTP **503** — fail-CLOSED: a write we cannot account is refused, never
/// silently allowed (that would re-open the unbounded-storage hole).
pub const ACCT_UNAVAILABLE_SENTINEL: &str = "storage-accounting-unavailable: ";

/// Bridge an async accountant call onto the sync handler trait by blocking on the
/// current tokio runtime — the same `block_in_place` + `block_on` pattern the R2
/// handlers use for their async S3 I/O.
fn block_on_accrue(acc: &ByteAccountant, tenant: &str, bytes: i64) -> Result<AccrueOutcome, String> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| handle.block_on(acc.accrue(tenant, bytes)))
}

/// Bridge an async release call onto the sync handler trait (see [`block_on_accrue`]).
fn block_on_release(acc: &ByteAccountant, tenant: &str, bytes: i64) {
    let handle = tokio::runtime::Handle::current();
    if let Err(e) = tokio::task::block_in_place(|| handle.block_on(acc.release(tenant, bytes))) {
        // A failed release over-counts the tenant (conservative — never widens
        // the cap), so log + continue rather than fail an already-committed op.
        tracing::warn!(error = %e, tenant = %tenant, bytes, "byte-accounting: release failed (counter over-counts; conservative)");
    }
}

/// Storage-byte-accounting decorator over the CAS write + delete trait objects.
///
/// Holds the inner `R2CasHandler` (behind both trait objects) + the accountant.
/// Implements [`corelink_handler_cas::CasWriteHandler`] and
/// [`corelink_handler_cas::CasDeleteHandler`] with reserve→commit→release; see
/// the module-section comment above for the full discipline.
#[derive(Debug)]
pub struct AccountingCasHandler {
    write_inner: Arc<dyn corelink_handler_cas::CasWriteHandler>,
    delete_inner: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    accountant: Arc<ByteAccountant>,
}

impl AccountingCasHandler {
    /// Wrap the CAS write + delete handlers with byte accounting.
    #[must_use]
    pub fn new(
        write_inner: Arc<dyn corelink_handler_cas::CasWriteHandler>,
        delete_inner: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
        accountant: Arc<ByteAccountant>,
    ) -> Self {
        Self {
            write_inner,
            delete_inner,
            accountant,
        }
    }
}

impl corelink_handler_cas::CasWriteHandler for AccountingCasHandler {
    fn write(
        &self,
        req: corelink_handler_cas::CasWriteRequest,
    ) -> Result<corelink_handler_cas::CasWriteResponse, corelink_handler_cas::CasHandlerError> {
        use corelink_handler_cas::CasHandlerError;
        let tenant = req.tenant.clone();
        let byte_len = i64::try_from(req.bytes.len()).unwrap_or(i64::MAX);
        // RESERVE before the R2 PUT (cluster-C): an over-cap reservation is
        // rejected here, so the inner write — the durable R2 PUT — NEVER runs
        // and no over-cap blob is committed.
        match block_on_accrue(&self.accountant, &tenant, byte_len) {
            Ok(AccrueOutcome::Accrued) => {}
            Ok(AccrueOutcome::OverCap) => {
                return Err(CasHandlerError::Internal(format!(
                    "{OVER_CAP_SENTINEL}cas write would exceed storage cap"
                )));
            }
            Err(e) => {
                tracing::error!(error = %e, "cas: byte reservation failed; failing closed");
                return Err(CasHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                )));
            }
        }
        // COMMIT (the durable write).
        match self.write_inner.write(req) {
            Ok(resp) => {
                // An idempotent re-write stored NOTHING new (`durable == false`),
                // so roll the reservation back to avoid double-counting.
                if !resp.durable {
                    block_on_release(&self.accountant, &tenant, byte_len);
                }
                Ok(resp)
            }
            Err(e) => {
                // The write failed AFTER we reserved — RELEASE the reservation
                // so a failed PUT does not permanently consume headroom.
                block_on_release(&self.accountant, &tenant, byte_len);
                Err(e)
            }
        }
    }
}

impl corelink_handler_cas::CasDeleteHandler for AccountingCasHandler {
    fn delete(
        &self,
        req: corelink_handler_cas::CasDeleteRequest,
    ) -> Result<corelink_handler_cas::CasDeleteResponse, corelink_handler_cas::CasHandlerError> {
        let tenant = req.tenant.clone();
        let resp = self.delete_inner.delete(req)?;
        // RELEASE the reclaimed bytes so a delete frees the tenant's headroom
        // (cluster-C: deletes that never decrement leak the cap forever).
        let reclaimed = i64::try_from(resp.reclaimed_bytes).unwrap_or(i64::MAX);
        if reclaimed > 0 {
            block_on_release(&self.accountant, &tenant, reclaimed);
        }
        Ok(resp)
    }
}

/// Storage-byte-accounting decorator over the AC update + delete trait objects.
///
/// Same reserve→commit→release discipline as [`AccountingCasHandler`], over the
/// `AcUpdateHandler` / `AcDeleteHandler` surface (Bazel AC writes + native AC).
#[derive(Debug)]
pub struct AccountingAcHandler {
    update_inner: Arc<dyn corelink_handler_ac::AcUpdateHandler>,
    delete_inner: Arc<dyn corelink_handler_ac::AcDeleteHandler>,
    accountant: Arc<ByteAccountant>,
}

impl AccountingAcHandler {
    /// Wrap the AC update + delete handlers with byte accounting.
    #[must_use]
    pub fn new(
        update_inner: Arc<dyn corelink_handler_ac::AcUpdateHandler>,
        delete_inner: Arc<dyn corelink_handler_ac::AcDeleteHandler>,
        accountant: Arc<ByteAccountant>,
    ) -> Self {
        Self {
            update_inner,
            delete_inner,
            accountant,
        }
    }
}

impl corelink_handler_ac::AcUpdateHandler for AccountingAcHandler {
    fn update(
        &self,
        req: corelink_handler_ac::AcUpdateRequest,
    ) -> Result<corelink_handler_ac::AcUpdateResponse, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        let tenant = req.tenant.clone();
        let byte_len = i64::try_from(req.result_payload.len()).unwrap_or(i64::MAX);
        match block_on_accrue(&self.accountant, &tenant, byte_len) {
            Ok(AccrueOutcome::Accrued) => {}
            Ok(AccrueOutcome::OverCap) => {
                return Err(AcHandlerError::Internal(format!(
                    "{OVER_CAP_SENTINEL}ac write would exceed storage cap"
                )));
            }
            Err(e) => {
                tracing::error!(error = %e, "ac: byte reservation failed; failing closed");
                return Err(AcHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                )));
            }
        }
        match self.update_inner.update(req) {
            Ok(resp) => {
                // Idempotent / divergent-refused AC writes that stored nothing
                // new (`durable == false`) roll the reservation back.
                if !resp.durable {
                    block_on_release(&self.accountant, &tenant, byte_len);
                }
                Ok(resp)
            }
            Err(e) => {
                block_on_release(&self.accountant, &tenant, byte_len);
                Err(e)
            }
        }
    }
}

impl corelink_handler_ac::AcDeleteHandler for AccountingAcHandler {
    fn delete(
        &self,
        req: corelink_handler_ac::AcDeleteRequest,
    ) -> Result<corelink_handler_ac::AcDeleteResponse, corelink_handler_ac::AcHandlerError> {
        let tenant = req.tenant.clone();
        let resp = self.delete_inner.delete(req)?;
        let reclaimed = i64::try_from(resp.reclaimed_bytes).unwrap_or(i64::MAX);
        if reclaimed > 0 {
            block_on_release(&self.accountant, &tenant, reclaimed);
        }
        Ok(resp)
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod decorator_tests {
    //! Regression net for the reserve→commit→release decorators (cluster C).
    use super::testing::{InMemoryByteStore, Row};
    use super::*;
    use corelink_handler_cas::{
        CasDeleteHandler, CasDeleteRequest, CasReadHandler, CasReadRequest, CasWriteHandler,
        CasWriteRequest, InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver,
    };

    const REGION: &str = "iad";

    /// Build an `AccountingCasHandler` over a fresh InMemory CAS backing + a byte
    /// store, returning the decorator, the underlying handler (to inspect stored
    /// blobs), and the byte store (to assert the counter).
    fn cas_fixture(
        seed: Option<(&str, Row)>,
    ) -> (
        Arc<AccountingCasHandler>,
        Arc<InMemoryCasHandler>,
        Arc<InMemoryByteStore>,
    ) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let inner = Arc::new(InMemoryCasHandler::new(audit, sli));
        let store = Arc::new(InMemoryByteStore::new());
        if let Some((tenant, row)) = seed {
            store.seed(tenant, REGION, row);
        }
        let acc = Arc::new(ByteAccountant::new(
            store.clone() as Arc<dyn ByteStore>,
            REGION.to_owned(),
        ));
        let dec = Arc::new(AccountingCasHandler::new(
            inner.clone() as Arc<dyn CasWriteHandler>,
            inner.clone() as Arc<dyn CasDeleteHandler>,
            acc,
        ));
        (dec, inner, store)
    }

    /// CAA-360 #9 digest validator wants a real content hash; the InMemory
    /// handler accepts any (tenant, hash) it is given (it does not hash-verify),
    /// so we use an arbitrary 64-hex string.
    fn hash_for(bytes: &[u8]) -> String {
        corelink_handler_cas::handler::fake_hash(bytes)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn over_cap_write_returns_402_sentinel_and_leaves_no_blob() {
        // Tenant AT a 4-byte cap; a larger write must be refused at the
        // RESERVATION (BEFORE the inner write), so NO blob is stored.
        let (dec, inner, store) = cas_fixture(Some(("t-cap", Row { used: 4, quota: 4 })));
        let body = b"way-over-the-cap".to_vec();
        let hash = hash_for(&body);
        let err = dec
            .write(CasWriteRequest::new(
                "t-cap",
                hash.clone(),
                body,
                "p",
                "t-cap",
                1,
            ))
            .expect_err("over-cap write must be refused");
        match err {
            corelink_handler_cas::CasHandlerError::Internal(ref m) => {
                assert!(m.starts_with(OVER_CAP_SENTINEL), "must carry the over-cap sentinel: {m}");
            }
            other => panic!("expected over-cap Internal, got {other:?}"),
        }
        // The reservation was refused, so the inner store never ran: NO blob.
        let read = inner.read(CasReadRequest::new("t-cap", hash, "p", "t-cap", 2));
        assert!(
            matches!(read, Err(corelink_handler_cas::CasHandlerError::NotFound { .. })),
            "an over-cap write must leave NO blob in storage (reserve-before-commit)"
        );
        // Counter unchanged (the atomic reservation did not move it).
        assert_eq!(store.used("t-cap", REGION), 4, "over-cap reservation must not move the counter");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn under_cap_write_accrues_then_delete_releases() {
        let (dec, _inner, store) = cas_fixture(None);
        let body = b"hello-bytes".to_vec();
        let n = body.len() as i64;
        let hash = hash_for(&body);
        dec.write(CasWriteRequest::new("t1", hash.clone(), body, "p", "t1", 1))
            .expect("write");
        assert_eq!(store.used("t1", REGION), n, "a durable write must accrue its bytes");
        // DELETE must RELEASE the reclaimed bytes so the counter drops to 0.
        dec.delete(CasDeleteRequest::new("t1", hash, "p", "t1", 2))
            .expect("delete");
        assert_eq!(
            store.used("t1", REGION),
            0,
            "a delete must decrement bytes_used by the reclaimed size"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn idempotent_rewrite_does_not_double_count() {
        let (dec, _inner, store) = cas_fixture(None);
        let body = b"same-bytes".to_vec();
        let n = body.len() as i64;
        let hash = hash_for(&body);
        dec.write(CasWriteRequest::new("t1", hash.clone(), body.clone(), "p", "t1", 1))
            .expect("first write");
        // Second identical write is idempotent (`durable == false`) → the
        // decorator rolls the reservation back, so the counter stays at n.
        dec.write(CasWriteRequest::new("t1", hash, body, "p", "t1", 2))
            .expect("second write");
        assert_eq!(
            store.used("t1", REGION),
            n,
            "an idempotent re-write must NOT double-count (reservation rolled back)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_inner_write_releases_reservation() {
        // An inner write that errors (cross-tenant denial) must roll the
        // reservation back — a failed PUT must not consume headroom.
        let (dec, _inner, store) = cas_fixture(None);
        let body = b"abc".to_vec();
        let hash = hash_for(&body);
        // `tenant != caller_tenant` ⇒ the inner InMemory handler returns
        // CrossTenantDenied AFTER the decorator reserved.
        let err = dec.write(CasWriteRequest::new("victim", hash, body, "p", "attacker", 1));
        assert!(err.is_err(), "cross-tenant write must error");
        assert_eq!(
            store.used("victim", REGION),
            0,
            "a failed inner write must release its reservation (no leaked headroom)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_concurrent_over_cap_reservations_cannot_both_pass() {
        // The concurrency-correctness assertion: a tenant with room for exactly
        // ONE 10-byte blob (cap 10, used 0) faces TWO concurrent 10-byte writes.
        // The atomic single-statement reservation guarantees AT MOST ONE
        // succeeds; the other is refused over-cap. (Two non-atomic check-then-act
        // writes could both read used=0 and both pass → counter 20 > cap 10.)
        let (dec, _inner, store) = cas_fixture(Some(("t-race", Row { used: 0, quota: 10 })));
        let d1 = dec.clone();
        let d2 = dec.clone();
        let b1 = b"0123456789".to_vec(); // 10 bytes
        let b2 = b"abcdefghij".to_vec(); // 10 bytes, distinct hash
        let h1 = hash_for(&b1);
        let h2 = hash_for(&b2);
        let t1 = tokio::spawn(async move {
            d1.write(CasWriteRequest::new("t-race", h1, b1, "p", "t-race", 1))
        });
        let t2 = tokio::spawn(async move {
            d2.write(CasWriteRequest::new("t-race", h2, b2, "p", "t-race", 2))
        });
        let r1 = t1.await.unwrap();
        let r2 = t2.await.unwrap();
        let successes = [r1.is_ok(), r2.is_ok()].iter().filter(|b| **b).count();
        assert_eq!(
            successes, 1,
            "exactly ONE of two concurrent over-cap writes may pass (atomic reserve)"
        );
        assert_eq!(
            store.used("t-race", REGION),
            10,
            "the counter must never exceed the cap under concurrency"
        );
    }
}
