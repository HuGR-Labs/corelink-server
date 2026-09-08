/// Default bloom bit-array size in **bits** (`2^20` = 1 Mibit = 128 KiB). With
/// the default `k = 7` hashes this holds ~100 000 erased digests below a ~1 %
/// false-positive rate — and erasures are RARE (GDPR Art.17 + per-blob operator
/// erases), so a single tenant practically never approaches this. Bounded by
/// construction: the bit-array never grows (invariant 4).
const DEFAULT_BLOOM_BITS: usize = 1 << 20;

/// Default number of hash probes per element (Kirsch–Mitzenmacher double
/// hashing). `k = 7` is near-optimal for the default fill (~1 % FP at 100k
/// elements in a 1 Mibit array).
const DEFAULT_BLOOM_HASHES: u32 = 7;

/// Default per-tenant bloom refresh staleness window. After this elapses since
/// a tenant's bloom was last (re)loaded from the inner store, the NEXT
/// fast-path `definitely-not-present` answer for that tenant is downgraded to a
/// fall-through to the authoritative inner store, which both answers correctly
/// AND triggers a reload — so a tombstone written by ANOTHER container instance
/// becomes locally visible within at most this window (invariant 1).
const DEFAULT_BLOOM_REFRESH: Duration = Duration::from_secs(30);

/// Upper bound on the number of live per-tenant blooms the store retains
/// (finding H3 OOM fix). Each bloom is a fixed 128 KiB array
/// ([`DEFAULT_BLOOM_BITS`]); with NO bound the `tenants` map lazily mints one
/// per distinct tenant ever read/written and NEVER frees it — a slow
/// unbounded-memory creep (≈1.3 GB at 10 000 tenants) that OOM-kills the
/// memory-capped `basic` container (1 GiB / 0.25 vCPU). We cap it as an LRU: when the map is
/// full and a NEW tenant must be inserted, the least-recently-accessed bloom is
/// evicted first. Evicting a bloom is ALWAYS safe — it is a pure read-through
/// cache, so the next lookup for that tenant simply reloads (authoritatively)
/// from D1 (invariant 1 preserved: the reload re-seeds from the durable set).
/// 2048 blooms ⇒ ≤256 MiB of fixed bit arrays worst-case, charged to the
/// shared container budget; a fixed 2 MiB metadata allowance is charged beside
/// those arrays. This is a safe fraction of the container budget;
/// a real SMB fleet has far fewer *concurrently-active* tenants, so steady-state
/// eviction is rare — the cap exists to defeat a long-tail / adversarial churn
/// of distinct tenant ids, not to throttle legitimate multi-tenancy.
const DEFAULT_MAX_TENANT_BLOOMS: usize = 2048;
/// Tenant identifiers are bounded before entering this cache; the remaining
/// fixed allowance covers each map entry and allocator bucket.
const MAX_BLOOM_TENANT_ID_BYTES: usize = 256;
const MAX_BLOOM_ENTRY_METADATA_BYTES: usize = 768;

const _: () = assert!(
    (DEFAULT_MAX_TENANT_BLOOMS as u64) * ((DEFAULT_BLOOM_BITS / 8) as u64)
        <= BLOOM_CACHE_BIT_ARRAY_BYTES
);
const _: () = assert!(
    (DEFAULT_MAX_TENANT_BLOOMS as u64)
        * (MAX_BLOOM_TENANT_ID_BYTES as u64 + MAX_BLOOM_ENTRY_METADATA_BYTES as u64)
        <= BLOOM_CACHE_METADATA_BYTES
);

/// A fixed-size, thread-safe Bloom filter over arbitrary `&str` keys.
///
/// Hand-rolled (no external crate added — see WP-2b note): two SipHash-1-3
/// digests via the std [`std::collections::hash_map::DefaultHasher`] are
/// combined Kirsch–Mitzenmacher style (`h_i = h1 + i*h2`) to synthesise `k`
/// probe positions. The bit-array is a `Vec<AtomicU64>` of fixed length, so
/// concurrent `insert`/`contains` need no lock and the memory is bounded for
/// life (invariants 4 + 5).
///
/// A Bloom filter has **false positives but never false negatives**: once a key
/// is `insert`ed, `contains` returns `true` for it forever (until the whole
/// filter is reset). That one-sided error is the entire safety basis of
/// invariant 1 below.
#[derive(Debug)]
struct Bloom {
    /// Bit-array, packed 64 bits per word. Length is fixed at construction.
    words: Vec<AtomicU64>,
    /// Number of probe positions per key (`k`).
    k: u32,
    /// Total number of bits (`words.len() * 64`); cached for the modulo.
    nbits: u64,
}

impl Bloom {
    /// Build a bloom with at least `bits` bits and `k` probes (both clamped to
    /// sane minimums so a misconfiguration can never produce a zero-sized or
    /// zero-probe filter that would silently degrade to "always-absent").
    fn new(bits: usize, k: u32) -> Self {
        let words = bits.max(64).div_ceil(64);
        let k = k.max(1);
        let mut v = Vec::with_capacity(words);
        for _ in 0..words {
            v.push(AtomicU64::new(0));
        }
        let nbits = (words as u64) * 64;
        Self { words: v, k, nbits }
    }

    /// Two independent 64-bit hashes of `key` (seeded `DefaultHasher`s).
    fn hashes(key: &str) -> (u64, u64) {
        let mut h1 = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h1);
        let a = h1.finish();
        let mut h2 = std::collections::hash_map::DefaultHasher::new();
        // Distinct seed so h2 is independent of h1 (avoids correlated probes).
        0xD1F2_B100_ADEF_C0DEu64.hash(&mut h2);
        key.hash(&mut h2);
        // Force h2 odd so it is coprime with the power-of-two-ish bit count and
        // the probe sequence cycles through distinct positions.
        (a, h2.finish() | 1)
    }

    /// The `k` probe bit-indices for `key`.
    fn probes(&self, key: &str) -> impl Iterator<Item = (usize, u64)> + '_ {
        let (h1, h2) = Self::hashes(key);
        (0..self.k).map(move |i| {
            let bit = h1.wrapping_add((i as u64).wrapping_mul(h2)) % self.nbits;
            let word = (bit / 64) as usize;
            let mask = 1u64 << (bit % 64);
            (word, mask)
        })
    }

    /// Set the `k` bits for `key` (idempotent). `word` is always in range by
    /// construction (`word = (h % nbits) / 64 < words.len()`), but we index via
    /// `.get` to satisfy `-D clippy::indexing_slicing`; a `None` would only
    /// arise from an impossible state and is a safe no-op (the bit simply is
    /// not set — at worst a fall-through to the inner store, never a false 410).
    fn insert(&self, key: &str) {
        for (word, mask) in self.probes(key) {
            if let Some(w) = self.words.get(word) {
                // Relaxed is sufficient: each bit is set-only (monotone), order
                // across bits/keys does not matter for set-membership correctness.
                w.fetch_or(mask, Ordering::Relaxed);
            }
        }
    }

    /// `true` if EVERY probe bit for `key` is set (i.e. maybe-present). `false`
    /// means definitely-absent — the one-sided guarantee. (`.get` for the same
    /// clippy reason as [`Self::insert`]; an out-of-range word is treated as an
    /// unset bit ⇒ `false`/definitely-absent only if it were the sole probe,
    /// which is unreachable by construction.)
    fn contains(&self, key: &str) -> bool {
        for (word, mask) in self.probes(key) {
            match self.words.get(word) {
                Some(w) if w.load(Ordering::Relaxed) & mask != 0 => {}
                _ => return false,
            }
        }
        true
    }
}

/// Per-tenant bloom + the instant it was last (re)loaded from the inner store.
#[derive(Debug)]
struct TenantBloom {
    bloom: Bloom,
    /// Wall-clock instant the bloom was last populated from the inner store.
    loaded_at: Instant,
}

/// One entry in the LRU-bounded per-tenant bloom map (finding H3): the tenant's
/// shared [`TenantBloom`] plus the monotonic tick at which it was last accessed
/// (created or fetched). The smallest tick is the least-recently-used ⇒ the
/// eviction candidate.
#[derive(Debug)]
struct TenantBloomEntry {
    bloom: Arc<TenantBloom>,
    /// Monotonic LRU recency tick (last get-or-insert); smallest = LRU.
    last_access: u64,
}

/// A drop-in [`TombstoneStore`] that fronts an authoritative inner store with
/// an in-memory per-tenant Bloom filter, removing the synchronous D1-over-HTTP
/// round-trip from the **99.99 %-common non-erased** CAS read.
///
/// Wiring: the lead constructs `BloomTombstoneStore::new(inner)` and stores it
/// as the route/handler's `Arc<dyn TombstoneStore>`; the read path
/// (`cas.rs`) calls `is_tombstoned` unchanged. This type does NOT alter the
/// [`TombstoneStore`] trait nor `cas.rs`.
///
/// # The fast path
///
/// `is_tombstoned(tenant, digest)`:
/// 1. Ensure the tenant's bloom exists and is fresher than the staleness
///    window; if missing or stale, **reload it from the inner store** (one D1
///    scan of the tenant's tombstone set — amortised over the whole window).
/// 2. If the (fresh) bloom says **definitely-absent** → return `Ok(false)`
///    WITHOUT touching the inner store (the common case — zero D1 calls).
/// 3. If the bloom says **maybe-present** → fall through to the inner store and
///    return its authoritative `Result` UNCHANGED.
///
/// The write path `upsert` sets the bloom bit (and inserts into the local
/// tenant index) **before** delegating to the inner store, so a tombstone
/// written THROUGH this instance is immediately visible to this instance.
///
/// # The five invariants (INVIOLABLE)
///
/// 1. **NO FALSE NEGATIVE — the GDPR-critical one.** The wrapper must never
///    answer `Ok(false)` for a digest that IS tombstoned in the inner store.
///    A Bloom filter has false positives but, *by construction*, **no false
///    negatives**: a bit, once set, stays set, so a key that was inserted
///    always passes `contains`. The only way `contains` can be `false` for an
///    inner-store tombstone is if THIS instance never learned about it —
///    namely a tombstone written by ANOTHER container instance directly to D1
///    after this instance's bloom was loaded. We bound that gap with a
///    **refresh window** (`refresh`, default 30 s): a tenant's bloom is
///    reloaded from the inner store on first touch and whenever it is older
///    than the window, so a cross-instance erasure becomes locally visible
///    within **at most one window** (≤ `refresh`). During that ≤-window the
///    wrapper could fast-path `Ok(false)` for a freshly cross-instance-erased
///    digest. This is **≤ the existing posture**: (a) the read gate already
///    **fails OPEN** on any transient D1 error (serves the blob on a blip), so
///    a bounded staleness window is strictly no weaker than the status quo;
///    and (b) the erase *write-side* is the source of truth and is
///    synchronous — the bytes are deleted from R2 before the tombstone row is
///    written, so within the window a GET that slips past the gate 404s
///    (bytes already gone) rather than serving erased content. The window is
///    explicit, configurable, and tested.
/// 2. **False-positive is safe.** A bloom hit (maybe-present) always falls
///    through to the inner store, which returns the authoritative answer, so a
///    spurious bloom hit on a LIVE blob never wrongly 410s it — it costs one
///    extra D1 check, nothing more.
/// 3. **Fail-safe on inner error.** On the maybe-present path the inner
///    `Result` is returned UNCHANGED — an inner `Err` propagates so the caller
///    keeps today's fail-OPEN semantics; the wrapper never swallows it into a
///    bogus `Ok(false)`.
/// 4. **Bounded memory.** Each tenant bloom is a fixed `bits`-bit array
///    (default `2^20` bits = 128 KiB) that never grows, AND the number of live
///    tenant blooms is itself LRU-capped at `max_tenants` (default
///    [`DEFAULT_MAX_TENANT_BLOOMS`] ⇒ ≤256 MiB of bit arrays (plus the fixed
///    shared metadata allowance) — finding H3. When
///    the map is full and a NEW tenant is inserted, the least-recently-accessed
///    bloom is evicted; because a bloom is a pure read-through cache the evicted
///    tenant's next lookup just reloads authoritatively from D1 (invariant 1
///    preserved). The current live count is exposed via
///    [`BloomTombstoneStore::tenant_bloom_count`] as a map-size gauge.
/// 5. **Concurrency-safe.** The bit-array is `Vec<AtomicU64>` (lock-free
///    set/test). The tenant map is behind a `Mutex` held only for the brief
///    get-or-create; the reload reads the inner store and repopulates the
///    tenant's own bloom. No torn membership state is observable: a concurrent
///    `contains` during a reload sees a monotone superset-then-reset-then-
///    repopulate, and any inner-store tombstone is re-set by the reload.
#[non_exhaustive]
pub struct BloomTombstoneStore {
    /// The authoritative durable store (D1 in prod).
    inner: Arc<dyn TombstoneStore>,
    /// Per-tenant blooms + LRU recency (invariant 4). LRU-capped at
    /// `max_tenants` so the map footprint is bounded (finding H3).
    tenants: Mutex<std::collections::HashMap<String, TenantBloomEntry>>,
    /// Bloom bit-array size (bits) per tenant.
    bits: usize,
    /// Probe count `k`.
    k: u32,
    /// Bounded staleness window for cross-instance freshness (invariant 1).
    refresh: Duration,
    /// LRU capacity of the `tenants` map (finding H3). A field (not a const) so
    /// a test can shrink it to drive the eviction path deterministically.
    max_tenants: usize,
    /// Monotonic clock for the per-tenant bloom LRU recency order. Bumped on
    /// every get-or-insert; the smallest tick is the least-recently-used.
    tick: AtomicU64,
    /// Live per-tenant bloom count — a lock-free map-size gauge (finding H3),
    /// updated under the map lock, readable via [`Self::tenant_bloom_count`].
    bloom_count: AtomicU64,
}

impl std::fmt::Debug for BloomTombstoneStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BloomTombstoneStore")
            .field("inner", &self.inner)
            .field("bits", &self.bits)
            .field("k", &self.k)
            .field("refresh", &self.refresh)
            .field("max_tenants", &self.max_tenants)
            .field("bloom_count", &self.bloom_count.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_1_REANCHOR: () = ();
