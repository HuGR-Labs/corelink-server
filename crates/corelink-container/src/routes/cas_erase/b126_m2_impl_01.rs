/// HTTP header carrying the shared internal-auth secret (mirrors
/// [`crate::routes::internal_pat`] byte-for-byte).
const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Canonical erase route path (matchit-0.7 `:name` captures — see the DEBT-029
/// note in [`crate::routes::cas`]).
pub const CAS_ERASE_ROUTE: &str = "/_internal/cas/{tenant}/{hash}/erase";

/// Max accepted audit reason length (bounds the D1 row; never PII).
const MAX_REASON_LEN: usize = 256;

// ──────────────────────────────────────────────────────────────────────────────
// Collaborator traits
// ──────────────────────────────────────────────────────────────────────────────

/// Durable store for the 410-Gone tombstones (`cas_tombstone`, migration 0067).
#[async_trait]
pub trait TombstoneStore: Send + Sync + std::fmt::Debug {
    /// Is `(tenant, digest)` tombstoned? Drives the read-path 410 gate.
    async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String>;

    /// AUTHORITATIVE tombstone check — MUST consult the durable store on every
    /// call, NEVER a cached/bloom fast-path negative.
    ///
    /// The **write** gate uses this (finding H3). A within-window bloom
    /// false-negative is *tolerable on the READ path* (invariant 1: the bytes
    /// are already gone from R2, so a GET that slips past the gate 404s rather
    /// than serving erased content), but on the **WRITE path** the same
    /// false-negative would let a re-PUT **RESURRECT legally-erased bytes** at
    /// the same content address — the erasure attestation becomes false. So a
    /// write must never trust a fast-path `Ok(false)`.
    ///
    /// The default delegates to [`Self::is_tombstoned`] — already authoritative
    /// for the D1 / in-memory stores. [`BloomTombstoneStore`] overrides it to
    /// bypass its own bloom fast-path and always hit the inner (D1) store.
    async fn is_tombstoned_authoritative(
        &self,
        tenant: &str,
        digest: &str,
    ) -> Result<bool, String> {
        self.is_tombstoned(tenant, digest).await
    }

    /// Upsert a tombstone (idempotent). Returns whether a row already existed
    /// (so the route can report `AlreadyErased` for an idempotent re-erase).
    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        reason: &str,
        erased_at_ms: i64,
    ) -> Result<bool, String>;

    /// List ALL tombstoned digests for `tenant` (the durable per-tenant set).
    ///
    /// Backs the [`BloomTombstoneStore`] (re)load so its in-memory bloom is
    /// seeded from the AUTHORITATIVE D1 set — including tombstones written by
    /// ANOTHER container instance (or by the separate erase-route `D1TombstoneStore`
    /// that does not share this bloom). Without this seed the bloom only knew its
    /// OWN in-process writes, so a cross-writer tombstone read as NOT-present for
    /// the whole refresh window after its first read (F-010 false-negative — a
    /// GDPR no-false-negative invariant violation). Erasures are RARE (Art.17 +
    /// per-blob operator erases), so a per-tenant enumerate is cheap and bounded.
    ///
    /// # Errors
    /// Propagates the underlying store transport error; the bloom (re)load treats
    /// an `Err` as "could not seed" and downgrades to the authoritative
    /// fall-through path (never a silent stale `Ok(false)`).
    async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String>;
}

/// Hard-deletes a single content-addressed blob's bytes from R2.
///
/// The production impl composes the DSR Wave 1 R2 CAS primitives (PR #254);
/// see the module-level "Composition with DSR Wave 1" note.
#[async_trait]
pub trait CasBlobEraser: Send + Sync + std::fmt::Debug {
    /// Delete every R2 object for `(tenant, digest)` across the CAS regions.
    /// Idempotent: deleting an absent object is a no-op success.
    async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String>;
}

// ──────────────────────────────────────────────────────────────────────────────
// Route state
// ──────────────────────────────────────────────────────────────────────────────

/// Shared state for the erase route.
#[derive(Clone)]
pub struct CasEraseRouteState {
    /// Tombstone persistence (D1 in prod).
    pub tombstones: Arc<dyn TombstoneStore>,
    /// R2 blob byte-deletion (the #254 seam).
    pub eraser: Arc<dyn CasBlobEraser>,
    /// Shared internal-auth secret (constant-time compared).
    pub internal_auth_key: Arc<str>,
    /// DSR legitimacy pre-check (rt-nuclear #18/#19, r34 #8/#9). Binds the
    /// per-blob erase to a durable, D1-authenticated `dsr_requested` row so a
    /// leaked internal/erase key alone CANNOT erase arbitrary blobs: the
    /// attacker cannot forge a `dsr_requested` row (written only by the
    /// D1-authenticated Clerk `user.deleted` path). `None` only in the
    /// in-memory/test fallback; `build_state_from_env` fail-CLOSES (route
    /// unmounted) if it cannot be built in prod, so a `None` here on a
    /// mounted prod route is unreachable — and the handler treats `None` as
    /// DENY (503) regardless.
    pub legitimacy: Option<Arc<dyn DsrLegitimacyStore>>,
}

impl core::fmt::Debug for CasEraseRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasEraseRouteState")
            .field("internal_auth_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Build the erase router. Mounted top-level so the path is directly
/// addressable from the DO (mirrors `internal_pat`).
pub fn router(state: CasEraseRouteState) -> Router {
    Router::new()
        .route(CAS_ERASE_ROUTE, post(handle_erase))
        .with_state(state)
}

/// Erase request body. `tenant` is the authenticated tenant; `dsr_id` is the
/// canonical UUID of the DSR ticket that legitimately requested this erasure
/// (REQUIRED — missing/invalid ⇒ 400 at deserialization); `reason` is a
/// bounded, free-form audit string.
#[derive(Debug, serde::Deserialize)]
struct EraseBody {
    tenant: String,
    /// DSR request id. The erase is authorised ONLY if a live `dsr_requested`
    /// row exists for `(dsr_id, tenant)` — see [`CasEraseRouteState::legitimacy`].
    dsr_id: Uuid,
    #[serde(default)]
    reason: String,
}

/// `POST /_internal/cas/:tenant/:hash/erase`.
///
/// Order (fail-CLOSED): internal-auth gate (constant-time, BEFORE body parse) →
/// parse body → cross-tenant + digest validation (pure handler) → R2 delete →
/// tombstone upsert. The tombstone is written AFTER the bytes are gone so a
/// crash between the two leaves bytes-gone-without-tombstone (a subsequent GET
/// 404s, never serves bytes) rather than tombstone-without-bytes — the safe
/// failure direction; the operation is idempotent so a retry completes it.
async fn handle_erase(
    State(state): State<CasEraseRouteState>,
    Path((path_tenant, hash)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 1. Internal-auth gate — BEFORE the body is parsed (mirrors internal_pat).
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // 2. Parse the body only after auth passed.
    let req: EraseBody = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "cas_erase: invalid request body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_body" })),
            )
                .into_response();
        }
    };

    // 3. Bound the audit reason.
    if req.reason.len() > MAX_REASON_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "reason_too_long" })),
        )
            .into_response();
    }

    // 4. Pure validation → tombstone marker (cross-tenant + digest shape).
    let erase_req = corelink_handler_cas_erase::CasEraseRequest::new(
        req.tenant.clone(),
        path_tenant,
        hash.clone(),
    );
    let _marker = match prepare_erase(&erase_req, &req.reason) {
        Ok(m) => m,
        Err(corelink_handler_cas_erase::CasEraseError::CrossTenantDenied { .. }) => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "cross-tenant" })),
            )
                .into_response();
        }
        Err(corelink_handler_cas_erase::CasEraseError::InvalidDigest { .. }) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_digest" })),
            )
                .into_response();
        }
        Err(corelink_handler_cas_erase::CasEraseError::Transport(_)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal" })),
            )
                .into_response();
        }
        // `CasEraseError` is `#[non_exhaustive]`: a future variant must
        // fail-CLOSED (never silently fall through to the erase) — map any
        // unknown error to 500 so a new error kind can never ship without an
        // explicit decision here.
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal" })),
            )
                .into_response();
        }
    };

    // 4b. DSR legitimacy gate (rt-nuclear #18/#19, r34 #8/#9). The
    //     internal-auth gate alone proves only "holds the shared/erase key";
    //     it does NOT prove "this erasure was legitimately requested". A
    //     leaked key would otherwise allow arbitrary cross-tenant blob
    //     deletion + permanent 410-Gone poison with no proof-of-request.
    //     Bind the erase to a durable, D1-authenticated `dsr_requested` row
    //     for THIS (dsr_id, tenant) — which a leaked-key attacker cannot
    //     forge. Placed AFTER cross-tenant/digest validation and BEFORE the
    //     irreversible R2 delete. Fail-CLOSED on every ambiguity.
    //
    //     The tenant is bound as a UUID so a `dsr_id` legitimate for tenant A
    //     cannot authorise erasing tenant B (`is_requested` keys on BOTH).
    let tenant_uuid = match Uuid::try_parse(&req.tenant) {
        Ok(u) => u,
        Err(_) => {
            // A non-UUID tenant can never match a `dsr_requested` row (the
            // table stores canonical UUID tenant ids written by the
            // D1-authenticated Clerk path) → DENY rather than bypass the gate.
            tracing::warn!(
                "cas_erase: non-UUID tenant on erase — denying (no legitimacy possible)"
            );
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "forbidden" })),
            )
                .into_response();
        }
    };
    match &state.legitimacy {
        Some(store) => match store.is_requested(req.dsr_id, tenant_uuid) {
            Ok(true) => { /* legitimate — proceed */ }
            Ok(false) => {
                // No live `dsr_requested` row for (dsr_id, tenant): not a
                // legitimate erasure request (or wrong tenant). 403.
                tracing::warn!(
                    dsr_id = %req.dsr_id,
                    "cas_erase: no dsr_requested row for (dsr_id, tenant) — forbidden"
                );
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "forbidden" })),
                )
                    .into_response();
            }
            Err(e) => {
                // Ambiguous legitimacy (D1 fault). Erasure is IRREVERSIBLE,
                // so a store error MUST DENY (fail-CLOSED), never proceed.
                tracing::error!(error = %e, "cas_erase: legitimacy lookup failed — fail-CLOSED (503)");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({ "error": "legitimacy_unavailable" })),
                )
                    .into_response();
            }
        },
        None => {
            // No legitimacy store wired. In prod this is unreachable
            // (`build_state_from_env` fail-CLOSES to an unmounted route), but
            // if it ever happens the irreversible erase MUST NOT run.
            tracing::error!("cas_erase: legitimacy store absent — fail-CLOSED (503)");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "legitimacy_unavailable" })),
            )
                .into_response();
        }
    }

    // 5. Sample prior tombstone presence (for idempotent-outcome reporting).
    let was_tombstoned = match state.tombstones.is_tombstoned(&req.tenant, &hash).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "cas_erase: tombstone lookup failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal" })),
            )
                .into_response();
        }
    };

    // 6. Delete the R2 bytes (idempotent).
    if let Err(e) = state.eraser.erase_blob(&req.tenant, &hash).await {
        tracing::error!(error = %e, "cas_erase: R2 blob erase failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal" })),
        )
            .into_response();
    }

    // 7. Upsert the tombstone (idempotent).
    let now_ms = now_unix_ms();
    if let Err(e) = state
        .tombstones
        .upsert(&req.tenant, &hash, &req.reason, now_ms)
        .await
    {
        tracing::error!(error = %e, "cas_erase: tombstone upsert failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal" })),
        )
            .into_response();
    }

    let outcome = erase_outcome(was_tombstoned);
    let outcome_str = match outcome {
        EraseOutcome::Erased => "erased",
        EraseOutcome::AlreadyErased => "already_erased",
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": outcome_str })),
    )
        .into_response()
}

/// Constant-time internal-auth check (mirrors
/// [`crate::routes::internal_pat`] `internal_auth_ok` byte-for-byte).
#[must_use]
fn internal_auth_ok(expected: &[u8], headers: &HeaderMap) -> bool {
    let provided = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let provided_bytes = provided.as_bytes();
    let provided_padded: Vec<u8> = if provided_bytes.len() >= expected.len() {
        provided_bytes.get(..expected.len()).unwrap_or(&[]).to_vec()
    } else {
        let mut v = provided_bytes.to_vec();
        v.resize(expected.len(), 0);
        v
    };
    let content_ok = expected.ct_eq(&provided_padded).unwrap_u8();
    let len_ok = u8::from(expected.len() == provided_bytes.len());
    (content_ok & len_ok) == 1
}

/// Current Unix time in milliseconds (saturating; 0 on a pre-epoch clock).
#[must_use]
fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

// ──────────────────────────────────────────────────────────────────────────────
// D1-backed tombstone store
// ──────────────────────────────────────────────────────────────────────────────

/// `cas_tombstone` D1-over-HTTP tombstone store (migration 0067).
pub struct D1TombstoneStore {
    d1: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl std::fmt::Debug for D1TombstoneStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1TombstoneStore").finish_non_exhaustive()
    }
}

impl D1TombstoneStore {
    /// Build over a shared [`D1HttpClient`](crate::storage::d1_http::D1HttpClient).
    #[must_use]
    pub fn new(d1: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Build from env (`StorageEnv`); `None` when D1 env is absent.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let env = crate::storage::StorageEnv::from_env()?;
        match crate::storage::d1_http::D1HttpClient::new(&env) {
            Ok(c) => Some(Self::new(Arc::new(c))),
            Err(e) => {
                tracing::warn!(error = %e, "cas_erase: D1 client build failed");
                None
            }
        }
    }
}

#[async_trait]
impl TombstoneStore for D1TombstoneStore {
    async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String> {
        let rows = self
            .d1
            .query(
                "SELECT 1 AS present FROM cas_tombstone \
                 WHERE tenant_id = ?1 AND digest = ?2 LIMIT 1",
                &[serde_json::json!(tenant), serde_json::json!(digest)],
            )
            .await?;
        Ok(!rows.is_empty())
    }

    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        reason: &str,
        erased_at_ms: i64,
    ) -> Result<bool, String> {
        let existed = self.is_tombstoned(tenant, digest).await?;
        // INSERT OR REPLACE keyed on (tenant_id, digest) → idempotent re-erase.
        self.d1
            .query(
                "INSERT INTO cas_tombstone (tenant_id, digest, reason, erased_at_ms) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(tenant_id, digest) DO UPDATE SET \
                 reason = excluded.reason, erased_at_ms = excluded.erased_at_ms",
                &[
                    serde_json::json!(tenant),
                    serde_json::json!(digest),
                    serde_json::json!(reason),
                    serde_json::json!(erased_at_ms),
                ],
            )
            .await?;
        Ok(existed)
    }

    async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String> {
        let rows = self
            .d1
            .query(
                "SELECT digest FROM cas_tombstone WHERE tenant_id = ?1",
                &[serde_json::json!(tenant)],
            )
            .await?;
        let mut digests = Vec::with_capacity(rows.len());
        for row in &rows {
            if let Some(d) = row.get("digest").and_then(serde_json::Value::as_str) {
                digests.push(d.to_owned());
            }
        }
        Ok(digests)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Bloom-fronted tombstone store (WP-2b — the read-hot-path D1 round-trip cut)
// ──────────────────────────────────────────────────────────────────────────────

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
