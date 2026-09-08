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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
                    // stored `bytes_quota` is NEVER lowered). Adapter writes with
                    // no fresh cap (brew/pip, or npm only through an indeterminate
                    // compatibility path) then keep gating against the STALE higher
                    // stored cap and accrue past the paid-for cap forever — a
                    // COGS-evasion deadlock (the reconcile was coupled to a
                    // SUCCESSFUL native write that, for an over-cap tenant, can
                    // never succeed). Production npm resolves the PAT-derived cap
                    // before this post-buffer moat write, so it is not a None-seed
                    // path when the resolver has an answer.
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

include!("b126_m2_impl_01_part_02.rs");
