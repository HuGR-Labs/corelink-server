impl BloomTombstoneStore {
    /// Wrap `inner` with the default bloom geometry + staleness window + LRU cap.
    #[must_use]
    pub fn new(inner: Arc<dyn TombstoneStore>) -> Self {
        Self::with_params(
            inner,
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            DEFAULT_BLOOM_REFRESH,
        )
    }

    /// Wrap `inner` with explicit bloom geometry + staleness window (tests +
    /// tuning), using the default tenant-bloom LRU cap. `bits`/`k` are clamped
    /// to sane minimums by [`Bloom::new`].
    #[must_use]
    pub fn with_params(
        inner: Arc<dyn TombstoneStore>,
        bits: usize,
        k: u32,
        refresh: Duration,
    ) -> Self {
        Self::with_params_capped(inner, bits, k, refresh, DEFAULT_MAX_TENANT_BLOOMS)
    }

    /// Wrap `inner` with explicit bloom geometry, staleness window, AND
    /// tenant-bloom LRU cap (finding H3 — the `max_tenants` bound). A test can
    /// pass a tiny `max_tenants` to drive the eviction path deterministically.
    #[must_use]
    pub fn with_params_capped(
        inner: Arc<dyn TombstoneStore>,
        bits: usize,
        k: u32,
        refresh: Duration,
        max_tenants: usize,
    ) -> Self {
        Self {
            inner,
            tenants: Mutex::new(std::collections::HashMap::new()),
            // Never let a tuning/configuration value exceed the geometry
            // charged by the shared capacity contract.
            bits: bits.min(DEFAULT_BLOOM_BITS),
            k,
            refresh,
            // A zero cap would defeat the cache AND wedge the eviction loop;
            // clamp to at least 1 live bloom.
            max_tenants: max_tenants.max(1).min(DEFAULT_MAX_TENANT_BLOOMS),
            tick: AtomicU64::new(0),
            bloom_count: AtomicU64::new(0),
        }
    }

    /// Current number of live per-tenant blooms — the map-size gauge (H3).
    /// Lock-free (reads the atomic maintained under the map lock).
    #[must_use]
    pub fn tenant_bloom_count(&self) -> u64 {
        self.bloom_count.load(Ordering::Relaxed)
    }

    /// Inner-store key for a `(tenant, digest)` membership bit. Tenant is folded
    /// into the bloom key (not just the digest) so blooms are keyed per
    /// `(tenant, digest)` and one tenant's erasures can never mask or surface
    /// another's. (Blooms are ALSO partitioned per tenant in the map, so this
    /// is belt-and-braces.)
    fn bloom_key(tenant: &str, digest: &str) -> String {
        // A length-prefixed join avoids ambiguity between e.g. ("ab","c") and
        // ("a","bc").
        format!("{}:{tenant}/{digest}", tenant.len())
    }

    /// Synchronously fetch a still-fresh tenant bloom, if one exists. Returns
    /// `None` when the tenant has no bloom yet or its epoch is staler than the
    /// refresh window (⇒ the caller must (re)load via [`Self::reload_tenant_bloom`]).
    fn fresh_tenant_bloom(&self, tenant: &str) -> Option<Arc<TenantBloom>> {
        let mut g = lock_or_recover(&self.tenants);
        let tick = self.tick.fetch_add(1, Ordering::Relaxed);
        match g.get_mut(tenant) {
            Some(entry) if entry.bloom.loaded_at.elapsed() < self.refresh => {
                // Touch ⇒ most-recently-used (keeps a hot tenant from eviction).
                entry.last_access = tick;
                Some(Arc::clone(&entry.bloom))
            }
            _ => None,
        }
    }

    /// Evict the least-recently-accessed tenant bloom (finding H3). Caller holds
    /// the map lock. Unlike the PAT-permit LRU there is NO in-flight hazard: a
    /// bloom is a pure read-through cache, so dropping ANY tenant's bloom only
    /// forces its next lookup to reload authoritatively from D1 (invariant 1
    /// preserved via the reload re-seed). Evicts nothing on an empty map.
    fn evict_lru(map: &mut std::collections::HashMap<String, TenantBloomEntry>) {
        let victim = map
            .iter()
            .min_by_key(|(_, e)| e.last_access)
            .map(|(k, _)| k.clone());
        if let Some(k) = victim {
            tracing::debug!(tenant = %k, "cas_erase: evicting LRU tenant bloom (map at cap)");
            map.remove(&k);
        }
    }

    /// (Re)load a tenant's bloom, SEEDING it from the AUTHORITATIVE inner store
    /// (`list_tenant_tombstones`) so it reflects EVERY durable tombstone for the
    /// tenant — including ones written by another container instance OR by the
    /// separate erase-route `D1TombstoneStore` that never shares this bloom
    /// (F-010). The prior implementation re-seeded only from this instance's own
    /// in-process write history, so a cross-writer tombstone read as
    /// definitely-absent (a false negative) for the whole window after its first
    /// read — a GDPR no-false-negative invariant violation. Now the reload
    /// inserts the D1 set into a fresh bloom, so subsequent fast-path lookups of
    /// that digest correctly return maybe-present → fall through to the
    /// authoritative inner store → 410.
    ///
    /// Returns `(bloom, just_reloaded)`. On an inner-store error the bloom is
    /// still stamped fresh from this instance's own writes (carry-forward,
    /// monotone) and `just_reloaded=true` is returned so [`Self::is_tombstoned`]
    /// forces a one-shot authoritative fall-through — never a silent stale
    /// `Ok(false)` (invariant 1 / invariant 3 preserved on the degraded path).
    async fn reload_tenant_bloom(&self, tenant: &str) -> (Arc<TenantBloom>, bool) {
        // Read the authoritative durable set FIRST (no lock held across the
        // await). An error degrades to a carry-forward-only re-stamp.
        let seed = self.inner.list_tenant_tombstones(tenant).await;

        let mut g = lock_or_recover(&self.tenants);
        let prior = g.get(tenant).map(|e| Arc::clone(&e.bloom));
        let fresh = Arc::new(TenantBloom {
            bloom: Bloom::new(self.bits, self.k),
            loaded_at: Instant::now(),
        });
        // Carry forward this instance's own prior writes (monotone — never lose a
        // through-this-instance tombstone on re-stamp).
        if let Some(prev) = prior.as_ref() {
            for (dst, src) in fresh.bloom.words.iter().zip(prev.bloom.words.iter()) {
                dst.store(src.load(Ordering::Relaxed), Ordering::Relaxed);
            }
        }
        // Seed from the authoritative D1 set (the F-010 fix). On a store error we
        // keep only the carry-forward bits; the caller's just_reloaded=true still
        // forces an authoritative fall-through this turn.
        if let Ok(digests) = seed {
            for d in &digests {
                fresh.bloom.insert(&Self::bloom_key(tenant, d));
            }
        }
        // LRU cap (finding H3): enforce the bound BEFORE inserting a genuinely
        // NEW tenant key. Replacing an existing tenant's bloom (prior.is_some())
        // does not grow the map, so it never triggers eviction.
        if prior.is_none() && g.len() >= self.max_tenants {
            Self::evict_lru(&mut g);
        }
        let tick = self.tick.fetch_add(1, Ordering::Relaxed);
        g.insert(
            tenant.to_owned(),
            TenantBloomEntry {
                bloom: Arc::clone(&fresh),
                last_access: tick,
            },
        );
        // Refresh the map-size gauge under the lock (bounded by max_tenants).
        self.bloom_count.store(g.len() as u64, Ordering::Relaxed);
        (fresh, true)
    }
}

#[async_trait]
impl TombstoneStore for BloomTombstoneStore {
    async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String> {
        if tenant.len() > MAX_BLOOM_TENANT_ID_BYTES {
            return Err("tenant id exceeds bloom cache key bound".to_owned());
        }
        let (tb, just_reloaded) = match self.fresh_tenant_bloom(tenant) {
            Some(tb) => (tb, false),
            None => self.reload_tenant_bloom(tenant).await,
        };
        let key = Self::bloom_key(tenant, digest);

        if !just_reloaded && !tb.bloom.contains(&key) {
            // Fast path: definitely-absent in a fresh-enough bloom → no D1. Safe
            // because the fresh bloom was SEEDED from the authoritative D1 set
            // (F-010): a digest tombstoned by ANY writer before the last reload
            // is present in the bloom, so a definitely-absent answer is correct.
            return Ok(false);
        }
        // maybe-present OR a just-(re)loaded epoch → authoritative inner answer.
        // The just-reloaded fall-through bounds invariant 1; the bloom seed (the
        // F-010 fix) closes the within-window false-negative for cross-writer
        // tombstones written before the reload. Propagate the inner Result
        // UNCHANGED (invariant 3).
        self.inner.is_tombstoned(tenant, digest).await
    }

    async fn is_tombstoned_authoritative(
        &self,
        tenant: &str,
        digest: &str,
    ) -> Result<bool, String> {
        // WRITE-side gate (finding H3): consult the AUTHORITATIVE inner store
        // directly, bypassing the bloom fast-path ENTIRELY. A cross-instance /
        // cross-region erase written straight to D1 (via the erase route's own
        // `D1TombstoneStore`, which never seeds THIS instance's bloom) within
        // the ≤`refresh` staleness window would otherwise be invisible to this
        // bloom → a fast-path `Ok(false)` → a re-PUT that resurrects the
        // legally-erased bytes at the same content address. Reads tolerate that
        // window (invariant 1 — the bytes are already gone from R2), but a WRITE
        // must not: it always pays the one authoritative D1 read. Writes are far
        // rarer than reads and already do R2 + accounting work, so this is an
        // acceptable cost for the GDPR no-resurrection guarantee.
        self.inner.is_tombstoned(tenant, digest).await
    }

    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        reason: &str,
        erased_at_ms: i64,
    ) -> Result<bool, String> {
        if tenant.len() > MAX_BLOOM_TENANT_ID_BYTES {
            return Err("tenant id exceeds bloom cache key bound".to_owned());
        }
        // Synchronously record in the bloom BEFORE the inner write, so a
        // tombstone written through THIS instance is immediately visible to
        // this instance's reads (invariant 1, local-write arm). Setting the
        // bit before the inner write is conservative: if the inner write then
        // fails, the bloom merely has an extra maybe-present bit → an extra
        // (harmless) fall-through to the inner store, never a false 410.
        let tb = match self.fresh_tenant_bloom(tenant) {
            Some(tb) => tb,
            None => self.reload_tenant_bloom(tenant).await.0,
        };
        tb.bloom.insert(&Self::bloom_key(tenant, digest));
        self.inner
            .upsert(tenant, digest, reason, erased_at_ms)
            .await
    }

    async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String> {
        // Enumerate authoritatively from the inner store (pass-through).
        self.inner.list_tenant_tombstones(tenant).await
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tombstone-gated CAS handler decorator (F-004 — the shared CAS read/write seam)
// ──────────────────────────────────────────────────────────────────────────────

/// Sentinel carried in [`corelink_handler_cas::CasHandlerError::Internal`] by
/// [`TombstoneGatedCasHandler`] when a WRITE targets a tombstoned (erased)
/// `(tenant, hash)` — a re-PUT that would otherwise resurrect GDPR-erased bytes
/// (F-004). The CAS route's `map_err` maps this to HTTP 410 Gone (an erased
/// artifact must never be re-created at the same address). Mirrors the
/// `STORAGE_UNAVAILABLE_SENTINEL` / `OVER_CAP_SENTINEL` sentinel-tagging pattern
/// (the handler-error enum is `#[non_exhaustive]` with no dedicated `Gone`
/// variant, so the distinction is threaded through `Internal`).
pub const TOMBSTONE_GONE_SENTINEL: &str = "tombstone-gone: ";

/// Sentinel for a tombstone-gate lookup TRANSPORT fault on a read/write — maps to
/// 503 (fail-CLOSED): confidentiality of legally-erased data outweighs
/// availability of a live blob during a transient D1 blip, so the shared handler
/// must NOT serve/commit when it cannot consult the gate. Mirrors the route-level
/// fail-CLOSED on `handle_read` (cas.rs).
pub const TOMBSTONE_UNAVAILABLE_SENTINEL: &str = "tombstone-unavailable: ";

/// A drop-in CAS read/write/delete handler decorator that enforces the 410-Gone
/// erasure tombstone on the **shared CAS trait objects** — the single chokepoint
/// every CAS surface (native CAS, Bazel REAPI, OCI, and the cargo/brew/npm/pip
/// language adapters) drives. Wrapping the shared `Arc<dyn CasReadHandler>` /
/// `Arc<dyn CasWriteHandler>` here (in `routes::build_with_factory`, exactly
/// where [`crate::byte_accounting::AccountingCasHandler`] is wired) means ALL of
/// them inherit identical erasure semantics with no per-surface plumbing —
/// closing F-004 (the native route's inline 410 gate was the ONLY gate, so
/// erased bytes were re-PUT-able + readable through every other surface).
///
/// # Read
///
/// `read` consults the tombstone store BEFORE delegating: a tombstoned
/// `(tenant, hash)` is refused (never serve legally-erased bytes through any
/// surface). The error is plain [`corelink_handler_cas::CasHandlerError::NotFound`]
/// — the safe, surface-agnostic answer (every surface already maps `NotFound` to
/// its own 404-class). The NATIVE route keeps its own inline gate, which
/// short-circuits to a precise **410 Gone** BEFORE reaching this handler, so the
/// native UX is unchanged and this decorator is the cross-surface safety net.
/// `exists` inherits the same gate (a tombstoned blob reports absent).
///
/// # Write
///
/// `write` refuses a re-PUT of a tombstoned `(tenant, claimed_hash)` with an
/// [`TOMBSTONE_GONE_SENTINEL`]-tagged `Internal` (→ 410), so erased content
/// cannot be silently resurrected at the same content address by ANY write
/// surface (the prior write path was explicitly NOT gated). The write gate uses
/// the AUTHORITATIVE tombstone check ([`TombstoneStore::is_tombstoned_authoritative`]),
/// NOT the read-path bloom fast-path: a within-window cross-instance erase would
/// otherwise be invisible to a stale local bloom and the re-PUT would resurrect
/// the bytes (finding H3).
///
/// # Fail-CLOSED
///
/// A tombstone-lookup transport fault yields the [`TOMBSTONE_UNAVAILABLE_SENTINEL`]
/// (→ 503) on both read and write — never serve/commit when the gate is
/// unconsultable (PEN-2/REV-S1).
///
/// # Delete
///
/// `delete` is a pass-through: deleting an already-erased blob is a harmless
/// idempotent no-op, and DSR/erase deletes must never be blocked by a tombstone.
pub struct TombstoneGatedCasHandler {
    read: Arc<dyn corelink_handler_cas::CasReadHandler>,
    write: Arc<dyn corelink_handler_cas::CasWriteHandler>,
    delete: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    tombstones: Arc<dyn TombstoneStore>,
}

impl std::fmt::Debug for TombstoneGatedCasHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TombstoneGatedCasHandler")
            .field("tombstones", &self.tombstones)
            .finish_non_exhaustive()
    }
}

impl TombstoneGatedCasHandler {
    /// Wrap the shared read/write/delete handlers behind the `tombstones` gate.
    #[must_use]
    pub fn new(
        read: Arc<dyn corelink_handler_cas::CasReadHandler>,
        write: Arc<dyn corelink_handler_cas::CasWriteHandler>,
        delete: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
        tombstones: Arc<dyn TombstoneStore>,
    ) -> Self {
        Self {
            read,
            write,
            delete,
            tombstones,
        }
    }

    /// Synchronously resolve the tombstone gate for `(tenant, digest)`, bridging
    /// the async store onto the sync handler trait via `block_in_place` +
    /// `block_on` — the SAME pattern [`crate::byte_accounting::AccountingCasHandler`]
    /// uses for its async D1 accrual. Returns the gate decision as a tri-state:
    /// `Ok(true)` tombstoned, `Ok(false)` not, `Err` transport fault.
    fn is_tombstoned_blocking(&self, tenant: &str, digest: &str) -> Result<bool, String> {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| {
            handle.block_on(self.tombstones.is_tombstoned(tenant, digest))
        })
    }

    /// Like [`Self::is_tombstoned_blocking`] but AUTHORITATIVE — always consults
    /// the durable store, never a bloom fast-path negative. The WRITE gate uses
    /// this so a within-window cross-instance erase can NEVER be resurrected by
    /// a re-PUT (finding H3). See
    /// [`TombstoneStore::is_tombstoned_authoritative`].
    fn is_tombstoned_authoritative_blocking(
        &self,
        tenant: &str,
        digest: &str,
    ) -> Result<bool, String> {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| {
            handle.block_on(self.tombstones.is_tombstoned_authoritative(tenant, digest))
        })
    }
}

impl corelink_handler_cas::CasReadHandler for TombstoneGatedCasHandler {
    fn read(
        &self,
        req: corelink_handler_cas::CasReadRequest,
    ) -> Result<corelink_handler_cas::CasReadResponse, corelink_handler_cas::CasHandlerError> {
        match self.is_tombstoned_blocking(&req.tenant, &req.hash) {
            Ok(true) => Err(corelink_handler_cas::CasHandlerError::NotFound {
                tenant: req.tenant.clone(),
                hash: req.hash.clone(),
            }),
            Ok(false) => self.read.read(req),
            Err(e) => Err(corelink_handler_cas::CasHandlerError::Internal(format!(
                "{TOMBSTONE_UNAVAILABLE_SENTINEL}{e}"
            ))),
        }
    }

    fn exists(
        &self,
        req: corelink_handler_cas::CasReadRequest,
    ) -> Result<bool, corelink_handler_cas::CasHandlerError> {
        match self.is_tombstoned_blocking(&req.tenant, &req.hash) {
            // A tombstoned blob is absent (erased) — never report it present.
            Ok(true) => Ok(false),
            Ok(false) => self.read.exists(req),
            Err(e) => Err(corelink_handler_cas::CasHandlerError::Internal(format!(
                "{TOMBSTONE_UNAVAILABLE_SENTINEL}{e}"
            ))),
        }
    }
}

impl corelink_handler_cas::CasWriteHandler for TombstoneGatedCasHandler {
    fn write(
        &self,
        req: corelink_handler_cas::CasWriteRequest,
    ) -> Result<corelink_handler_cas::CasWriteResponse, corelink_handler_cas::CasHandlerError> {
        // Tombstone-gate the WRITE: a re-PUT of an erased blob must NOT resurrect
        // legally-erased bytes at the same content address (F-004). Checked BEFORE
        // delegating to the (accounting-wrapped) write handler. Uses the
        // AUTHORITATIVE check (finding H3) so a cross-instance erase written to D1
        // within the bloom staleness window can never slip a re-PUT through a
        // stale fast-path `Ok(false)`.
        match self.is_tombstoned_authoritative_blocking(&req.tenant, &req.claimed_hash) {
            Ok(true) => Err(corelink_handler_cas::CasHandlerError::Internal(format!(
                "{TOMBSTONE_GONE_SENTINEL}re-PUT of erased (tenant, hash) refused"
            ))),
            Ok(false) => self.write.write(req),
            Err(e) => Err(corelink_handler_cas::CasHandlerError::Internal(format!(
                "{TOMBSTONE_UNAVAILABLE_SENTINEL}{e}"
            ))),
        }
    }
}

impl corelink_handler_cas::CasDeleteHandler for TombstoneGatedCasHandler {
    fn delete(
        &self,
        req: corelink_handler_cas::CasDeleteRequest,
    ) -> Result<corelink_handler_cas::CasDeleteResponse, corelink_handler_cas::CasHandlerError>
    {
        // Pass-through: deleting an already-tombstoned blob is an idempotent
        // no-op, and DSR/erase deletes must never be gated.
        self.delete.delete(req)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// In-memory fakes (tests + the pre-#254 unmounted default)
// ──────────────────────────────────────────────────────────────────────────────

/// In-memory tombstone store (tests).
#[derive(Debug, Default)]
pub struct InMemoryTombstoneStore {
    set: Mutex<HashSet<(String, String)>>,
}

impl InMemoryTombstoneStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronously seed a tombstone (test helper for the read-gate fixtures
    /// in [`crate::routes::cas`]; the in-memory store does no real I/O).
    #[cfg(test)]
    pub fn seed(&self, tenant: &str, digest: &str) {
        let mut g = lock_or_recover(&self.set);
        g.insert((tenant.to_owned(), digest.to_owned()));
    }
}

#[async_trait]
impl TombstoneStore for InMemoryTombstoneStore {
    async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String> {
        let g = lock_or_recover(&self.set);
        Ok(g.contains(&(tenant.to_owned(), digest.to_owned())))
    }

    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        _reason: &str,
        _erased_at_ms: i64,
    ) -> Result<bool, String> {
        let mut g = lock_or_recover(&self.set);
        let existed = !g.insert((tenant.to_owned(), digest.to_owned()));
        Ok(existed)
    }

    async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String> {
        let g = lock_or_recover(&self.set);
        Ok(g.iter()
            .filter(|(t, _)| t == tenant)
            .map(|(_, d)| d.clone())
            .collect())
    }
}

/// Lock a `Mutex`, recovering the guard if the lock was poisoned (a poisoned
/// in-memory test fake must not panic the handler — clippy denies unwrap/panic).
fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// In-memory blob eraser (tests). Records erased `(tenant, digest)` keys; a
/// real R2 deletion is composed via the #254 adapter in prod.
#[derive(Debug, Default)]
pub struct InMemoryBlobEraser {
    erased: Mutex<HashSet<(String, String)>>,
}

impl InMemoryBlobEraser {
    /// Construct an empty eraser.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Was `(tenant, digest)` erased? (test assertion helper).
    #[must_use]
    pub fn was_erased(&self, tenant: &str, digest: &str) -> bool {
        let g = lock_or_recover(&self.erased);
        g.contains(&(tenant.to_owned(), digest.to_owned()))
    }
}

#[async_trait]
impl CasBlobEraser for InMemoryBlobEraser {
    async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String> {
        let mut g = lock_or_recover(&self.erased);
        g.insert((tenant.to_owned(), digest.to_owned()));
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Production R2 blob eraser (the #254 seam, now filled)
// ──────────────────────────────────────────────────────────────────────────────

// Canonical CAS storage regions — the `<region>/` key-prefix segment.
//
// Re-used from the SINGLE SOURCE OF TRUTH (`crate::storage::region_map::CAS_REGIONS`)
// so the per-hash erase sweep, the DSR CAS adapter, and the DSR AC adapter can
// never drift. CAS is **one bucket**; the storage region lives in the key prefix
// `<region>/<tenant_prefix>/<digest>`. The container writes through a single
// global `R2_CAS_REGION`, but a per-hash erase sweeps all regions so it is
// robust to a deployment whose write region changed over time (cheap — a
// once-per-erase LIST). Superset-safety vs the colo map is gated by
// `region_map::tests::cas_regions_superset_of_all_colos`.
use crate::storage::region_map::CAS_REGIONS;

/// Default single CAS bucket; overridable via `R2_CAS_BUCKET` (non-prod).
/// Mirrors `routes::dsr::adapter_r2_cas::DEFAULT_CAS_BUCKET`.
const DEFAULT_CAS_BUCKET: &str = "corelink-cas-prod";

/// Length (chars) of the materialised tenant prefix in an R2 key.
#[cfg(test)]
const TENANT_PREFIX_LEN: usize = 16;

/// Production [`CasBlobEraser`] over R2.
///
/// Given `(tenant, digest)`, derives the tenant prefix the **same way the CAS
/// writer did** ([`crate::storage::r2_s3::R2CasHandler`]'s `r2_key`: parse the
/// tenant as a UUID and `derive_prefix(tdk, uuid)`, else the raw-padded
/// fallback) and, for each of the five CAS regions, LISTs
/// `<region>/<tenant_prefix>/<digest>` and DELETEs the matching object(s) via
/// [`crate::storage::r2_s3::R2S3Client`]. Idempotent — re-erasing an absent
/// blob is a no-op success (S3 `DeleteObject` semantics).
///
/// Key layout matches by construction: the LIST prefix is built with the SAME
/// `R2S3Client::blob_key(region, prefix, digest)` leading path the writer
/// keys under, so a key-derivation mismatch (the earlier `R2Ac` silent-no-op
/// class of bug) is impossible.
pub struct R2CasBlobEraser {
    /// Single CAS bucket name (e.g. `corelink-cas-prod`).
    cas_bucket: String,
    /// Tenant derivation key — required (fail-CLOSED without it).
    tdk: Arc<TenantDerivationKey>,
}

impl std::fmt::Debug for R2CasBlobEraser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact the TDK; surface only the bucket.
        f.debug_struct("R2CasBlobEraser")
            .field("cas_bucket", &self.cas_bucket)
            .field("tdk", &"[REDACTED]")
            .finish()
    }
}

impl R2CasBlobEraser {
    /// Construct over an explicit TDK + CAS bucket.
    #[must_use]
    pub fn new(tdk: Arc<TenantDerivationKey>, cas_bucket: String) -> Self {
        Self { cas_bucket, tdk }
    }

    /// Build the production eraser from env (`R2_TDK_HEX` + `R2_CAS_BUCKET`).
    /// `None` when the TDK is absent — fail-CLOSED, since without it the tenant
    /// (or `_public` sentinel) prefix cannot be derived and the erase could not
    /// address the stored objects. Reused by the `_public` revocation route
    /// ([`crate::routes::public_revoke`]) so both erase surfaces share one
    /// TDK-keyed derivation.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let tdk = load_tdk_from_env()?;
        let cas_bucket = crate::storage::env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
        let cas_region = crate::storage::env_or("R2_CAS_REGION", "iad");
        crate::storage::r2_s3::validate_cas_bucket_for_region(&cas_bucket, &cas_region).ok()?;
        Some(Self::new(Arc::new(tdk), cas_bucket))
    }

    /// Derive the 16-char tenant prefix the SAME way the CAS writer
    /// ([`crate::storage::r2_s3::R2CasHandler`]'s `r2_key`) did: parse the
    /// tenant as a UUID and `derive_prefix(tdk, uuid)`, else raw-pad/truncate
    /// the tenant string to 16 chars (the writer's non-UUID/dev fallback).
    /// Keeping the two derivations identical is what makes the erase key match
    /// the stored object **by construction**.
    fn tenant_prefix(&self, tenant: &str) -> Result<String, String> {
        // `_public` shared-dedup namespace (F3.2): NOT a UUID tenant. Its bytes
        // are stored under the reserved TDK-keyed sentinel prefix, NOT a
        // per-tenant HMAC. Derive via the SINGLE SOURCE the CAS writer uses
        // (`r2_s3::public_namespace_prefix`) so the public-revocation eraser
        // addresses the exact key the `_public` write path created. Placed
        // BEFORE the UUID parse (the sentinel is not a parseable UUID).
        if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            return Ok(crate::storage::r2_s3::public_namespace_prefix(&self.tdk));
        }
        if let Ok(uid) = Uuid::try_parse(tenant) {
            return Ok(derive_prefix(&self.tdk, uid).to_string());
        }
        // Non-UUID tenant: FAIL CLOSED on the prod erasure path. A degraded
        // truncate+pad prefix collapses non-derivable tenants into a SHARED
        // keyspace — on the GDPR erasure path that risks erasing under (or
        // missing) the wrong tenant's prefix. Same fail-closed posture as the
        // CAS/AC storage layer (audit 2026-06-15 #2/#8). cfg(test) keeps the
        // deterministic pad for fixtures only.
        #[cfg(not(test))]
        {
            Err(format!(
                "non-derivable tenant '{tenant}' on R2 CAS erase — refusing degraded prefix (fail-closed)"
            ))
        }
        #[cfg(test)]
        {
            let mut p = tenant.to_owned();
            p.truncate(TENANT_PREFIX_LEN);
            while p.len() < TENANT_PREFIX_LEN {
                p.push('0');
            }
            Ok(p)
        }
    }
}

#[async_trait]
impl CasBlobEraser for R2CasBlobEraser {
    async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String> {
        let prefix = self.tenant_prefix(tenant)?;
        let env = crate::storage::StorageEnv::from_env()
            .ok_or_else(|| "StorageEnv unavailable for R2 CAS erase".to_owned())?;
        let client = crate::storage::r2_s3::R2S3Client::new(&env, self.cas_bucket.clone()).await?;
        for region in CAS_REGIONS {
            // The LIST prefix is the EXACT whole-blob key for this digest:
            // `<region>/<tenant_prefix>/<digest>` (per `R2S3Client::blob_key`).
            // LISTing it (rather than a bare DELETE) lets a re-erase of an
            // absent blob short-circuit and stays robust if a future multipart
            // path ever keys companion objects under the same digest prefix.
            // Native BLAKE3 keyspace — correct by design for this per-blob
            // erase: it is the native-CAS single-blob DELETE surface (clw D-1),
            // reached only with a BLAKE3 digest. Bazel REAPI exposes no
            // per-blob delete, and the Bazel `bazel/sha256/` keyspace is fully
            // covered by the GDPR Art.17 full-tenant erasure, which is
            // prefix-wide (`<region>/<tenant_prefix>/` → deletes everything
            // beneath, including `…/bazel/sha256/*`; see
            // `dsr/adapter_r2_cas.rs::list_and_delete_cas`). So a tenant wipe
            // leaves no Bazel residue; this path stays native-keyspace-scoped.
            let key = crate::storage::r2_s3::R2S3Client::blob_key(
                region,
                &prefix,
                digest,
                corelink_handler_cas::DigestAlgo::Blake3,
            );
            let keys = client.list_objects_v2(&key).await?;
            for k in keys {
                client.delete(&k).await?;
            }
        }
        Ok(())
    }
}

/// Build the route state from env.
///
/// Returns `Some` only when ALL of the prod transports build from env: the
/// **R2 TDK** (`R2_TDK_HEX`), the **D1 tombstone store**, the **D1 DSR
/// legitimacy store**, and the supplied **internal-auth key**. Any missing
/// piece ⇒ `None` and the route is NOT
/// mounted (fail-CLOSED): the container never runs a half-built erase that
/// could drop the tombstone without deleting the bytes, or — the load-bearing
/// failure mode — derive the WRONG R2 key and silently no-op the deletion
/// while writing a 410 tombstone (bytes-still-resident DSR breach). Without a
/// TDK the eraser cannot address the tenant's objects, so it MUST NOT be
/// constructed (mirrors the DSR `adapter_r2_cas` `load_tdk()` fail-CLOSED).
#[must_use]
pub fn build_state_from_env(internal_auth_key: Option<Arc<str>>) -> Option<CasEraseRouteState> {
    let internal_auth_key = internal_auth_key?;

    // Fail-CLOSED without a TDK: we cannot derive the tenant prefix the writer
    // used, so we cannot prove which R2 objects to delete (mirrors DSR).
    let tdk = load_tdk_from_env()?;

    // D1-backed tombstone store; absent D1 env ⇒ unmounted.
    let tombstones: Arc<dyn TombstoneStore> = Arc::new(D1TombstoneStore::from_env()?);

    // D1-backed DSR legitimacy gate (rt-nuclear #18/#19, r34 #8/#9). Fail
    // CLOSED in prod: if the legitimacy store cannot be built (no D1), the
    // route is NOT mounted — same posture as the erase-key/TDK gate above. We
    // MUST NOT mount the irreversible erase route without a legitimacy anchor,
    // or a leaked internal key would be sufficient to erase arbitrary blobs.
    let legitimacy: Arc<dyn DsrLegitimacyStore> = Arc::new(D1DsrLegitimacyStore::from_env()?);

    let cas_bucket = crate::storage::env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
    let cas_region = crate::storage::env_or("R2_CAS_REGION", "iad");
    crate::storage::r2_s3::validate_cas_bucket_for_region(&cas_bucket, &cas_region).ok()?;
    let eraser: Arc<dyn CasBlobEraser> = Arc::new(R2CasBlobEraser::new(Arc::new(tdk), cas_bucket));

    Some(CasEraseRouteState {
        tombstones,
        eraser,
        internal_auth_key,
        legitimacy: Some(legitimacy),
    })
}

/// Load the tenant derivation key from `R2_TDK_HEX` (64 hex chars = 32 bytes).
///
/// Mirrors `crate::storage::r2_s3::load_tdk_from_env` /
/// `routes::dsr::d1util::load_tdk` byte-for-byte (same env var, same length
/// gate, same hex decode) so the prefix this eraser derives matches the one
/// the writer/DSR adapter derive. Returns `None` (fail-CLOSED) when the var is
/// absent, the wrong length, or not valid hex.
fn load_tdk_from_env() -> Option<TenantDerivationKey> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        tracing::warn!(
            len = hex_str.len(),
            "cas_erase: R2_TDK_HEX has wrong length; eraser NOT built (route unmounted)"
        );
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    if hex::decode_to_slice(hex_str, bytes.as_mut()).is_err() {
        tracing::warn!(
            "cas_erase: R2_TDK_HEX is not valid hex; eraser NOT built (route unmounted)"
        );
        return None;
    }
    Some(TenantDerivationKey::from_bytes(bytes))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("b126_m2_test_1_1.rs");
    include!("b126_m2_test_1_2.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_1_1_REANCHOR, B126_M2_TEST_1_2_REANCHOR];
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
