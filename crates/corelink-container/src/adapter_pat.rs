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
//!    PHC hash. Run on a blocking thread (Argon2id is CPU-heavy).
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

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use corelink_pat::{verify_hmac_only_multi, verify_with_hash_multi, PatHash, PatSigningKey};
use futures::future::{BoxFuture, FutureExt, Shared};
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
    #[error("invalid PAT")]
    InvalidPat,
    /// Verifier backend (D1, corrupt row) failed. Surfaces as HTTP 503.
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
const PAT_LOOKUP_SQL: &str = "SELECT tenant_id, pat_hash, scope FROM pat \
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

        Ok(Some(PatRow {
            tenant_id,
            pat_hash,
            scope,
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
                    let fut: SharedLookup =
                        async move { Arc::new(inner.lookup(&tid).await) }.boxed().shared();
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
/// sub-cap — so the unknown-token path can never starve real tenants. The key
/// is not a valid tenant UUID, so it cannot collide with a real tenant bucket.
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
    /// [`ARGON2_PER_TENANT_PERMITS`]. A verify acquires the global permit
    /// FIRST, then this tenant's permit (CONSISTENT order => no deadlock), so a
    /// single tenant flooding distinct PATs can hold at most
    /// `ARGON2_PER_TENANT_PERMITS` global permits at once -- leaving the rest of
    /// the global pool for other tenants. Wrapped in a `Mutex` only to guard
    /// the map's get-or-insert; the `Mutex` is NEVER held across the Argon2id
    /// work or the async acquire (it is dropped before either). FAIL-SAFE: a
    /// poisoned lock falls back to global-only bounding (a bookkeeping fault
    /// must never block a legitimate auth) -- see [`Self::per_tenant_semaphore`].
    ///
    /// BOUNDED (#1/#12 follow-up): the map is an LRU capped at
    /// [`Self::per_tenant_map_cap`]. Each entry carries its semaphore plus a
    /// monotonic last-access tick (the [`PerTenantEntry`]); on a get-or-insert
    /// that would exceed the cap, the least-recently-used **fully-idle** entry
    /// is evicted first (never an active/contended one, never the shared
    /// [`UNKNOWN_TOKEN_BUCKET`]). This caps the steady-state memory the map can
    /// retain without ever disrupting an in-flight verify.
    per_tenant_permits: Arc<Mutex<HashMap<String, PerTenantEntry>>>,
    /// Monotonic clock for the LRU recency order on `per_tenant_permits`. Bumped
    /// on every get-or-insert; the smallest tick is the least-recently-used.
    /// `u64` never realistically wraps (one tick per verify ⇒ ~10^11 years at
    /// 1M/s), so a simple increment is sound.
    per_tenant_tick: Arc<AtomicU64>,
    /// The cap each per-tenant semaphore is created with
    /// ([`ARGON2_PER_TENANT_PERMITS`] in production). A field (not a const) so a
    /// test can shrink it to drive the two-tier interaction deterministically.
    per_tenant_cap: usize,
    /// LRU capacity of the `per_tenant_permits` map
    /// ([`PER_TENANT_MAP_CAP`] in production). A field (not a const) so a test
    /// can shrink it to drive the eviction path deterministically.
    per_tenant_map_cap: usize,
}

impl std::fmt::Debug for PatVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatVerifier")
            .field("lookup", &"Arc<dyn PatRowLookup>")
            .field("signing_keys", &format_args!("[{} REDACTED]", self.signing_keys.len()))
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
            per_tenant_permits: Arc::new(Mutex::new(HashMap::new())),
            per_tenant_tick: Arc::new(AtomicU64::new(0)),
            per_tenant_cap: ARGON2_PER_TENANT_PERMITS,
            per_tenant_map_cap: PER_TENANT_MAP_CAP,
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

    /// Resolve (get-or-create) the per-tenant Argon2id semaphore for `tenant`,
    /// for the two-tier fairness gate (finding #1).
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
    /// BOUNDED LRU (#1/#12 follow-up): the map is capped at
    /// [`Self::per_tenant_map_cap`]. A get bumps the entry's recency tick. An
    /// insert that would exceed the cap first evicts the least-recently-used
    /// entry **that is fully idle** — `available_permits() == per_tenant_cap`,
    /// i.e. no in-flight verify holds any of its permits — so eviction can never
    /// disrupt an active or contended tenant. The shared
    /// [`UNKNOWN_TOKEN_BUCKET`] is NEVER an eviction candidate. If no idle entry
    /// can be freed (every entry is in-flight — pathological, far beyond a 10k
    /// real-tenant working set), we skip eviction and let the map grow past the
    /// cap transiently rather than block/evict an active tenant; it shrinks back
    /// as those verifies complete and a later insert finds an idle victim.
    /// Re-inserting an evicted tenant simply recreates its (idle) semaphore —
    /// semantically identical.
    fn per_tenant_semaphore(&self, tenant: &str) -> Option<Arc<Semaphore>> {
        let mut map = match self.per_tenant_permits.lock() {
            Ok(guard) => guard,
            // Poisoned: a previous holder panicked while mutating the map. Do
            // NOT block auth on bookkeeping — fall back to global-only.
            Err(_poisoned) => return None,
        };
        let tick = self.per_tenant_tick.fetch_add(1, Ordering::Relaxed);
        if let Some(entry) = map.get_mut(tenant) {
            entry.last_access = tick; // touch ⇒ most-recently-used
            return Some(Arc::clone(&entry.sem));
        }
        // New tenant. Enforce the LRU cap BEFORE inserting: if at/over capacity,
        // evict the least-recently-used FULLY-IDLE entry (never the shared
        // UNKNOWN_TOKEN_BUCKET, never an in-flight tenant).
        if map.len() >= self.per_tenant_map_cap {
            self.evict_one_idle(&mut map);
        }
        let sem = Arc::new(Semaphore::new(self.per_tenant_cap));
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
    /// idle (`available_permits() == per_tenant_cap` ⇒ no in-flight verify holds
    /// a permit) and NOT the shared [`UNKNOWN_TOKEN_BUCKET`]. If no such entry
    /// exists, evict nothing (the map grows transiently past the cap rather than
    /// disrupt an active tenant). Caller holds the map lock.
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
            if entry.sem.available_permits() != self.per_tenant_cap {
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
    /// global→per-tenant ⇒ deadlock-free).
    async fn acquire_per_tenant(
        &self,
        tenant: &str,
    ) -> Result<Option<OwnedSemaphorePermit>, ()> {
        let Some(sem) = self.per_tenant_semaphore(tenant) else {
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
                // The dummy burn ALSO runs Argon2id (for timing parity), so it
                // must be bounded by the same gate — otherwise an attacker who
                // floods valid-HMAC tokens for NON-existent token_ids could
                // OOM the container exactly like the hot path. Acquire a permit
                // (bounded wait) before the blocking burn; on overload, skip
                // the burn and return the uniform InvalidPat. (The lost timing
                // parity under sustained overload is acceptable: the request
                // already shares its fate with every other overloaded one, so
                // there is no per-token oracle to exploit.)
                let permit = match tokio::time::timeout(
                    ARGON2_PERMIT_WAIT,
                    Arc::clone(&self.argon2_permits).acquire_owned(),
                )
                .await
                {
                    Ok(Ok(permit)) => permit,
                    // Acquire failed (timeout) or the semaphore was closed —
                    // skip the burn and fail-CLOSED uniformly.
                    _ => return Err(VerifyError::InvalidPat),
                };
                // Finding #12: cap the dummy-burn path through ONE shared
                // synthetic bucket (consistent global→per-tenant order) so a
                // leaked-key flood across bogus token_ids cannot drain the
                // global pool via this path. On sub-cap saturation OR fail-safe
                // fall-through we simply skip the burn and fail-CLOSED uniformly
                // (the lost timing parity under flood is acceptable — every
                // request shares its fate, so there is no per-token oracle).
                let _tenant_permit = match self.acquire_per_tenant(UNKNOWN_TOKEN_BUCKET).await {
                    Ok(maybe_permit) => maybe_permit,
                    Err(()) => return Err(VerifyError::InvalidPat),
                };
                let plaintext = pat_plaintext.to_owned();
                let _ = tokio::task::spawn_blocking(move || {
                    let r = corelink_pat::dummy_verify_for_constant_time(&plaintext);
                    drop(permit); // hold the global permit ONLY across the blocking work
                    drop(_tenant_permit); // release the synthetic-bucket permit too
                    r
                })
                .await;
                return Err(VerifyError::InvalidPat);
            }
        };

        // 3. Full crypto verify on a blocking thread (Argon2id is
        //    CPU-heavy and must not stall the async worker). Re-parses,
        //    constant-time matches token_id, re-checks HMAC, then
        //    Argon2id-verifies the secret segment against the stored hash.
        let plaintext = pat_plaintext.to_owned();
        let signing_keys = Arc::clone(&self.signing_keys);
        let stored_hash = PatHash::from_phc_string(row.pat_hash);
        // Finding #2: bound concurrent Argon2id. Acquire a permit (bounded
        // wait) BEFORE the blocking work; on acquire-timeout the verifier is
        // overloaded — return `Backend` (reusing the existing variant, which
        // the OCI `/token` handler and the native gate already map to a
        // non-leaky fail-CLOSED rejection) rather than letting the request pile
        // on more 64-MiB allocations. The permit is moved into the blocking
        // closure and dropped there, so it is held ONLY across the Argon2id.
        let permit = match tokio::time::timeout(
            ARGON2_PERMIT_WAIT,
            Arc::clone(&self.argon2_permits).acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_closed)) => {
                return Err(VerifyError::Backend("pat verifier overloaded".into()))
            }
            Err(_timeout) => {
                return Err(VerifyError::Backend("pat verifier overloaded".into()))
            }
        };
        // Finding #1: per-tenant fairness. With the GLOBAL permit already held,
        // acquire this tenant's sub-permit (consistent order global→per-tenant
        // ⇒ deadlock-free). If the tenant is at its sub-cap, fail-CLOSED with
        // the SAME overloaded signal as a global timeout — so ONE tenant
        // flooding distinct PATs cannot drain the whole global pool and starve
        // others. The `permit` (global) is dropped on this early return by
        // RAII, so a per-tenant rejection does NOT leak a global permit.
        // `Ok(None)` is the fail-safe fall-through to global-only bounding.
        let tenant_permit = match self.acquire_per_tenant(&row.tenant_id).await {
            Ok(maybe_permit) => maybe_permit,
            Err(()) => return Err(VerifyError::Backend("pat verifier overloaded".into())),
        };
        let verify_result = tokio::task::spawn_blocking(move || {
            let r = verify_with_hash_multi(&plaintext, &token_id, &stored_hash, &signing_keys);
            drop(permit); // release the global Argon2id permit the moment the work ends
            drop(tenant_permit); // release the per-tenant permit too (RAII, both paths)
            r
        })
        .await
        .map_err(|e| VerifyError::Backend(format!("verify join: {e}")))?;
        verify_result.map_err(|_| VerifyError::InvalidPat)?;

        // 4. Scope gate — fail-CLOSED on NO cache capability at all, then
        //    surface the read/write split so credential-minting callers can
        //    downscope.
        if !requires_cache_read(&row.scope) {
            return Err(VerifyError::InvalidPat);
        }
        let can_write = requires_cache_write(&row.scope);

        Ok((row.tenant_id, can_write))
    }

    /// Test-only: shrink the per-tenant LRU map cap so the eviction path can be
    /// driven deterministically (the production cap of 10k is too large to fill
    /// in a unit test). Returns `self` for chaining off a constructor.
    #[cfg(test)]
    #[must_use]
    fn with_map_cap(mut self, cap: usize) -> Self {
        self.per_tenant_map_cap = cap;
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
        self.per_tenant_permits.lock().expect("map lock").len()
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
            per_tenant_permits: Arc::new(Mutex::new(HashMap::new())),
            per_tenant_tick: Arc::new(AtomicU64::new(0)),
            per_tenant_cap: per_tenant_permits,
            per_tenant_map_cap: PER_TENANT_MAP_CAP,
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
        let verifier = PatVerifier::with_key_set(
            lookup.clone(),
            vec![(*new_key).clone(), (*old_key).clone()],
        );
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

    /// With permits available, the hot path still succeeds end-to-end — the
    /// gate is transparent under normal load (no regression to the verify
    /// pipeline).
    #[tokio::test]
    async fn verify_succeeds_when_permits_available() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 71, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 2);
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
        assert_eq!(lookup.call_count(), 1, "global gate sits after the D1 lookup");
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
        let map = Arc::clone(&verifier.per_tenant_permits);
        let _ = std::thread::spawn(move || {
            let _guard = map.lock().unwrap();
            panic!("intentional poison");
        })
        .join();
        assert!(
            verifier.per_tenant_permits.is_poisoned(),
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
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            8,
            1,
        );

        // Hold the tenant's single per-tenant permit (model PAT #1 in-flight).
        let sem = verifier.per_tenant_semaphore(&tenant).expect("tenant sem");
        let _hold = Arc::clone(&sem).try_acquire_owned().expect("hold the only permit");

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
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            8,
            2,
        )
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
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            8,
            2,
        )
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
            Self { calls: Arc::new(AtomicUsize::new(0)), delay_ms, result: Mutex::new(result) }
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
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a parallel burst does ONE inner D1 read");
    }

    #[tokio::test]
    async fn single_flight_is_not_a_cache_sequential_reads_are_fresh() {
        let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 0));
        let calls = Arc::clone(&inner.calls);
        let sf = SingleFlightPatLookup::new(inner);
        let _ = sf.lookup("tok").await.unwrap();
        let _ = sf.lookup("tok").await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "sequential reads are NOT cached");
    }

    #[tokio::test]
    async fn single_flight_revocation_is_immediate() {
        // A resolved flight is never reused: after a row is returned, swapping the
        // inner to None (revocation) is visible on the very next read.
        let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 0));
        let calls = Arc::clone(&inner.calls);
        let inner_for_swap = Arc::clone(&inner);
        let sf = SingleFlightPatLookup::new(inner);
        assert_eq!(sf.lookup("tok").await.unwrap(), Some(row("h", "t", "cas:rw")));
        inner_for_swap.set(Ok(None)); // revoke
        assert_eq!(sf.lookup("tok").await.unwrap(), None, "revocation is immediate (no cache)");
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
        assert_eq!(calls.load(Ordering::SeqCst), 2, "different token_ids each read");
    }

    #[tokio::test]
    async fn single_flight_backend_error_is_not_cached() {
        let inner = Arc::new(SwitchableLookup::new(Err("D1 down".to_owned()), 0));
        let calls = Arc::clone(&inner.calls);
        let inner_for_swap = Arc::clone(&inner);
        let sf = SingleFlightPatLookup::new(inner);
        assert!(sf.lookup("tok").await.is_err());
        inner_for_swap.set(Ok(Some(row("h", "t", "cas:rw"))));
        assert_eq!(sf.lookup("tok").await.unwrap(), Some(row("h", "t", "cas:rw")), "error not cached");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
