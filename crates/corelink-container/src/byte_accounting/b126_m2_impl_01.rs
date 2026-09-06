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
    // This bridge is called from the synchronous accounting decorator. Keep
    // the phase strictly around the D1 future; the inner R2 handler is outside
    // this scope and records `ostore` independently.
    let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
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
    let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
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
        // Reject a forged physical/accounting pairing before touching the
        // quota ledger.  The inner handler repeats the gate (and emits the
        // canonical denial audit), but the decorator must not even transiently
        // reserve another tenant's bytes for a malformed request.
        if !req.is_authorized_for_caller() {
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }
        // `tenant` is the physical storage namespace (and BYOK namespace),
        // while `accounting_tenant` is the authenticated tenant whose quota
        // pays for this reservation.  Public moat writes intentionally have
        // `tenant == "_public"` and a real `accounting_tenant`.
        let storage_namespace = req.tenant.clone();
        let accounting_tenant = req.accounting_tenant.clone();
        let plaintext_len = i64::try_from(req.bytes.len()).unwrap_or(i64::MAX);
        // The resolved cap is always the caller's cap, including public writes.
        // A missing cap therefore fails closed for a fresh real tenant; `_public`
        // is never an implicit unlimited escape hatch.
        let quota_seed = req.storage_quota_bytes;
        // rt-nuclear C2: hold the per-`(accounting tenant, hash)` serialization
        // guard across
        // the WHOLE reserve→commit→release below, so a concurrent `delete` of the
        // SAME content-addressed key cannot interleave its delete→release with our
        // reserve/release and under-count `bytes_used`. Distinct keys map to other
        // shards and stay concurrent.
        let _key_guard = self.lock_for(&accounting_tenant, &req.claimed_hash);
        // BYOK Wave 3b (audit C3): account the COMMITTED (stored) size — for a
        // BYOK-`active` tenant the R2 object is the ciphertext blob (plaintext +
        // BYOK_CLB1_OVERHEAD), so reserve THAT size (the delete path already
        // releases the real R2 object size) → reserve == release, no drift. A
        // config read error fails CLOSED (503). `None` cache / non-BYOK tenant ⇒
        // `byte_len == plaintext_len`, byte-identical to today.
        let committed_len = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
            byok_committed_len(
                self.byok_config_cache.as_ref(),
                &storage_namespace,
                plaintext_len,
            )
        };
        let byte_len = match committed_len {
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
        match block_on_accrue(&self.accountant, &accounting_tenant, byte_len, quota_seed) {
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
                    tenant = %accounting_tenant,
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
                    block_on_release(&self.accountant, &accounting_tenant, byte_len);
                }
                Ok(resp)
            }
            Err(e) => {
                // The write failed AFTER we reserved — RELEASE the reservation
                // so a failed PUT does not permanently consume headroom.
                block_on_release(&self.accountant, &accounting_tenant, byte_len);
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

#[allow(dead_code)]
const B126_M2_IMPL_1_REANCHOR: () = ();
