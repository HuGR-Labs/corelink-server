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

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use async_trait::async_trait;

use crate::customer_d1::ByokCryptoMode;
use crate::storage::byok_cas::{
    engagement_for, ByokConfigCache, ByokEngagement, BYOK_CLB1_OVERHEAD, BYOK_CLB2_OVERHEAD,
};

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

/// Name of the **server-trusted** request header carrying the tenant's resolved
/// per-tier storage cap in bytes, set SOLELY by the Worker (the quota-resolution
/// authority) on every data-plane write-forward and stripped from any client-
/// supplied value (`stripClientTrustHeaders`), exactly like `x-corelink-tenant-id`.
///
/// Value semantics (parsed by [`storage_quota_from_headers`]):
/// - a non-negative integer string `"n"` → `Some(n)` (`"0"` = genuine-unlimited);
/// - absent / empty / unparseable → `None` (indeterminate → fail-closed on a
///   fresh row).
pub const STORAGE_QUOTA_HEADER: &str = "x-corelink-storage-quota-bytes";

/// Parse the Worker-set [`STORAGE_QUOTA_HEADER`] into a cap to seed a fresh
/// `tenant_storage_state` row. Returns `None` (indeterminate) when the header is
/// absent, empty, non-ASCII-decodable, not a valid `i64`, or negative — every
/// such case fails CLOSED on a fresh row rather than seeding it uncapped.
#[must_use]
pub fn storage_quota_from_headers(headers: &axum::http::HeaderMap) -> Option<i64> {
    let raw = headers.get(STORAGE_QUOTA_HEADER)?.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }
    match raw.parse::<i64>() {
        Ok(n) if n >= 0 => Some(n),
        _ => None,
    }
}

/// Outcome of an [`ByteAccountant::accrue`] attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccrueOutcome {
    /// The bytes were accrued (under the cap, or the row is genuinely uncapped).
    Accrued,
    /// The accrual would push `bytes_used` past `bytes_quota` (the cap tripped).
    /// The caller MUST reject the write (the bytes were NOT counted) → HTTP 402.
    OverCap,
    /// The reservation could NOT be resolved because the cap was **indeterminate**
    /// for a tenant with no existing `tenant_storage_state` row: there is no
    /// prior row to read a cap from AND the caller supplied no resolved cap
    /// (`storage_quota_bytes == None`). We FAIL CLOSED rather than seed an
    /// uncapped row — a fresh/unsynced tenant must NEVER be treated as unlimited
    /// (only a genuine unlimited-tier tenant, which the caller signals with an
    /// explicit `Some(0)`). The caller maps this to HTTP 503 (fail-closed),
    /// matching the canonical billing crate's `TenantStorageStateMissing`
    /// posture. (`Some(n)` callers never see this — they seed the fresh row.)
    Indeterminate,
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
    /// `bytes_used`, gated by the cap.
    ///
    /// `quota_seed` carries the tenant's resolved per-tier storage cap, used
    /// ONLY to seed a **fresh** row (the INSERT branch); an existing row keeps
    /// its already-stored `bytes_quota` (the UPDATE branch):
    ///
    /// - `Some(n)`, `n > 0` — a fresh row is seeded with `bytes_quota = n` (the
    ///   real cap, NOT the legacy hard-coded `0` that meant "unlimited"). The
    ///   accrual is applied iff it stays within the cap.
    /// - `Some(0)` — a genuinely-unlimited tier: a fresh row is seeded with the
    ///   `0` sentinel and the accrual always applies.
    /// - `None` — the cap is **indeterminate**. An *existing* row accrues against
    ///   its stored cap as normal; a **missing** row must NOT be created (we
    ///   never seed an uncapped row from absence) → `Indeterminate` (fail-closed).
    ///
    /// Returns:
    /// - `Ok(AccrueOutcome::Accrued)` — applied;
    /// - `Ok(AccrueOutcome::OverCap)` — refused (over the cap; counter unchanged);
    /// - `Ok(AccrueOutcome::Indeterminate)` — refused (no row + no cap; fail-closed);
    /// - `Err(_)` — transport / decode error (the caller fails CLOSED).
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        region: &str,
        bytes: i64,
        quota_seed: Option<i64>,
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

    /// Accrue `bytes` of newly-written storage against `tenant`, seeding a fresh
    /// `tenant_storage_state` row with `quota_seed` (the tenant's resolved
    /// per-tier cap) when none exists.
    ///
    /// Called by a billable write handler AFTER the store write committed and
    /// BEFORE returning success. A non-positive `bytes` (e.g. an idempotent
    /// re-write that stored nothing new) accrues nothing and returns
    /// [`AccrueOutcome::Accrued`].
    ///
    /// See [`ByteStore::check_and_accrue`] for the `quota_seed` semantics
    /// (`Some(n)` finite cap / `Some(0)` genuine-unlimited / `None`
    /// indeterminate → fail-closed on a fresh row).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on a store transport / decode error; the caller
    /// fails CLOSED (503).
    pub async fn accrue(
        &self,
        tenant: &str,
        bytes: i64,
        quota_seed: Option<i64>,
    ) -> Result<AccrueOutcome, String> {
        if bytes <= 0 {
            return Ok(AccrueOutcome::Accrued);
        }
        let now_ms = current_unix_ms();
        self.store
            .check_and_accrue(tenant, &self.region, bytes, quota_seed, now_ms)
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
        quota_seed: Option<i64>,
        now_ms: i64,
    ) -> Result<AccrueOutcome, String> {
        match quota_seed {
            // ── Resolved cap (`Some`): atomic check-and-accrue UPSERT ─────────
            // On a FRESH row the INSERT seeds `bytes_quota = ?5` — the tenant's
            // REAL per-tier cap (`?5 = 0` ONLY for a genuinely-unlimited tier;
            // a fresh row is therefore NEVER created uncapped from absence). On
            // conflict the increment happens DB-side, gated by the serialized
            // cap predicate (`bytes_quota = 0` unlimited OR projected total
            // within cap) so concurrent over-cap writes cannot both pass.
            //
            // A FRESH-row INSERT seeding a `?5`-byte cap with a `bytes`-sized
            // first write can itself be over that cap — the INSERT would then
            // create a row already past its own cap. To keep the fresh-row check
            // serialized too, the INSERT is guarded: when the very first write
            // exceeds a finite seed cap, no row is created and the result set is
            // empty ⇒ OverCap (nothing committed). We express that by only
            // INSERTing when `?5 = 0` (unlimited) or `bytes <= ?5`; otherwise
            // the statement degrades to a conflict-less no-op (empty set).
            //
            // A non-empty result set ⇒ accrued; an empty set ⇒ the cap tripped
            // (existing-row UPDATE predicate failed, or fresh-row INSERT guard
            // failed) — refused, counter unchanged.
            Some(seed) => {
                // rt-nuclear #16: RECONCILE the stored cap on conflict. The
                // previous ON CONFLICT updated `bytes_used`/timestamps but NOT
                // `bytes_quota`, so a tenant whose tier was DOWNGRADED kept the
                // old (higher) cap forever — the new lower cap never took effect.
                // We now reseed `bytes_quota` to the incoming authoritative cap
                // `?5` WHEN it is a real finite value (`?5 > 0`); a `Some(0)`
                // (genuinely-unlimited) incoming cap is NOT clobbered onto the
                // stored cap (`COALESCE(NULLIF(?5,0), …existing)` keeps the
                // existing cap), so an unlimited carrier never silently lowers a
                // finite cap and the finding's "do not clobber a legit unlimited"
                // rule holds. The cap PREDICATE is evaluated against the EFFECTIVE
                // new cap so a downgrade is enforced on this very write:
                //   - incoming unlimited (`?5 = 0`)        ⇒ pass (unlimited);
                //   - existing unlimited + finite incoming ⇒ gate by `?5`;
                //   - finite incoming                      ⇒ gate by `?5`;
                //   - else (existing finite, unlimited in) ⇒ gate by stored cap.
                let rows = self
                    .client
                    .query(
                        "INSERT INTO tenant_storage_state \
                           (tenant_id, region, bytes_used, bytes_quota, \
                            bytes_used_updated_at_ms, last_synced_at_ms, \
                            bytes_reclaimed_lifetime, created_at_ms, updated_at_ms) \
                         SELECT ?1, ?2, ?3, ?5, ?4, ?4, 0, ?4, ?4 \
                           WHERE ?5 = 0 OR ?3 <= ?5 \
                         ON CONFLICT(tenant_id, region) DO UPDATE SET \
                           bytes_used               = bytes_used + ?3, \
                           bytes_quota              = COALESCE(NULLIF(?5, 0), tenant_storage_state.bytes_quota), \
                           bytes_used_updated_at_ms = ?4, \
                           updated_at_ms            = ?4 \
                         WHERE ?5 = 0 \
                            OR tenant_storage_state.bytes_used + ?3 <= ?5 \
                         RETURNING bytes_used",
                        &[
                            serde_json::Value::String(tenant_id.to_owned()),
                            serde_json::Value::String(region.to_owned()),
                            serde_json::Value::from(bytes),
                            serde_json::Value::from(now_ms),
                            serde_json::Value::from(seed),
                        ],
                    )
                    .await?;
                if rows.is_empty() {
                    // brutal-audit H3 (money/COGS reconcile-DEADLOCK): the
                    // in-UPSERT `bytes_quota` reconcile above lives INSIDE the
                    // ON CONFLICT DO UPDATE, which is GATED by the cap predicate —
                    // so it fires ONLY on a write that is itself WITHIN the (new)
                    // cap. A tenant whose tier was DOWNGRADED below its current
                    // usage is already OVER the new cap: every NATIVE write it
                    // makes is refused (empty RETURNING ⇒ DO UPDATE skipped ⇒ the
                    // stored `bytes_quota` is NEVER lowered). Adapter writes
                    // (brew/npm/pip pass `None`) then keep gating against the STALE
                    // higher stored cap and accrue past the paid-for cap forever —
                    // a COGS-evasion deadlock (the reconcile was coupled to a
                    // SUCCESSFUL native write that, for an over-cap tenant, can
                    // never succeed).
                    //
                    // Break the coupling: on the refused path, reconcile the stored
                    // cap in a SEPARATE, UN-gated UPDATE so the lowered cap is
                    // observed by subsequent adapter writes WITHOUT requiring any
                    // native write to succeed. The over-cap write itself stays
                    // REJECTED (we still return `OverCap`) — enforcement is
                    // unchanged; only the stale stored cap is corrected. This extra
                    // hop is paid ONLY on the (rare) refused path; the hot accepted
                    // path keeps its single statement (its UPSERT already reconciled
                    // the cap via `COALESCE(NULLIF(?5,0), …)`).
                    //
                    // Skipped for a `Some(0)` genuine-unlimited carrier — it must
                    // never clobber a finite stored cap (mirrors the
                    // `COALESCE(NULLIF(?5,0), …)` rule above); and a no-op for the
                    // fresh-row case an over-cap first write left uncreated (the
                    // `WHERE` matches no row), which is the correct fail-closed
                    // posture (absence is never seeded uncapped).
                    if seed > 0 {
                        self.client
                            .query(
                                "UPDATE tenant_storage_state SET \
                                   bytes_quota   = ?4, \
                                   updated_at_ms = ?3 \
                                 WHERE tenant_id = ?1 AND region = ?2 \
                                   AND bytes_quota <> ?4",
                                &[
                                    serde_json::Value::String(tenant_id.to_owned()),
                                    serde_json::Value::String(region.to_owned()),
                                    serde_json::Value::from(now_ms),
                                    serde_json::Value::from(seed),
                                ],
                            )
                            .await?;
                    }
                    Ok(AccrueOutcome::OverCap)
                } else {
                    Ok(AccrueOutcome::Accrued)
                }
            }
            // ── Indeterminate cap (`None`): UPDATE-ONLY, never seed a row ──────
            // With no resolved cap we must NOT create a row (that would seed it
            // uncapped from absence — the very fail-open this fix closes). We run
            // an UPDATE-only against the existing row, gated by the SAME cap
            // predicate. Outcomes:
            //   - one row back ⇒ accrued against the existing (already-seeded) cap;
            //   - empty back   ⇒ either the row is MISSING (→ Indeterminate,
            //     fail-closed) or it EXISTS but the cap tripped (→ OverCap). We
            //     disambiguate with one keyed existence read on the empty path.
            None => {
                let rows = self
                    .client
                    .query(
                        "UPDATE tenant_storage_state SET \
                           bytes_used               = bytes_used + ?3, \
                           bytes_used_updated_at_ms = ?4, \
                           updated_at_ms            = ?4 \
                         WHERE tenant_id = ?1 AND region = ?2 \
                           AND (bytes_quota = 0 \
                                OR bytes_used + ?3 <= bytes_quota) \
                         RETURNING bytes_used",
                        &[
                            serde_json::Value::String(tenant_id.to_owned()),
                            serde_json::Value::String(region.to_owned()),
                            serde_json::Value::from(bytes),
                            serde_json::Value::from(now_ms),
                        ],
                    )
                    .await?;
                if !rows.is_empty() {
                    return Ok(AccrueOutcome::Accrued);
                }
                // Empty: disambiguate missing-row (fail-closed) from cap-tripped.
                let existing = self
                    .client
                    .query(
                        "SELECT bytes_used FROM tenant_storage_state \
                           WHERE tenant_id = ?1 AND region = ?2",
                        &[
                            serde_json::Value::String(tenant_id.to_owned()),
                            serde_json::Value::String(region.to_owned()),
                        ],
                    )
                    .await?;
                if existing.is_empty() {
                    // No row AND no resolved cap → never seed uncapped.
                    Ok(AccrueOutcome::Indeterminate)
                } else {
                    // Row exists but the cap predicate failed → over the cap.
                    Ok(AccrueOutcome::OverCap)
                }
            }
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
/// handlers use for their async S3 I/O. `quota_seed` is the request's resolved
/// per-tier cap (used to seed a fresh `tenant_storage_state` row; see
/// [`ByteStore::check_and_accrue`]).
fn block_on_accrue(
    acc: &ByteAccountant,
    tenant: &str,
    bytes: i64,
    quota_seed: Option<i64>,
) -> Result<AccrueOutcome, String> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| handle.block_on(acc.accrue(tenant, bytes, quota_seed)))
}

/// Compute the **committed (stored) byte size** the accountant must reserve and
/// release for a write — the size that actually lands in R2, so reserve ==
/// release == the eventual delete-release (which measures the real R2 object)
/// and `bytes_used` can NEVER drift (audit C3 CRITICAL).
///
/// For a BYOK-ENCRYPTING tenant the stored object is the `CLB1` convergent blob
/// = `plaintext_len + BYOK_CLB1_OVERHEAD` (32 B). Such a tenant is detected via
/// the SAME [`ByokConfigCache`] the storage handlers use — engagement
/// [`ByokEngagement::Encrypt`] for a non-`_public` tenant — so a write that
/// stores ciphertext is accounted at its ciphertext size on BOTH the reserve and
/// (on rollback) the release.
///
/// - `cache == None` (today's production / tests) ⇒ plaintext size — byte-for-
///   byte the legacy behaviour, zero change for every non-BYOK deployment.
/// - `_public`, not configured, or `Plaintext` engagement ⇒ plaintext size.
/// - `FailClosed` engagement (Mode B / partial) ⇒ plaintext size: the inner
///   storage write fails closed and stores NOTHING, so the (plaintext-sized)
///   reservation is rolled back net-zero — no ciphertext is ever committed.
/// - `Encrypt` ⇒ `plaintext_len + BYOK_CLB1_OVERHEAD`.
///
/// FAIL-CLOSED: a config-cache read error returns `Err` — the caller maps it to
/// the 503 fail-closed sentinel. We must NEVER under-reserve an active tenant on
/// an undetermined config (and the shared cache means the inner handler would
/// fail closed on the same error anyway).
fn byok_committed_len(
    cache: Option<&Arc<ByokConfigCache>>,
    tenant: &str,
    plaintext_len: i64,
) -> Result<i64, String> {
    let Some(cache) = cache else {
        return Ok(plaintext_len);
    };
    // `_public` is deterministic public content — never encrypted (dedup), so it
    // is byte-identical to today (frozen policy: non-BYOK + `_public` unchanged).
    if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
        return Ok(plaintext_len);
    }
    let handle = tokio::runtime::Handle::current();
    let cfg = tokio::task::block_in_place(|| handle.block_on(cache.get(tenant)))
        .map_err(|e| format!("byok config read (accounting): {e}"))?;
    let Some(cfg) = cfg else {
        return Ok(plaintext_len);
    };
    match engagement_for(&cfg) {
        // Mode A (convergent) stores `CLB1` (+32 B); Mode B (random) stores `CLB2`
        // (+20 B — the nonce lives in `byok_envelope`, not inline). Account the
        // committed CIPHERTEXT size so the reservation matches the real R2 object.
        ByokEngagement::Encrypt(ByokCryptoMode::Convergent) => {
            Ok(plaintext_len.saturating_add(i64::try_from(BYOK_CLB1_OVERHEAD).unwrap_or(i64::MAX)))
        }
        ByokEngagement::Encrypt(ByokCryptoMode::Random) => {
            Ok(plaintext_len.saturating_add(i64::try_from(BYOK_CLB2_OVERHEAD).unwrap_or(i64::MAX)))
        }
        ByokEngagement::Plaintext | ByokEngagement::FailClosed(_) => Ok(plaintext_len),
    }
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

/// Number of per-`(tenant, hash)` serialization-lock shards held by an
/// [`AccountingCasHandler`].
///
/// rt-nuclear C2 (CAS write-vs-delete byte-accounting race): `delete_if_present`
/// in `r2_s3` serializes the HEAD+DELETE measure-and-delete **per key**, but that
/// lock covers delete-vs-delete ONLY. A concurrent overwrite-`write` of the SAME
/// content-addressed key takes NO part in it, so a `delete` of key K (size L) can
/// observe size L and `release` L while a racing `write` independently
/// reserves+commits its own bytes — the two operations' reserve/release no longer
/// net to the true on-disk total, UNDER-counting `bytes_used` by up to L when the
/// write wins (the blob is on disk but the counter was decremented) → a
/// storage-quota evasion.
///
/// We close that by serializing the FULL reserve→commit→release of a `write` and
/// the FULL delete→release of a `delete` against the SAME `(tenant, hash)` under
/// one lock — so their accounting sequences can never interleave for one key,
/// while distinct keys stay fully concurrent.
///
/// A FIXED, power-of-two shard array keeps the lock set **memory-bounded** (no
/// per-key map that grows with the live keyspace and needs pruning, unlike the
/// `r2_s3` delete map): every `(tenant, hash)` deterministically maps to one of
/// these shards. Distinct keys that collide on a shard serialize (a rare,
/// correctness-preserving false-share); the same key ALWAYS maps to the same
/// shard, which is the property the race requires. 256 shards keep cross-key
/// contention negligible for any realistic per-container concurrency.
const CAS_LOCK_SHARDS: usize = 256;

impl AccountingCasHandler {
    /// Acquire the per-`(tenant, hash)` serialization guard (the shard the key
    /// hashes to) and block on it via the SAME `block_in_place` + `block_on`
    /// bridge the R2 handlers use for their async I/O.
    ///
    /// Held by BOTH [`Self::write`] (across reserve→inner-PUT→release) and
    /// [`Self::delete`] (across inner-delete→release) so a write and a delete of
    /// the SAME content-addressed key cannot interleave their byte-accounting
    /// sequences (rt-nuclear C2). The returned guard must be held for the whole
    /// accounting sequence.
    ///
    /// Returns an [`tokio::sync::OwnedMutexGuard`] (the shard `Arc` is cloned so
    /// the guard owns its reference and need not borrow the array) — held by the
    /// caller across the entire reserve/commit/release sequence.
    fn lock_for(&self, tenant: &str, hash: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tenant.hash(&mut hasher);
        // A separator so `(a, bc)` and `(ab, c)` cannot collapse to one key.
        0u8.hash(&mut hasher);
        hash.hash(&mut hasher);
        // Map the key hash onto a shard. The modulo is correct for any shard
        // count; `CAS_LOCK_SHARDS` (256) is a power of two so the distribution is
        // uniform and the op is a single cheap division off a 64-bit hash.
        let idx = (hasher.finish() as usize) % self.key_locks.len();
        // `idx < len` by construction (modulo), so `get` is always `Some`; the
        // `unwrap_or_else` is unreachable totality that keeps clippy's
        // `indexing_slicing` happy without a panic path.
        let lock = self
            .key_locks
            .get(idx)
            .map(Arc::clone)
            .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(())));
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(lock.lock_owned()))
    }
}

/// Storage-byte-accounting decorator over the CAS write + delete trait objects.
///
/// Holds the inner `R2CasHandler` (behind both trait objects) + the accountant.
/// Implements [`corelink_handler_cas::CasWriteHandler`] and
/// [`corelink_handler_cas::CasDeleteHandler`] with reserve→commit→release; see
/// the module-section comment above for the full discipline.
///
/// `key_locks` is a FIXED [`CAS_LOCK_SHARDS`]-wide array of per-`(tenant, hash)`
/// serialization locks (see [`CAS_LOCK_SHARDS`] for the rt-nuclear C2 rationale):
/// both `write` and `delete` acquire the shard their key maps to for their entire
/// reserve/commit/release sequence, so a write and a delete of the SAME key can
/// never interleave their accounting (which would under-count `bytes_used`).
pub struct AccountingCasHandler {
    write_inner: Arc<dyn corelink_handler_cas::CasWriteHandler>,
    delete_inner: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    accountant: Arc<ByteAccountant>,
    /// Fixed, memory-bounded shard array of per-`(tenant, hash)` async locks.
    key_locks: Arc<Vec<Arc<tokio::sync::Mutex<()>>>>,
    /// BYOK Wave 3b (GATED-INERT): the SAME per-tenant config cache the storage
    /// handlers use. `None` ⇒ plaintext-size accounting (today's behaviour). When
    /// `Some` AND a tenant is BYOK-`active`, the reserve/release size is the
    /// committed CIPHERTEXT size (`plaintext + BYOK_CLB1_OVERHEAD`) so it matches
    /// the on-disk object the delete path releases (audit C3 — no drift).
    byok_config_cache: Option<Arc<ByokConfigCache>>,
}

impl core::fmt::Debug for AccountingCasHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountingCasHandler")
            .field("accountant", &self.accountant)
            .field("key_lock_shards", &self.key_locks.len())
            .finish_non_exhaustive()
    }
}

impl AccountingCasHandler {
    /// Wrap the CAS write + delete handlers with byte accounting.
    #[must_use]
    pub fn new(
        write_inner: Arc<dyn corelink_handler_cas::CasWriteHandler>,
        delete_inner: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
        accountant: Arc<ByteAccountant>,
    ) -> Self {
        let key_locks = (0..CAS_LOCK_SHARDS)
            .map(|_| Arc::new(tokio::sync::Mutex::new(())))
            .collect::<Vec<_>>();
        Self {
            write_inner,
            delete_inner,
            accountant,
            key_locks: Arc::new(key_locks),
            byok_config_cache: None,
        }
    }

    /// Attach the BYOK Wave-3b config cache so a BYOK-`active` tenant is
    /// reserved/released at its committed CIPHERTEXT size (audit C3). Mirrors
    /// [`crate::storage::r2_s3::R2CasHandler::with_byok`]'s gating; pass the SAME
    /// `ByokConfigCache` Arc the storage handler holds so the active-ness lookup
    /// is a shared in-memory cache HIT (one D1 hop total). `None` (the default)
    /// keeps the exact plaintext-size accounting.
    #[must_use]
    pub fn with_byok(mut self, byok_config_cache: Arc<ByokConfigCache>) -> Self {
        self.byok_config_cache = Some(byok_config_cache);
        self
    }
}

impl corelink_handler_cas::CasWriteHandler for AccountingCasHandler {
    fn write(
        &self,
        req: corelink_handler_cas::CasWriteRequest,
    ) -> Result<corelink_handler_cas::CasWriteResponse, corelink_handler_cas::CasHandlerError> {
        use corelink_handler_cas::CasHandlerError;
        let tenant = req.tenant.clone();
        let plaintext_len = i64::try_from(req.bytes.len()).unwrap_or(i64::MAX);
        // The Worker-resolved per-tier cap (threaded via the request) seeds a
        // FRESH `tenant_storage_state` row; `None` ⇒ indeterminate ⇒ a fresh row
        // FAILS CLOSED (never seeded uncapped).
        let quota_seed = req.storage_quota_bytes;
        // rt-nuclear C2: hold the per-`(tenant, hash)` serialization guard across
        // the WHOLE reserve→commit→release below, so a concurrent `delete` of the
        // SAME content-addressed key cannot interleave its delete→release with our
        // reserve/release and under-count `bytes_used`. Distinct keys map to other
        // shards and stay concurrent.
        let _key_guard = self.lock_for(&tenant, &req.claimed_hash);
        // BYOK Wave 3b (audit C3): account the COMMITTED (stored) size — for a
        // BYOK-`active` tenant the R2 object is the ciphertext blob (plaintext +
        // BYOK_CLB1_OVERHEAD), so reserve THAT size (the delete path already
        // releases the real R2 object size) → reserve == release, no drift. A
        // config read error fails CLOSED (503). `None` cache / non-BYOK tenant ⇒
        // `byte_len == plaintext_len`, byte-identical to today.
        let byte_len = match byok_committed_len(
            self.byok_config_cache.as_ref(),
            &tenant,
            plaintext_len,
        ) {
            Ok(n) => n,
            Err(e) => {
                tracing::error!(error = %e, "cas: byok committed-size lookup failed; failing closed");
                return Err(CasHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                )));
            }
        };
        // RESERVE before the R2 PUT (cluster-C): an over-cap / indeterminate
        // reservation is rejected here, so the inner write — the durable R2 PUT
        // — NEVER runs and no uncounted blob is committed.
        match block_on_accrue(&self.accountant, &tenant, byte_len, quota_seed) {
            Ok(AccrueOutcome::Accrued) => {}
            Ok(AccrueOutcome::OverCap) => {
                return Err(CasHandlerError::Internal(format!(
                    "{OVER_CAP_SENTINEL}cas write would exceed storage cap"
                )));
            }
            Ok(AccrueOutcome::Indeterminate) => {
                // Fresh/unsynced tenant + no resolved cap → we refuse to seed an
                // uncapped row. Fail CLOSED (503) — absence is NOT unlimited.
                tracing::error!(
                    tenant = %tenant,
                    "cas: storage cap indeterminate for an unseeded tenant; failing closed"
                );
                return Err(CasHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}storage cap indeterminate (no row, no resolved cap)"
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
    ) -> Result<corelink_handler_cas::CasDeleteResponse, corelink_handler_cas::CasHandlerError>
    {
        let tenant = req.tenant.clone();
        // rt-nuclear C2: hold the SAME per-`(tenant, hash)` serialization guard the
        // write path uses, across the WHOLE inner-delete→release below, so a
        // concurrent overwrite-`write` of the SAME content-addressed key cannot
        // interleave its reserve/release with our delete→release (which would let
        // the delete release this key's bytes while the write re-commits them →
        // `bytes_used` under-count). Distinct keys hash to other shards (concurrent).
        let _key_guard = self.lock_for(&tenant, &req.hash);
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
///
/// `key_locks` is a FIXED [`CAS_LOCK_SHARDS`]-wide array of per-`(tenant,
/// action_digest)` serialization locks — the EXACT mirror of
/// [`AccountingCasHandler`]'s (see [`CAS_LOCK_SHARDS`] for the rt-nuclear C2
/// rationale). AC entries are mutable (a result payload's size can change), so a
/// concurrent AC `update` + `delete` of the SAME key has the identical
/// write-vs-delete byte-accounting race the CAS plane already closed: the delete
/// releases a stale `reclaimed_bytes` while the update independently
/// reserves/commits → `bytes_used` UNDER-count (storage-quota evasion). Both
/// `update` and `delete` acquire the shard their `action_digest` maps to for
/// their entire reserve/commit/release sequence, so a write and a delete of the
/// SAME key can never interleave their accounting; distinct keys map to other
/// shards and stay fully concurrent.
pub struct AccountingAcHandler {
    update_inner: Arc<dyn corelink_handler_ac::AcUpdateHandler>,
    delete_inner: Arc<dyn corelink_handler_ac::AcDeleteHandler>,
    accountant: Arc<ByteAccountant>,
    /// Fixed, memory-bounded shard array of per-`(tenant, action_digest)` async
    /// locks (mirrors [`AccountingCasHandler::key_locks`]).
    key_locks: Arc<Vec<Arc<tokio::sync::Mutex<()>>>>,
    /// BYOK Wave 3b (GATED-INERT): the SAME per-tenant config cache the AC
    /// storage handler uses — see [`AccountingCasHandler::byok_config_cache`].
    /// `None` ⇒ plaintext-size accounting (today's behaviour).
    byok_config_cache: Option<Arc<ByokConfigCache>>,
}

impl core::fmt::Debug for AccountingAcHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountingAcHandler")
            .field("accountant", &self.accountant)
            .field("key_lock_shards", &self.key_locks.len())
            .finish_non_exhaustive()
    }
}

impl AccountingAcHandler {
    /// Wrap the AC update + delete handlers with byte accounting.
    #[must_use]
    pub fn new(
        update_inner: Arc<dyn corelink_handler_ac::AcUpdateHandler>,
        delete_inner: Arc<dyn corelink_handler_ac::AcDeleteHandler>,
        accountant: Arc<ByteAccountant>,
    ) -> Self {
        let key_locks = (0..CAS_LOCK_SHARDS)
            .map(|_| Arc::new(tokio::sync::Mutex::new(())))
            .collect::<Vec<_>>();
        Self {
            update_inner,
            delete_inner,
            accountant,
            key_locks: Arc::new(key_locks),
            byok_config_cache: None,
        }
    }

    /// Attach the BYOK Wave-3b config cache so a BYOK-`active` tenant is
    /// reserved/released at its committed CIPHERTEXT size (audit C3); mirror of
    /// [`AccountingCasHandler::with_byok`]. `None` (the default) keeps the exact
    /// plaintext-size accounting.
    #[must_use]
    pub fn with_byok(mut self, byok_config_cache: Arc<ByokConfigCache>) -> Self {
        self.byok_config_cache = Some(byok_config_cache);
        self
    }

    /// Acquire the per-`(tenant, action_digest)` serialization guard (the shard
    /// the key hashes to) and block on it via the SAME `block_in_place` +
    /// `block_on` bridge the R2 handlers use for their async I/O.
    ///
    /// Byte-identical to [`AccountingCasHandler::lock_for`]. Held by BOTH
    /// [`Self::update`] (across reserve→inner-update→release) and [`Self::delete`]
    /// (across inner-delete→release) so an update and a delete of the SAME AC key
    /// cannot interleave their byte-accounting sequences (rt-nuclear C2 sibling).
    /// The returned guard must be held for the whole accounting sequence.
    ///
    /// Returns an [`tokio::sync::OwnedMutexGuard`] (the shard `Arc` is cloned so
    /// the guard owns its reference and need not borrow the array).
    fn lock_for(&self, tenant: &str, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tenant.hash(&mut hasher);
        // A separator so `(a, bc)` and `(ab, c)` cannot collapse to one key.
        0u8.hash(&mut hasher);
        key.hash(&mut hasher);
        // Map the key hash onto a shard. The modulo is correct for any shard
        // count; `CAS_LOCK_SHARDS` (256) is a power of two so the distribution is
        // uniform and the op is a single cheap division off a 64-bit hash.
        let idx = (hasher.finish() as usize) % self.key_locks.len();
        // `idx < len` by construction (modulo), so `get` is always `Some`; the
        // `unwrap_or_else` is unreachable totality that keeps clippy's
        // `indexing_slicing` happy without a panic path.
        let lock = self
            .key_locks
            .get(idx)
            .map(Arc::clone)
            .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(())));
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(lock.lock_owned()))
    }
}

impl corelink_handler_ac::AcUpdateHandler for AccountingAcHandler {
    fn update(
        &self,
        req: corelink_handler_ac::AcUpdateRequest,
    ) -> Result<corelink_handler_ac::AcUpdateResponse, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        let tenant = req.tenant.clone();
        let plaintext_len = i64::try_from(req.result_payload.len()).unwrap_or(i64::MAX);
        // Worker-resolved per-tier cap (seeds a FRESH row; `None` ⇒ fail-closed).
        let quota_seed = req.storage_quota_bytes;
        // rt-nuclear C2 sibling (AC plane): hold the per-`(tenant, action_digest)`
        // serialization guard across the WHOLE reserve→commit→release below, so a
        // concurrent `delete` of the SAME AC key cannot interleave its
        // delete→release with our reserve/release and under-count `bytes_used`.
        // Distinct keys map to other shards and stay concurrent.
        let _key_guard = self.lock_for(&tenant, &req.action_digest);
        // BYOK Wave 3b (audit C3): account the COMMITTED (stored) size — a
        // BYOK-`active` tenant's AC object is the ciphertext blob (plaintext +
        // BYOK_CLB1_OVERHEAD); reserve THAT so it matches the real R2 object the
        // delete path releases → no drift. Config error ⇒ fail CLOSED (503).
        // `None` cache / non-BYOK ⇒ `byte_len == plaintext_len` (unchanged).
        let byte_len = match byok_committed_len(
            self.byok_config_cache.as_ref(),
            &tenant,
            plaintext_len,
        ) {
            Ok(n) => n,
            Err(e) => {
                tracing::error!(error = %e, "ac: byok committed-size lookup failed; failing closed");
                return Err(AcHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                )));
            }
        };
        match block_on_accrue(&self.accountant, &tenant, byte_len, quota_seed) {
            Ok(AccrueOutcome::Accrued) => {}
            Ok(AccrueOutcome::OverCap) => {
                return Err(AcHandlerError::Internal(format!(
                    "{OVER_CAP_SENTINEL}ac write would exceed storage cap"
                )));
            }
            Ok(AccrueOutcome::Indeterminate) => {
                tracing::error!(
                    tenant = %tenant,
                    "ac: storage cap indeterminate for an unseeded tenant; failing closed"
                );
                return Err(AcHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}storage cap indeterminate (no row, no resolved cap)"
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
        // rt-nuclear C2 sibling (AC plane): hold the SAME per-`(tenant,
        // action_digest)` serialization guard the update path uses, across the
        // WHOLE inner-delete→release below, so a concurrent `update` of the SAME
        // AC key cannot interleave its reserve/release with our delete→release
        // (which would let the delete release this key's bytes while the update
        // re-commits them → `bytes_used` under-count). Distinct keys hash to other
        // shards (concurrent).
        let _key_guard = self.lock_for(&tenant, &req.action_digest);
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
                .and_then(|r| {
                    r.get(&(tenant.to_owned(), region.to_owned()))
                        .map(|row| row.used)
                })
                .unwrap_or(0)
        }

        /// Read a row's `bytes_quota` (test assertion helper); `0` when absent.
        #[must_use]
        pub fn quota(&self, tenant: &str, region: &str) -> i64 {
            self.rows
                .lock()
                .ok()
                .and_then(|r| {
                    r.get(&(tenant.to_owned(), region.to_owned()))
                        .map(|row| row.quota)
                })
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
            quota_seed: Option<i64>,
            _now_ms: i64,
        ) -> Result<AccrueOutcome, String> {
            let mut rows = self
                .rows
                .lock()
                .map_err(|_| "InMemoryByteStore: poisoned lock".to_owned())?;
            let key = (tenant_id.to_owned(), region.to_owned());
            match rows.get_mut(&key) {
                // ── Existing row: accrue against the AUTHORITATIVE cap ────────
                // (mirrors the D1 UPSERT-conflict branch incl. rt-nuclear #16).
                // A finite incoming `quota_seed` (`Some(n)`, `n > 0`) RECONCILES
                // the stored cap (tier downgrade takes effect) and gates this
                // write by the new cap; an unlimited / absent incoming cap does
                // NOT clobber the stored cap and gates by the stored value.
                Some(row) => {
                    // Reseed the stored cap only when the incoming cap is finite.
                    // brutal-audit H3: this reconcile is DELIBERATELY decoupled from
                    // the accrual outcome below — it runs BEFORE the over-cap check,
                    // so a downgrade carrier that is itself OverCap STILL lowers the
                    // stored cap (mirroring the D1 path's separate refused-path
                    // reconcile UPDATE). Do NOT move it after the OverCap return, or
                    // the deadlock (adapter writes gating on a stale higher cap)
                    // re-opens and this fake stops faithfully modelling D1.
                    if let Some(seed) = quota_seed {
                        if seed != 0 {
                            row.quota = seed;
                        }
                    }
                    // Effective cap for this write: the finite incoming cap when
                    // provided, else the (possibly just-reconciled) stored cap.
                    let effective_quota = match quota_seed {
                        Some(seed) if seed != 0 => seed,
                        _ => row.quota,
                    };
                    if effective_quota != 0 && row.used.saturating_add(bytes) > effective_quota {
                        return Ok(AccrueOutcome::OverCap);
                    }
                    row.used = row.used.saturating_add(bytes);
                    Ok(AccrueOutcome::Accrued)
                }
                // ── Fresh row: seed from `quota_seed` (mirrors the D1 INSERT) ─
                None => match quota_seed {
                    // No resolved cap → never seed uncapped; fail CLOSED.
                    None => Ok(AccrueOutcome::Indeterminate),
                    // Finite cap whose very first write already exceeds it →
                    // refuse, no row created (mirrors the D1 INSERT guard).
                    Some(seed) if seed != 0 && bytes > seed => Ok(AccrueOutcome::OverCap),
                    // Seed the fresh row with the real cap (`0` = genuine
                    // unlimited) and apply the first write.
                    Some(seed) => {
                        rows.insert(
                            key,
                            Row {
                                used: bytes,
                                quota: seed,
                            },
                        );
                        Ok(AccrueOutcome::Accrued)
                    }
                },
            }
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
            _quota_seed: Option<i64>,
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

    /// The genuine-unlimited cap seed (`Some(0)`), kept readable in the tests.
    const UNLIMITED: Option<i64> = Some(0);

    #[tokio::test]
    async fn accrue_increments_bytes_used_for_genuine_unlimited() {
        // The load-bearing finding-#1 assertion: a write accrues bytes_used (the
        // counter the storage cap reads — previously NEVER moved). A genuine
        // unlimited tier seeds the fresh row with the `0` sentinel deliberately.
        let store = Arc::new(InMemoryByteStore::new());
        let acc = accountant(store.clone());
        assert_eq!(
            acc.accrue("t1", 1_000, UNLIMITED).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(
            acc.accrue("t1", 500, UNLIMITED).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(
            store.used("t1", REGION),
            1_500,
            "concurrent accruals must sum"
        );
    }

    #[tokio::test]
    async fn fresh_capped_tenant_first_write_seeds_real_cap_not_zero() {
        // (b) A FRESH capped tenant whose first write is UNDER the seeded cap
        // accrues AND the seeded row carries the REAL cap (not 0/unlimited) — so
        // a later over-cap write is correctly refused. This is the core fix:
        // a fresh row must NOT be uncapped.
        let store = Arc::new(InMemoryByteStore::new());
        let acc = accountant(store.clone());
        // First write 600 under a 1000-byte cap → accrues, seeds quota=1000.
        assert_eq!(
            acc.accrue("t-fresh", 600, Some(1_000)).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(store.used("t-fresh", REGION), 600);
        // The seeded cap is REAL: a follow-up that would exceed 1000 is refused
        // even with `None` (the existing row's stored cap governs).
        assert_eq!(
            acc.accrue("t-fresh", 500, None).await.unwrap(),
            AccrueOutcome::OverCap
        );
        assert_eq!(
            store.used("t-fresh", REGION),
            600,
            "over-cap must not move the counter"
        );
        // And exactly filling the remaining headroom is allowed.
        assert_eq!(
            acc.accrue("t-fresh", 400, None).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(store.used("t-fresh", REGION), 1_000);
    }

    #[tokio::test]
    async fn tier_downgrade_reseeds_stored_cap_to_lower_value() {
        // rt-nuclear #16: a row seeded with a HIGH cap, then a write carrying a
        // LOWER (finite) cap, must RECONCILE the stored cap down — the downgrade
        // takes effect (previously the ON CONFLICT never updated bytes_quota, so
        // the tenant kept the old higher cap forever).
        let store = Arc::new(InMemoryByteStore::new());
        store.seed(
            "t-down",
            REGION,
            Row {
                used: 300,
                quota: 10_000,
            },
        );
        let acc = accountant(store.clone());
        // A 100-byte write carrying the NEW lower cap (500) reconciles the stored
        // cap down to 500 and accrues (300 + 100 = 400 <= 500).
        assert_eq!(
            acc.accrue("t-down", 100, Some(500)).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(
            store.quota("t-down", REGION),
            500,
            "stored cap must be reseeded to the lower tier cap"
        );
        assert_eq!(store.used("t-down", REGION), 400);
        // The new lower cap is now enforced: a write that fits the OLD cap but
        // exceeds the NEW one is refused.
        assert_eq!(
            acc.accrue("t-down", 200, Some(500)).await.unwrap(),
            AccrueOutcome::OverCap
        );
        assert_eq!(
            store.used("t-down", REGION),
            400,
            "an over-(new)-cap write must not move the counter"
        );
        // An unlimited / absent incoming cap must NOT clobber the finite stored cap.
        assert_eq!(
            acc.accrue("t-down", 1, UNLIMITED).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(
            store.quota("t-down", REGION),
            500,
            "an unlimited carrier must not lower/raise the stored finite cap"
        );
    }

    #[tokio::test]
    async fn over_cap_downgrade_write_still_reconciles_so_adapter_writes_are_gated() {
        // brutal-audit H3 (HIGH money/COGS) — the reconcile-DEADLOCK.
        //
        // A tenant is DOWNGRADED below its current usage, so it is ALREADY OVER
        // the new (lower) cap. Its NATIVE writes carry the new cap but are refused
        // (OverCap). Pre-fix, the stored `bytes_quota` was reconciled ONLY inside
        // the cap-gated accrue, so a refused write never lowered it — and ADAPTER
        // writes (brew/npm/pip pass `None`, gating against the STORED cap) kept
        // accruing past the paid-for cap indefinitely. The fix decouples the
        // reconcile from accrual success: a refused native write STILL lowers the
        // stored cap, WITHOUT any native write needing to succeed.
        let store = Arc::new(InMemoryByteStore::new());
        // Seeded HIGH cap (10_000), already at 800 used. New tier cap = 500 ⇒ the
        // tenant is over the new cap from the outset.
        store.seed(
            "t-dead",
            REGION,
            Row {
                used: 800,
                quota: 10_000,
            },
        );
        let acc = accountant(store.clone());

        // (1) A NATIVE write carrying the new lower cap (500): 800 + 10 = 810 > 500
        // ⇒ OverCap (correctly REJECTED; counter unchanged). The stored cap MUST
        // still be reconciled DOWN to 500 even though this write was rejected.
        assert_eq!(
            acc.accrue("t-dead", 10, Some(500)).await.unwrap(),
            AccrueOutcome::OverCap
        );
        assert_eq!(
            store.used("t-dead", REGION),
            800,
            "a rejected write must not move the counter"
        );
        assert_eq!(
            store.quota("t-dead", REGION),
            500,
            "the REJECTED downgrade write MUST still reconcile the stored cap down \
             (reconcile decoupled from accrual success) — else adapter writes evade the cap",
        );

        // (2) An ADAPTER write (brew/npm/pip pass `None`) now gates against the
        // reconciled stored cap (500): 800 + 10 = 810 > 500 ⇒ OverCap. Pre-fix it
        // gated against the STALE 10_000 cap and wrongly Accrued (the COGS evasion).
        assert_eq!(
            acc.accrue("t-dead", 10, None).await.unwrap(),
            AccrueOutcome::OverCap
        );
        assert_eq!(
            store.used("t-dead", REGION),
            800,
            "the adapter write must be blocked at the LOWERED cap, not the stale one"
        );

        // (3) Headroom under the new cap is still honoured: dropping below 500 (via
        // a delete) lets an adapter write through at the lowered limit.
        acc.release("t-dead", 400).await.unwrap(); // 800 → 400
        assert_eq!(
            acc.accrue("t-dead", 50, None).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(
            store.used("t-dead", REGION),
            450,
            "writes within the lowered cap still accrue"
        );
    }

    #[tokio::test]
    async fn fresh_capped_tenant_first_write_over_cap_is_refused_not_uncapped() {
        // (a) A FRESH capped tenant whose VERY FIRST write already exceeds the
        // seeded cap must be refused (OverCap) with NO row created — NOT accrued
        // uncapped. (The pre-fix bug seeded quota=0 and let it through unbounded.)
        let store = Arc::new(InMemoryByteStore::new());
        let acc = accountant(store.clone());
        assert_eq!(
            acc.accrue("t-big", 5_000, Some(1_000)).await.unwrap(),
            AccrueOutcome::OverCap
        );
        assert_eq!(
            store.used("t-big", REGION),
            0,
            "a refused first write must create no row"
        );
    }

    #[tokio::test]
    async fn fresh_tenant_with_no_resolved_cap_fails_closed() {
        // (d) A row missing AND the cap header absent (`None`) must FAIL CLOSED
        // (Indeterminate) — never seeded uncapped. Absence is NOT unlimited.
        let store = Arc::new(InMemoryByteStore::new());
        let acc = accountant(store.clone());
        assert_eq!(
            acc.accrue("t-unknown", 100, None).await.unwrap(),
            AccrueOutcome::Indeterminate
        );
        assert_eq!(
            store.used("t-unknown", REGION),
            0,
            "fail-closed must create no row"
        );
    }

    #[tokio::test]
    async fn accrue_over_cap_is_blocked_and_counter_unchanged() {
        // A tenant at 900/1000 bytes; a 200-byte write would hit 1100 > 1000 ⇒
        // OverCap, and the counter must NOT move (atomic check-and-accrue).
        let store = Arc::new(InMemoryByteStore::new());
        store.seed(
            "t-cap",
            REGION,
            Row {
                used: 900,
                quota: 1_000,
            },
        );
        let acc = accountant(store.clone());
        assert_eq!(
            acc.accrue("t-cap", 200, None).await.unwrap(),
            AccrueOutcome::OverCap
        );
        assert_eq!(
            store.used("t-cap", REGION),
            900,
            "an over-cap accrual must not move the counter"
        );
        // A write that exactly fills the cap is allowed (`<=` predicate).
        assert_eq!(
            acc.accrue("t-cap", 100, None).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(store.used("t-cap", REGION), 1_000);
        // Now AT the cap; one more byte trips it.
        assert_eq!(
            acc.accrue("t-cap", 1, None).await.unwrap(),
            AccrueOutcome::OverCap
        );
    }

    #[tokio::test]
    async fn release_saturates_at_zero() {
        let store = Arc::new(InMemoryByteStore::new());
        store.seed(
            "t-del",
            REGION,
            Row {
                used: 300,
                quota: 0,
            },
        );
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
        // A no-new-bytes write (idempotent re-write) accrues nothing — and never
        // reaches the store, so even a `None` cap is a safe no-op (no fresh row).
        assert_eq!(
            acc.accrue("t0", 0, None).await.unwrap(),
            AccrueOutcome::Accrued
        );
        assert_eq!(
            acc.accrue("t0", -5, None).await.unwrap(),
            AccrueOutcome::Accrued
        );
        acc.release("t0", 0).await.unwrap();
        assert_eq!(store.used("t0", REGION), 0);
    }

    #[tokio::test]
    async fn store_error_surfaces_for_fail_closed_handling() {
        // The handler maps an accrue Err to 503 (fail-CLOSED) — assert the error
        // propagates rather than being silently swallowed.
        let acc = accountant(Arc::new(ErroringByteStore));
        assert!(acc.accrue("t-err", 100, Some(1_000)).await.is_err());
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

    /// AC-plane fixtures (rt-nuclear C2 sibling): the AC `update`-vs-`delete`
    /// write-vs-delete byte-accounting race, mirroring the CAS suite below.
    mod ac {
        use super::{ByteAccountant, ByteStore, InMemoryByteStore, Row, REGION};
        use corelink_handler_ac::{
            AcDeleteHandler, AcDeleteRequest, AcLookupHandler, AcLookupRequest, AcUpdateHandler,
            AcUpdateRequest, InMemoryAcHandler, InMemoryAuditSink, InMemorySliObserver,
        };
        use std::sync::Arc;

        use crate::byte_accounting::AccountingAcHandler;

        /// Build an `AccountingAcHandler` over a fresh InMemory AC backing + a
        /// byte store, returning the decorator, the underlying handler (to inspect
        /// stored entries), and the byte store (to assert the counter). Mirrors
        /// the CAS `cas_fixture` below.
        fn ac_fixture(
            seed: Option<(&str, Row)>,
        ) -> (
            Arc<AccountingAcHandler>,
            Arc<InMemoryAcHandler>,
            Arc<InMemoryByteStore>,
        ) {
            let audit = Arc::new(InMemoryAuditSink::new());
            let sli = Arc::new(InMemorySliObserver::new());
            let inner = Arc::new(InMemoryAcHandler::new(audit, sli));
            let store = Arc::new(InMemoryByteStore::new());
            if let Some((tenant, row)) = seed {
                store.seed(tenant, REGION, row);
            }
            let acc = Arc::new(ByteAccountant::new(
                store.clone() as Arc<dyn ByteStore>,
                REGION.to_owned(),
            ));
            let dec = Arc::new(AccountingAcHandler::new(
                inner.clone() as Arc<dyn AcUpdateHandler>,
                inner.clone() as Arc<dyn AcDeleteHandler>,
                acc,
            ));
            (dec, inner, store)
        }

        /// Is the AC key present on disk in the inner handler?
        fn is_present(inner: &InMemoryAcHandler, tenant: &str, digest: &str) -> bool {
            inner
                .lookup(AcLookupRequest::new(tenant, digest, "p", tenant, 9))
                .is_ok()
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_update_vs_delete_same_key_nets_to_truth() {
            // rt-nuclear C2 sibling (AC plane): an AC `delete` of key K (size L)
            // racing a concurrent `update` of the SAME (tenant, action_digest)
            // must leave `bytes_used` EQUAL to the on-disk reality — L when the
            // update wins (entry present), 0 when the delete wins (entry absent).
            // Before the AC-plane lock, the delete's release(L) and the update's
            // independent reserve/release could interleave so the two did NOT net
            // to the true on-disk total, UNDER-counting `bytes_used` by up to L
            // (storage-quota evasion). The per-(tenant, action_digest) decorator
            // lock now serializes the full reserve/commit/release of UPDATE against
            // the full delete/release of DELETE for one key, so the counter always
            // tracks the truth (and never underflows).
            //
            // The InMemory AC handler is content-addressed: re-`update` with the
            // SAME body is an idempotent no-op when the entry is present (durable =
            // false ⇒ the decorator rolls the reservation back) but a durable
            // re-insert once the delete has removed it (⇒ accrues L). Either way
            // the lock makes the (counter == on-disk) invariant hold each race.
            let digest = "a".repeat(64);
            let body = b"ac-race-the-same-key".to_vec();
            let n = body.len() as i64;
            for iter in 0..200u64 {
                // Fresh fixture per iteration with the entry already present + the
                // counter already reflecting it (the on-disk truth at the start).
                let (dec, inner, store) = ac_fixture(Some(("t", Row { used: n, quota: 0 })));
                // Seed the entry on disk so a `delete` actually reclaims `n` bytes.
                dec.update(
                    AcUpdateRequest::new("t", digest.clone(), body.clone(), "p", "t", iter)
                        .with_storage_quota_bytes(Some(0)),
                )
                .expect("seed update");
                // The seed update was a fresh insert (durable), so it accrued
                // another `n`; normalise the counter back to the single-copy
                // on-disk truth so the race starts from (counter == on-disk).
                store.seed("t", REGION, Row { used: n, quota: 0 });

                let du = dec.clone();
                let dd = dec.clone();
                let dgu = digest.clone();
                let dgd = digest.clone();
                let bu = body.clone();
                // Concurrent UPDATE and DELETE of the SAME (tenant, action_digest).
                let tu = tokio::spawn(async move {
                    let _ = du.update(
                        AcUpdateRequest::new("t", dgu, bu, "p", "t", 1)
                            .with_storage_quota_bytes(Some(0)),
                    );
                });
                let td = tokio::spawn(async move {
                    let _ = dd.delete(AcDeleteRequest::new("t", dgd, "p", "t", 2));
                });
                tu.await.unwrap();
                td.await.unwrap();

                // The on-disk truth after the race: is the entry present?
                let present = is_present(&inner, "t", &digest);
                let counter = store.used("t", REGION);
                let expected = if present { n } else { 0 };
                assert_eq!(
                    counter, expected,
                    "iter {iter}: bytes_used ({counter}) must equal the on-disk truth \
                     ({expected}; present={present}) — AC update-vs-delete accounting must net exactly"
                );
                assert!(
                    counter >= 0,
                    "iter {iter}: bytes_used must never underflow below zero"
                );
            }
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn distinct_keys_update_and_delete_stay_concurrent() {
            // The lock must serialize only the SAME (tenant, action_digest): an
            // update of key A and a delete of key B must both proceed and account
            // independently (different shards / no false dependency).
            let (dec, _inner, store) = ac_fixture(None);
            let dig_a = "a".repeat(64);
            let dig_b = "b".repeat(64);
            let ba = b"ac-key-a-bytes".to_vec();
            let bb = b"ac-key-b-different".to_vec();
            let na = ba.len() as i64;
            let nb = bb.len() as i64;
            // Pre-seed key B so its delete reclaims real bytes; counter reflects B.
            dec.update(
                AcUpdateRequest::new("t", dig_b.clone(), bb, "p", "t", 1)
                    .with_storage_quota_bytes(Some(0)),
            )
            .expect("seed B");
            assert_eq!(store.used("t", REGION), nb, "seed of B accrues B's bytes");

            let du = dec.clone();
            let dd = dec.clone();
            let tu = tokio::spawn(async move {
                du.update(
                    AcUpdateRequest::new("t", dig_a, ba, "p", "t", 2)
                        .with_storage_quota_bytes(Some(0)),
                )
            });
            let td =
                tokio::spawn(
                    async move { dd.delete(AcDeleteRequest::new("t", dig_b, "p", "t", 3)) },
                );
            tu.await.unwrap().expect("update A");
            td.await.unwrap().expect("delete B");

            // Net effect: +na (A written) and −nb (B deleted) over the seeded nb →
            // exactly na. Distinct keys never block each other and account cleanly.
            assert_eq!(
                store.used("t", REGION),
                na,
                "distinct-key update + delete must account independently (only A's bytes remain)"
            );
        }
    }

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
            .write(
                CasWriteRequest::new("t-cap", hash.clone(), body, "p", "t-cap", 1)
                    .with_storage_quota_bytes(Some(4)),
            )
            .expect_err("over-cap write must be refused");
        match err {
            corelink_handler_cas::CasHandlerError::Internal(ref m) => {
                assert!(
                    m.starts_with(OVER_CAP_SENTINEL),
                    "must carry the over-cap sentinel: {m}"
                );
            }
            other => panic!("expected over-cap Internal, got {other:?}"),
        }
        // The reservation was refused, so the inner store never ran: NO blob.
        let read = inner.read(CasReadRequest::new("t-cap", hash, "p", "t-cap", 2));
        assert!(
            matches!(
                read,
                Err(corelink_handler_cas::CasHandlerError::NotFound { .. })
            ),
            "an over-cap write must leave NO blob in storage (reserve-before-commit)"
        );
        // Counter unchanged (the atomic reservation did not move it).
        assert_eq!(
            store.used("t-cap", REGION),
            4,
            "over-cap reservation must not move the counter"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn under_cap_write_accrues_then_delete_releases() {
        let (dec, _inner, store) = cas_fixture(None);
        let body = b"hello-bytes".to_vec();
        let n = body.len() as i64;
        let hash = hash_for(&body);
        // Fresh tenant with a real resolved cap — seeds the row with the cap.
        dec.write(
            CasWriteRequest::new("t1", hash.clone(), body, "p", "t1", 1)
                .with_storage_quota_bytes(Some(1_000_000)),
        )
        .expect("write");
        assert_eq!(
            store.used("t1", REGION),
            n,
            "a durable write must accrue its bytes"
        );
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
    async fn genuine_unlimited_tenant_accrues_unbounded() {
        // (c) A genuine unlimited-tier tenant (cap signalled as `Some(0)`)
        // accrues without bound: a fresh row is seeded with the `0` sentinel and
        // a large write far past any finite cap still succeeds + is counted.
        let (dec, _inner, store) = cas_fixture(None);
        let body = vec![b'x'; 4096];
        let n = body.len() as i64;
        let hash = hash_for(&body);
        dec.write(
            CasWriteRequest::new("t-unl", hash, body, "p", "t-unl", 1)
                .with_storage_quota_bytes(Some(0)),
        )
        .expect("unlimited write must succeed");
        assert_eq!(
            store.used("t-unl", REGION),
            n,
            "unlimited tenant still accrues bytes_used"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fresh_tenant_no_cap_header_fails_closed_no_blob() {
        // (d) at the decorator: a fresh tenant whose request carries NO resolved
        // cap (`None`) must be refused (indeterminate → 503 sentinel) with NO
        // blob stored — absence is never treated as unlimited.
        let (dec, inner, store) = cas_fixture(None);
        let body = b"no-cap-known".to_vec();
        let hash = hash_for(&body);
        let err = dec
            .write(CasWriteRequest::new(
                "t-nocap",
                hash.clone(),
                body,
                "p",
                "t-nocap",
                1,
            ))
            .expect_err("indeterminate-cap write must be refused");
        match err {
            corelink_handler_cas::CasHandlerError::Internal(ref m) => {
                assert!(
                    m.starts_with(ACCT_UNAVAILABLE_SENTINEL),
                    "must carry the accounting-unavailable (fail-closed/503) sentinel: {m}"
                );
            }
            other => panic!("expected fail-closed Internal, got {other:?}"),
        }
        let read = inner.read(CasReadRequest::new("t-nocap", hash, "p", "t-nocap", 2));
        assert!(
            matches!(
                read,
                Err(corelink_handler_cas::CasHandlerError::NotFound { .. })
            ),
            "an indeterminate-cap write must leave NO blob (fail-closed before commit)"
        );
        assert_eq!(
            store.used("t-nocap", REGION),
            0,
            "fail-closed must create no row"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn idempotent_rewrite_does_not_double_count() {
        let (dec, _inner, store) = cas_fixture(None);
        let body = b"same-bytes".to_vec();
        let n = body.len() as i64;
        let hash = hash_for(&body);
        dec.write(
            CasWriteRequest::new("t1", hash.clone(), body.clone(), "p", "t1", 1)
                .with_storage_quota_bytes(Some(1_000_000)),
        )
        .expect("first write");
        // Second identical write is idempotent (`durable == false`) → the
        // decorator rolls the reservation back, so the counter stays at n.
        dec.write(
            CasWriteRequest::new("t1", hash, body, "p", "t1", 2)
                .with_storage_quota_bytes(Some(1_000_000)),
        )
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
        let err = dec.write(
            CasWriteRequest::new("victim", hash, body, "p", "attacker", 1)
                .with_storage_quota_bytes(Some(1_000_000)),
        );
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_write_vs_delete_same_key_nets_to_truth() {
        // rt-nuclear C2: a `delete` of CAS key K (size L) racing a concurrent
        // overwrite-`write` of the SAME (tenant, hash) must leave `bytes_used`
        // EQUAL to the on-disk reality — L when the write wins (blob present), 0
        // when the delete wins (blob absent). Before the fix the delete's
        // release(L) and the write's independent reserve/release could interleave
        // so the two did NOT net to the true on-disk total, UNDER-counting
        // `bytes_used` by up to L (storage-quota evasion). The per-(tenant, hash)
        // decorator lock now serializes the full reserve/commit/release of WRITE
        // against the full delete/release of DELETE for one key, so the counter
        // always tracks the truth (and never underflows).
        //
        // Many iterations exercise the scheduler so an unserialized interleaving
        // would be hit with overwhelming probability.
        let body = b"race-the-same-key".to_vec();
        let n = body.len() as i64;
        let hash = hash_for(&body);
        for iter in 0..200u64 {
            // Fresh fixture per iteration with the key already present + the
            // counter already reflecting it (the on-disk truth at the start).
            let (dec, inner, store) = cas_fixture(Some(("t", Row { used: n, quota: 0 })));
            // Seed the blob on disk so a `delete` actually reclaims `n` bytes.
            dec.write(
                CasWriteRequest::new("t", hash.clone(), body.clone(), "p", "t", iter)
                    .with_storage_quota_bytes(Some(0)),
            )
            .expect("seed write");
            // The seed write was a fresh insert (durable), so it accrued another
            // `n`; normalise the counter back to the single-copy on-disk truth so
            // the race starts from a consistent (counter == on-disk) state.
            store.seed("t", REGION, Row { used: n, quota: 0 });

            let dw = dec.clone();
            let dd = dec.clone();
            let hw = hash.clone();
            let hd = hash.clone();
            let bw = body.clone();
            // Concurrent overwrite-WRITE and DELETE of the SAME (tenant, hash).
            let tw = tokio::spawn(async move {
                let _ = dw.write(
                    CasWriteRequest::new("t", hw, bw, "p", "t", 1)
                        .with_storage_quota_bytes(Some(0)),
                );
            });
            let td = tokio::spawn(async move {
                let _ = dd.delete(CasDeleteRequest::new("t", hd, "p", "t", 2));
            });
            tw.await.unwrap();
            td.await.unwrap();

            // The on-disk truth after the race: is the blob present?
            let present = inner
                .read(CasReadRequest::new("t", hash.clone(), "p", "t", 3))
                .is_ok();
            let counter = store.used("t", REGION);
            let expected = if present { n } else { 0 };
            assert_eq!(
                counter, expected,
                "iter {iter}: bytes_used ({counter}) must equal the on-disk truth \
                 ({expected}; present={present}) — write-vs-delete accounting must net exactly"
            );
            assert!(
                counter >= 0,
                "iter {iter}: bytes_used must never underflow below zero"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn distinct_keys_write_and_delete_stay_concurrent() {
        // The lock must serialize only the SAME (tenant, hash): a write of key A
        // and a delete of key B must both proceed and account independently
        // (different shards / no false dependency on the happy path).
        let (dec, _inner, store) = cas_fixture(None);
        let ba = b"key-a-bytes".to_vec();
        let bb = b"key-b-different".to_vec();
        let na = ba.len() as i64;
        let nb = bb.len() as i64;
        let ha = hash_for(&ba);
        let hb = hash_for(&bb);
        // Pre-seed key B so its delete reclaims real bytes; counter reflects B.
        dec.write(
            CasWriteRequest::new("t", hb.clone(), bb, "p", "t", 1)
                .with_storage_quota_bytes(Some(0)),
        )
        .expect("seed B");
        assert_eq!(store.used("t", REGION), nb, "seed of B accrues B's bytes");

        let dw = dec.clone();
        let dd = dec.clone();
        let tw = tokio::spawn(async move {
            dw.write(
                CasWriteRequest::new("t", ha, ba, "p", "t", 2).with_storage_quota_bytes(Some(0)),
            )
        });
        let td =
            tokio::spawn(async move { dd.delete(CasDeleteRequest::new("t", hb, "p", "t", 3)) });
        tw.await.unwrap().expect("write A");
        td.await.unwrap().expect("delete B");

        // Net effect: +na (A written) and −nb (B deleted) over the seeded nb →
        // exactly na. Distinct keys never block each other and account cleanly.
        assert_eq!(
            store.used("t", REGION),
            na,
            "distinct-key write + delete must account independently (only A's bytes remain)"
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
mod byok_accounting_tests {
    //! BYOK Wave 3b (audit C3 CRITICAL): the accountant reserves/releases the
    //! COMMITTED (stored) size — `plaintext + BYOK_CLB1_OVERHEAD` for a
    //! BYOK-`active` tenant — so reserve == release == the on-disk object the
    //! delete path frees, and `bytes_used` never drifts. Non-BYOK tenants are
    //! byte-identical to today.
    use super::testing::InMemoryByteStore;
    use super::*;
    use crate::customer_d1::{
        ByokConfigError, ByokCryptoMode, ByokMode, ByokState, TenantByokConfig,
    };
    use crate::storage::byok_cas::{ByokConfigCache, ByokConfigSource};
    use corelink_handler_cas::{
        CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasWriteHandler,
        CasWriteRequest, CasWriteResponse,
    };

    const REGION: &str = "iad";
    const TENANT: &str = "byok-acct-tenant";

    fn cfg(mode: ByokCryptoMode, state: ByokState) -> TenantByokConfig {
        TenantByokConfig {
            tenant_id: TENANT.to_owned(),
            mode: ByokMode::Byok,
            crypto_mode: mode,
            cmk_provider: Some("aws".to_owned()),
            cmk_key_id: Some("arn:cmk".to_owned()),
            cmk_region: Some("iad".to_owned()),
            state,
        }
    }

    #[derive(Debug)]
    struct CfgSrc {
        cfg: Option<TenantByokConfig>,
        fail: bool,
    }
    #[async_trait]
    impl ByokConfigSource for CfgSrc {
        async fn get_byok_config(
            &self,
            _t: &str,
        ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
            if self.fail {
                return Err(ByokConfigError::Transport("byok config down".to_owned()));
            }
            Ok(self.cfg.clone())
        }
    }

    fn cache(cfg: Option<TenantByokConfig>, fail: bool) -> Arc<ByokConfigCache> {
        Arc::new(ByokConfigCache::new(Arc::new(CfgSrc { cfg, fail }), 60))
    }

    // ── byok_committed_len: the reserve/release sizing decision ──────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn committed_len_is_plaintext_when_cache_absent() {
        // `None` cache (today's production / tests) ⇒ plaintext size verbatim.
        assert_eq!(byok_committed_len(None, TENANT, 1000).unwrap(), 1000);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn committed_len_adds_overhead_only_for_active_convergent() {
        let active = cache(
            Some(cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        );
        assert_eq!(
            byok_committed_len(Some(&active), TENANT, 1000).unwrap(),
            1000 + BYOK_CLB1_OVERHEAD as i64,
            "an active convergent tenant stores ciphertext ⇒ reserve plaintext + 32"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn committed_len_is_plaintext_for_inactive_and_unconfigured() {
        let inactive = cache(
            Some(cfg(ByokCryptoMode::Convergent, ByokState::Inactive)),
            false,
        );
        assert_eq!(
            byok_committed_len(Some(&inactive), TENANT, 1000).unwrap(),
            1000
        );
        let unconfigured = cache(None, false);
        assert_eq!(
            byok_committed_len(Some(&unconfigured), TENANT, 1000).unwrap(),
            1000
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn committed_len_is_plaintext_for_public_namespace() {
        // `_public` stays plaintext (dedup) even under an active config.
        let active = cache(
            Some(cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        );
        assert_eq!(
            byok_committed_len(Some(&active), crate::adapter_cache::PUBLIC_NAMESPACE, 1000)
                .unwrap(),
            1000,
            "_public is never encrypted ⇒ plaintext-size accounting"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn committed_len_adds_clb2_overhead_for_active_random() {
        // Wave 3c: Mode B (random) is an ENCRYPTING mode and stores a `CLB2` blob
        // (+20 B — nonce lives in `byok_envelope`, not inline), so the reservation
        // must reflect the committed ciphertext size.
        let mode_b = cache(Some(cfg(ByokCryptoMode::Random, ByokState::Active)), false);
        assert_eq!(
            byok_committed_len(Some(&mode_b), TENANT, 1000).unwrap(),
            1000 + BYOK_CLB2_OVERHEAD as i64,
            "an active random tenant stores CLB2 ciphertext ⇒ reserve plaintext + 20"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn committed_len_is_plaintext_for_failclosed_modes() {
        // `partial` (backfill dual-read) still engages `FailClosed` (Wave 4): the
        // inner write stores NOTHING (fails closed), so the reservation rolls back
        // net-zero ⇒ plaintext size.
        let partial = cache(
            Some(cfg(ByokCryptoMode::Convergent, ByokState::Partial)),
            false,
        );
        assert_eq!(
            byok_committed_len(Some(&partial), TENANT, 1000).unwrap(),
            1000
        );
        let partial_random = cache(Some(cfg(ByokCryptoMode::Random, ByokState::Partial)), false);
        assert_eq!(
            byok_committed_len(Some(&partial_random), TENANT, 1000).unwrap(),
            1000
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn committed_len_fails_closed_on_config_error() {
        // A config-read error must NOT under-reserve an active tenant → Err (503).
        let broken = cache(None, true);
        assert!(byok_committed_len(Some(&broken), TENANT, 1000).is_err());
    }

    // ── decorator net-zero with a faithful encrypting inner ──────────────────

    /// A fake inner CAS handler that models the production BYOK R2 handler: it
    /// stores the CIPHERTEXT object (`plaintext + BYOK_CLB1_OVERHEAD`) and, on
    /// delete, reclaims exactly that committed object size — so a write→delete
    /// cycle's reserve and release both move by the committed size.
    #[derive(Debug, Default)]
    struct EncryptingCasInner {
        stored: std::sync::Mutex<std::collections::HashMap<(String, String), u64>>,
    }
    impl CasWriteHandler for EncryptingCasInner {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            let committed = req.bytes.len() as u64 + BYOK_CLB1_OVERHEAD;
            let mut m = self.stored.lock().unwrap();
            let key = (req.tenant.clone(), req.claimed_hash.clone());
            // Content-addressed: a re-PUT of an already-present key is idempotent.
            let durable = !m.contains_key(&key);
            m.insert(key, committed);
            Ok(CasWriteResponse::new(req.claimed_hash, durable))
        }
    }
    impl CasDeleteHandler for EncryptingCasInner {
        fn delete(&self, req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
            let mut m = self.stored.lock().unwrap();
            let reclaimed = m
                .remove(&(req.tenant.clone(), req.hash.clone()))
                .unwrap_or(0);
            Ok(CasDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn active_byok_write_then_delete_nets_to_zero_at_committed_size() {
        // The C3 assertion: for a BYOK-active tenant the accountant reserves the
        // ciphertext size (plaintext + 32) on write and the delete releases the
        // SAME committed size (the real R2 object), so a write→delete cycle leaves
        // bytes_used at exactly zero — no over-release, no under-count.
        let store = Arc::new(InMemoryByteStore::new());
        let acc = Arc::new(ByteAccountant::new(
            store.clone() as Arc<dyn ByteStore>,
            REGION.to_owned(),
        ));
        let inner = Arc::new(EncryptingCasInner::default());
        let dec = AccountingCasHandler::new(
            inner.clone() as Arc<dyn CasWriteHandler>,
            inner as Arc<dyn CasDeleteHandler>,
            acc,
        )
        .with_byok(cache(
            Some(cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        ));

        let body = vec![b'x'; 500];
        let n = body.len() as i64;
        let committed = n + BYOK_CLB1_OVERHEAD as i64;
        let hash = "a".repeat(64);
        dec.write(
            CasWriteRequest::new(TENANT, hash.clone(), body, "p", TENANT, 1)
                .with_storage_quota_bytes(Some(0)),
        )
        .expect("active write");
        assert_eq!(
            store.used(TENANT, REGION),
            committed,
            "an active-BYOK write must reserve the COMMITTED ciphertext size (plaintext + 32)"
        );

        dec.delete(CasDeleteRequest::new(TENANT, hash, "p", TENANT, 2))
            .expect("delete");
        assert_eq!(
            store.used(TENANT, REGION),
            0,
            "the delete must release the SAME committed size ⇒ net-zero, no drift (C3)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inactive_tenant_with_cache_wired_is_unchanged_plaintext_accounting() {
        // A non-BYOK (inactive) tenant, even with the cache wired, accounts at the
        // plaintext size — zero behaviour change. The faithful inner stores the
        // committed size, but since the tenant is inactive the accountant reserves
        // plaintext; the delete still nets to zero because the inner reclaims what
        // it stored (here +32) — proving the accountant tracks the inner, and that
        // for an INACTIVE tenant the reserve is plaintext (not plaintext+32).
        let store = Arc::new(InMemoryByteStore::new());
        let acc = Arc::new(ByteAccountant::new(
            store.clone() as Arc<dyn ByteStore>,
            REGION.to_owned(),
        ));
        let inner = Arc::new(EncryptingCasInner::default());
        let dec = AccountingCasHandler::new(
            inner.clone() as Arc<dyn CasWriteHandler>,
            inner as Arc<dyn CasDeleteHandler>,
            acc,
        )
        .with_byok(cache(
            Some(cfg(ByokCryptoMode::Convergent, ByokState::Inactive)),
            false,
        ));

        let body = vec![b'y'; 500];
        let n = body.len() as i64;
        let hash = "b".repeat(64);
        dec.write(
            CasWriteRequest::new(TENANT, hash.clone(), body, "p", TENANT, 1)
                .with_storage_quota_bytes(Some(0)),
        )
        .expect("inactive write");
        assert_eq!(
            store.used(TENANT, REGION),
            n,
            "an INACTIVE tenant must reserve the PLAINTEXT size (no BYOK overhead)"
        );
    }
}
