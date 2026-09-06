//! Bounded per-tenant Argon2 admission gate.
//!
//! B126-H2 symbol map: PerTenantGate, its LRU entries, and admission constants.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::container_capacity::ARGON2_VERIFY_PERMITS;

/// Process-wide cap on the number of Argon2id verifications running
/// concurrently (red-team finding #2, HIGH). Argon2id is deliberately
/// memory-hard: each verify allocates ~`m_cost` MiB (64 MiB at the
/// production cost). With NO cap, a flood of concurrent `GET /token`
/// requests bearing a valid PAT fans out unbounded 64-MiB allocations on
/// `spawn_blocking` threads and OOM-kills the shared container — a registry
/// outage for ALL tenants. The count is derived from the deployed basic
/// container's dedicated Argon2id slice (see [`crate::container_capacity`]),
/// not from an obsolete instance-size assumption. At 64 MiB per verify this
/// pool admits one concurrent job and leaves the other process-wide slices
/// available to cache traffic.
///
/// Per-tenant sub-cap on concurrent Argon2id verifications (red-team finding
/// #1, HIGH — per-tenant fairness). The global [`ARGON2_VERIFY_PERMITS`] bound
/// alone is NOT fair: a single tenant flooding distinct PATs (each forcing a
/// fresh Argon2id verify) could take ALL of the global permits and 503 the
/// native plane for EVERY OTHER tenant. We add a per-tenant semaphore so no
/// single tenant can hold more than this many global permits at once — the
/// remaining global capacity stays available to serve other tenants.
///
/// Sizing: `max(1, ARGON2_VERIFY_PERMITS / 2)` = 1 at the production global cap
/// of 1. Rationale:
/// - A LEGITIMATE tenant's adapter (cargo/npm/oci/…) almost never needs more
///   than a couple of *simultaneous* in-flight Argon2id verifies — each request
///   verifies once, briefly, then the result is reused; one leaves forward
///   headroom for genuine bursts without starving the tenant itself.
/// - It caps any ONE tenant at the global pool's available work, and the FIFO
///   process gate prevents concurrent cross-tenant overcommit. The floor of 1
///   keeps a tenant able to make
///   forward progress.
pub(super) const ARGON2_PER_TENANT_PERMITS: usize = {
    let half = ARGON2_VERIFY_PERMITS / 2;
    if half > 1 {
        half
    } else {
        1
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
pub(super) const PER_TENANT_MAP_CAP: usize = 10_000;

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
pub(super) const UNKNOWN_TOKEN_BUCKET: &str = "\0argon2-dummy-burn-bucket\0";

/// How long a verify will wait for an Argon2id permit before declaring the
/// verifier overloaded. Short by design: a `/token` caller waiting longer
/// than this is better served a fast fail-CLOSED than a stalled request that
/// holds an async task (and its connection) hostage under a DoS flood.
pub(super) const ARGON2_PERMIT_WAIT: Duration = Duration::from_millis(250);

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
pub(super) struct PerTenantGate {
    /// `tenant_id -> entry`. Wrapped in a `Mutex` only to guard the map's
    /// get-or-insert; the guard is NEVER held across the Argon2id work or the
    /// async acquire (it is dropped before either). FAIL-SAFE: a poisoned lock
    /// falls back to global-only bounding (a bookkeeping fault must never block
    /// a legitimate auth) — see [`Self::semaphore`].
    pub(super) permits: Mutex<HashMap<String, PerTenantEntry>>,
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
    pub(super) fn new(cap: usize, map_cap: usize) -> Self {
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
    pub(super) fn semaphore(&self, tenant: &str) -> Option<Arc<Semaphore>> {
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
    pub(super) async fn acquire(&self, tenant: &str) -> Result<Option<OwnedSemaphorePermit>, ()> {
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
    pub(super) fn try_acquire(&self, tenant: &str) -> Result<Option<OwnedSemaphorePermit>, ()> {
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
