//! Shared container-side PAT verifier for the cache adapters
//! (cargo / brew / npm / oci / pip) — Option B (defense-in-depth).
//!
//! # Why this exists
//!
//! Each `corelink_adapter_host` adapter is designed to **self-validate**
//! the bearer PAT inside the container rather than trusting the
//! Worker-injected `x-corelink-tenant-id`. The owner chose Option B
//! (2026-06-05, FINDING-sccache-adapter-gaps §Gap 2): the container
//! re-verifies the PAT against the D1 `pat` store. The Worker already
//! validates the PAT (HMAC fast-fail + D1 lookup) under its cpu_ms
//! budget; this verifier repeats the **full** verification — including
//! the Argon2id possession check the Worker skips — so a compromised or
//! misconfigured Worker cannot grant cache access on its own.
//!
//! Each adapter declares its OWN (nominally distinct) `TenantResolver`
//! port trait, so this module exposes a trait-agnostic [`PatVerifier`]
//! and each adapter route module wraps it in a thin newtype shell that
//! impls that adapter's `TenantResolver` (see `routes/<adapter>.rs`).
//! The verification pipeline lives here, once.
//!
//! # Verification pipeline (mirrors `auth_model.md §2.3`)
//!
//! 1. **HMAC fast-reject** ([`verify_hmac_only`]) — parse the plaintext
//!    and reject a bad signature in ≤100µs, *before* any D1 round-trip,
//!    so forged tokens cannot drive D1 query cost.
//! 2. **D1 lookup** by the non-secret `token_id` (expiry filtered in SQL).
//!    This row lookup runs BEFORE the expensive Argon2id verify (finding #12):
//!    a valid-HMAC token for a nonexistent / expired / revoked / wrong-tenant
//!    `token_id` is decided here at the cheap D1 stage, so a leaked-signing-key
//!    attacker minting valid-HMAC tokens for bogus `token_id`s never reaches a
//!    full Argon2id verify (the dummy timing-burn it DOES hit is itself bounded
//!    — global permit + a single shared synthetic per-tenant bucket).
//! 3. **Full verify** ([`verify_with_hash`]) — constant-time `token_id`
//!    match + HMAC + Argon2id of the secret segment against the stored
//!    PHC hash. Run on a blocking thread (Argon2id is CPU-heavy), and
//!    **memoised** by [`SecretMatchMemo`]: the memoised fact is the
//!    immutable "this plaintext matches this PHC hash", never an
//!    authorization decision. Steps 1 and 2 still run on every request,
//!    so revocation, expiry, scope and tenant binding are never cached
//!    and stay immediate on this plane.
//! 4. **Scope gate** — fail-CLOSED unless the D1 `scope` string grants a
//!    cache capability. The port carries no operation, so this asserts
//!    only "has SOME cache capability"; per-operation read/write is
//!    enforced one layer up at each adapter route from the Worker-set
//!    `x-corelink-scope` header.
//!
//! Every distinguishable failure (bad parse, bad sig, unknown token,
//! expired, wrong secret, no cache scope) collapses to
//! [`VerifyError::InvalidPat`] so the wire surface cannot tell an
//! attacker *why* a token was rejected. Only genuine backend faults (D1
//! unreachable, corrupt row) surface as [`VerifyError::Backend`].
//!
//! # The overload shed is uniform (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`)
//!
//! There is one axis on which the two outcomes are deliberately NOT the
//! uniform `InvalidPat`: the **load shed**. When the Argon2id permit pool
//! (global or per-tenant) cannot be entered within `ARGON2_PERMIT_WAIT` —
//! or, on the dummy-burn arm's shared bucket, cannot be entered at once —
//! the pipeline gives up *before deciding anything about the credential* —
//! so the honest answer is "verifier overloaded", not "invalid PAT", and
//! every shed arm returns [`VerifyError::Backend`] ⇒ HTTP 503 +
//! `Retry-After`. WHEN a tier sheds is a tuning choice; WHAT it answers is
//! the invariant.
//!
//! The invariant that matters is that the shed is **symmetric across row
//! existence**: the row-FOUND arm (step 3) and the row-NOT-FOUND
//! dummy-burn arm (step 2) shed with the identical variant and message.
//! An asymmetric shed would be a *row-existence oracle*: an attacker who
//! can saturate the pool (cheap — the pool is small and Argon2id is slow)
//! would read "503 vs 401" as "this `token_id` is live vs unknown",
//! which is strictly more than the latency signal the dummy burn exists
//! to hide. Concretely: under saturation EVERY outcome is `Backend`/503,
//! and outside saturation EVERY rejection is `InvalidPat`/401.
//!
//! Two arms are NOT part of the shed and stay `InvalidPat` by design:
//! the HMAC fast-reject (upstream of every permit, so an attacker without
//! the signing key can never reach the shed at all) and the terminal
//! rejection after the dummy burn actually ran (a completed burn IS the
//! uniform 401 path).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use corelink_pat::{verify_hmac_only_multi, verify_with_hash_multi, PatHash, PatSigningKey};
use futures::future::{BoxFuture, FutureExt, Shared};
use sha2::{Digest, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::scope::{requires_cache_read, requires_cache_write};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::{non_empty_env, StorageEnv};

/// Failure surface of [`PatVerifier::verify`].
///
/// Adapter route shells map this onto their adapter's local
/// `TenantResolveError` (`InvalidPat` ⇒ 401, `Backend` ⇒ 503).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// PAT not found, expired, forged, wrong secret, or lacking a cache
    /// scope. Uniform by design (no oracle). Surfaces as HTTP 401.
    ///
    /// NEVER returned from a load shed — see the module header on
    /// `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`.
    #[error("invalid PAT")]
    InvalidPat,
    /// Verifier backend (D1, corrupt row) failed, **or** the Argon2id
    /// permit pool shed this request under saturation (message `pat
    /// verifier overloaded`, identical on the row-FOUND and row-NOT-FOUND
    /// arms — `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`). Surfaces as HTTP 503;
    /// the adapter surfaces attach `Retry-After: 1` because a shed is
    /// transient and converges as soon as the first in-flight verify
    /// populates the memo.
    #[error("verifier backend: {0}")]
    Backend(String),
}

/// A single `pat` row, reduced to the fields PAT verification needs.
///
/// `token_id` is the non-secret lookup key; the secret material is the
/// caller-supplied plaintext, verified against `pat_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatRow {
    /// Tenant UUID (text) that owns the PAT.
    pub tenant_id: String,
    /// Argon2id PHC hash string from `pat.pat_hash`.
    pub pat_hash: String,
    /// The PAT's D1 `scope` string (e.g. `cas:rw`, `admin`, or `""` for
    /// legacy rows). Empty / absent ⇒ fail-CLOSED at the scope gate.
    pub scope: String,
    /// D1 `pat.find_only` (migration 0093, ADR-0071): `true` = a FIND-ONLY
    /// least-privilege PAT whose base `scope` is the CHECK-safe `read-only`, but
    /// which grants ONLY cache find-missing. The adapter planes (npm/pip/brew/
    /// cargo/OCI) have NO find-missing operation, and — unlike the native cache
    /// planes — the OCI adapter authorizes from THIS D1 `scope` directly (not the
    /// Worker's `x-corelink-scope` header), so a find-only PAT MUST be rejected
    /// here (else its `read-only` base would grant e.g. `docker pull`). Legacy /
    /// non-find rows are `false`.
    pub find_only: bool,
}

/// Fetch a `pat` row by its non-secret `token_id`, already expiry-filtered.
///
/// Abstracted as a trait so the security-critical verification pipeline in
/// [`PatVerifier`] can be unit-tested hermetically with real crypto and a
/// fake row source — the production impl talks to D1 over HTTP, which a
/// unit test cannot reach.
#[async_trait]
pub trait PatRowLookup: Send + Sync {
    /// Return the row for `token_id`, or `None` when no live (non-expired,
    /// non-revoked) row exists. `Err` is reserved for backend faults.
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String>;
}

/// The D1 `pat` lookup SQL for the container verifier.
///
/// Mirrors the Worker hot-path (migration `0054_pat_token_id`): an `O(1)`
/// covering-index lookup on `token_id`, with the same SQL-side expiry
/// filter (`expires_ms = 0` ⇒ no-expiry token) and the same soft-revocation
/// filter (migration `0063_pat_customer_keys`: `revoked_at_ms IS NULL` ⇒
/// active; a revoked row is indistinguishable from an absent one).
const PAT_LOOKUP_SQL: &str = "SELECT tenant_id, pat_hash, scope, find_only FROM pat \
     WHERE token_id = ?1 \
       AND (expires_ms = 0 OR expires_ms > unixepoch('now', 'subsec') * 1000) \
       AND revoked_at_ms IS NULL \
     LIMIT 1";

/// Production [`PatRowLookup`] over the CF D1 HTTP API.
#[async_trait]
impl PatRowLookup for D1HttpClient {
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        let rows = self
            .query(
                PAT_LOOKUP_SQL,
                &[serde_json::Value::String(token_id.to_owned())],
            )
            .await?;

        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };

        let tenant_id = row
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .ok_or("D1 pat: missing `tenant_id` column")?
            .to_owned();
        let pat_hash = row
            .get("pat_hash")
            .and_then(|v| v.as_str())
            .ok_or("D1 pat: missing `pat_hash` column")?
            .to_owned();
        // `scope` may be NULL on legacy rows; map that to "" (fail-CLOSED
        // at the scope gate) rather than a backend error.
        let scope = row
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        // `find_only` (0093): NULL/0 = normal PAT; 1 = find-missing-only. Absent on
        // a legacy row ⇒ false (a normal PAT). The D1 HTTP API returns integers as
        // JSON numbers.
        let find_only = row.get("find_only").and_then(serde_json::Value::as_i64) == Some(1);

        Ok(Some(PatRow {
            tenant_id,
            pat_hash,
            scope,
            find_only,
        }))
    }
}

/// The shared, cloneable in-flight lookup future used by [`SingleFlightPatLookup`].
/// `Arc<..>` so every joiner clones one heap result; `Shared` so one poll drives
/// the single inner D1 read for all joiners.
type SharedLookup = Shared<BoxFuture<'static, Arc<Result<Option<PatRow>, String>>>>;

/// A **no-cache single-flight** wrapper over an inner [`PatRowLookup`].
///
/// The cold-hydrate PAT read-herd (2026-07-08: ~57 D1 `pat` reads during a
/// 668 MB pull) is a burst of the SAME runner PAT arriving in parallel — every
/// op re-reads the same `token_id` row. This coalesces a burst of concurrent
/// lookups for one `token_id` into ONE inner D1 read: the first caller leads the
/// read, the rest join its [`Shared`] future.
///
/// # It is NOT a cache — revocation stays immediate
///
/// The shared flight is dropped the instant it resolves, and a late joiner that
/// finds an already-RESOLVED flight (`peek().is_some()`) REFUSES to reuse it and
/// leads a fresh read instead. So there is no window where a resolved result is
/// served to a request that started after it: every returned row is FRESH, and
/// the SQL-side `revoked_at_ms IS NULL` / expiry filters run on every real read
/// — `INV-PAT-REVOKE-PROPAGATION` is preserved (a revoked token surfaces as
/// `None` on the very next non-in-flight lookup). Coalescing is keyed on the
/// **non-secret** `token_id`; the per-request Argon2id verify in
/// [`PatVerifier::verify_capability`] is UNTOUCHED (it still runs once per
/// request and is the sole possession check), so this changes no auth decision,
/// adds no timing oracle (the D1 hop is dwarfed by Argon2id), and leaves the
/// dummy-burn / OOM-permit machinery exactly as is.
pub struct SingleFlightPatLookup {
    inner: Arc<dyn PatRowLookup>,
    /// `token_id -> in-flight shared read`. Sync mutex; the guard is NEVER held
    /// across an `.await` (we clone the `Shared` out, then drop the guard).
    inflight: Mutex<HashMap<String, SharedLookup>>,
}

impl std::fmt::Debug for SingleFlightPatLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleFlightPatLookup")
            .field("inner", &"Arc<dyn PatRowLookup>")
            .finish()
    }
}

impl SingleFlightPatLookup {
    /// Wrap an inner lookup with per-`token_id` single-flight coalescing.
    #[must_use]
    pub fn new(inner: Arc<dyn PatRowLookup>) -> Self {
        Self {
            inner,
            inflight: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PatRowLookup for SingleFlightPatLookup {
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        // Join an UNRESOLVED in-flight read for this token_id, or lead a fresh
        // one. A poisoned lock falls back to a direct read (fail-safe: correct,
        // just uncoalesced).
        let (shared, is_leader) = {
            let Ok(mut map) = self.inflight.lock() else {
                return self.inner.lookup(token_id).await;
            };
            match map.get(token_id) {
                // Only JOIN a flight that has NOT resolved — never reuse a
                // completed read (freshness / revocation immediacy).
                Some(existing) if existing.peek().is_none() => (existing.clone(), false),
                _ => {
                    let inner = Arc::clone(&self.inner);
                    let tid = token_id.to_owned();
                    let fut: SharedLookup = async move { Arc::new(inner.lookup(&tid).await) }
                        .boxed()
                        .shared();
                    // `insert` REPLACES any resolved-stale entry for this key.
                    let _ = map.insert(token_id.to_owned(), fut.clone());
                    (fut, true)
                }
            }
        };

        let result = shared.await;

        // The leader evicts the now-resolved flight so the NEXT lookup is fresh.
        // Guard: remove ONLY if the current entry is resolved (a fresh leader may
        // have already replaced it with a new unresolved flight — leave that).
        if is_leader {
            if let Ok(mut map) = self.inflight.lock() {
                if let Some(cur) = map.get(token_id) {
                    if cur.peek().is_some() {
                        let _ = map.remove(token_id);
                    }
                }
            }
        }

        (*result).clone()
    }
}

/// Process-wide cap on the number of Argon2id verifications running
/// concurrently (red-team finding #2, HIGH). Argon2id is deliberately
/// memory-hard: each verify allocates ~`m_cost` MiB (64 MiB at the
/// production cost). With NO cap, a flood of concurrent `GET /token`
/// requests bearing a valid PAT fans out unbounded 64-MiB allocations on
/// `spawn_blocking` threads and OOM-kills the shared container — a registry
/// outage for ALL tenants. Sizing: floor(usable_RAM_MiB / (m_cost_MiB *
/// safety)); on a standard-1 instance (~4 GiB) with a 64-MiB `m_cost` and a
/// ~4× safety headroom for the runtime + other allocations ⇒ ~16-24. We
/// pick 16 as the conservative floor.
const ARGON2_VERIFY_PERMITS: usize = 16;

/// Per-tenant sub-cap on concurrent Argon2id verifications (red-team finding
/// #1, HIGH — per-tenant fairness). The global [`ARGON2_VERIFY_PERMITS`] bound
/// alone is NOT fair: a single tenant flooding distinct PATs (each forcing a
/// fresh Argon2id verify) could take ALL of the global permits and 503 the
/// native plane for EVERY OTHER tenant. We add a per-tenant semaphore so no
/// single tenant can hold more than this many global permits at once — the
/// remaining global capacity stays available to serve other tenants.
///
/// Sizing: `max(2, ARGON2_VERIFY_PERMITS / 4)` = 4 at the production global cap
/// of 16. Rationale:
/// - A LEGITIMATE tenant's adapter (cargo/npm/oci/…) almost never needs more
///   than a couple of *simultaneous* in-flight Argon2id verifies — each request
///   verifies once, briefly, then the result is reused; 4 leaves comfortable
///   headroom for genuine bursts without starving the tenant itself.
/// - It caps any ONE tenant at 1/4 of the global pool, so at least 3/4 of the
///   capacity (≥12 permits) is always reachable by other tenants even under a
///   single-tenant flood — fairness without sacrificing the OOM/CPU guard.
/// - The floor of 2 keeps the sub-cap meaningful (never 0/1) if the global cap
///   is ever tuned down (e.g. a test override), so a tenant can always make
///   forward progress.
const ARGON2_PER_TENANT_PERMITS: usize = {
    let quarter = ARGON2_VERIFY_PERMITS / 4;
    if quarter > 2 {
        quarter
    } else {
        2
    }
};

/// Upper bound on the number of live per-tenant Argon2id semaphores the
/// verifier retains (#1/#12 follow-up: bound the per-tenant map growth). The
/// `per_tenant_permits` map lazily creates one `Arc<Semaphore>` per distinct
/// real `tenant_id`; with NO bound it accumulates one entry forever — a slow
/// unbounded-memory creep over a long-running container. We cap it as an LRU:
/// when the map is full and a NEW tenant must be inserted, we evict the
/// least-recently-used entry **that is fully idle** (see
/// [`PatVerifier::per_tenant_semaphore`]).
///
/// 10k is deliberately generous: each entry is tiny (a `String` key + an
/// `Arc<Semaphore>`, well under ~100 B), so the whole map is ~MB-scale even
/// full — far below the Argon2id `m_cost` pressure the permits themselves
/// guard. A real SMB fleet has far fewer than 10k *concurrently-active*
/// tenants, so steady-state eviction is rare; the cap exists to defeat a
/// long-tail / adversarial churn of distinct tenant_ids, not to throttle
/// legitimate multi-tenancy. A re-inserted evicted tenant simply recreates an
/// (idle) semaphore — semantically identical to never having evicted it.
const PER_TENANT_MAP_CAP: usize = 10_000;

/// Synthetic per-tenant bucket key for the None-row timing-parity dummy burn
/// (finding #12, defence-in-depth). A valid-HMAC token for a NON-existent /
/// expired / revoked token_id has no real owning tenant, yet the dummy Argon2id
/// burn (run for timing parity) still consumes a GLOBAL permit. A leaked-key
/// attacker could spread a flood across many bogus token_ids to drain the
/// global pool through this path. Routing every dummy burn through ONE shared
/// synthetic bucket caps the *total* concurrent dummy burns at the per-tenant
/// sub-cap. The key is not a valid tenant UUID, so it cannot collide with a
/// real tenant bucket.
///
/// ⚠️ **The bucket alone caps BURNS, not global-pool OCCUPANCY — the acquire
/// ORDER is what makes it cap the pool.** As originally written this arm took
/// the global permit first and then waited up to [`ARGON2_PERMIT_WAIT`] on this
/// bucket, so a request already destined to shed still pinned a global permit
/// for the entire wait. Concurrent *burns* were capped at the sub-cap exactly as
/// claimed, while the number of global permits held by this arm was bounded only
/// by the flood's arrival rate — the drain the bucket was introduced to close,
/// still open one step to the left. The arm therefore takes this bucket FIRST
/// and NON-BLOCKINGLY ([`PerTenantGate::try_acquire`]): a shed holds no global
/// permit at any instant, so the arm's global footprint really is the sub-cap.
/// Do not restore the global-first order here (see the arm in
/// [`PatVerifier::verify_capability`]).
const UNKNOWN_TOKEN_BUCKET: &str = "\0argon2-dummy-burn-bucket\0";

/// How long a verify will wait for an Argon2id permit before declaring the
/// verifier overloaded. Short by design: a `/token` caller waiting longer
/// than this is better served a fast fail-CLOSED than a stalled request that
/// holds an async task (and its connection) hostage under a DoS flood.
const ARGON2_PERMIT_WAIT: Duration = Duration::from_millis(250);

/// One entry in the bounded per-tenant Argon2id semaphore LRU
/// (`PatVerifier::per_tenant_permits`): the tenant's `Arc<Semaphore>` plus the
/// monotonic tick at which it was last touched (created or fetched). The tick
/// orders the LRU; the smallest tick among the **idle** entries is the eviction
/// candidate. Cloning yields a fresh `Arc` clone of the same semaphore.
#[derive(Clone)]
struct PerTenantEntry {
    sem: Arc<Semaphore>,
    last_access: u64,
}

/// The per-tenant Argon2id fairness tier (finding #1), extracted from
/// [`PatVerifier`] so it can be held behind one `Arc` and moved into the
/// `'static` coalescing flight future ([`FlightGroup`]) — the flight runs the
/// permit acquires itself, so it needs the gate, not a borrow of the verifier.
///
/// Owns the bounded LRU of `tenant_id -> Arc<Semaphore>` plus its recency clock.
struct PerTenantGate {
    /// `tenant_id -> entry`. Wrapped in a `Mutex` only to guard the map's
    /// get-or-insert; the guard is NEVER held across the Argon2id work or the
    /// async acquire (it is dropped before either). FAIL-SAFE: a poisoned lock
    /// falls back to global-only bounding (a bookkeeping fault must never block
    /// a legitimate auth) — see [`Self::semaphore`].
    permits: Mutex<HashMap<String, PerTenantEntry>>,
    /// Monotonic clock for the LRU recency order. Bumped on every
    /// get-or-insert; the smallest tick is the least-recently-used. `u64` never
    /// realistically wraps (one tick per verify ⇒ ~10^11 years at 1M/s).
    tick: AtomicU64,
    /// The cap each per-tenant semaphore is created with
    /// ([`ARGON2_PER_TENANT_PERMITS`] in production). A field (not a const) so a
    /// test can shrink it to drive the two-tier interaction deterministically.
    cap: usize,
    /// LRU capacity of the map ([`PER_TENANT_MAP_CAP`] in production). A field
    /// (not a const) so a test can shrink it to drive eviction deterministically.
    map_cap: usize,
}

impl PerTenantGate {
    fn new(cap: usize, map_cap: usize) -> Self {
        Self {
            permits: Mutex::new(HashMap::new()),
            tick: AtomicU64::new(0),
            cap,
            map_cap,
        }
    }

    /// Resolve (get-or-create) the per-tenant Argon2id semaphore for `tenant`.
    ///
    /// Returns `Some(sem)` with a clone of the tenant's `Arc<Semaphore>`, or
    /// `None` to signal FAIL-SAFE fall-through to global-only bounding. We
    /// return `None` (rather than propagating an error) on a poisoned lock so a
    /// bookkeeping fault can NEVER block a legitimate auth — the global bound
    /// still protects against OOM; we merely lose per-tenant fairness for the
    /// rare poisoned-lock window.
    ///
    /// The `Mutex` is held ONLY for the brief get-or-insert (+ any LRU
    /// eviction) and is dropped before the caller awaits the (async) permit
    /// acquire or runs Argon2id — so it never serializes the verify path nor
    /// risks a lock-across-await.
    ///
    /// BOUNDED LRU (#1/#12 follow-up): the map is capped at [`Self::map_cap`]. A
    /// get bumps the entry's recency tick. An insert that would exceed the cap
    /// first evicts the least-recently-used entry **that is fully idle** —
    /// `available_permits() == cap`, i.e. no in-flight verify holds any of its
    /// permits — so eviction can never disrupt an active or contended tenant.
    /// The shared [`UNKNOWN_TOKEN_BUCKET`] is NEVER an eviction candidate. If no
    /// idle entry can be freed (every entry is in-flight — pathological, far
    /// beyond a 10k real-tenant working set), we skip eviction and let the map
    /// grow past the cap transiently rather than block/evict an active tenant;
    /// it shrinks back as those verifies complete and a later insert finds an
    /// idle victim. Re-inserting an evicted tenant simply recreates its (idle)
    /// semaphore — semantically identical.
    fn semaphore(&self, tenant: &str) -> Option<Arc<Semaphore>> {
        let mut map = match self.permits.lock() {
            Ok(guard) => guard,
            // Poisoned: a previous holder panicked while mutating the map. Do
            // NOT block auth on bookkeeping — fall back to global-only.
            Err(_poisoned) => return None,
        };
        let tick = self.tick.fetch_add(1, Ordering::Relaxed);
        if let Some(entry) = map.get_mut(tenant) {
            entry.last_access = tick; // touch ⇒ most-recently-used
            return Some(Arc::clone(&entry.sem));
        }
        // New tenant. Enforce the LRU cap BEFORE inserting: if at/over capacity,
        // evict the least-recently-used FULLY-IDLE entry (never the shared
        // UNKNOWN_TOKEN_BUCKET, never an in-flight tenant).
        if map.len() >= self.map_cap {
            self.evict_one_idle(&mut map);
        }
        let sem = Arc::new(Semaphore::new(self.cap));
        map.insert(
            tenant.to_owned(),
            PerTenantEntry {
                sem: Arc::clone(&sem),
                last_access: tick,
            },
        );
        Some(sem)
    }

    /// Evict the single least-recently-used entry that is SAFE to drop: fully
    /// idle (`available_permits() == cap` ⇒ no in-flight verify holds a permit)
    /// and NOT the shared [`UNKNOWN_TOKEN_BUCKET`]. If no such entry exists,
    /// evict nothing (the map grows transiently past the cap rather than disrupt
    /// an active tenant). Caller holds the map lock.
    fn evict_one_idle(&self, map: &mut HashMap<String, PerTenantEntry>) {
        let mut victim: Option<(&str, u64)> = None;
        for (key, entry) in map.iter() {
            // SAFETY INVARIANTS for eviction:
            //  - never the synthetic dummy-burn bucket (must always exist), and
            //  - only a fully-idle semaphore (no permit checked out) so an
            //    active/contended tenant is never disrupted.
            if key == UNKNOWN_TOKEN_BUCKET {
                continue;
            }
            if entry.sem.available_permits() != self.cap {
                continue; // in-flight verify(s) for this tenant — do not evict
            }
            match victim {
                Some((_, best_tick)) if entry.last_access >= best_tick => {}
                _ => victim = Some((key.as_str(), entry.last_access)),
            }
        }
        if let Some((key, _)) = victim {
            let key = key.to_owned();
            map.remove(&key);
        }
    }

    /// Acquire the per-tenant Argon2id permit for `tenant` under the same
    /// bounded wait as the global permit, returning a held
    /// `Option<OwnedSemaphorePermit>`:
    ///
    /// - `Ok(Some(permit))` — tenant permit held (the common case).
    /// - `Ok(None)` — FAIL-SAFE: per-tenant bookkeeping was unavailable
    ///   (poisoned lock), so we proceed under the GLOBAL bound only. A legit
    ///   auth is never blocked by a bookkeeping fault.
    /// - `Err(())` — the tenant is at its sub-cap and did not free a permit
    ///   within the wait. The caller maps this to the SAME overloaded
    ///   fail-CLOSED as a global-permit timeout, so one tenant's flood cannot
    ///   monopolise the global pool.
    ///
    /// MUST be called AFTER the global permit is held (consistent acquire order
    /// global→per-tenant ⇒ deadlock-free). The one caller that inverts the order
    /// — the dummy-burn arm — uses the non-blocking [`Self::try_acquire`]
    /// instead, which cannot participate in a wait-for cycle at all and so
    /// preserves that rationale rather than breaking it.
    async fn acquire(&self, tenant: &str) -> Result<Option<OwnedSemaphorePermit>, ()> {
        let Some(sem) = self.semaphore(tenant) else {
            // Fail-safe: no per-tenant bookkeeping ⇒ global-only.
            return Ok(None);
        };
        match tokio::time::timeout(ARGON2_PERMIT_WAIT, sem.acquire_owned()).await {
            Ok(Ok(permit)) => Ok(Some(permit)),
            // Semaphore closed (never happens in practice — we never close it)
            // OR the per-tenant sub-cap was saturated for the whole wait. Both
            // collapse to a fail-CLOSED overloaded signal.
            Ok(Err(_)) | Err(_) => Err(()),
        }
    }

    /// NON-BLOCKING sibling of [`Self::acquire`], for a caller that must take a
    /// per-tenant permit BEFORE the global one. Same three outcomes with the
    /// same meanings — `Ok(Some)` held, `Ok(None)` fail-safe global-only,
    /// `Err(())` at the sub-cap ⇒ the caller's usual fail-CLOSED overloaded
    /// signal — except that a full bucket is reported IMMEDIATELY instead of
    /// after [`ARGON2_PERMIT_WAIT`].
    ///
    /// Why a `try` and not the bounded wait: this is the only acquire that runs
    /// with NO global permit held, so it inverts the module's
    /// global→per-tenant order. A blocking wait in that position would be a
    /// genuine lock-order inversion; a non-blocking one cannot be, because it
    /// never enters a wait-for relation — it either succeeds outright or fails
    /// outright, so no cycle can form and the deadlock-freedom rationale on
    /// [`Self::acquire`] is preserved unchanged.
    ///
    /// See the dummy-burn arm of [`PatVerifier::verify_capability`] for why the
    /// inversion is worth having: shedding a request that was going to be shed
    /// anyway must not first park a global permit for the whole wait.
    fn try_acquire(&self, tenant: &str) -> Result<Option<OwnedSemaphorePermit>, ()> {
        let Some(sem) = self.semaphore(tenant) else {
            // Fail-safe: no per-tenant bookkeeping ⇒ global-only. Identical to
            // `acquire`'s fall-through — a bookkeeping fault never blocks auth.
            return Ok(None);
        };
        // Saturated bucket OR closed semaphore (never happens — we never close
        // it) collapse to the same fail-CLOSED overloaded signal `acquire` uses.
        sem.try_acquire_owned().map(Some).map_err(|_| ())
    }
}

/// TTL of a memoised Argon2id secret-match ([`SecretMatchMemo`]).
///
/// ⚠️ **This is NOT a revocation window and must not be read as one.** The
/// repo's other PAT caches (`native_pat_gate::VERIFY_CACHE_TTL`, the Worker's
/// `PAT_VERIFY_CACHE_TTL_MS`) memoise the *authorization decision* — they skip
/// the D1 row on a hit, so their TTL literally bounds how long a revoked PAT
/// keeps access, and both are pinned at 5 s against `SLO-FRESH-PAT-REVOKE`.
/// This memo caches something categorically different: the single boolean
/// "Argon2id(`plaintext`) matches this exact stored PHC hash". That is a **pure
/// function of the memo key itself** — it can never become false, and it
/// carries no tenant, no scope, no expiry, and no revocation state. Every one
/// of those still comes from the per-request, uncached D1 row read
/// (`PAT_LOOKUP_SQL`, which filters `revoked_at_ms IS NULL` + expiry in SQL),
/// so **revocation stays immediate on the adapter plane** — strictly stronger
/// than the ≤5 s window the native plane accepts.
///
/// The TTL therefore exists only as a memory-hygiene bound (bounded staleness
/// for a token that stops being used), not as a security control. 300 s keeps a
/// long build's thousands of requests on ONE Argon2id while still letting an
/// idle fleet's entries age out well inside a container's lifetime.
const SECRET_MATCH_MEMO_TTL: Duration = Duration::from_secs(300);

/// Upper bound on live [`SecretMatchMemo`] entries. Each entry is a 64-char hex
/// fingerprint + an `Instant` (~100 B), so 32 768 is ~3 MiB — still orders of
/// magnitude below the 64-MiB-per-verify Argon2id allocations this memo exists
/// to avoid, on a 4 GiB container.
///
/// ⚠️ Sizing must assume a FLEET-WIDE live set, not one tenant's. The
/// tenant-scoped adapter routes (`/cargo`, `/npm`, `/pip`, `/brew`) do route one
/// tenant to one Durable Object and therefore one container
/// (`worker/src/index.ts` `idFromName(tenant_id)`), but the OCI `/token`
/// exchange and the public/anonymous surfaces pin ALL tenants onto the SHARED
/// `_oci` / `_anonymous` DOs — the same reason `ARGON2_VERIFY_PERMITS` above
/// talks about OOM-killing "the shared container … a registry outage for ALL
/// tenants". A cap sized for one tenant would put that shared container into
/// at-capacity eviction on a working set it should comfortably hold.
///
/// Be honest about what this buys: raising the cap DEFERS saturation, it does
/// not remove it. Once a shared DO's live set does reach the cap, every
/// cold-miss insert takes the at-capacity branch and pays an O(cap) purge while
/// holding the same `std::sync::Mutex` that each warm `contains()` needs — and a
/// bigger cap makes that pass longer, not shorter. It is acceptable rather than
/// ideal because the insert RATE is bounded by [`ARGON2_VERIFY_PERMITS`]: a cold
/// miss cannot happen without an Argon2id, so inserts arrive at most ~50/s even
/// on a saturated box, which is a small duty cycle for a microsecond-scale pass
/// over an in-memory map. If a shared surface is ever observed to sit at
/// capacity in steady state, the right fix is an O(1)-amortised eviction (or
/// moving the purge off the request path), NOT another cap bump.
const SECRET_MATCH_MEMO_CAP: usize = 32_768;

/// One memoised secret-match: the tick at which it was recorded. The value is
/// the *presence* of the key — there is deliberately nothing else to store (see
/// [`SECRET_MATCH_MEMO_TTL`] on why no authorization state may live here).
#[derive(Debug, Clone, Copy)]
struct SecretMatchEntry {
    verified_at: Instant,
}

/// A bounded, TTL'd set of proven Argon2id secret-matches.
///
/// # What is memoised
///
/// Exactly one fact: *"the presented plaintext's secret segment Argon2id-verifies
/// against this exact stored PHC hash"*. The key is a domain-separated,
/// length-prefixed SHA-256 over
/// `(plaintext, token_id, stored_pat_hash, scope, find_only)` — never the
/// plaintext itself, so the map cannot leak a usable secret (same posture as
/// `native_pat_gate::fingerprint`). See [`secret_match_fingerprint`] for why
/// `scope` / `find_only` are in the key even though the memoised fact does not
/// depend on them.
///
/// # Why memoising it is sound
///
/// - **Immutable fact.** Argon2id is deterministic, so the memoised boolean is a
///   pure function of the key. It cannot go stale in the direction that matters
///   (a `true` cannot silently become `false`).
/// - **Re-hash invalidates automatically.** `stored_pat_hash` is *in* the key, so
///   if the row's `pat_hash` ever changes the old entry is simply unreachable.
/// - **Key-rotation is still enforced per request.** The HMAC fast-reject (stage 1)
///   runs against the *current* signing-key overlap set on EVERY request, before
///   this memo is consulted — dropping a rotated-out key still rejects immediately.
/// - **No authorization state.** tenant / scope / `find_only` / expiry /
///   revocation all come from the fresh per-request D1 row. See
///   [`SECRET_MATCH_MEMO_TTL`].
///
/// # Why it adds no timing oracle
///
/// The pair that must stay indistinguishable is the two ways to earn a **401**:
/// (a) a valid-HMAC token for an unknown/expired/revoked `token_id` → the bounded
/// dummy burn, and (b) a known `token_id` presented with the WRONG secret → a real
/// Argon2id. A wrong secret changes the plaintext, hence the key, so case (b) can
/// **never** hit the memo — it always pays full Argon2id, exactly as before. A hit
/// is only reachable for a plaintext that already verified, which is a request that
/// returns 200 anyway. Parity between the two 401 paths is preserved.
struct SecretMatchMemo {
    /// `fingerprint -> entry`. Sync mutex; the guard is NEVER held across an
    /// `.await` (every method here is synchronous and returns before the caller
    /// awaits anything).
    entries: Mutex<HashMap<String, SecretMatchEntry>>,
    /// Entry cap ([`SECRET_MATCH_MEMO_CAP`] in production). A field, not a const,
    /// so a test can shrink it to drive the eviction path deterministically.
    cap: usize,
    /// Entry TTL ([`SECRET_MATCH_MEMO_TTL`] in production). A field, not a const,
    /// so a test can shrink it to drive expiry deterministically.
    ttl: Duration,
}

impl SecretMatchMemo {
    fn new(cap: usize, ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            cap,
            ttl,
        }
    }

    /// `true` when `fp` has a non-expired proven match.
    ///
    /// FAIL-SAFE: a poisoned lock reports `false`, so the caller runs the full
    /// Argon2id. A bookkeeping fault must only ever cost performance, never
    /// admit an unverified secret — and never reject a legitimate one.
    fn contains(&self, fp: &str) -> bool {
        let Ok(mut entries) = self.entries.lock() else {
            return false;
        };
        match entries.get(fp) {
            Some(entry) if entry.verified_at.elapsed() < self.ttl => true,
            Some(_) => {
                // Expired — evict on read so a recurring token cannot accumulate
                // stale entries between eviction sweeps.
                let _ = entries.remove(fp);
                false
            }
            None => false,
        }
    }

    /// Record a proven match for `fp`, bounding the map at [`Self::cap`].
    ///
    /// On a full map a single pass drops every expired entry and remembers the
    /// oldest survivor; if that pass freed nothing, the oldest survivor is
    /// evicted so the insert still lands. O(n) only on the insert that finds the
    /// map full — never on a hit, and never on an insert with headroom.
    ///
    /// FAIL-SAFE: a poisoned lock silently skips the insert (the next request
    /// simply re-runs Argon2id).
    fn insert(&self, fp: String) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= self.cap && !entries.contains_key(&fp) {
            let now = Instant::now();
            let ttl = self.ttl;
            let mut oldest: Option<(String, Instant)> = None;
            entries.retain(|key, entry| {
                let live = now.duration_since(entry.verified_at) < ttl;
                // `Option::is_none_or` would read better but is stable only
                // since 1.82; this crate's MSRV is 1.80 (clippy::incompatible_msrv).
                let is_oldest_so_far = match oldest.as_ref() {
                    Some((_, at)) => entry.verified_at < *at,
                    None => true,
                };
                if live && is_oldest_so_far {
                    oldest = Some((key.clone(), entry.verified_at));
                }
                live
            });
            // Purging expired entries freed nothing ⇒ evict the oldest survivor
            // so the map stays bounded and the new entry still lands.
            if entries.len() >= self.cap {
                if let Some((key, _)) = oldest {
                    let _ = entries.remove(&key);
                }
            }
        }
        let _ = entries.insert(
            fp,
            SecretMatchEntry {
                verified_at: Instant::now(),
            },
        );
    }

    /// Test-only: live entry count, for asserting the cap actually bounds.
    #[cfg(test)]
    fn len_for_test(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }
}

/// Hand-written so the fingerprint set can never reach a log. A derived `Debug`
/// would print every live entry, and the set of fingerprints currently held IS
/// a live-PAT-presence oracle for anyone with log access — the same reason
/// `PatVerifier`'s own `Debug` redacts `signing_keys`. Only the count escapes.
impl std::fmt::Debug for SecretMatchMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretMatchMemo")
            .field(
                "entries",
                &format_args!("[{} REDACTED]", self.entries.lock().map_or(0, |e| e.len())),
            )
            .field("cap", &self.cap)
            .field("ttl", &self.ttl)
            .finish()
    }
}

/// The [`SecretMatchMemo`] key: a domain-separated, length-prefixed SHA-256 over
/// every field of the row that the decision depends on —
/// `(plaintext, token_id, stored_pat_hash, scope, find_only)`.
///
/// Length-prefixing every field makes the pre-image unambiguous (no
/// concatenation-boundary confusion between two different tuples), and the
/// domain tag keeps this fingerprint from ever colliding with another
/// SHA-256-of-a-token use in the codebase (e.g. `native_pat_gate::fingerprint`).
/// Only the digest is retained, so the memo never holds recoverable secret
/// material.
///
/// `scope` and `find_only` are in the key even though the gate re-reads them
/// fresh every request and the memoised fact does not depend on them. They are
/// here to close the last latency tail: without them, a PAT that verified
/// successfully and was then *downgraded* in D1 (rather than revoked) would keep
/// hitting the memo for the rest of the TTL and 401 in ~0 ms, while a *revoked*
/// PAT still pays the slow dummy burn — distinguishing "downgraded" from
/// "revoked" on latency alone. With them in the key, any change to either field
/// makes the old entry unreachable, exactly as a `pat_hash` change already does,
/// and the two rejections stay uniform.
fn secret_match_fingerprint(
    plaintext: &str,
    token_id: &str,
    pat_hash: &str,
    scope: &str,
    find_only: bool,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"corelink/adapter-pat/secret-match/v2\0");
    let find_only = if find_only { "1" } else { "0" };
    for part in [plaintext, token_id, pat_hash, scope, find_only] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// The [`FlightGroup`] key for the None-row dummy burn: a domain-separated
/// SHA-256 over the plaintext alone.
///
/// # Why the plaintext alone, and why that is the SAME key the hot path uses
///
/// The two arms must coalesce **identically for the same plaintext whether or
/// not the D1 row exists** — otherwise `token_id` liveness becomes observable in
/// the concurrency dimension (the finding that killed the first attempt at this
/// fix). The hot path keys on [`secret_match_fingerprint`], i.e. on
/// `(plaintext, token_id, stored pat_hash, scope, find_only)`. Every extra field
/// is, at any instant, a **pure function of the plaintext**: `token_id` is parsed
/// out of the plaintext itself (stage 1), and `pat_hash` / `scope` / `find_only`
/// are whatever the single D1 row for that `token_id` holds. So both arms key on
/// a pure function of the plaintext,
/// and N concurrent copies of ONE plaintext collapse to exactly ONE Argon2id on
/// either arm. The keys need not be *equal* across the arms — they must only
/// *partition identically*, and they do. They are deliberately in SEPARATE maps
/// with separate domain tags so a flight can never hand a result from one arm to
/// a waiter on the other (a token revoked mid-burst must not join the hot-path
/// flight that started before the revocation).
fn dummy_burn_fingerprint(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"corelink/adapter-pat/dummy-burn/v1\0");
    hasher.update((plaintext.len() as u64).to_be_bytes());
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

/// One in-flight, cloneable unit of coalesced work. `Arc<..>` so every joiner
/// clones one heap result; `Shared` so one poll drives the single inner run for
/// all joiners.
type SharedFlight<T> = Shared<BoxFuture<'static, Arc<T>>>;

/// Upper bound on retained [`FlightGroup`] entries before an insert purges the
/// resolved ones.
///
/// A flight is normally removed by the first awaiter to observe it resolved, so
/// the steady-state map holds only genuinely in-flight work — a handful of
/// entries, bounded in practice by [`ARGON2_VERIFY_PERMITS`] plus whatever is
/// inside its 250 ms admission wait. The cap exists for the one case that leaks:
/// if EVERY awaiter of a flight is dropped (client disconnect cancels the
/// request future) after the flight resolved but before anyone removed it, the
/// resolved entry lingers until some later request for the same key replaces it
/// — which may never come. 8192 × (a 64-char hex key + a resolved `Shared`) is
/// ~2 MiB worst case, and the purge that bounds it drops only RESOLVED entries,
/// which are pure garbage (a joiner already refuses to reuse them).
const FLIGHT_GROUP_CAP: usize = 8_192;

/// A **no-cache single-flight** over a keyed unit of async work — the same shape
/// [`SingleFlightPatLookup`] uses for the D1 row read, generalised so the
/// Argon2id stage can use it too.
///
/// The first caller for a key LEADS: it builds the future and drives it. Every
/// concurrent caller for that key JOINS the leader's [`Shared`] future and
/// resolves the instant the ONE run completes. There is deliberately **no queue
/// and no lock to wait behind**: the N-th joiner waits exactly as long as the
/// 1st, not N× as long. That is the property that distinguishes this from a
/// mutex — a mutex converts a burst into an N-deep FIFO whose tail waits
/// unboundedly, in front of the very load-shed that exists to bound it.
///
/// # It is NOT a cache
///
/// The flight is removed as soon as it resolves, and an awaiter that finds an
/// already-RESOLVED flight REFUSES to reuse it and leads a fresh run instead. So
/// a result is only ever shared with callers that were **concurrent with the run
/// that produced it** — never with one that started afterwards. Anything that
/// must be re-decided per request (here: the D1 row, hence revocation, expiry,
/// tenant and scope) is read OUTSIDE the flight and is unaffected.
struct FlightGroup<T> {
    /// `key -> in-flight shared run`. Sync mutex; the guard is NEVER held across
    /// an `.await` (we clone the `Shared` out, then drop the guard).
    inflight: Mutex<HashMap<String, SharedFlight<T>>>,
    /// Entry cap ([`FLIGHT_GROUP_CAP`] in production). A field, not a const, so a
    /// test can shrink it to drive the purge path deterministically.
    cap: usize,
}

impl<T: Send + Sync + 'static> FlightGroup<T> {
    fn new(cap: usize) -> Self {
        Self {
            inflight: Mutex::new(HashMap::new()),
            cap,
        }
    }

    /// Join the UNRESOLVED flight for `key`, or lead a fresh one built by
    /// `lead`. `lead` is invoked ONLY when leading, so a joiner never builds
    /// (or runs) the work. `on_abort` supplies the outcome if the work's task
    /// dies (panic/abort) — pass the type's fail-CLOSED variant.
    ///
    /// # The work is SPAWNED, and that is load-bearing
    ///
    /// A `Shared` future is driven only by whoever polls it, so if every awaiter
    /// goes away — one client disconnecting is enough, since an unbursted
    /// request has exactly one awaiter — nothing would ever poll it again. The
    /// map still holds a clone, so the inner future would not be dropped either;
    /// it would simply be frozen wherever it was parked. If that happened to be
    /// the per-tenant acquire, it would be frozen **holding a global Argon2id
    /// permit**, with its own `tokio::time::timeout` unable to fire (timers need
    /// polling) — pinning a permit until some later request for the same key
    /// happened to resume it. Uncoalesced code has no such hazard: dropping the
    /// request future drops the permit by RAII.
    ///
    /// Spawning restores that property and strengthens it. The one run is owned
    /// by the runtime, so it always completes on its own: the bounded waits fire,
    /// the permits are released, and the result is recorded, regardless of
    /// whether anybody is still listening.
    ///
    /// FAIL-SAFE: a poisoned lock runs the work inline, uncoalesced — correct,
    /// merely unshared, and cancellation-safe in the pre-coalescing way. A
    /// bookkeeping fault must only ever cost throughput.
    async fn run<F>(&self, key: &str, lead: F, on_abort: fn(String) -> T) -> Arc<T>
    where
        F: FnOnce() -> BoxFuture<'static, T>,
    {
        let shared = {
            let Ok(mut map) = self.inflight.lock() else {
                return Arc::new(lead().await);
            };
            match map.get(key) {
                // Only JOIN a flight that has NOT resolved — never reuse a
                // completed run (see "It is NOT a cache" above).
                Some(existing) if existing.peek().is_none() => existing.clone(),
                _ => {
                    // Bound the map before inserting. Only RESOLVED entries are
                    // dropped: they are unreachable to joiners anyway, so this
                    // can never disrupt in-flight work. If nothing is resolved
                    // the map grows transiently — bounded by live concurrency,
                    // itself bounded by the permits below.
                    if map.len() >= self.cap {
                        map.retain(|_, flight| flight.peek().is_none());
                    }
                    // Build AND spawn here, synchronously under the guard, so the
                    // run is owned by the runtime from the instant it is
                    // published — never contingent on a caller polling it. (Only
                    // the resulting `'static` future is moved, so `lead` itself
                    // never has to be `Send + 'static`.)
                    let handle = tokio::spawn(lead());
                    let fut: SharedFlight<T> = async move {
                        Arc::new(match handle.await {
                            Ok(outcome) => outcome,
                            Err(e) => on_abort(format!("verify flight aborted: {e}")),
                        })
                    }
                    .boxed()
                    .shared();
                    let _ = map.insert(key.to_owned(), fut.clone());
                    fut
                }
            }
        };

        let result = shared.await;

        // Retire the now-resolved flight so the NEXT call for this key is fresh.
        // EVERY awaiter attempts this (not just the leader): the leader's own
        // task may have been cancelled mid-flight, in which case a joiner is
        // what drove the work to completion and must do the cleanup.
        // Guard: remove ONLY if the entry currently under this key is resolved —
        // a fresh leader may already have replaced it with a new unresolved
        // flight, which must be left alone.
        if let Ok(mut map) = self.inflight.lock() {
            if let Some(cur) = map.get(key) {
                if cur.peek().is_some() {
                    let _ = map.remove(key);
                }
            }
        }

        result
    }

    /// Test-only: live entry count, for asserting the map does not retain
    /// resolved flights.
    #[cfg(test)]
    fn len_for_test(&self) -> usize {
        self.inflight.lock().map(|m| m.len()).unwrap_or(0)
    }
}

/// Hand-written for the same reason [`SecretMatchMemo`]'s is: the live key set
/// is a PAT-presence oracle for anyone with log access. Only the count escapes.
impl<T> std::fmt::Debug for FlightGroup<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlightGroup")
            .field(
                "inflight",
                &format_args!("[{} REDACTED]", self.inflight.lock().map_or(0, |m| m.len())),
            )
            .finish()
    }
}

/// Outcome of ONE coalesced hot-path Argon2id run, cloned to every joiner.
///
/// Carrying the outcome (rather than re-deciding per joiner) is sound because
/// every joiner shares the flight KEY — the same `(plaintext, token_id, stored
/// pat_hash, scope, find_only)` tuple — and the outcome is a pure function of
/// exactly that tuple. No joiner receives a decision its own inputs did not
/// already determine. Since the key binds `scope` and `find_only` too, joiners
/// are guaranteed to reach the SAME step-4 scope verdict as the leader; and
/// everything the key does NOT bind (tenant, expiry, revocation) is read per
/// request, outside the flight.
#[derive(Clone, Debug, PartialEq, Eq)]
enum VerifyFlight {
    /// Argon2id proved the presented secret matches the stored PHC hash.
    Proven,
    /// Argon2id ran and the secret did NOT match ⇒ uniform `InvalidPat`.
    Rejected,
    /// Fail-CLOSED: a bounded permit acquire shed, or the blocking join faulted.
    Backend(String),
}

/// Outcome of ONE coalesced dummy burn. Exactly two outcomes are observable,
/// and they mirror [`VerifyFlight`]'s so the two 401 arms cannot diverge:
/// a burn that RAN is the uniform `InvalidPat`, and a burn that was SHED is
/// `Backend("pat verifier overloaded")` — see
/// `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum BurnFlight {
    /// The dummy Argon2id ran (timing parity preserved) ⇒ uniform `InvalidPat`.
    Burned,
    /// Fail-CLOSED: a bounded permit acquire shed at EITHER tier (global pool
    /// or the shared [`UNKNOWN_TOKEN_BUCKET`] sub-cap), or the flight task
    /// faulted. Identical to [`VerifyFlight::Backend`] on the row-FOUND arm,
    /// which is what keeps the shed free of a row-existence oracle.
    Backend(String),
}

/// Container-side PAT → tenant verifier (Option B). Trait-agnostic: each
/// adapter route module wraps an `Arc<PatVerifier>` in a thin newtype
/// that impls that adapter's `TenantResolver` port.
pub struct PatVerifier {
    lookup: Arc<dyn PatRowLookup>,
    /// The HMAC overlap key set: `PAT_SIGNING_KEY` (current) plus any
    /// `PAT_SIGNING_KEY_PREV` / `PAT_SIGNING_KEY_NEW` rotation siblings.
    /// A PAT minted under any key in the set validates during the
    /// rotation overlap window (`key_management.md §3.2.1`), so rotating
    /// the current key on compromise does NOT instantly invalidate the
    /// live fleet. Always non-empty by construction (fail-closed
    /// otherwise — see [`PatVerifier::new`] / [`PatVerifier::from_env`]).
    signing_keys: Arc<Vec<PatSigningKey>>,
    /// Process-wide bound on concurrent Argon2id work (finding #2). A permit
    /// is acquired AFTER the cheap HMAC fast-reject (so forged tokens never
    /// consume one) and held ONLY across the `spawn_blocking` Argon2id call —
    /// both on the hot path and on the None-row dummy-burn path (which also
    /// runs Argon2id for timing parity). Acquire-timeout ⇒ `Backend`
    /// ("overloaded"), fail-CLOSED.
    argon2_permits: Arc<Semaphore>,
    /// Per-tenant Argon2id sub-limit (finding #1, fairness). A lazily-created
    /// `Arc<Semaphore>` per `tenant_id`, each capped at
    /// [`ARGON2_PER_TENANT_PERMITS`]. A row-FOUND verify acquires the global
    /// permit FIRST, then this tenant's permit (CONSISTENT order => no
    /// deadlock), so a single tenant flooding distinct PATs can hold at most
    /// `ARGON2_PER_TENANT_PERMITS` global permits at once -- leaving the rest of
    /// the global pool for other tenants. The dummy-burn arm INVERTS that order
    /// (shared bucket first, non-blocking) for the reason spelled out on
    /// [`UNKNOWN_TOKEN_BUCKET`]: it is the only way its sub-cap bounds
    /// global-pool occupancy and not merely concurrent burns. See
    /// [`PerTenantGate`] for the bounded LRU and the fail-safe posture; it lives
    /// behind an `Arc` so the coalescing flight future can own a handle to it.
    per_tenant: Arc<PerTenantGate>,
    /// Memoised Argon2id secret-matches — the fix for the adapter plane's
    /// throughput ceiling.
    ///
    /// Argon2id at the OWASP-2024 cost (`m=64 MiB, t=3, p=4`) costs **at least
    /// ~0.15 CPU-s** per verify — that figure is a LOWER BOUND measured with the
    /// C reference implementation on an Apple-silicon core, and the pure-Rust
    /// `argon2` crate on a fraction of an x86 vCPU is slower, not faster. The
    /// production container is provisioned at **0.5 vCPU**
    /// (`corelink-prod-corelinkserver-prod`, read back from the CF Containers
    /// API). Running it on EVERY request therefore capped a tenant's adapter
    /// plane at roughly 3 requests/second regardless of client concurrency —
    /// crippling for the small-object, high-frequency traffic a build cache is
    /// made of (sccache/cargo, npm, pip, brew, OCI all issue thousands of tiny
    /// requests per build).
    ///
    /// Evidence from the sccache CI pilot (#1017), stated at the precision it
    /// actually supports: the fully-warm run served **827 cache operations at a
    /// 100 % hit rate in 631 s against a 409-423 s cold baseline** (+208…222 s).
    /// The predicted Argon2id floor for those 827 verifies is
    /// `827 × 0.15 ÷ 0.5 ≈ 248 s`. That is the right order of magnitude and the
    /// same direction, which makes Argon2id a SUFFICIENT explanation of the
    /// regression — it is not a per-request measurement of the authenticated
    /// path, and none has been captured yet.
    ///
    /// The native plane already solved this with
    /// `native_pat_gate::NativePatGate`'s verify cache; the adapter plane never
    /// got one. This memo closes that gap WITHOUT native's ≤5 s revocation
    /// window, because it memoises only the immutable Argon2id comparison and
    /// leaves the D1 row read per-request. See [`SecretMatchMemo`].
    secret_match_memo: Arc<SecretMatchMemo>,
    /// Coalesces a burst of concurrent COLD memo misses for the SAME
    /// `(plaintext, token_id, pat_hash, scope, find_only)` onto ONE Argon2id.
    ///
    /// The memo above only helps once a first request has paid Argon2id. A cold
    /// container's first burst — a `cargo -jN` build's opening N requests, all
    /// bearing the same PAT — misses it N times simultaneously, and only
    /// [`ARGON2_PER_TENANT_PERMITS`] of them are admitted; a single Argon2id at
    /// 0.5 vCPU far exceeds the 250 ms [`ARGON2_PERMIT_WAIT`], so the surplus
    /// sheds into `Backend` ⇒ **HTTP 503 on the first request of every cold
    /// build**. Coalescing collapses that burst to one Argon2id under one
    /// permit, so no request is shed for work another request is already doing.
    verify_flights: Arc<FlightGroup<VerifyFlight>>,
    /// The mirror of `verify_flights` on the unknown/expired/revoked arm.
    ///
    /// Its ONLY purpose is symmetry. Coalescing just the row-FOUND arm would
    /// make N concurrent copies of one wrong-secret token cost ~1×Argon2id when
    /// the `token_id` exists and ~N× when it does not — re-opening, in the
    /// concurrency dimension, exactly the token-enumeration oracle the dummy
    /// burn exists to close. See [`dummy_burn_fingerprint`] for why the two keys
    /// partition identically.
    burn_flights: Arc<FlightGroup<BurnFlight>>,
}

impl std::fmt::Debug for PatVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatVerifier")
            .field("lookup", &"Arc<dyn PatRowLookup>")
            .field(
                "signing_keys",
                &format_args!("[{} REDACTED]", self.signing_keys.len()),
            )
            .field(
                "argon2_permits_available",
                &self.argon2_permits.available_permits(),
            )
            .finish()
    }
}

impl PatVerifier {
    /// Construct from an explicit row source + a single signing key
    /// (used by the production wiring and by tests with a fake lookup).
    #[must_use]
    pub fn new(lookup: Arc<dyn PatRowLookup>, signing_key: Arc<PatSigningKey>) -> Self {
        Self::with_key_set(lookup, vec![(*signing_key).clone()])
    }

    /// Construct from an explicit row source + an HMAC overlap key set
    /// (current + optional rotation predecessor/successor). The order is
    /// irrelevant (verify is constant-time over the whole set). An empty
    /// set fails CLOSED — every verify returns `InvalidPat`.
    #[must_use]
    pub fn with_key_set(lookup: Arc<dyn PatRowLookup>, signing_keys: Vec<PatSigningKey>) -> Self {
        Self {
            lookup,
            signing_keys: Arc::new(signing_keys),
            // Process-wide Argon2id concurrency cap (finding #2). Constructed
            // here so EVERY constructor path (`new`, `from_env`, and the test
            // wiring) gets the bound without changing any public signature.
            argon2_permits: Arc::new(Semaphore::new(ARGON2_VERIFY_PERMITS)),
            // Per-tenant fairness sub-limit (finding #1). Empty map; tenant
            // semaphores are created lazily on first verify for that tenant.
            // BOUNDED LRU (#1/#12 follow-up): capped at PER_TENANT_MAP_CAP.
            per_tenant: Arc::new(PerTenantGate::new(
                ARGON2_PER_TENANT_PERMITS,
                PER_TENANT_MAP_CAP,
            )),
            // Memoise the (immutable) Argon2id secret-match so a build's
            // thousands of requests pay it once, not once each. Revocation is
            // unaffected — the D1 row is still read per request.
            secret_match_memo: Arc::new(SecretMatchMemo::new(
                SECRET_MATCH_MEMO_CAP,
                SECRET_MATCH_MEMO_TTL,
            )),
            // …and coalesce the COLD burst that misses that memo, on BOTH 401
            // arms, so the cold-start 503 disappears without making `token_id`
            // liveness observable in the concurrency dimension.
            verify_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
            burn_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
        }
    }

    /// Build the production verifier from process env: a D1 HTTP client
    /// (from [`StorageEnv`]) plus the HMAC overlap key set —
    /// `PAT_SIGNING_KEY` (required, current) and the optional rotation
    /// siblings `PAT_SIGNING_KEY_PREV` / `PAT_SIGNING_KEY_NEW`.
    /// Returns `None` when the D1 client or the *current* key is
    /// missing/invalid, so the caller can fail-CLOSED (not mount the
    /// adapter route) in dev/CI — mirroring
    /// `internal_pat::build_state_from_env`.
    ///
    /// An optional sibling that is *present but malformed* (bad hex /
    /// too short) fails CLOSED too: the whole verifier is refused rather
    /// than silently dropping a key the operator believes is live. An
    /// *absent* sibling is simply omitted from the set.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let storage_env = StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env)
            .map_err(|e| tracing::warn!(error = %e, "adapter PAT verifier: D1 client init failed"))
            .ok()?;

        // Current key is REQUIRED.
        let current = Self::decode_key_env("PAT_SIGNING_KEY")??;

        let mut signing_keys = vec![current];

        // Optional rotation overlap siblings. `None` ⇒ absent (skip);
        // `Some(None)` ⇒ present-but-malformed (fail CLOSED).
        for name in ["PAT_SIGNING_KEY_PREV", "PAT_SIGNING_KEY_NEW"] {
            match Self::decode_key_env(name) {
                None => {} // absent — not part of the overlap set
                Some(Some(key)) => signing_keys.push(key),
                Some(None) => return None, // present but invalid — refuse to mount
            }
        }

        // Front the per-op D1 `pat` read with single-flight coalescing so a cold
        // parallel burst of the SAME runner PAT (a hydrate) collapses to one D1
        // read instead of a thundering herd — no cache, so revocation stays
        // immediate. See [`SingleFlightPatLookup`].
        let lookup: Arc<dyn PatRowLookup> = Arc::new(SingleFlightPatLookup::new(Arc::new(d1)));
        Some(Self::with_key_set(lookup, signing_keys))
    }

    /// Decode one hex `PatSigningKey` from the named env var.
    ///
    /// - `None` — the var is absent / empty (caller decides whether that
    ///   is fatal: required for the current key, fine for siblings).
    /// - `Some(None)` — the var is set but malformed (bad hex or < 32
    ///   bytes); the caller treats this as fail-CLOSED.
    /// - `Some(Some(key))` — a valid key.
    fn decode_key_env(name: &str) -> Option<Option<PatSigningKey>> {
        let raw = non_empty_env(name)?;
        let Ok(key_bytes) = hex::decode(raw.trim()) else {
            tracing::warn!("{name} is not valid hex; adapter routes NOT mounted");
            return Some(None);
        };
        match PatSigningKey::from_bytes(key_bytes) {
            Ok(key) => Some(Some(key)),
            Err(e) => {
                tracing::warn!(error = %e, "{name} invalid; adapter routes NOT mounted");
                Some(None)
            }
        }
    }

    /// Test-only: reach the per-tenant fairness gate's semaphore for `tenant`
    /// through the exact get-or-insert path a verify uses, so a test can
    /// saturate (or pin) a bucket. Production code goes through
    /// [`PerTenantGate::acquire`].
    #[cfg(test)]
    fn per_tenant_semaphore(&self, tenant: &str) -> Option<Arc<Semaphore>> {
        self.per_tenant.semaphore(tenant)
    }

    /// The full Option-B verification pipeline, returning the PAT's owning
    /// tenant id **and** whether it carries cache WRITE capability.
    ///
    /// Callers that mint a downstream credential FROM the PAT (the OCI
    /// `/token` Basic→Bearer exchange) use the write bit to downscope the
    /// grant to the PAT's real rights — otherwise a read-only (`cas:r`)
    /// PAT could obtain a `push` registry token. The simpler [`Self::verify`]
    /// discards the bit (per-op write enforcement for the header-scoped
    /// adapters stays at the route from `x-corelink-scope`).
    pub async fn verify_capability(
        &self,
        pat_plaintext: &str,
    ) -> Result<(String, bool), VerifyError> {
        // 1. HMAC fast-reject (pre-D1). A forged token is rejected here
        //    without a D1 round-trip. Uniform InvalidPat. Checked against
        //    the overlap key set so a token minted under the rotation
        //    predecessor/successor still fast-passes here.
        let (_env, token_id) = verify_hmac_only_multi(pat_plaintext, &self.signing_keys)
            .map_err(|_| VerifyError::InvalidPat)?;

        // 2. D1 lookup by the non-secret token_id (expiry filtered in SQL).
        let row = match self
            .lookup
            .lookup(token_id.as_str())
            .await
            .map_err(VerifyError::Backend)?
        {
            Some(row) => row,
            // Unknown / expired / revoked. Burn the SAME Argon2id cost as
            // the hot path before returning so response latency does not
            // leak whether the token_id exists (token-enumeration oracle
            // defence). Errors here are ignored: it is timing padding, not
            // an auth decision.
            None => {
                // COALESCED, exactly like the hot path below — see
                // `burn_flights`. N concurrent copies of ONE plaintext run ONE
                // dummy burn here, just as N concurrent copies of one plaintext
                // run ONE Argon2id there, so the cost of a burst is independent
                // of whether the token_id exists.
                //
                // ⚠️ INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM (#1034) is preserved
                // through the coalescing: EVERY shed inside this flight —
                // global acquire timeout, closed semaphore, saturated
                // per-tenant sub-cap, or an aborted flight task — resolves to
                // `Backend("pat verifier overloaded")`, byte-identical to the
                // row-FOUND arm below. The two shed arms MUST stay identical:
                // the row-FOUND path sheds with `Backend` (503) and this
                // row-NOT-FOUND path used to shed with `InvalidPat` (401), so
                // under saturation the status code alone answered "does this
                // token_id exist in D1?" — a row-existence oracle strictly
                // stronger than the latency one the burn exists to close.
                // Uniformity, not the burn, is what makes the shed safe: on a
                // shed the burn is deliberately SKIPPED (there are no permits
                // to run it with), so timing parity is already gone and the
                // status must carry no information either. Every rejection that
                // is NOT a shed still collapses to `InvalidPat`.
                //
                // Coalescing only ever makes that uniformity easier to hold: a
                // burst of one plaintext now takes ONE trip through these two
                // acquires instead of N, so the number of requests that can
                // reach a shed at all strictly decreases.
                let plaintext = pat_plaintext.to_owned();
                let permits = Arc::clone(&self.argon2_permits);
                let per_tenant = Arc::clone(&self.per_tenant);
                let outcome = self
                    .burn_flights
                    .run(
                        &dummy_burn_fingerprint(pat_plaintext),
                        move || {
                            async move {
                                // Finding #12: cap the dummy-burn path through ONE
                                // shared synthetic bucket so a leaked-key flood
                                // across bogus token_ids cannot drain the global
                                // pool via this path.
                                //
                                // ⚠️ ORDER IS INVERTED HERE ON PURPOSE, and it is
                                // the whole defence. This arm takes the SHARED
                                // bucket FIRST and the global permit only after.
                                // The original order (global, then a 250 ms
                                // bounded wait on the bucket) capped concurrent
                                // BURNS at the sub-cap but did NOT cap this arm's
                                // global-pool occupancy: a request already
                                // destined to shed still parked a global permit
                                // for the full `ARGON2_PERMIT_WAIT` while queued
                                // on the saturated bucket, so at the flood rate
                                // the bucket itself admits, nearly the entire
                                // global pool sat pinned by requests that would
                                // go on to burn nothing. Bucket-first caps the
                                // arm's global footprint at the sub-cap for real:
                                // a shed here holds no global permit at any
                                // instant.
                                //
                                // The acquire is NON-BLOCKING
                                // ([`PerTenantGate::try_acquire`]) precisely
                                // because it runs with no global permit held and
                                // therefore inverts the module's
                                // global→per-tenant order (see
                                // [`PerTenantGate::acquire`]). That order exists
                                // to keep the two tiers deadlock-free by being
                                // consistent; a `try` cannot deadlock in any
                                // order, because it never waits, so the rationale
                                // survives the inversion intact. The cost is the
                                // deliberate semantic change: a burn that would
                                // previously have waited up to
                                // `ARGON2_PERMIT_WAIT` for a bucket slot now
                                // sheds at once — a shed either way, just sooner
                                // and without holding capacity hostage meanwhile.
                                let tenant_permit =
                                    match per_tenant.try_acquire(UNKNOWN_TOKEN_BUCKET) {
                                        Ok(maybe_permit) => maybe_permit,
                                        // PER-TENANT shed: skip the burn and shed
                                        // with the SAME overloaded signal as the
                                        // global tier and as the row-FOUND arm
                                        // (INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM).
                                        // `Ok(None)` is the fail-safe fall-through
                                        // to global-only bounding and proceeds to
                                        // the burn.
                                        Err(()) => {
                                            return BurnFlight::Backend(
                                                "pat verifier overloaded".into(),
                                            )
                                        }
                                    };
                                // The dummy burn ALSO runs Argon2id (for timing
                                // parity), so it must be bounded by the same gate —
                                // otherwise an attacker who floods valid-HMAC tokens
                                // for NON-existent token_ids could OOM the container
                                // exactly like the hot path.
                                let permit = match tokio::time::timeout(
                                    ARGON2_PERMIT_WAIT,
                                    permits.acquire_owned(),
                                )
                                .await
                                {
                                    Ok(Ok(permit)) => permit,
                                    // GLOBAL shed. Returns `Backend`, the SAME code
                                    // the hot path returns for the SAME condition —
                                    // see the status-parity note below. `tenant_permit`
                                    // is released by RAII on this early return, so a
                                    // global shed never strands a bucket slot either.
                                    _ => {
                                        return BurnFlight::Backend(
                                            "pat verifier overloaded".into(),
                                        )
                                    }
                                };
                                let _ = tokio::task::spawn_blocking(move || {
                                    let r =
                                        corelink_pat::dummy_verify_for_constant_time(&plaintext);
                                    drop(permit); // hold the global permit ONLY across the blocking work
                                    drop(tenant_permit); // release the synthetic-bucket permit too
                                    r
                                })
                                .await;
                                BurnFlight::Burned
                            }
                            .boxed()
                        },
                        BurnFlight::Backend,
                    )
                    .await;
                // STATUS PARITY between the two ways to earn a 401
                // (INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM, #1034).
                //
                // A COMPLETED burn is the uniform 401 path — the burn ran, the
                // credential was decided, and the answer is `InvalidPat`
                // exactly as it is for a wrong secret on a live row.
                //
                // A SHED — at EITHER tier, global or per-tenant, and whether it
                // fires inside the flight or the flight task itself dies —
                // decided nothing about the credential, so it answers
                // `Backend("pat verifier overloaded")`, identical to the
                // row-FOUND arm. Uniformity across the two tiers matters as
                // much as uniformity across the two arms: if the per-tenant
                // tier still answered 401 here while the global tier answered
                // 503, an attacker who can pick WHICH tier sheds would recover
                // the same bit the asymmetry used to hand over for free.
                return Err(match &*outcome {
                    BurnFlight::Burned => VerifyError::InvalidPat,
                    BurnFlight::Backend(msg) => VerifyError::Backend(msg.clone()),
                });
            }
        };

        // 3. Full crypto verify on a blocking thread (Argon2id is
        //    CPU-heavy and must not stall the async worker). Re-parses,
        //    constant-time matches token_id, re-checks HMAC, then
        //    Argon2id-verifies the secret segment against the stored hash.
        //
        //    MEMOISED ([`SecretMatchMemo`]): Argon2id at the OWASP-2024 cost is
        //    ~0.15 CPU-s and the container runs at 0.5 vCPU, so paying it per
        //    request capped one tenant's whole adapter plane at ~3 req/s. The
        //    memo is keyed on (plaintext, token_id, stored pat_hash, scope,
        //    find_only) and holds ONLY the immutable "these match" boolean —
        //    stages 1 and 2 above
        //    (HMAC against the CURRENT key set; the expiry- and
        //    revocation-filtered D1 row read) still run on EVERY request, so
        //    this changes no authorization decision and opens NO revocation
        //    window. A wrong secret yields a different key and therefore always
        //    pays the full Argon2id, which is what keeps the two 401 paths
        //    (unknown token_id ⇒ dummy burn / wrong secret ⇒ real verify) in
        //    timing parity.
        //
        //    COALESCED on a shared FUTURE ([`FlightGroup`]) so a burst of COLD
        //    memo misses for the SAME key runs ONE Argon2id, not N. Without
        //    it the memo does nothing for the very first burst of a cold
        //    container — a `cargo -jN` build opens with N simultaneous misses,
        //    `ARGON2_PER_TENANT_PERMITS` (4) are admitted, and since one
        //    Argon2id at 0.5 vCPU dwarfs the 250 ms `ARGON2_PERMIT_WAIT` the
        //    surplus sheds to 503.
        //
        //    An earlier revision coalesced on a sharded MUTEX instead, and three
        //    independent reviewers killed it. The shared future is not the same
        //    object, and it is worth recording exactly why each finding does not
        //    survive the change of shape:
        //      (a) ORACLE — the mutex sat on the row-FOUND path only, so N
        //          concurrent copies of one wrong-secret token cost ~1×Argon2id
        //          when the `token_id` existed and ~N× when it did not. Here BOTH
        //          arms coalesce, on keys that partition identically for a given
        //          plaintext (see `dummy_burn_fingerprint`), so a burst costs
        //          ~1×Argon2id either way and liveness stays unobservable in the
        //          concurrency dimension.
        //      (b) UNBOUNDED WAIT IN FRONT OF THE LOAD SHED — the mutex made a
        //          burst an N-deep FIFO whose tail waited ~N× one Argon2id, with
        //          no timeout, IN FRONT of the two `ARGON2_PERMIT_WAIT` sheds.
        //          A shared future has no queue: every joiner resolves together
        //          when the ONE run completes, so the N-th waits exactly as long
        //          as the 1st. That wait is `250 ms + 250 ms + one Argon2id` —
        //          i.e. the worst case of a request that is admitted TODAY. The
        //          shed still bounds admission; it is now inside the flight, and
        //          when it fires every joiner gets the same fail-CLOSED
        //          `Backend`. Coalescing removes work; it never adds a wait
        //          longer than the successful path already has.
        //      (c) FAIRNESS BYPASS — the mutex was taken BEFORE the per-tenant
        //          sub-permit, so on the SHARED `_oci` / `_anonymous` Durable
        //          Objects one tenant's flood could block another upstream of the
        //          cap. Here the acquires stay INSIDE the flight and in the
        //          unchanged global→per-tenant order, and a joiner takes no
        //          permit at all. Concurrent Argon2ids for a tenant still equal
        //          that tenant's held sub-permits, so `ARGON2_PER_TENANT_PERMITS`
        //          bounds them exactly as before — and a waiter only ever waits
        //          on its own key's flight, which by construction belongs to its
        //          own tenant. There is no cross-tenant coupling to bypass.
        let fp = secret_match_fingerprint(
            pat_plaintext,
            token_id.as_str(),
            &row.pat_hash,
            &row.scope,
            row.find_only,
        );
        // Whether THIS request went through the Argon2id stage rather than
        // being served by the memo. Under coalescing that includes a request
        // that JOINED another's flight instead of running the hash itself —
        // which is correct: the flight key binds `scope` and `find_only`, so a
        // joiner reaches the identical step-4 verdict, and the memo is populated
        // only at the very END of the pipeline, after that gate. See the comment
        // there for why populating it earlier would be an oracle.
        let mut proved_here = false;
        if !self.secret_match_memo.contains(&fp) {
            proved_here = true;
            let plaintext = pat_plaintext.to_owned();
            let signing_keys = Arc::clone(&self.signing_keys);
            let stored_hash = PatHash::from_phc_string(row.pat_hash.clone());
            let permits = Arc::clone(&self.argon2_permits);
            let per_tenant = Arc::clone(&self.per_tenant);
            let tenant_id = row.tenant_id.clone();
            let outcome = self
                .verify_flights
                .run(
                    &fp,
                    move || {
                        async move {
                            // Finding #2: bound concurrent Argon2id. Acquire a permit
                            // (bounded wait) BEFORE the blocking work; on
                            // acquire-timeout the verifier is overloaded — return
                            // `Backend` (reusing the existing variant, which the OCI
                            // `/token` handler and the native gate already map to a
                            // non-leaky fail-CLOSED rejection) rather than letting
                            // the request pile on more 64-MiB allocations. The permit
                            // is moved into the blocking closure and dropped there,
                            // so it is held ONLY across the Argon2id.
                            let permit = match tokio::time::timeout(
                                ARGON2_PERMIT_WAIT,
                                permits.acquire_owned(),
                            )
                            .await
                            {
                                Ok(Ok(permit)) => permit,
                                // Semaphore closed (never in practice) or the
                                // bounded wait elapsed — both fail CLOSED.
                                Ok(Err(_)) | Err(_) => {
                                    return VerifyFlight::Backend("pat verifier overloaded".into())
                                }
                            };
                            // Finding #1: per-tenant fairness. With the GLOBAL permit
                            // already held, acquire this tenant's sub-permit
                            // (consistent order global→per-tenant ⇒ deadlock-free).
                            // If the tenant is at its sub-cap, fail-CLOSED with the
                            // SAME overloaded signal as a global timeout — so ONE
                            // tenant flooding distinct PATs cannot drain the whole
                            // global pool and starve others. The `permit` (global) is
                            // dropped on this early return by RAII, so a per-tenant
                            // rejection does NOT leak a global permit. `Ok(None)` is
                            // the fail-safe fall-through to global-only bounding.
                            let tenant_permit = match per_tenant.acquire(&tenant_id).await {
                                Ok(maybe_permit) => maybe_permit,
                                Err(()) => {
                                    return VerifyFlight::Backend("pat verifier overloaded".into())
                                }
                            };
                            let joined = tokio::task::spawn_blocking(move || {
                                let r = verify_with_hash_multi(
                                    &plaintext,
                                    &token_id,
                                    &stored_hash,
                                    &signing_keys,
                                );
                                drop(permit); // release the global Argon2id permit the moment the work ends
                                drop(tenant_permit); // release the per-tenant permit too (RAII, both paths)
                                r
                            })
                            .await;
                            // ⚠️ The flight proves the SECRET and nothing else. It
                            // must NOT populate the memo here, even though this is
                            // where the proof lands: the memo is written only past
                            // the step-4 scope gate, per request — see the comment
                            // there. A scope-rejected PAT memoised at this point
                            // would 401 in ~0 ms forever after, which is the
                            // latency oracle that gate placement exists to close.
                            match joined {
                                Ok(Ok(_verified)) => VerifyFlight::Proven,
                                Ok(Err(_)) => VerifyFlight::Rejected,
                                Err(e) => VerifyFlight::Backend(format!("verify join: {e}")),
                            }
                        }
                        .boxed()
                    },
                    VerifyFlight::Backend,
                )
                .await;
            match &*outcome {
                VerifyFlight::Proven => {}
                VerifyFlight::Rejected => return Err(VerifyError::InvalidPat),
                VerifyFlight::Backend(msg) => return Err(VerifyError::Backend(msg.clone())),
            }
        }

        // 4. Scope gate — fail-CLOSED on NO cache capability at all, then
        //    surface the read/write split so credential-minting callers can
        //    downscope.
        //
        // ADR-0071 (find-only least-privilege): a find-only PAT stores the
        // CHECK-safe base `scope = 'read-only'` and is narrowed to find-missing
        // ONLY at the Worker's `x-corelink-scope` header. But this adapter verifier
        // authorizes package-manager reads (npm/pip/brew/cargo/OCI) directly from
        // the D1 `scope` — the OCI `/token` exchange (`routes/oci.rs`) has NO header
        // gate — so the `read-only` base would otherwise grant e.g. `docker pull`.
        // The adapter planes have no find-missing operation, so a find-only PAT is
        // rejected here fail-CLOSED (defense-in-depth for every adapter surface).
        if row.find_only {
            return Err(VerifyError::InvalidPat);
        }
        if !requires_cache_read(&row.scope) {
            return Err(VerifyError::InvalidPat);
        }
        let can_write = requires_cache_write(&row.scope);

        // ⚠️ The memo is populated HERE — past the scope gate — and nowhere
        // earlier. Populating it at the end of step 3 (right after the Argon2id
        // succeeded) looks natural and is an ORACLE: a PAT whose secret is
        // CORRECT but whose scope is rejected (`find_only`, or no cache grant)
        // would be memoised on its first, slow 401, and every later attempt with
        // that same token would 401 in ~0 ms instead of paying Argon2id. An
        // attacker replaying a bag of leaked plaintexts TWICE could then sort
        // "live credential, insufficient scope" from "dead/unknown/wrong-secret"
        // — and a *revoked* PAT (row gone ⇒ slow dummy burn) from a merely
        // *scope-downgraded* one (fast memo hit) — purely on latency, with no
        // grant of any kind. The module header requires every rejection reason to
        // be indistinguishable on the wire; latency is part of the wire.
        //
        // Gating on `proved_here` (rather than inserting unconditionally) keeps a
        // warm hit off the memo's write lock and keeps the TTL anchored at the
        // proof, not sliding on use.
        //
        // Coalescing does not disturb this. A burst's joiners all have
        // `proved_here == true` and so all reach this line, but they also all
        // reach the SAME scope verdict as their leader — the flight key binds
        // `scope` and `find_only`, so a burst cannot straddle the gate — and the
        // insert is idempotent on a shared key. A scope-REJECTED burst returns
        // above without any member inserting, exactly as a single request does.
        if proved_here {
            self.secret_match_memo.insert(fp);
        }

        Ok((row.tenant_id, can_write))
    }

    /// Test-only: shrink the per-tenant LRU map cap so the eviction path can be
    /// driven deterministically (the production cap of 10k is too large to fill
    /// in a unit test). Returns `self` for chaining off a constructor.
    #[cfg(test)]
    #[must_use]
    fn with_map_cap(mut self, cap: usize) -> Self {
        // The gate is behind an `Arc`, so rebuild it. Only ever called straight
        // off a constructor, where the map is still empty.
        self.per_tenant = Arc::new(PerTenantGate::new(self.per_tenant.cap, cap));
        self
    }

    /// Test-only: current number of live per-tenant semaphore entries (LRU map
    /// size). Used to assert the map stays bounded under distinct-tenant churn.
    #[cfg(test)]
    #[allow(
        clippy::expect_used,
        reason = "test-only helper; a poisoned mutex here is itself a test bug \
                  and should fail loudly. Mirrors the test module's allow — this \
                  cfg(test) method sits on the impl block, outside that module's scope."
    )]
    fn per_tenant_map_len(&self) -> usize {
        self.per_tenant.permits.lock().expect("map lock").len()
    }

    /// Test-only constructor that overrides the Argon2id concurrency bound so
    /// a unit test can drive the semaphore to exhaustion deterministically
    /// (the production const is too large to fill in a test). Not part of the
    /// public production surface.
    #[cfg(test)]
    #[must_use]
    fn with_key_set_and_permits(
        lookup: Arc<dyn PatRowLookup>,
        signing_keys: Vec<PatSigningKey>,
        permits: usize,
    ) -> Self {
        Self::with_key_set_and_permits_per_tenant(
            lookup,
            signing_keys,
            permits,
            ARGON2_PER_TENANT_PERMITS,
        )
    }

    /// Test-only constructor that overrides BOTH the global Argon2id concurrency
    /// bound AND the per-tenant sub-cap, so the fairness test can drive the
    /// two-tier interaction deterministically (e.g. a per-tenant cap small
    /// enough to saturate while global headroom remains for other tenants).
    #[cfg(test)]
    #[must_use]
    fn with_key_set_and_permits_per_tenant(
        lookup: Arc<dyn PatRowLookup>,
        signing_keys: Vec<PatSigningKey>,
        permits: usize,
        per_tenant_permits: usize,
    ) -> Self {
        Self {
            lookup,
            signing_keys: Arc::new(signing_keys),
            argon2_permits: Arc::new(Semaphore::new(permits)),
            per_tenant: Arc::new(PerTenantGate::new(per_tenant_permits, PER_TENANT_MAP_CAP)),
            secret_match_memo: Arc::new(SecretMatchMemo::new(
                SECRET_MATCH_MEMO_CAP,
                SECRET_MATCH_MEMO_TTL,
            )),
            verify_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
            burn_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
        }
    }

    /// The full Option-B verification pipeline. Returns the PAT's owning
    /// tenant id on success. Thin wrapper over [`Self::verify_capability`]
    /// for callers that don't need the write-capability bit.
    pub async fn verify(&self, pat_plaintext: &str) -> Result<String, VerifyError> {
        self.verify_capability(pat_plaintext)
            .await
            .map(|(tenant, _can_write)| tenant)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as failures by design"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use corelink_pat::{
        mint, PatEnv, PatScopes, PrincipalId, TenantId, SCOPE_CACHE_R, SCOPE_CACHE_RW,
    };
    use uuid::Uuid;

    use super::*;

    /// Fake row source: a `token_id → PatRow` map plus a call counter and
    /// an optional forced backend error. Drives the full verification
    /// pipeline with real crypto but no network.
    #[derive(Default)]
    struct FakeLookup {
        rows: HashMap<String, PatRow>,
        calls: AtomicUsize,
        backend_err: Option<String>,
    }

    impl FakeLookup {
        fn with_row(token_id: &str, row: PatRow) -> Self {
            let mut rows = HashMap::new();
            rows.insert(token_id.to_owned(), row);
            Self {
                rows,
                calls: AtomicUsize::new(0),
                backend_err: None,
            }
        }
        fn backend(err: &str) -> Self {
            Self {
                backend_err: Some(err.to_owned()),
                ..Self::default()
            }
        }
        fn empty() -> Self {
            Self::default()
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl PatRowLookup for FakeLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.backend_err {
                return Err(e.clone());
            }
            Ok(self.rows.get(token_id).cloned())
        }
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Mint a real PAT and return `(plaintext, token_id, pat_hash, tenant_string)`.
    fn mint_pat(
        key: &PatSigningKey,
        tenant: u128,
        scopes: u64,
    ) -> (String, String, String, String) {
        let tenant_id = TenantId(Uuid::from_u128(tenant));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(tenant + 1000)),
            PatScopes::from_u64(scopes),
            None,
            key,
            1,
        )
        .unwrap();
        (
            plaintext.into_string(),
            pat.token_id.as_str().to_owned(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    fn row(pat_hash: &str, tenant: &str, scope: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: pat_hash.to_owned(),
            scope: scope.to_owned(),
            find_only: false,
        }
    }

    #[tokio::test]
    async fn forged_token_rejected_before_d1() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier
            .verify("corelink_pat_not-a-real-token")
            .await
            .unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "forged token must not reach D1");
    }

    #[tokio::test]
    async fn wrong_signing_key_rejected_before_d1() {
        let mint_key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&mint_key, 7, SCOPE_CACHE_RW);
        let other_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let lookup = Arc::new(FakeLookup::with_row(
            "ignored",
            row(&hash, &tenant, "cas:rw"),
        ));
        let verifier = PatVerifier::new(lookup.clone(), other_key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
    }

    #[tokio::test]
    async fn valid_pat_with_cache_scope_resolves_tenant() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 42, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::new(lookup.clone(), key);
        let resolved = verifier.verify(&pt).await.unwrap();
        assert_eq!(resolved, tenant);
        assert_eq!(lookup.call_count(), 1);
    }

    #[tokio::test]
    async fn pat_minted_under_prev_key_validates_during_overlap() {
        // Rotation overlap: the verifier's CURRENT key differs from the key
        // the PAT was minted under, but the old key is still in the overlap
        // set — so the PAT must still validate (no instant fleet-wide outage).
        let old_key = test_key();
        let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let (pt, tid, hash, tenant) = mint_pat(&old_key, 60, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Overlap set = [new (current), old (prev)].
        let verifier =
            PatVerifier::with_key_set(lookup.clone(), vec![(*new_key).clone(), (*old_key).clone()]);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn pat_rejected_when_minting_key_not_in_overlap_set() {
        // After the overlap window closes (old key dropped), a PAT minted
        // under the now-retired key must be rejected.
        let old_key = test_key();
        let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let (pt, tid, hash, tenant) = mint_pat(&old_key, 61, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Overlap set = [new] only — old key retired.
        let verifier = PatVerifier::with_key_set(lookup.clone(), vec![(*new_key).clone()]);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
    }

    #[tokio::test]
    async fn empty_key_set_fails_closed() {
        // No key bound ⇒ nothing verifies (fail-closed).
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 62, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::with_key_set(lookup.clone(), vec![]);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "empty key set rejects pre-D1");
    }

    #[tokio::test]
    async fn admin_scope_grants_cache() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 43, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "admin")));
        let verifier = PatVerifier::new(lookup, key);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn unknown_token_id_is_invalid() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 44, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 1, "valid HMAC must reach D1");
    }

    #[tokio::test]
    async fn wrong_stored_hash_is_invalid() {
        let key = test_key();
        let (pt, tid, _hash, tenant) = mint_pat(&key, 45, SCOPE_CACHE_RW);
        let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 46, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(
            &tid,
            row(&other_hash, &tenant, "cas:rw"),
        ));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
    }

    #[tokio::test]
    async fn empty_scope_fails_closed() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 47, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
    }

    #[tokio::test]
    async fn read_only_scope_still_resolves() {
        // The verifier gates on "has cache read" (the port carries no op);
        // a cas:r PAT resolves here — per-op write-deny is the route's job.
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 48, SCOPE_CACHE_R);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:r")));
        let verifier = PatVerifier::new(lookup, key);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn find_only_pat_is_rejected_on_the_adapter_plane() {
        // ADR-0071: a find-only PAT stores the CHECK-safe base `read-only` but is
        // narrowed to find-missing at the Worker header. The adapter verifier
        // authorizes package-manager reads (OCI has NO header gate) directly from
        // the D1 scope, so `requires_cache_read("read-only")` would otherwise grant
        // e.g. `docker pull`. The `find_only` marker MUST fail-CLOSE here even
        // though the base scope reads. (Same possession proof as a normal PAT.)
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 52, SCOPE_CACHE_R);
        let find_only_row = PatRow {
            tenant_id: tenant.clone(),
            pat_hash: hash,
            scope: "read-only".to_owned(),
            find_only: true,
        };
        let lookup = Arc::new(FakeLookup::with_row(&tid, find_only_row));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "a find-only PAT must be rejected on every adapter surface; got {err:?}"
        );
    }

    #[tokio::test]
    async fn revoked_row_is_invalid_pat() {
        // Soft revocation (migration 0063) is enforced INSIDE the lookup
        // SQL (`AND revoked_at_ms IS NULL`), so a revoked row surfaces to
        // the pipeline exactly like an absent one: `lookup → None`. Model
        // that here (the FakeLookup map simply does not contain the
        // revoked row) and assert the uniform InvalidPat — a revoked PAT
        // must be indistinguishable from an unknown one on the wire.
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 50, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 1, "revocation is decided at D1");
    }

    #[test]
    fn lookup_sql_filters_revoked_and_expired_rows() {
        // The production D1 query must carry BOTH SQL-side liveness
        // filters — losing either one silently re-admits dead tokens.
        assert!(
            PAT_LOOKUP_SQL.contains("AND revoked_at_ms IS NULL"),
            "lookup SQL must exclude soft-revoked rows (migration 0063)"
        );
        assert!(
            PAT_LOOKUP_SQL.contains("expires_ms = 0 OR expires_ms >"),
            "lookup SQL must keep the expiry filter"
        );
        assert!(PAT_LOOKUP_SQL.contains("WHERE token_id = ?1"));
    }

    #[tokio::test]
    async fn backend_error_is_not_invalid_pat() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 49, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::backend("d1 unreachable"));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        match err {
            VerifyError::Backend(m) => assert!(m.contains("d1 unreachable")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    /// Finding #2: the Argon2id concurrency gate bounds work — when every
    /// permit is held, a hot-path verify (valid HMAC + real D1 row, so it
    /// reaches the Argon2id stage) must give up after the bounded wait and
    /// surface `Backend("…overloaded…")` rather than piling on another
    /// 64-MiB Argon2id allocation. We model "all permits held" by building a
    /// verifier with ZERO permits, so the acquire can never succeed and the
    /// timeout path is exercised deterministically.
    #[tokio::test]
    async fn exhausted_argon2_permits_yields_backend_overloaded() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 70, SCOPE_CACHE_RW);
        // A live row so the pipeline gets PAST the HMAC fast-reject and the D1
        // lookup, all the way to the permit acquire that guards Argon2id.
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 0);
        let err = verifier.verify(&pt).await.unwrap_err();
        match err {
            VerifyError::Backend(m) => {
                assert!(m.contains("overloaded"), "expected overloaded, got {m}")
            }
            other => panic!("expected Backend(overloaded), got {other:?}"),
        }
        // The D1 row WAS consulted (we are past the fast-reject) — the gate
        // sits AFTER the lookup, on the expensive stage only.
        assert_eq!(lookup.call_count(), 1);
    }

    /// The permit gate must NOT consume a permit for a forged token: the HMAC
    /// fast-reject fires first, so even with zero permits a forged token is
    /// still a plain `InvalidPat` (an attacker without the key cannot drive the
    /// verifier into the overloaded path).
    #[tokio::test]
    async fn forged_token_does_not_touch_argon2_permits() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 0);
        let err = verifier
            .verify("corelink_pat_not-a-real-token")
            .await
            .unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "forged token must fast-reject before the permit gate"
        );
        assert_eq!(lookup.call_count(), 0);
    }

    // ------------------------------------------------------------------
    // INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM — the load shed is symmetric
    // across D1 row existence.
    //
    // The shed used to be asymmetric: the row-FOUND arm shed with
    // `Backend` (⇒ 503) and the row-NOT-FOUND dummy-burn arm shed with
    // `InvalidPat` (⇒ 401). An attacker who can saturate the (small,
    // slow) Argon2id pool could therefore read the status code as a
    // ROW-EXISTENCE oracle — strictly more than the latency signal the
    // dummy burn exists to hide, because the burn is skipped on a shed
    // anyway. These tests pin BOTH halves of the contract:
    //
    //   under saturation  ⇒ every outcome is Backend("…overloaded…")
    //   outside saturation ⇒ every rejection is InvalidPat
    //
    // The idiom is #1022's: build the verifier with exactly ONE permit,
    // HOLD it, and let the acquire time out deterministically. A control
    // arm is mandatory — without it the test would pass just as happily
    // if the permit had never actually been held.
    // ------------------------------------------------------------------

    /// THE NEW CASE. A saturated pool + an unknown/absent D1 row must shed as
    /// `Backend`, not `InvalidPat`. This is the arm that used to leak row
    /// existence through the status code.
    #[tokio::test]
    async fn saturated_pool_sheds_an_unknown_row_as_backend() {
        let key = test_key();
        // A genuinely-minted PAT (so it clears the HMAC fast-reject) whose
        // token_id has NO row — the dummy-burn arm.
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 90, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);

        // Drain the one and only permit and keep it drained.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a shed on the row-NOT-FOUND arm must be Backend(overloaded), got {err:?}"
        );
        assert_eq!(lookup.call_count(), 1, "the shed sits AFTER the D1 lookup");
    }

    /// The per-tenant sub-cap arm of the same shed. The dummy burn is routed
    /// through ONE shared synthetic bucket; saturating THAT bucket (with global
    /// headroom to spare) must also shed as `Backend`, never `InvalidPat`.
    #[tokio::test]
    async fn saturated_dummy_burn_bucket_sheds_an_unknown_row_as_backend() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 91, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        // Global 8 (ample) so a rejection can ONLY come from the per-tenant
        // tier; synthetic bucket sub-cap 1 so holding one permit saturates it.
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            8,
            1,
        );
        let bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("synthetic bucket semaphore");
        let _held = Arc::clone(&bucket)
            .try_acquire_owned()
            .expect("bucket permit");
        assert_eq!(bucket.available_permits(), 0, "burn bucket saturated");

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a sub-cap shed on the row-NOT-FOUND arm must be Backend(overloaded), got {err:?}"
        );
    }

    /// The row-FOUND half of the same contract — existing behaviour, pinned
    /// here in the one-permit idiom so a future refactor cannot quietly
    /// re-asymmetrise the pair by changing only this side.
    #[tokio::test]
    async fn saturated_pool_sheds_a_live_row_as_backend() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 92, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);

        // Drain BEFORE the first verify so the PAT is never memoised (a warm
        // memo hit consumes no permit and would sail straight through).
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a shed on the row-FOUND arm must be Backend(overloaded), got {err:?}"
        );
    }

    /// THE ORACLE TEST. On ONE saturated verifier, a live-row PAT and an
    /// unknown-row PAT must produce byte-identical errors — same variant, same
    /// message. Anything an attacker could `!=` on here is a row-existence
    /// oracle.
    #[tokio::test]
    async fn shed_is_indistinguishable_between_live_and_unknown_rows() {
        let key = test_key();
        let (pt_live, tid_live, hash_live, tenant_live) = mint_pat(&key, 93, SCOPE_CACHE_RW);
        let (pt_unknown, _tid_u, _hash_u, _tenant_u) = mint_pat(&key, 94, SCOPE_CACHE_RW);
        // Only the FIRST token_id has a row; the second is unknown to D1.
        let lookup = Arc::new(FakeLookup::with_row(
            &tid_live,
            row(&hash_live, &tenant_live, "cas:rw"),
        ));
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 1);

        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err_live = verifier.verify(&pt_live).await.unwrap_err();
        let err_unknown = verifier.verify(&pt_unknown).await.unwrap_err();

        match (&err_live, &err_unknown) {
            (VerifyError::Backend(a), VerifyError::Backend(b)) => assert_eq!(
                a, b,
                "the two shed arms must be byte-identical (row-existence oracle)"
            ),
            other => panic!("both arms must shed as Backend, got {other:?}"),
        }
    }

    /// The anti-blanket-conversion control: with the pool NOT saturated, an
    /// unknown row must still be the uniform `InvalidPat` (401). If this ever
    /// flips to `Backend`, the fix stopped being a shed rule and became "the
    /// verifier answers 503 for every unknown token", which would 503 the whole
    /// world on a single bad credential.
    #[tokio::test]
    async fn unsaturated_pool_still_rejects_an_unknown_row_as_invalid_pat() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 95, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        // Permits available ⇒ the dummy burn actually runs ⇒ terminal InvalidPat.
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 2);

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "an unknown row with permits free must stay InvalidPat, got {err:?}"
        );
        assert_eq!(lookup.call_count(), 1, "valid HMAC must reach D1");
    }

    /// The HMAC fast-reject is UPSTREAM of every permit, so a forged token can
    /// never be pushed onto the shed path — it stays `InvalidPat` even with the
    /// pool fully drained. This is what stops an attacker without the signing
    /// key from using saturation to turn 401s into 503s (or from probing the
    /// shed at all).
    #[tokio::test]
    async fn forged_token_stays_invalid_pat_under_saturation() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier
            .verify("corelink_pat_not-a-real-token")
            .await
            .unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "a forged token must fast-reject ahead of the shed, got {err:?}"
        );
        assert_eq!(lookup.call_count(), 0, "forged token must not reach D1");
    }

    /// With permits available, the hot path still succeeds end-to-end — the
    /// gate is transparent under normal load (no regression to the verify
    /// pipeline).
    #[tokio::test]
    async fn verify_succeeds_when_permits_available() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 71, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 2);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    // ------------------------------------------------------------------
    // Finding #1 — per-tenant fairness (concurrency).
    //
    // NOTE ON TOOLING: the ideal tester here is SHUTTLE (deterministic async
    // interleaving for `tokio::sync::Semaphore`), but shuttle is NOT a
    // dependency anywhere in this workspace (no Cargo.lock entry, no usage) and
    // retrofitting it is heavy — it requires swapping `tokio::sync` for its
    // shimmed primitives AND it cannot model the `spawn_blocking` + real
    // Argon2id crypto on the verify path. Per the WP brief, we do NOT half-wire
    // shuttle; the deterministic-stress `#[tokio::test]` below proves the
    // fairness INVARIANT directly. Wiring shuttle (a feature-gated
    // sync-primitive swap on this module) is the owner-aware follow-up.
    // ------------------------------------------------------------------

    /// FAIRNESS INVARIANT (finding #1): a single tenant A that has SATURATED its
    /// per-tenant Argon2id sub-cap must NOT be able to deny tenant B its verify,
    /// even though A could in principle hold many global permits. We prove this
    /// directly by exhausting tenant A's per-tenant semaphore (held open) and
    /// showing a fresh B verify still succeeds while global headroom remains.
    ///
    /// Setup: global cap = 8 (plenty), per-tenant cap = 2. We pre-acquire BOTH
    /// of tenant A's per-tenant permits and hold them — modelling "A is at its
    /// fair share". A third A-verify is then capped at the per-tenant gate
    /// (fail-CLOSED overloaded), while B — a DIFFERENT tenant — sails through on
    /// its own untouched per-tenant semaphore. This is the anti-starvation
    /// property: one tenant's flood is contained to its own bucket.
    #[tokio::test]
    async fn per_tenant_cap_contains_one_tenant_without_starving_another() {
        let key = test_key();
        let (pt_a, tid_a, hash_a, tenant_a) = mint_pat(&key, 80, SCOPE_CACHE_RW);
        let (pt_b, tid_b, hash_b, tenant_b) = mint_pat(&key, 81, SCOPE_CACHE_RW);
        assert_ne!(tenant_a, tenant_b);

        let mut rows = HashMap::new();
        rows.insert(tid_a.clone(), row(&hash_a, &tenant_a, "cas:rw"));
        rows.insert(tid_b.clone(), row(&hash_b, &tenant_b, "cas:rw"));
        let lookup = Arc::new(FakeLookup {
            rows,
            calls: AtomicUsize::new(0),
            backend_err: None,
        });

        // Global cap 8 (ample headroom), per-tenant sub-cap 2.
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            8,
            2,
        );

        // Saturate tenant A's per-tenant semaphore by HOLDING both of its
        // permits — model A's in-flight flood occupying its whole fair share.
        let sem_a = verifier
            .per_tenant_semaphore(&tenant_a)
            .expect("tenant A semaphore");
        let _hold1 = Arc::clone(&sem_a).try_acquire_owned().expect("permit 1");
        let _hold2 = Arc::clone(&sem_a).try_acquire_owned().expect("permit 2");
        assert_eq!(sem_a.available_permits(), 0, "A at its per-tenant cap");

        // A THIRD verify for tenant A must be denied at the per-tenant gate
        // (fail-CLOSED overloaded) — A cannot exceed its fair share. Global
        // permits remain plentiful, so this is the per-tenant bound at work,
        // NOT the global one.
        let err_a = verifier.verify(&pt_a).await.unwrap_err();
        match err_a {
            VerifyError::Backend(m) => assert!(
                m.contains("overloaded"),
                "A beyond its cap must be overloaded, got {m}"
            ),
            other => panic!("expected Backend(overloaded) for A, got {other:?}"),
        }

        // CRUX: tenant B is NOT starved by A's saturation — B's verify runs on
        // its OWN per-tenant semaphore and global headroom, and SUCCEEDS.
        let resolved_b = verifier.verify(&pt_b).await.expect("B must not be starved");
        assert_eq!(resolved_b, tenant_b);
    }

    /// The GLOBAL bound still holds with the two-tier gate in place: with the
    /// global pool exhausted (0 permits) but a generous per-tenant cap, a verify
    /// is still rejected at the GLOBAL gate (acquired FIRST) — proving the
    /// per-tenant tier did not weaken the OOM/CPU guard, and that the global
    /// permit is acquired before the per-tenant one (consistent order).
    #[tokio::test]
    async fn global_bound_still_holds_under_two_tier_gate() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 82, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Global 0 ⇒ no global permit can ever be had; per-tenant 8 ⇒ the
        // per-tenant tier is wide open, so a rejection here can ONLY be the
        // global gate (which is acquired first).
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            0,
            8,
        );
        let err = verifier.verify(&pt).await.unwrap_err();
        match err {
            VerifyError::Backend(m) => {
                assert!(m.contains("overloaded"), "expected overloaded, got {m}")
            }
            other => panic!("expected Backend(overloaded), got {other:?}"),
        }
        assert_eq!(
            lookup.call_count(),
            1,
            "global gate sits after the D1 lookup"
        );
    }

    /// FAIL-SAFE: a poisoned per-tenant map must fall back to GLOBAL-only
    /// bounding — a bookkeeping fault must NEVER block a legitimate auth. We
    /// poison the lock, then assert a valid verify still succeeds (it proceeds
    /// under the global bound with `Ok(None)` from the per-tenant acquire).
    #[tokio::test]
    async fn poisoned_per_tenant_map_falls_back_to_global_only() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 83, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            8,
            2,
        );
        // Poison the per-tenant mutex by panicking while holding the guard.
        let gate = Arc::clone(&verifier.per_tenant);
        let _ = std::thread::spawn(move || {
            let _guard = gate.permits.lock().unwrap();
            panic!("intentional poison");
        })
        .join();
        assert!(
            verifier.per_tenant.permits.is_poisoned(),
            "precondition: map must be poisoned"
        );
        // Despite the poison, the legit verify still resolves (fail-safe to
        // global-only bounding — no auth blocked on bookkeeping).
        assert_eq!(
            verifier.verify(&pt).await.expect("fail-safe global-only"),
            tenant
        );
    }

    /// A single tenant flooding distinct PATs (each a DIFFERENT token_id, all
    /// owned by the SAME tenant) is bounded by that tenant's per-tenant cap:
    /// once its sub-cap is held, additional concurrent verifies for the same
    /// tenant are rejected — exactly the abuse vector finding #1 describes
    /// (distinct PATs each forcing a fresh Argon2id), now contained.
    #[tokio::test]
    async fn same_tenant_distinct_pats_share_one_per_tenant_bucket() {
        let key = test_key();
        // Two DISTINCT PATs (distinct token_ids + hashes) for the SAME tenant.
        let (pt1, tid1, hash1, tenant) = mint_pat(&key, 84, SCOPE_CACHE_RW);
        // Mint a second PAT for the same tenant id (84) — different principal
        // offset via the mint counter, so a distinct token_id/hash.
        let tenant_id = TenantId(Uuid::from_u128(84));
        let (plaintext2, pat2) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(99_999)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt2 = plaintext2.into_string();
        let tid2 = pat2.token_id.as_str().to_owned();
        let hash2 = pat2.hash.as_str().to_owned();
        assert_ne!(tid1, tid2, "distinct PATs ⇒ distinct token_ids");

        let mut rows = HashMap::new();
        rows.insert(tid1, row(&hash1, &tenant, "cas:rw"));
        rows.insert(tid2, row(&hash2, &tenant, "cas:rw"));
        let lookup = Arc::new(FakeLookup {
            rows,
            calls: AtomicUsize::new(0),
            backend_err: None,
        });

        // Per-tenant cap 1 so a single held permit saturates the tenant.
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 1);

        // Hold the tenant's single per-tenant permit (model PAT #1 in-flight).
        let sem = verifier.per_tenant_semaphore(&tenant).expect("tenant sem");
        let _hold = Arc::clone(&sem)
            .try_acquire_owned()
            .expect("hold the only permit");

        // A verify for PAT #2 (same tenant, different token_id) is denied at the
        // shared per-tenant bucket — the flood is contained per tenant.
        let err = verifier.verify(&pt2).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "second distinct PAT for the SAME tenant must hit the per-tenant cap, got {err:?}"
        );
        // (pt1 unused beyond minting — it shares the bucket identity with pt2.)
        let _ = pt1;
    }

    /// BOUNDED-MAP INVARIANT (#1/#12 follow-up): the per-tenant semaphore map
    /// must stay bounded under a churn of MANY distinct tenants, while an
    /// actively-contended tenant's bucket is NEVER evicted.
    ///
    /// Setup: map cap = 4 (tiny, so eviction fires fast). We pin tenant
    /// "active" by HOLDING a permit on its bucket (an in-flight verify ⇒ NOT
    /// fully idle ⇒ ineligible for eviction). Then we touch 50 fresh distinct
    /// tenant_ids through the SAME get-or-insert path the verify uses. We assert:
    ///   (a) the map never exceeds the cap (bounded — no unbounded creep), and
    ///   (b) the "active" tenant's exact `Arc<Semaphore>` is still resident and
    ///       identical (same allocation) — contention shields it from eviction.
    #[tokio::test]
    async fn per_tenant_map_is_bounded_but_keeps_contended_tenant() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        // Map cap 4; per-tenant cap 2; global plenty (irrelevant here — we
        // exercise per_tenant_semaphore directly).
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 2)
                .with_map_cap(4);

        // Pin an actively-contended tenant: create its bucket and HOLD one of
        // its permits, so it is NOT fully idle (available < per_tenant_cap) and
        // must never be evicted.
        let active = "active-tenant";
        let active_sem = verifier
            .per_tenant_semaphore(active)
            .expect("active tenant sem");
        let _hold = Arc::clone(&active_sem)
            .try_acquire_owned()
            .expect("hold one active permit");
        assert!(
            active_sem.available_permits() < 2,
            "active tenant has an in-flight permit ⇒ not idle"
        );
        let active_ptr = Arc::as_ptr(&active_sem);

        // Churn 50 distinct fresh tenants through the same path verify uses.
        for i in 0..50 {
            let t = format!("churn-tenant-{i}");
            let _ = verifier.per_tenant_semaphore(&t).expect("churn tenant sem");
            // (a) BOUNDED: the map never exceeds its cap under churn.
            assert!(
                verifier.per_tenant_map_len() <= 4,
                "map must stay bounded (≤ cap) — got {} at i={i}",
                verifier.per_tenant_map_len()
            );
        }

        // (b) The contended tenant survived the entire churn — same key AND the
        // SAME underlying semaphore allocation (never evicted+recreated).
        let still = verifier
            .per_tenant_semaphore(active)
            .expect("active tenant still resident");
        assert_eq!(
            Arc::as_ptr(&still),
            active_ptr,
            "an actively-contended tenant must NOT be evicted (same Arc)"
        );
        assert!(
            still.available_permits() < 2,
            "and its held permit is still accounted (no reset via eviction)"
        );
    }

    /// The shared synthetic dummy-burn bucket ([`UNKNOWN_TOKEN_BUCKET`]) must
    /// NEVER be evicted, even when it is fully idle and the LRU is over capacity
    /// and churning — evicting it would lose the bounded dummy-burn cap.
    #[tokio::test]
    async fn unknown_token_bucket_is_never_evicted() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 2)
                .with_map_cap(3);

        // Create the synthetic bucket (idle — no in-flight burn), exactly as the
        // None-row dummy-burn path does, then leave it untouched (LRU-stale).
        let dummy = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let dummy_ptr = Arc::as_ptr(&dummy);

        // Churn well past the cap. The dummy bucket is fully idle and becomes
        // the least-recently-used entry, yet must be exempt from eviction.
        for i in 0..30 {
            let t = format!("churn-{i}");
            let _ = verifier.per_tenant_semaphore(&t).expect("churn");
            assert!(verifier.per_tenant_map_len() <= 3, "bounded under churn");
        }

        let still = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket must still be resident");
        assert_eq!(
            Arc::as_ptr(&still),
            dummy_ptr,
            "UNKNOWN_TOKEN_BUCKET must never be evicted (same Arc)"
        );
    }

    // ── SingleFlightPatLookup (cold-hydrate PAT read-herd) ─────────────────────

    /// A lookup fake that counts inner calls, delays (to force burst overlap),
    /// and whose returned result is SWAPPABLE (to simulate revocation between
    /// reads — proving the single-flight is NOT a cache).
    struct SwitchableLookup {
        calls: Arc<AtomicUsize>,
        delay_ms: u64,
        result: Mutex<Result<Option<PatRow>, String>>,
    }
    impl SwitchableLookup {
        fn new(result: Result<Option<PatRow>, String>, delay_ms: u64) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                delay_ms,
                result: Mutex::new(result),
            }
        }
        fn set(&self, r: Result<Option<PatRow>, String>) {
            *self.result.lock().unwrap() = r;
        }
    }
    #[async_trait]
    impl PatRowLookup for SwitchableLookup {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            self.result.lock().unwrap().clone()
        }
    }

    #[tokio::test]
    async fn single_flight_coalesces_a_concurrent_burst_to_one_read() {
        let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 30));
        let calls = Arc::clone(&inner.calls);
        let sf = Arc::new(SingleFlightPatLookup::new(inner));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..24 {
            let s = Arc::clone(&sf);
            set.spawn(async move { s.lookup("tok-hot").await.unwrap() });
        }
        while let Some(r) = set.join_next().await {
            assert_eq!(r.unwrap(), Some(row("h", "t", "cas:rw")));
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "a parallel burst does ONE inner D1 read"
        );
    }

    #[tokio::test]
    async fn single_flight_is_not_a_cache_sequential_reads_are_fresh() {
        let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 0));
        let calls = Arc::clone(&inner.calls);
        let sf = SingleFlightPatLookup::new(inner);
        let _ = sf.lookup("tok").await.unwrap();
        let _ = sf.lookup("tok").await.unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "sequential reads are NOT cached"
        );
    }

    #[tokio::test]
    async fn single_flight_revocation_is_immediate() {
        // A resolved flight is never reused: after a row is returned, swapping the
        // inner to None (revocation) is visible on the very next read.
        let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 0));
        let calls = Arc::clone(&inner.calls);
        let inner_for_swap = Arc::clone(&inner);
        let sf = SingleFlightPatLookup::new(inner);
        assert_eq!(
            sf.lookup("tok").await.unwrap(),
            Some(row("h", "t", "cas:rw"))
        );
        inner_for_swap.set(Ok(None)); // revoke
        assert_eq!(
            sf.lookup("tok").await.unwrap(),
            None,
            "revocation is immediate (no cache)"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn single_flight_distinct_token_ids_do_not_coalesce() {
        let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 20));
        let calls = Arc::clone(&inner.calls);
        let sf = Arc::new(SingleFlightPatLookup::new(inner));
        let (a, b) = (Arc::clone(&sf), Arc::clone(&sf));
        let h1 = tokio::spawn(async move { a.lookup("tok-A").await.unwrap() });
        let h2 = tokio::spawn(async move { b.lookup("tok-B").await.unwrap() });
        let _ = h1.await.unwrap();
        let _ = h2.await.unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "different token_ids each read"
        );
    }

    #[tokio::test]
    async fn single_flight_backend_error_is_not_cached() {
        let inner = Arc::new(SwitchableLookup::new(Err("D1 down".to_owned()), 0));
        let calls = Arc::clone(&inner.calls);
        let inner_for_swap = Arc::clone(&inner);
        let sf = SingleFlightPatLookup::new(inner);
        assert!(sf.lookup("tok").await.is_err());
        inner_for_swap.set(Ok(Some(row("h", "t", "cas:rw"))));
        assert_eq!(
            sf.lookup("tok").await.unwrap(),
            Some(row("h", "t", "cas:rw")),
            "error not cached"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    // ---------------------------------------------------------------------
    // SecretMatchMemo — the Argon2id memo that lifts the adapter plane's
    // ~3 req/s throughput ceiling. The tests below are written to prove the
    // two claims that make it safe, not merely that it is fast:
    //   (1) a memo HIT genuinely skips Argon2id, and
    //   (2) it caches NO authorization state — revocation, scope changes and
    //       re-hashes all still take effect on the very next request.
    // ---------------------------------------------------------------------

    /// A `FakeLookup` holding several rows (the single-row `with_row` cannot
    /// serve the control arm of the permit-exhaustion test, which needs a
    /// SECOND, un-memoised PAT to resolve).
    fn fake_with_rows(pairs: Vec<(String, PatRow)>) -> FakeLookup {
        FakeLookup {
            rows: pairs.into_iter().collect(),
            calls: AtomicUsize::new(0),
            backend_err: None,
        }
    }

    /// A memo HIT must not run Argon2id.
    ///
    /// Proven behaviourally rather than by counting: we HOLD the verifier's one
    /// and only Argon2id permit, which makes any real Argon2id impossible (the
    /// acquire times out after `ARGON2_PERMIT_WAIT` into `Backend`). A verify
    /// that still SUCCEEDS under that condition provably never entered the
    /// Argon2id path. The second half is the control — without it, the test
    /// would pass just as happily if the permit had never actually been held.
    #[tokio::test]
    async fn memo_hit_skips_argon2id_proven_by_holding_the_only_permit() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 21, SCOPE_CACHE_RW);
        let (pt_other, tid_other, hash_other, tenant_other) = mint_pat(&key, 22, SCOPE_CACHE_RW);
        let lookup = Arc::new(fake_with_rows(vec![
            (tid, row(&hash, &tenant, "cas:rw")),
            (tid_other, row(&hash_other, &tenant_other, "cas:rw")),
        ]));
        // ONE global permit, so holding it drains the pool completely.
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 1);

        // Cold: runs the real Argon2id and memoises the match.
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);

        // Drain the pool and keep it drained for the rest of the test.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        // Warm: succeeds with ZERO permits available ⇒ no Argon2id ran.
        assert_eq!(
            verifier.verify(&pt).await.unwrap(),
            tenant,
            "a memoised PAT must verify without an Argon2id permit"
        );

        // CONTROL: a different, un-memoised PAT MUST fail-closed on the very
        // same drained pool — which is what proves the pool really was empty.
        let err = verifier.verify(&pt_other).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "un-memoised PAT must still need (and fail to get) a permit, got {err:?}"
        );
    }

    /// The memo holds no authorization state: revocation and scope changes in
    /// D1 take effect on the NEXT request, with no TTL window. This is the
    /// property that makes the adapter plane strictly stronger than the native
    /// plane's ≤5 s `native_pat_gate::VERIFY_CACHE_TTL`.
    #[tokio::test]
    async fn memo_caches_no_authorization_state() {
        let key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&key, 31, SCOPE_CACHE_RW);
        let lookup = Arc::new(SwitchableLookup::new(
            Ok(Some(row(&hash, &tenant, "cas:rw"))),
            0,
        ));
        let verifier = PatVerifier::new(lookup.clone(), key);

        // Warm the memo on a read-write PAT.
        let (_t, can_write) = verifier.verify_capability(&pt).await.unwrap();
        assert!(can_write);

        // Scope downgraded in D1 → the write bit must drop immediately.
        lookup.set(Ok(Some(row(&hash, &tenant, "cas:r"))));
        let (_t, can_write) = verifier.verify_capability(&pt).await.unwrap();
        assert!(
            !can_write,
            "a scope downgrade must apply on the next request (scope is never memoised)"
        );

        // Revoked in D1 (the SQL filter makes a revoked row indistinguishable
        // from an absent one) → rejected immediately, NOT after a TTL.
        lookup.set(Ok(None));
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "revocation must be immediate with a warm memo, got {err:?}"
        );
    }

    /// The stored PHC hash is part of the memo key, so re-hashing the row makes
    /// the old proof unreachable — the real Argon2id runs again and fails.
    #[tokio::test]
    async fn a_rehashed_row_is_never_served_from_the_memo() {
        let key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&key, 41, SCOPE_CACHE_RW);
        // A hash of a DIFFERENT secret — what a re-mint/rotation would write.
        let (_pt2, _tid2, foreign_hash, _tenant2) = mint_pat(&key, 42, SCOPE_CACHE_RW);
        let lookup = Arc::new(SwitchableLookup::new(
            Ok(Some(row(&hash, &tenant, "cas:rw"))),
            0,
        ));
        let verifier = PatVerifier::new(lookup.clone(), key);

        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);

        lookup.set(Ok(Some(row(&foreign_hash, &tenant, "cas:rw"))));
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "the memo must not survive a pat_hash change, got {err:?}"
        );
    }

    /// A PAT whose SECRET is correct but whose SCOPE is rejected must keep
    /// paying full Argon2id on every attempt — it must never be memoised.
    ///
    /// This is the oracle the memo's placement past the step-4 scope gate
    /// closes. If the memo were populated as soon as the Argon2id succeeded, a
    /// live-but-insufficiently-scoped credential would 401 slowly ONCE and then
    /// ~instantly forever, which separates it on latency alone from a dead /
    /// unknown / wrong-secret token — and separates a revoked PAT from a merely
    /// scope-downgraded one. Every rejection must stay indistinguishable.
    ///
    /// Proven the same way as the hit test, in reverse: hold the verifier's only
    /// Argon2id permit, then present the scope-rejected PAT again. If it were
    /// memoised it would skip Argon2id and fall through to the scope gate for a
    /// fast `InvalidPat`; because it is NOT, it must block on the drained pool
    /// and surface the overloaded `Backend` instead.
    #[tokio::test]
    async fn a_scope_rejected_pat_is_never_memoised() {
        let key = test_key();
        // `SCOPE_CACHE_R` mints a genuine PAT; the D1 row then carries a scope
        // string that grants NOTHING, which is what the step-4 gate rejects.
        let (pt, tid, hash, tenant) = mint_pat(&key, 61, SCOPE_CACHE_R);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 1);

        // First attempt: real Argon2id runs, scope gate rejects.
        assert!(matches!(
            verifier.verify(&pt).await.unwrap_err(),
            VerifyError::InvalidPat
        ));

        // Drain the pool: no Argon2id can run from here on.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        // Second attempt MUST still try to run Argon2id ⇒ overloaded, not a fast
        // 401. A fast `InvalidPat` here would mean the scope-rejected PAT had
        // been memoised — the oracle.
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a scope-rejected PAT must re-pay Argon2id every request, got {err:?}"
        );
    }

    /// …and a scope-rejected BURST must not memoise either.
    ///
    /// This is the intersection of the scope-gate placement above and cold-miss
    /// coalescing, and it is the one way coalescing could quietly undo that fix:
    /// the flight is where the Argon2id proof lands, so writing the memo there
    /// would be the natural thing to do — and would be exactly the placement
    /// `a_scope_rejected_pat_is_never_memoised` forbids, reintroduced for every
    /// member of a burst at once. The proof stays inside the flight; the memo
    /// write stays outside it, past the gate, per request.
    ///
    /// A burst also cannot straddle the gate: the flight key binds `scope` and
    /// `find_only`, so every joiner reaches its leader's verdict.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_scope_rejected_burst_leaves_the_memo_empty() {
        const BURST: usize = 8;
        let key = test_key();
        // A genuine PAT whose D1 row grants NOTHING — rejected at step 4, after
        // a perfectly successful Argon2id.
        let (pt, tid, hash, tenant) = mint_pat(&key, 62, SCOPE_CACHE_R);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            1,
            1,
        ));

        // Coalescing means the burst is admitted rather than shed (that is the
        // point of this PR), so every member reaches the scope gate and every
        // member must be a uniform 401 — never a fast one.
        assert_eq!(
            burst_observations(&verifier, &pt, BURST).await,
            vec![Observed::Unauthorized; BURST]
        );
        assert_eq!(
            verifier.secret_match_memo.len_for_test(),
            0,
            "not one member of a scope-rejected burst may leave a proof behind"
        );

        // And the behavioural check the placement test uses: with the pool
        // drained, the next attempt must still TRY to run Argon2id.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "after a scope-rejected burst the PAT must still re-pay Argon2id, got {err:?}"
        );
    }

    /// A scope DOWNGRADE must invalidate the memo, so a downgraded PAT stays
    /// indistinguishable from a revoked one.
    ///
    /// Without `scope` in the fingerprint, a PAT that verified successfully and
    /// was then downgraded (rather than revoked) would keep hitting the memo for
    /// the rest of the TTL and 401 in ~0 ms, while a REVOKED PAT still pays the
    /// slow dummy burn — telling the holder "downgraded, not revoked" on latency
    /// alone. Both are uniform on `main` and must stay uniform.
    ///
    /// Proven with the drained-permit technique: after the downgrade the verify
    /// must attempt a fresh Argon2id (⇒ `Backend` on an exhausted pool). A fast
    /// `InvalidPat` would mean the stale entry was still being served.
    #[tokio::test]
    async fn a_scope_downgrade_invalidates_the_memo() {
        let key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&key, 71, SCOPE_CACHE_RW);
        let lookup = Arc::new(SwitchableLookup::new(
            Ok(Some(row(&hash, &tenant, "cas:rw"))),
            0,
        ));
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);

        // Warm the memo on the fully-scoped PAT.
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);

        // Downgrade in D1 to a scope that grants nothing (NOT a revocation).
        lookup.set(Ok(Some(row(&hash, &tenant, ""))));

        // Drain the pool: no Argon2id can run from here on.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a downgraded PAT must miss the memo and re-pay Argon2id, got {err:?}"
        );
    }

    /// The memo key binds EVERY decision field — plaintext, token_id, the stored
    /// PHC hash, `scope` and `find_only` — and is unambiguous across them.
    ///
    /// `scope` / `find_only` are not incidental: dropping them back out
    /// reintroduces the downgrade-vs-revocation latency split that
    /// `a_scope_downgrade_invalidates_the_memo` exists to catch.
    #[test]
    fn secret_match_fingerprint_binds_every_decision_field() {
        let base = secret_match_fingerprint("pt", "tid", "hash", "cas:rw", false);
        assert_ne!(
            base,
            secret_match_fingerprint("pt-x", "tid", "hash", "cas:rw", false)
        );
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid-x", "hash", "cas:rw", false)
        );
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid", "hash-x", "cas:rw", false)
        );
        // scope + find_only are in the key so a DOWNGRADE (as opposed to a
        // revocation) makes the old entry unreachable instead of serving a
        // fast 401 that a revoked PAT would not get.
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid", "hash", "cas:r", false)
        );
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid", "hash", "cas:rw", true)
        );
        // Length-prefixed ⇒ no concatenation-boundary collision between two
        // different triples that share a flat concatenation.
        assert_ne!(
            secret_match_fingerprint("ab", "c", "d", "e", false),
            secret_match_fingerprint("a", "bc", "d", "e", false)
        );
        // A digest, never recoverable material. `!contains("pt")` would be
        // VACUOUS here — `p` and `t` are not hex digits, so it holds for any
        // hex string, including `hex::encode(plaintext)`. Assert the properties
        // that actually constrain the output instead.
        assert_eq!(base.len(), 64);
        assert!(base
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(base, hex::encode("pt"), "must not be the plaintext encoded");
    }

    /// The memo is bounded and TTL'd — it can neither grow without limit nor
    /// serve an aged entry.
    #[test]
    fn memo_is_bounded_and_ttl_expired_entries_are_not_hits() {
        let memo = SecretMatchMemo::new(2, Duration::from_secs(300));
        // Space the inserts so "oldest" is unambiguous rather than clock-jitter.
        memo.insert("a".to_owned());
        std::thread::sleep(Duration::from_millis(5));
        memo.insert("b".to_owned());
        std::thread::sleep(Duration::from_millis(5));
        memo.insert("c".to_owned());
        assert!(
            memo.len_for_test() <= 2,
            "the cap must bound the map, got {}",
            memo.len_for_test()
        );
        // Asserting `contains("c")` would be VACUOUS: `insert` places the new
        // key unconditionally AFTER the eviction pass, so it holds even with
        // the whole eviction branch deleted. What eviction actually DECIDES is
        // WHICH pre-existing entry dies — the oldest, never the newer one.
        assert!(!memo.contains("a"), "eviction must drop the OLDEST entry");
        assert!(memo.contains("b"), "eviction must keep the newer survivor");

        let expiring = SecretMatchMemo::new(8, Duration::ZERO);
        expiring.insert("x".to_owned());
        assert!(!expiring.contains("x"), "an expired entry is not a hit");
        assert_eq!(
            expiring.len_for_test(),
            0,
            "and is evicted on the read that observed it expired"
        );
    }

    // ---------------------------------------------------------------------
    // FlightGroup — the shared-future coalescer that collapses a burst of COLD
    // memo misses onto ONE Argon2id, on BOTH 401 arms.
    //
    // These first four tests pin the coalescer's own contract deterministically
    // (an explicit `Notify` gate holds every run open until the whole burst has
    // provably joined, so "exactly N runs" is an assertion, not a race). The
    // verifier-level tests after them prove the wiring and the security
    // properties that wiring has to preserve.
    // ---------------------------------------------------------------------

    /// Drive `callers` concurrent [`FlightGroup::run`] calls spread over `keys`
    /// distinct keys, holding every run open until ALL callers have joined, and
    /// return how many times the work actually ran.
    ///
    /// # Why this is deterministic and not a race
    ///
    /// Two properties do the work, and BOTH are needed:
    ///
    /// 1. **The caller runs on a CURRENT-THREAD runtime** (plain `#[tokio::test]`),
    ///    which is cooperative: a task is never preempted mid-poll. A caller
    ///    bumps `entered` and then executes `run`'s whole synchronous prologue —
    ///    the map lock, the join-or-lead decision, and (for a leader) the first
    ///    poll of the work — with no await in between, so all of that happens in
    ///    ONE uninterruptible poll. Observing `entered == callers` therefore
    ///    proves every caller has already joined or led. On a multi-thread
    ///    runtime it would prove nothing: a caller could be mid-prologue on
    ///    another core, and releasing the gate there could let the flight resolve
    ///    before that caller joins, which would inflate the run count.
    /// 2. **The gate is LEVEL-triggered** — a `Semaphore` opened at zero permits
    ///    and then CLOSED, so an `acquire()` that arrives after the close still
    ///    returns immediately. An edge-triggered `Notify::notify_waiters()` here
    ///    would be a lost-wakeup hazard: it wakes only whoever is registered at
    ///    that instant, so any straggler would hang forever.
    async fn count_flight_runs(group: Arc<FlightGroup<u32>>, callers: usize, keys: usize) -> usize {
        let runs = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Semaphore::new(0));

        let mut tasks = Vec::with_capacity(callers);
        for i in 0..callers {
            let (group, runs, entered, gate) = (
                Arc::clone(&group),
                Arc::clone(&runs),
                Arc::clone(&entered),
                Arc::clone(&gate),
            );
            let key = format!("key-{}", i % keys);
            tasks.push(tokio::spawn(async move {
                entered.fetch_add(1, Ordering::SeqCst);
                *group
                    .run(
                        &key,
                        move || {
                            async move {
                                runs.fetch_add(1, Ordering::SeqCst);
                                // Blocks until the test CLOSES the gate — never
                                // resolves early, so no caller can arrive to find a
                                // finished flight.
                                let _ = gate.acquire().await;
                                7u32
                            }
                            .boxed()
                        },
                        |m| panic!("flight aborted: {m}"),
                    )
                    .await
            }));
        }

        // Every caller has now joined or led (see (1) above) — release.
        while entered.load(Ordering::SeqCst) < callers {
            tokio::task::yield_now().await;
        }
        gate.close();

        for t in tasks {
            assert_eq!(
                t.await.expect("flight task"),
                7,
                "every joiner sees the run"
            );
        }
        runs.load(Ordering::SeqCst)
    }

    /// A concurrent burst on ONE key runs the work EXACTLY once — this is the
    /// whole mechanism: the other 15 callers pay nothing and consume no permit.
    #[tokio::test]
    async fn flight_group_collapses_a_concurrent_burst_to_one_run() {
        let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
        let runs = count_flight_runs(Arc::clone(&group), 16, 1).await;
        assert_eq!(runs, 1, "16 concurrent callers on one key must run ONCE");
    }

    /// …and DISTINCT keys never share a run. This is the bound on the
    /// mechanism: coalescing must never merge two different units of work (a
    /// different plaintext, or a different stored hash, is a different proof and
    /// must pay its own Argon2id).
    #[tokio::test]
    async fn flight_group_never_merges_distinct_keys() {
        let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
        let runs = count_flight_runs(Arc::clone(&group), 16, 4).await;
        assert_eq!(runs, 4, "16 callers over 4 keys must run exactly 4 times");
    }

    /// It is NOT a cache: a call that starts AFTER a flight resolved leads a
    /// fresh run rather than reusing the finished one. This is what keeps the
    /// per-request D1 row (revocation, expiry, scope) authoritative — a burst
    /// shares only the Argon2id comparison it was concurrent with.
    #[tokio::test]
    async fn flight_group_is_not_a_cache_a_later_call_runs_again() {
        let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
        assert_eq!(count_flight_runs(Arc::clone(&group), 4, 1).await, 1);
        // Second, strictly-later burst on the SAME key ⇒ a fresh run.
        assert_eq!(
            count_flight_runs(Arc::clone(&group), 4, 1).await,
            1,
            "a later burst must not be served by the resolved flight"
        );
    }

    /// The map retains only genuinely in-flight work: every resolved flight is
    /// retired by the first awaiter to observe it, so a long-lived container
    /// does not accumulate them.
    #[tokio::test]
    async fn flight_group_retires_every_resolved_flight() {
        let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
        let _ = count_flight_runs(Arc::clone(&group), 16, 8).await;
        assert_eq!(
            group.len_for_test(),
            0,
            "resolved flights must not be retained"
        );
    }

    // ---------------------------------------------------------------------
    // The cold-burst 503 — the bug this change exists to kill, and the two
    // oracles the fix must not open while killing it.
    // ---------------------------------------------------------------------

    /// The COLD burst: N simultaneous requests bearing the SAME PAT, all missing
    /// the memo, must all succeed.
    ///
    /// This is the `cargo -jN` opening burst against a cold container, scaled
    /// down: global pool = 1 and per-tenant sub-cap = 1, so the container can
    /// run exactly ONE Argon2id at a time. Production admits 4 per tenant and a
    /// real build opens with far more than 4 — same shape, same outcome.
    ///
    /// PROVEN RED without the fix (measured, with coalescing disabled): fails
    /// with `burst member 0 was SHED (Backend("pat verifier overloaded"))`. With
    /// each request running its own Argon2id the burst serialises on that single
    /// permit, so a member cannot start until the members ahead of it have
    /// finished a full Argon2id — and one Argon2id at the OWASP-2024 cost
    /// (`m=64 MiB, t=3, p=4`) far exceeds the 250 ms `ARGON2_PERMIT_WAIT`, so
    /// everything behind the first verify times out. With the fix all 8 share
    /// ONE Argon2id under ONE permit and all 8 resolve.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_cold_burst_of_one_pat_is_not_shed() {
        const BURST: usize = 8;
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 91, SCOPE_CACHE_RW);
        let (pt_other, tid_other, hash_other, tenant_other) = mint_pat(&key, 92, SCOPE_CACHE_RW);
        let lookup = Arc::new(fake_with_rows(vec![
            (tid, row(&hash, &tenant, "cas:rw")),
            (tid_other, row(&hash_other, &tenant_other, "cas:rw")),
        ]));
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            1,
            1,
        ));

        let mut tasks = Vec::with_capacity(BURST);
        for _ in 0..BURST {
            let (v, pt) = (Arc::clone(&verifier), pt.clone());
            tasks.push(tokio::spawn(async move { v.verify(&pt).await }));
        }
        for (i, task) in tasks.into_iter().enumerate() {
            match task.await.expect("burst task") {
                Ok(resolved) => assert_eq!(resolved, tenant, "burst member {i}"),
                Err(e) => panic!(
                    "burst member {i} was SHED ({e:?}) — {BURST} concurrent copies of ONE \
                     cold PAT must coalesce onto ONE Argon2id, not race for one permit"
                ),
            }
        }

        // CONTROL — without it this test would pass just as happily against a
        // verifier whose pool was never scarce. Hold the ONE global permit and
        // show a cold, un-memoised PAT is still shed: the pool really is 1 and
        // the 250 ms fail-CLOSED really does fire on this verifier.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();
        let err = verifier.verify(&pt_other).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "control: with the only permit held, a cold PAT must shed, got {err:?}"
        );
    }

    /// An ABANDONED flight must still release its Argon2id permit.
    ///
    /// Coalescing introduces a hazard uncoalesced code does not have. A `Shared`
    /// future is driven only by whoever polls it, and an unbursted request has
    /// exactly ONE awaiter — so a single client disconnect can leave the run
    /// with nobody to poll it. The map still holds a clone, so it is not dropped
    /// either; it is frozen wherever it was parked. The dangerous park is the
    /// per-tenant acquire, because the GLOBAL permit is already held there and
    /// the `tokio::time::timeout` guarding it cannot fire without being polled.
    /// Uncoalesced, dropping the request future drops the permit by RAII.
    ///
    /// `FlightGroup::run` closes this by SPAWNING the work, so the runtime owns
    /// it and it always finishes. This test drives the exact window: it saturates
    /// the tenant bucket so the verify parks holding the global permit, waits for
    /// that state, then aborts the only awaiter.
    ///
    /// PROVEN RED without the spawn: the permit is never returned and the test
    /// fails on its 5 s budget.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_abandoned_flight_still_releases_its_argon2id_permit() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 99, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // One global permit, one per-tenant permit.
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            1,
            1,
        ));

        // Hold the tenant's ONLY sub-permit, so a verify takes the global permit
        // and then parks in the per-tenant acquire — the pinning window.
        let sem = verifier
            .per_tenant_semaphore(&tenant)
            .expect("tenant semaphore");
        let _held = Arc::clone(&sem)
            .try_acquire_owned()
            .expect("the only tenant permit");

        let task = {
            let (v, pt) = (Arc::clone(&verifier), pt.clone());
            tokio::spawn(async move { v.verify(&pt).await })
        };

        // Wait for the flight to actually be parked holding the global permit.
        for _ in 0..500 {
            if verifier.argon2_permits.available_permits() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            verifier.argon2_permits.available_permits(),
            0,
            "precondition: the flight must be parked HOLDING the global permit"
        );

        // The only awaiter goes away — an ordinary client disconnect.
        task.abort();

        // The run is owned by the runtime, so its bounded wait still fires and
        // the permit comes back. Budget generously past ARGON2_PERMIT_WAIT.
        for _ in 0..200 {
            if verifier.argon2_permits.available_permits() == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("an abandoned flight pinned the global Argon2id permit — a cancelled request must never strand one");
    }

    /// The response "shape" a caller can actually observe on the wire, so the
    /// two 401 arms can be compared as data rather than by eye.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Observed {
        Ok,
        Unauthorized,
        Overloaded,
    }

    fn observe(r: &Result<String, VerifyError>) -> Observed {
        match r {
            Ok(_) => Observed::Ok,
            Err(VerifyError::InvalidPat) => Observed::Unauthorized,
            Err(VerifyError::Backend(_)) => Observed::Overloaded,
        }
    }

    /// Fire `burst` concurrent copies of ONE plaintext at `verifier` and return
    /// what each caller observed.
    async fn burst_observations(
        verifier: &Arc<PatVerifier>,
        plaintext: &str,
        burst: usize,
    ) -> Vec<Observed> {
        let mut tasks = Vec::with_capacity(burst);
        for _ in 0..burst {
            let (v, pt) = (Arc::clone(verifier), plaintext.to_owned());
            tasks.push(tokio::spawn(async move { observe(&v.verify(&pt).await) }));
        }
        let mut out = Vec::with_capacity(burst);
        for t in tasks {
            out.push(t.await.expect("burst task"));
        }
        out
    }

    /// THE ORACLE TEST (the finding that killed the mutex version).
    ///
    /// A burst of N copies of one WRONG-SECRET token must cost the same, and
    /// look the same, whether that token's `token_id` exists or not. If only the
    /// row-FOUND arm coalesced, the existing-`token_id` burst would serialise on
    /// the permits and shed while the unknown-`token_id` burst sailed through —
    /// making `token_id` liveness readable straight off the responses, in the
    /// concurrency dimension, which is precisely the enumeration oracle the
    /// dummy burn exists to close.
    ///
    /// PROVEN RED without the fix (measured, with coalescing disabled): arm A —
    /// `token_id` EXISTS, wrong secret, so every request pays its own failing
    /// Argon2id — serialises on the single permit and comes back
    /// `[Overloaded, Overloaded, Unauthorized, Overloaded, Overloaded,
    /// Overloaded, Overloaded, Overloaded]`, i.e. 7 of 8 shed, while arm B —
    /// `token_id` ABSENT — comes back `Unauthorized` 8 times out of 8. One burst,
    /// one bit: the token_id is live. With the fix both arms are 8/8
    /// `Unauthorized`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_concurrent_burst_cannot_reveal_whether_the_token_id_exists() {
        const BURST: usize = 8;
        let key = test_key();
        // Arm A: a valid-HMAC PAT whose token_id IS in D1 — but the stored hash
        // belongs to a different secret, so every request pays a full (failing)
        // Argon2id. This is the "known token_id, wrong secret" 401.
        let (pt_known, tid_known, _hash_known, tenant) = mint_pat(&key, 93, SCOPE_CACHE_RW);
        let (_pt_foreign, _tid_foreign, foreign_hash, _t) = mint_pat(&key, 94, SCOPE_CACHE_RW);
        // Arm B: a valid-HMAC PAT whose token_id is NOT in D1 at all — the
        // "unknown/expired/revoked token_id" 401, served by the dummy burn.
        let (pt_unknown, _tid_unknown, _h, _t2) = mint_pat(&key, 95, SCOPE_CACHE_RW);

        let lookup = Arc::new(fake_with_rows(vec![(
            tid_known,
            row(&foreign_hash, &tenant, "cas:rw"),
        )]));
        // One Argon2id at a time, per-tenant sub-cap 1 — which is ALSO the cap
        // on the shared UNKNOWN_TOKEN_BUCKET, so neither arm is given an
        // advantage the other lacks.
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            1,
            1,
        ));

        let arm_known = burst_observations(&verifier, &pt_known, BURST).await;
        let arm_unknown = burst_observations(&verifier, &pt_unknown, BURST).await;

        assert_eq!(
            arm_known,
            vec![
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
            ],
            "a burst of one wrong-secret token for a LIVE token_id must be a \
             uniform 401 — any shed here is the enumeration oracle"
        );
        assert_eq!(
            arm_known, arm_unknown,
            "the two 401 arms must be indistinguishable under a concurrent burst"
        );

        // …and a rejected flight must never leave a proof behind: the burst ran
        // a FAILING Argon2id, so the memo must still be empty.
        assert_eq!(
            verifier.secret_match_memo.len_for_test(),
            0,
            "a rejected verify must never populate the memo"
        );
    }

    /// The GLOBAL load-shed answers the SAME status on both 401 arms — pinned
    /// here at ZERO permits, the one saturation an acquire can never win.
    ///
    /// The asymmetry itself (row-FOUND ⇒ `Backend`/503, unknown-`token_id` ⇒
    /// `InvalidPat`/401, an enumeration oracle readable with no timing
    /// measurement at all) was closed on `main` by #1034 —
    /// `shed_is_indistinguishable_between_live_and_unknown_rows` above is its
    /// oracle test. What THIS test adds is that coalescing does not undo it:
    /// the acquire whose failure produces the shed now lives inside the flight
    /// future, and an unresolvable acquire there must still surface as the
    /// identical `Backend` on both arms.
    #[tokio::test]
    async fn both_401_arms_shed_alike_when_the_global_pool_is_exhausted() {
        let key = test_key();
        let (pt_known, tid, hash, tenant) = mint_pat(&key, 96, SCOPE_CACHE_RW);
        let (pt_unknown, _tid2, _h2, _t2) = mint_pat(&key, 97, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // ZERO global permits ⇒ the acquire can never succeed on either arm.
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 0);

        let known = verifier.verify(&pt_known).await;
        let unknown = verifier.verify(&pt_unknown).await;
        assert_eq!(observe(&known), Observed::Overloaded);
        assert_eq!(
            observe(&unknown),
            Observed::Overloaded,
            "an unknown token_id must shed with the SAME status as a live one — \
             the status must not encode whether the row exists"
        );
    }

    /// …and so does the PER-TENANT tier, which is what
    /// `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` (#1034) requires and what this test
    /// pins THROUGH the coalescer.
    ///
    /// `saturated_dummy_burn_bucket_sheds_an_unknown_row_as_backend` above pins
    /// the row-NOT-FOUND half on its own. This pins the PAIR on one verifier, at
    /// the per-tenant tier specifically, because that is the tier the coalescing
    /// rewrote: the acquires now live INSIDE the flight future, so a resolution
    /// that mapped a flight's per-tenant shed back to `InvalidPat` would
    /// re-open the row-existence oracle in the exact place the shed moved to.
    /// Each arm's own bucket is saturated (the shared
    /// [`UNKNOWN_TOKEN_BUCKET`] for the unknown row, the tenant's own bucket for
    /// the live one) with the global pool left ample, so ONLY the per-tenant
    /// tier can reject and both arms must answer identically.
    #[tokio::test]
    async fn both_401_arms_shed_alike_when_the_per_tenant_tier_is_saturated() {
        let key = test_key();
        let (pt_known, tid, hash, tenant) = mint_pat(&key, 98, SCOPE_CACHE_RW);
        let (pt_unknown, _tid2, _h2, _t2) = mint_pat(&key, 99, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Global pool ample (8) so ONLY the per-tenant tier can reject; every
        // per-tenant bucket has a sub-cap of 1.
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 1);

        // Saturate BOTH buckets: the shared synthetic one the dummy burn uses…
        let burn_bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let _burn_held = Arc::clone(&burn_bucket)
            .try_acquire_owned()
            .expect("the only dummy-bucket permit");
        // …and the live row's own tenant bucket.
        let tenant_bucket = verifier
            .per_tenant_semaphore(&tenant)
            .expect("tenant bucket");
        let _tenant_held = Arc::clone(&tenant_bucket)
            .try_acquire_owned()
            .expect("the only tenant permit");

        let known = verifier.verify(&pt_known).await;
        let unknown = verifier.verify(&pt_unknown).await;
        assert_eq!(
            observe(&known),
            Observed::Overloaded,
            "a per-tenant shed on the row-FOUND arm must be a 503, got {known:?}"
        );
        assert_eq!(
            observe(&unknown),
            Observed::Overloaded,
            "a per-tenant shed on the row-NOT-FOUND arm must be the SAME 503 — \
             the status must not encode whether the row exists, got {unknown:?}"
        );
        match (&known, &unknown) {
            (Err(VerifyError::Backend(a)), Err(VerifyError::Backend(b))) => assert_eq!(
                a, b,
                "the two per-tenant shed arms must be byte-identical too"
            ),
            other => panic!("both arms must shed as Backend, got {other:?}"),
        }

        // CONTROL — without it this test would pass against a verifier that
        // 503s everything. Release the buckets and the same two PATs must go
        // back to their ordinary verdicts (resolve / uniform 401).
        drop(_burn_held);
        drop(_tenant_held);
        assert_eq!(observe(&verifier.verify(&pt_known).await), Observed::Ok);
        assert_eq!(
            observe(&verifier.verify(&pt_unknown).await),
            Observed::Unauthorized,
            "outside saturation an unknown row must be the uniform 401, never a 503"
        );
    }

    /// THE ACCEPTED DIVERGENCE, pinned. `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` is
    /// scoped to saturation of a tier BOTH arms share — the global pool, or a
    /// per-tenant tier where each arm's own bucket is full. There is exactly one
    /// state in between, which neither
    /// `shed_is_indistinguishable_between_live_and_unknown_rows` (it drains the
    /// GLOBAL pool) nor
    /// `both_401_arms_shed_alike_when_the_per_tenant_tier_is_saturated` (it
    /// saturates BOTH per-tenant buckets) reaches:
    ///
    /// > the shared [`UNKNOWN_TOKEN_BUCKET`] is saturated while the live
    /// > tenant's OWN bucket is free and the global pool is ample.
    ///
    /// There the two arms legitimately diverge — the unknown/expired/revoked
    /// `token_id` sheds `Backend("pat verifier overloaded")` (⇒ 503) while a
    /// live PAT routes to its own unsaturated bucket, pays a real Argon2id and
    /// resolves. That is not a regression of the uniformity rule; it is the
    /// direct consequence of the finding-#12 defence that gives the row-NOT-FOUND
    /// path its own capped bucket precisely so a leaked-key flood across bogus
    /// `token_id`s cannot drain the pool real tenants need.
    ///
    /// ⚠️ DO NOT "FIX" THIS INTO UNIFORMITY. It is accepted, and here is why it
    /// is not an exploitable row-existence oracle:
    ///
    ///  - **Reaching either arm requires the PAT signing key.** Both arms sit
    ///    downstream of the HMAC fast-reject (step 1 of
    ///    [`PatVerifier::verify_capability`]); without the key every probe is an
    ///    `InvalidPat` that never touches D1, a permit, or a bucket — see
    ///    `forged_token_does_not_touch_argon2_permits`. An attacker who holds
    ///    the signing key can mint valid PATs outright and has no use for a
    ///    liveness bit.
    ///  - **The one edge-unauthenticated surface collapses both arms anyway.**
    ///    OCI `/token` (`routes/oci.rs`) maps `InvalidPat` AND
    ///    `Backend(..)` to the same scrubbed `401 authentication failed (ref:…)`
    ///    envelope — pinned by `oci::tests::token_backend_fault_is_opaque_to_unauth_caller`
    ///    — so the divergence is not observable there even with the key.
    ///  - **Saturating the shared bucket is itself the flood the bucket exists
    ///    to bound**, and holding it saturated costs the attacker a sustained
    ///    valid-HMAC flood while learning nothing a `token_id`'s ordinary
    ///    latency signature would not already give (that axis is the dummy
    ///    burn's job — `INV-AUTH-CONSTANT-TIME-COLD-PAD`).
    ///
    /// What this test defends is the SHAPE of the state, so a future refactor
    /// cannot slide the live arm into the shared bucket (which would turn one
    /// bogus-token flood into a fleet-wide 503) or the unknown arm out of it
    /// (which would restore the drain vector) without turning this red.
    #[tokio::test]
    async fn a_saturated_shared_burn_bucket_sheds_only_the_unknown_arm() {
        let key = test_key();
        let (pt_live, tid, hash, tenant) = mint_pat(&key, 100, SCOPE_CACHE_RW);
        let (pt_unknown, _tid2, _h2, _t2) = mint_pat(&key, 101, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Ample global pool and the per-tenant sub-cap at its PRODUCTION value,
        // so the only thing that can reject is a bucket this test holds itself.
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            64,
            ARGON2_PER_TENANT_PERMITS,
        );

        // Saturate ONLY the shared synthetic bucket the dummy burn routes through.
        let burn_bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let held: Vec<_> = (0..ARGON2_PER_TENANT_PERMITS)
            .map(|_| {
                Arc::clone(&burn_bucket)
                    .try_acquire_owned()
                    .expect("burn-bucket permit")
            })
            .collect();
        assert_eq!(burn_bucket.available_permits(), 0, "burn bucket saturated");
        // …and prove the live tenant's OWN bucket is untouched, so an Ok below
        // cannot be explained by the live arm having been given headroom the
        // unknown arm lacked at the GLOBAL tier.
        let tenant_bucket = verifier
            .per_tenant_semaphore(&tenant)
            .expect("tenant bucket");
        assert_eq!(
            tenant_bucket.available_permits(),
            ARGON2_PER_TENANT_PERMITS,
            "the live tenant's own bucket must be FREE — that is the whole point"
        );
        assert!(
            verifier.argon2_permits.available_permits() >= ARGON2_PER_TENANT_PERMITS,
            "the global pool must stay ample so no shed can be attributed to it"
        );

        // Arm 1 — the unknown/revoked token_id sheds on the shared bucket.
        let unknown = verifier.verify(&pt_unknown).await;
        match &unknown {
            Err(VerifyError::Backend(m)) => assert_eq!(
                m, "pat verifier overloaded",
                "the shared-bucket shed must be the ordinary overloaded signal"
            ),
            other => panic!("expected Backend(pat verifier overloaded), got {other:?}"),
        }

        // Arm 2 — THE HALF THAT MAKES THIS A PIN RATHER THAN A TAUTOLOGY. In the
        // same instant, on the same verifier, a live PAT still resolves: it never
        // consults the shared bucket, so a bogus-token flood cannot 503 a paying
        // tenant.
        let live = verifier.verify(&pt_live).await;
        assert_eq!(
            observe(&live),
            Observed::Ok,
            "a saturated dummy-burn bucket must NOT shed a live PAT, got {live:?}"
        );
        assert_eq!(live.expect("live PAT resolves"), tenant);

        // CONTROL — release the shared bucket and the unknown arm goes back to
        // the uniform 401. Without this the test would pass just as happily
        // against a verifier that 503s every unknown token unconditionally.
        drop(held);
        assert_eq!(
            observe(&verifier.verify(&pt_unknown).await),
            Observed::Unauthorized,
            "outside saturation an unknown row must be the uniform 401, never a 503"
        );
    }

    /// THE ACQUIRE-ORDER PIN. `a_saturated_shared_burn_bucket_sheds_only_the_unknown_arm`
    /// above proves the shared bucket sheds the right arm; this proves the shed
    /// is FREE, which is the property finding #12 was actually claiming.
    ///
    /// The bucket caps concurrent dummy BURNS at the sub-cap by construction.
    /// It does NOT, by construction, cap this arm's occupancy of the GLOBAL
    /// pool: with the original global-then-bucket order, a request already
    /// destined to shed first took a global permit and then queued on the full
    /// bucket for the whole `ARGON2_PERMIT_WAIT`, holding pool capacity hostage
    /// for 250 ms in order to burn nothing. At the rate the bucket itself
    /// admits, that pinned essentially the entire pool — the drain the bucket
    /// exists to close, merely displaced one step upstream. Taking the bucket
    /// FIRST, and non-blockingly, is what turns the sub-cap into a real bound on
    /// the arm's global footprint.
    ///
    /// So a status assertion cannot carry this test: both orders shed, and both
    /// shed with the same message. What separates them is whether the pool was
    /// OCCUPIED while they did it, which is measured here directly — first for
    /// one request, then for a flood.
    ///
    /// ⚠️ A note on what this test deliberately does NOT assert, because the
    /// obvious formulation is a coin flip: "a concurrent live PAT still
    /// resolves" does not separate the two orders. `tokio::sync::Semaphore` is
    /// FIFO-fair, so a live request that queues behind the flood waits for
    /// exactly ONE release — and the pre-fix arm holds its permit for exactly
    /// `ARGON2_PERMIT_WAIT`, the same budget the live request is waiting under.
    /// Measured, the pre-fix order let the live PAT through about half the
    /// time. What a shedding request provably costs is CAPACITY, so capacity is
    /// what gets asserted: an outside consumer must be able to take the whole
    /// pool while the flood is in flight.
    ///
    /// PROVEN RED against the pre-fix order (measured, by reverting the arm to
    /// the bounded `acquire` after the global permit): phase 1 reports
    /// `parked the global permit on 30-38 of 40 samples`, and with phase 1
    /// neutralised phase 2 reports `only 0 of 4 global permits were free while
    /// 4 bogus token_ids shed` on 3 runs of 3. Both are green on 3 runs of 3
    /// with the fix.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_shed_on_the_shared_burn_bucket_holds_no_global_permit() {
        const FLOOD: usize = 4;
        let key = test_key();
        let (pt_live, tid, hash, tenant) = mint_pat(&key, 102, SCOPE_CACHE_RW);
        // FLOOD + 1 DISTINCT bogus PATs. Distinct plaintexts ⇒ distinct flight
        // keys ⇒ the coalescer cannot fold them into ONE burn, which is what
        // makes this a flood rather than one request wearing N hats.
        let (pt_probe, _tid_p, _h_p, _t_p) = mint_pat(&key, 200, SCOPE_CACHE_RW);
        let flood_pats: Vec<String> = (0..FLOOD)
            .map(|i| mint_pat(&key, 201 + i as u128, SCOPE_CACHE_RW).0)
            .collect();
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Global pool exactly FLOOD, so the flood is *able* to take all of it,
        // and a per-bucket sub-cap of one so a single held permit saturates the
        // shared burn bucket.
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            FLOOD,
            1,
        ));
        let burn_bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let held = Arc::clone(&burn_bucket)
            .try_acquire_owned()
            .expect("the only burn-bucket permit");
        assert_eq!(burn_bucket.available_permits(), 0, "burn bucket saturated");

        // ── PHASE 1: one request, sampled. It meets the saturated bucket and
        // is going to shed. Watch the pool across a 200 ms window — far wider
        // than a correct shed needs, and well inside the 250 ms the pre-fix
        // order would have parked for. The pool must stay whole throughout.
        let probe = {
            let (v, pt) = (Arc::clone(&verifier), pt_probe.clone());
            tokio::spawn(async move { observe(&v.verify(&pt).await) })
        };
        let mut parked = 0usize;
        for _ in 0..40 {
            if verifier.argon2_permits.available_permits() != FLOOD {
                parked += 1;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            probe.await.expect("probe task"),
            Observed::Overloaded,
            "precondition: the probe must actually have SHED on the shared bucket"
        );
        assert_eq!(
            parked, 0,
            "parked the global permit on {parked} of 40 samples — a request \
             destined to shed must never occupy the pool"
        );

        // ── PHASE 2: the flood, and the property stated as capacity. FLOOD
        // distinct bogus token_ids, all shedding on the saturated bucket, are
        // in flight. An outside consumer — standing in for the concurrent live
        // traffic the pool exists to serve — must be able to take EVERY global
        // permit, because a shed holds none.
        let flood: Vec<_> = flood_pats
            .iter()
            .map(|pt| {
                let (v, pt) = (Arc::clone(&verifier), pt.clone());
                tokio::spawn(async move { observe(&v.verify(&pt).await) })
            })
            .collect();
        // Sampled INSIDE the pre-fix park window (250 ms), so the two orders
        // are distinguished by capacity and not by who woke up first.
        tokio::time::sleep(Duration::from_millis(30)).await;
        let grabbed: Vec<_> = (0..FLOOD)
            .filter_map(|_| {
                Arc::clone(&verifier.argon2_permits)
                    .try_acquire_owned()
                    .ok()
            })
            .collect();
        assert_eq!(
            grabbed.len(),
            FLOOD,
            "only {} of {FLOOD} global permits were free while {FLOOD} bogus \
             token_ids shed — requests that ran no Argon2id at all were holding \
             the pool a paying tenant needs",
            grabbed.len()
        );
        drop(grabbed);
        for (i, t) in flood.into_iter().enumerate() {
            assert_eq!(
                t.await.expect("flood task"),
                Observed::Overloaded,
                "flood request {i} must still shed as the uniform overloaded signal"
            );
        }

        // CONTROLS. (a) The pool is genuinely usable, not merely idle: a live
        // PAT resolves with the burn bucket still saturated…
        let live = verifier.verify(&pt_live).await;
        assert_eq!(observe(&live), Observed::Ok, "live PAT must resolve");
        assert_eq!(live.expect("live PAT resolves"), tenant);
        // …and (b) releasing the bucket returns the unknown arm to the uniform
        // 401, so none of the above is a verifier that just 503s everything.
        drop(held);
        assert_eq!(
            observe(&verifier.verify(&pt_probe).await),
            Observed::Unauthorized,
            "outside saturation an unknown row must be the uniform 401, never a 503"
        );
    }
}
