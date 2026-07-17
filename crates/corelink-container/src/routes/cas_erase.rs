//! `POST /_internal/cas/:tenant/:hash/erase` — per-hash CAS erase + 410-Gone
//! tombstone (hugit-P2 seam B, WP-B).
//!
//! Operator/internal **write-side** endpoint (off the hot GET path). Deletes a
//! single content-addressed blob from the cold `corelink-cas-prod` R2 bucket
//! and writes a durable tombstone so subsequent `GET /v1/cas/:tenant/:hash`
//! returns HTTP **410 Gone** (never 404, never 200). The pure decision logic
//! lives in [`corelink_handler_cas_erase`]; this module is the transport wiring.
//!
//! # Auth
//!
//! Gated by the same constant-time `X-Corelink-Internal-Auth` shared-secret as
//! [`crate::routes::internal_pat`] (the DO is the only caller). The body carries
//! the authenticated `tenant` + an audit `reason`; the path `:tenant` is a
//! client echo that MUST equal the body tenant (cross-tenant ⇒ 403).
//!
//! # Composition with DSR Wave 1 (PR #254) — the R2 erase seam
//!
//! The blob byte-deletion is expressed through the [`CasBlobEraser`] trait. Its
//! production implementation reuses the **DSR Wave 1 R2 CAS primitives**
//! (`R2S3Client::{delete, list_objects_v2}` + `R2S3Client::blob_key`, added on
//! branch `feat/dsr-account-deletion`, PR #254 increment 3) — it LISTs the
//! tenant prefix across the five CAS regions and DELETEs the object(s) for the
//! one digest, exactly the per-key cousin of the DSR tenant-wide erase. WP-B
//! does **not** duplicate those primitives; the trait is the frozen seam #254
//! fills. The production [`R2CasBlobEraser`] reuses `R2S3Client` directly and
//! derives the tenant prefix the SAME way the CAS writer did (so the erase key
//! matches the stored object by construction); [`InMemoryBlobEraser`] backs the
//! unit tests. The env builder ([`build_state_from_env`]) returns `Some` only
//! when the R2 TDK + D1 tombstone store + internal-auth key are all present,
//! and `None` otherwise (route fail-CLOSED / unmounted) so no half-built erase
//! — and, critically, no wrong-key silent-no-op — can run in prod.
//!
//! # Tombstone store
//!
//! [`TombstoneStore`] persists/queries the `cas_tombstone` D1 table (migration
//! `0067`). [`D1TombstoneStore`] backs it over [`crate::storage::d1_http`];
//! [`InMemoryTombstoneStore`] backs the unit tests.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroizing;

use corelink_handler_cas_erase::{erase_outcome, prepare_erase, EraseOutcome};
use corelink_privacy_erasure_worker::legitimacy::DsrLegitimacyStore;

use crate::routes::dsr::legitimacy::D1DsrLegitimacyStore;

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
/// memory-capped `standard-1` container. We cap it as an LRU: when the map is
/// full and a NEW tenant must be inserted, the least-recently-accessed bloom is
/// evicted first. Evicting a bloom is ALWAYS safe — it is a pure read-through
/// cache, so the next lookup for that tenant simply reloads (authoritatively)
/// from D1 (invariant 1 preserved: the reload re-seeds from the durable set).
/// 2048 blooms ⇒ ≤256 MiB worst-case, a safe fraction of the container budget;
/// a real SMB fleet has far fewer *concurrently-active* tenants, so steady-state
/// eviction is rare — the cap exists to defeat a long-tail / adversarial churn
/// of distinct tenant ids, not to throttle legitimate multi-tenancy.
const DEFAULT_MAX_TENANT_BLOOMS: usize = 2048;

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
///    [`DEFAULT_MAX_TENANT_BLOOMS`] ⇒ ≤256 MiB worst-case) — finding H3. When
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
            bits,
            k,
            refresh,
            // A zero cap would defeat the cache AND wedge the eviction loop;
            // clamp to at least 1 live bloom.
            max_tenants: max_tenants.max(1),
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

    /// Derive the 16-char tenant prefix the SAME way the CAS writer
    /// ([`crate::storage::r2_s3::R2CasHandler`]'s `r2_key`) did: parse the
    /// tenant as a UUID and `derive_prefix(tdk, uuid)`, else raw-pad/truncate
    /// the tenant string to 16 chars (the writer's non-UUID/dev fallback).
    /// Keeping the two derivations identical is what makes the erase key match
    /// the stored object **by construction**.
    fn tenant_prefix(&self, tenant: &str) -> Result<String, String> {
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
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request};
    use tower::ServiceExt;

    use corelink_privacy_erasure_worker::legitimacy::{
        FailingDsrLegitimacyStore, InMemoryDsrLegitimacyStore,
    };

    const TEST_KEY: &str = "test-internal-auth-key-32-bytes-x";
    // The legitimacy gate keys on a canonical UUID tenant (the `dsr_requested`
    // table stores UUID tenant ids), so the route fixtures use a UUID tenant.
    const TENANT: &str = "550e8400-e29b-41d4-a716-446655440000";
    // A different legitimate tenant (cross-tenant forgery test).
    const TENANT_B: &str = "11111111-2222-3333-4444-555555555555";
    const DIGEST: &str = "deadbeef0123";
    // The DSR id the in-memory legitimacy store is seeded with for TENANT.
    const DSR_ID: &str = "99999999-8888-7777-6666-555544443333";

    /// Build a route state with an in-memory legitimacy store pre-seeded so
    /// `(DSR_ID, TENANT)` is legitimate. Returns the eraser + tombstone fakes
    /// for assertions.
    fn state() -> (
        CasEraseRouteState,
        Arc<InMemoryBlobEraser>,
        Arc<InMemoryTombstoneStore>,
    ) {
        let legit = InMemoryDsrLegitimacyStore::new();
        legit.insert_requested(
            Uuid::try_parse(DSR_ID).expect("dsr uuid"),
            Uuid::try_parse(TENANT).expect("tenant uuid"),
        );
        build_state(Arc::new(legit))
    }

    /// Build a route state with an explicit legitimacy store.
    fn build_state(
        legitimacy: Arc<dyn DsrLegitimacyStore>,
    ) -> (
        CasEraseRouteState,
        Arc<InMemoryBlobEraser>,
        Arc<InMemoryTombstoneStore>,
    ) {
        let tombstones = Arc::new(InMemoryTombstoneStore::new());
        let eraser = Arc::new(InMemoryBlobEraser::new());
        let st = CasEraseRouteState {
            tombstones: tombstones.clone(),
            eraser: eraser.clone(),
            internal_auth_key: Arc::from(TEST_KEY),
            legitimacy: Some(legitimacy),
        };
        (st, eraser, tombstones)
    }

    fn erase_req(auth: &str, tenant: &str, digest: &str, body_tenant: &str) -> Request<Body> {
        erase_req_dsr(auth, tenant, digest, body_tenant, DSR_ID)
    }

    fn erase_req_dsr(
        auth: &str,
        tenant: &str,
        digest: &str,
        body_tenant: &str,
        dsr_id: &str,
    ) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri(format!("/_internal/cas/{tenant}/{digest}/erase"));
        if !auth.is_empty() {
            b = b.header(INTERNAL_AUTH_HEADER, auth);
        }
        b.body(Body::from(
            serde_json::json!({ "tenant": body_tenant, "dsr_id": dsr_id, "reason": "dsr" })
                .to_string(),
        ))
        .expect("request")
    }

    #[tokio::test]
    async fn erase_without_auth_is_401() {
        let (st, _e, _t) = state();
        let resp = router(st)
            .oneshot(erase_req("", TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn erase_happy_path_deletes_bytes_and_writes_tombstone() {
        let (st, eraser, tombstones) = state();
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            eraser.was_erased(TENANT, DIGEST),
            "R2 bytes must be deleted"
        );
        assert!(
            tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"),
            "tombstone must be written"
        );
    }

    #[tokio::test]
    async fn erase_cross_tenant_is_403_before_storage() {
        let (st, eraser, _t) = state();
        // path tenant TENANT, body tenant TENANT_B → cross-tenant (pure handler).
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT_B))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(!eraser.was_erased(TENANT_B, DIGEST));
        assert!(!eraser.was_erased(TENANT, DIGEST));
    }

    #[tokio::test]
    async fn erase_invalid_digest_is_400() {
        let (st, _e, _t) = state();
        // matchit will not match a '/' inside :hash, so use a non-slash invalid
        // char that still reaches the handler: an overlong digest.
        let long = "a".repeat(200);
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, TENANT, &long, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn re_erase_is_idempotent_ok() {
        let (st, _e, tombstones) = state();
        let app = router(st);
        let r1 = app
            .clone()
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(r1.status(), StatusCode::OK);
        // second erase = idempotent no-op, still 200.
        let r2 = app
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(r2.status(), StatusCode::OK);
        assert!(tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
    }

    #[tokio::test]
    async fn in_memory_tombstone_upsert_reports_prior_existence() {
        let t = InMemoryTombstoneStore::new();
        assert!(!t.upsert(TENANT, DIGEST, "r", 1).await.expect("u1"));
        assert!(t.upsert(TENANT, DIGEST, "r", 2).await.expect("u2"));
    }

    // ──────────────────────────────────────────────────────────────────────
    // DSR legitimacy gate (rt-nuclear #18/#19, r34 #8/#9) — leaked-key defense
    // ──────────────────────────────────────────────────────────────────────

    /// Valid auth + a present `dsr_requested` row (seeded) ⇒ the erase
    /// proceeds (200) and the bytes are deleted.
    #[tokio::test]
    async fn erase_with_legit_dsr_row_proceeds() {
        let (st, eraser, tombstones) = state();
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(eraser.was_erased(TENANT, DIGEST));
        assert!(tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
    }

    /// Valid auth + a `dsr_id` with NO matching `dsr_requested` row ⇒ 403 and
    /// NOTHING is erased (the leaked-internal-key blast radius is closed).
    #[tokio::test]
    async fn erase_no_dsr_row_is_403_and_no_delete() {
        // Empty legitimacy store: every (dsr_id, tenant) is NOT requested.
        let (st, eraser, tombstones) = build_state(Arc::new(InMemoryDsrLegitimacyStore::new()));
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(
            !eraser.was_erased(TENANT, DIGEST),
            "no delete without a legit row"
        );
        assert!(!tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
    }

    /// Legitimacy store FAULT (D1 error) ⇒ fail-CLOSED 503, NOTHING erased.
    /// Erasure is irreversible — ambiguous legitimacy must DENY.
    #[tokio::test]
    async fn erase_legitimacy_error_is_503_fail_closed() {
        let (st, eraser, tombstones) = build_state(Arc::new(FailingDsrLegitimacyStore::new()));
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            !eraser.was_erased(TENANT, DIGEST),
            "no delete on legitimacy fault"
        );
        assert!(!tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
    }

    /// No legitimacy store wired (state.legitimacy == None) ⇒ fail-CLOSED 503.
    /// (Unreachable in prod — build_state_from_env fail-CLOSES the route — but
    /// the handler must still DENY if it ever occurs.)
    #[tokio::test]
    async fn erase_legitimacy_none_is_503_fail_closed() {
        let tombstones = Arc::new(InMemoryTombstoneStore::new());
        let eraser = Arc::new(InMemoryBlobEraser::new());
        let st = CasEraseRouteState {
            tombstones,
            eraser: eraser.clone(),
            internal_auth_key: Arc::from(TEST_KEY),
            legitimacy: None,
        };
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(!eraser.was_erased(TENANT, DIGEST));
    }

    /// Cross-tenant via the legitimacy gate: a `dsr_id` legitimate for TENANT
    /// CANNOT authorise erasing TENANT_B (the gate binds on BOTH dsr_id AND
    /// tenant). Here path==body==TENANT_B (so the pure cross-tenant check
    /// passes) but only `(DSR_ID, TENANT)` is seeded ⇒ 403 from the gate.
    #[tokio::test]
    async fn erase_dsr_id_for_other_tenant_is_403() {
        // Seed legitimacy ONLY for TENANT, then attempt to erase TENANT_B's
        // blob with TENANT's dsr_id.
        let legit = InMemoryDsrLegitimacyStore::new();
        legit.insert_requested(
            Uuid::try_parse(DSR_ID).expect("dsr uuid"),
            Uuid::try_parse(TENANT).expect("tenant uuid"),
        );
        let (st, eraser, tombstones) = build_state(Arc::new(legit));
        let resp = router(st)
            .oneshot(erase_req_dsr(TEST_KEY, TENANT_B, DIGEST, TENANT_B, DSR_ID))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(
            !eraser.was_erased(TENANT_B, DIGEST),
            "no cross-tenant erase via foreign dsr_id"
        );
        assert!(!tombstones.is_tombstoned(TENANT_B, DIGEST).await.expect("q"));
    }

    /// A missing `dsr_id` field in the body ⇒ 400 (deserialization rejects it
    /// before any storage touch). Required-field enforcement.
    #[tokio::test]
    async fn erase_missing_dsr_id_is_400() {
        let body = serde_json::json!({ "tenant": TENANT, "reason": "dsr" }).to_string();
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/_internal/cas/{TENANT}/{DIGEST}/erase"))
            .header(INTERNAL_AUTH_HEADER, TEST_KEY)
            .body(Body::from(body))
            .expect("request");
        let (st, eraser, _t) = state();
        let resp = router(st).oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(!eraser.was_erased(TENANT, DIGEST));
    }

    // ──────────────────────────────────────────────────────────────────────
    // Production R2 eraser — by-construction key-layout proof + fail-CLOSED
    // ──────────────────────────────────────────────────────────────────────

    /// A fixed, non-zero TDK so the derived prefix is deterministic.
    fn test_tdk() -> Arc<TenantDerivationKey> {
        let bytes = Zeroizing::new([7u8; 32]);
        Arc::new(TenantDerivationKey::from_bytes(bytes))
    }

    /// The eraser's LIST/DELETE key for a UUID tenant is EXACTLY the whole-blob
    /// key the CAS writer keys under: `<region>/<derive_prefix(tdk,uuid)>/<digest>`.
    /// This is the by-construction guarantee that the erase cannot silently
    /// no-op on a key-derivation mismatch (the earlier R2Ac bug class).
    #[test]
    fn eraser_prefix_matches_writer_derivation_for_uuid_tenant() {
        let tdk = test_tdk();
        let eraser = R2CasBlobEraser::new(tdk.clone(), "corelink-cas-prod".to_owned());
        let tenant_uuid = "550e8400-e29b-41d4-a716-446655440000";
        let uid = Uuid::try_parse(tenant_uuid).expect("uuid");
        // What the WRITER derives (R2CasHandler::r2_key UUID branch).
        let writer_prefix = derive_prefix(&tdk, uid).to_string();
        // What the ERASER derives.
        let eraser_prefix = eraser.tenant_prefix(tenant_uuid).unwrap();
        assert_eq!(eraser_prefix, writer_prefix, "prefix must match the writer");
        assert_eq!(writer_prefix.len(), TENANT_PREFIX_LEN);
        // And the assembled LIST key is the leading path of the blob key.
        let blob_key = crate::storage::r2_s3::R2S3Client::blob_key(
            "iad",
            &writer_prefix,
            DIGEST,
            corelink_handler_cas::DigestAlgo::Blake3,
        );
        let list_key = crate::storage::r2_s3::R2S3Client::blob_key(
            "iad",
            &eraser_prefix,
            DIGEST,
            corelink_handler_cas::DigestAlgo::Blake3,
        );
        assert_eq!(list_key, blob_key);
        assert!(blob_key.starts_with("iad/"), "key={blob_key}");
        assert!(blob_key.ends_with(&format!("/{DIGEST}")), "key={blob_key}");
    }

    /// Non-UUID tenant uses the writer's raw-padded 16-char fallback (dev/test
    /// fixtures), so a digest erase still keys identically to the writer.
    #[test]
    fn eraser_prefix_pads_non_uuid_tenant_to_16() {
        let eraser = R2CasBlobEraser::new(test_tdk(), "corelink-cas-prod".to_owned());
        let prefix = eraser.tenant_prefix("t1").unwrap();
        assert_eq!(prefix.len(), TENANT_PREFIX_LEN);
        assert!(prefix.starts_with("t1"), "prefix={prefix}");
    }

    /// The five canonical CAS regions match the DSR Wave 1 tenant-wide adapter
    /// (`adapter_r2_cas::CAS_REGIONS`) — same sweep, same order.
    #[test]
    fn cas_regions_match_dsr_adapter() {
        assert_eq!(CAS_REGIONS, &["sam", "iad", "lhr", "nrt", "syd"]);
    }

    /// Fail-CLOSED: with no internal-auth key the route state is never built
    /// (route unmounted) even if other env were present.
    #[test]
    fn build_state_without_auth_key_is_none() {
        assert!(build_state_from_env(None).is_none());
    }

    /// Fail-CLOSED: an auth key is present but `R2_TDK_HEX` is absent ⇒ the
    /// eraser cannot derive the tenant prefix, so the route is NOT mounted.
    /// (We cannot mutate process env safely in a shared test binary, so this
    /// asserts the no-TDK branch directly via the loader.)
    #[test]
    fn build_state_fail_closed_without_tdk() {
        std::env::remove_var("R2_TDK_HEX");
        assert!(load_tdk_from_env().is_none(), "no TDK ⇒ loader None");
        // With the loader None, build_state_from_env short-circuits to None
        // regardless of D1/bucket env.
        assert!(build_state_from_env(Some(Arc::from(TEST_KEY))).is_none());
    }

    // ──────────────────────────────────────────────────────────────────────
    // WP-2b — BloomTombstoneStore (the read-hot-path D1 round-trip cut)
    // ──────────────────────────────────────────────────────────────────────

    use std::sync::atomic::AtomicUsize;

    /// Inner [`TombstoneStore`] that counts `is_tombstoned` calls (proves the
    /// fast path never touches the inner store) and wraps an
    /// [`InMemoryTombstoneStore`] for the authoritative answer. `seed` writes
    /// DIRECTLY to the inner set (NOT through the bloom) — simulating a
    /// tombstone written by ANOTHER container instance.
    #[derive(Debug)]
    struct CountingInner {
        inner: InMemoryTombstoneStore,
        reads: AtomicUsize,
    }
    impl CountingInner {
        fn new() -> Self {
            Self {
                inner: InMemoryTombstoneStore::new(),
                reads: AtomicUsize::new(0),
            }
        }
        fn reads(&self) -> usize {
            self.reads.load(std::sync::atomic::Ordering::SeqCst)
        }
        /// Write a tombstone DIRECTLY to the inner store (another instance).
        fn seed_inner(&self, tenant: &str, digest: &str) {
            self.inner.seed(tenant, digest);
        }
    }
    #[async_trait]
    impl TombstoneStore for CountingInner {
        async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.is_tombstoned(tenant, digest).await
        }
        async fn upsert(
            &self,
            tenant: &str,
            digest: &str,
            reason: &str,
            erased_at_ms: i64,
        ) -> Result<bool, String> {
            self.inner
                .upsert(tenant, digest, reason, erased_at_ms)
                .await
        }
        async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String> {
            // NOT counted in `reads` (which counts `is_tombstoned` probes only):
            // the bloom reload seeds from here (F-010), but the assertion target
            // is the per-lookup D1 round-trip, not the once-per-window reload.
            self.inner.list_tenant_tombstones(tenant).await
        }
    }

    /// Inner store whose `is_tombstoned` always errors (D1 fault) — to prove
    /// the wrapper propagates the inner `Err` unchanged on the maybe path.
    #[derive(Debug, Default)]
    struct ErroringInner;
    #[async_trait]
    impl TombstoneStore for ErroringInner {
        async fn is_tombstoned(&self, _t: &str, _d: &str) -> Result<bool, String> {
            Err("d1 fault".to_owned())
        }
        async fn upsert(&self, _t: &str, _d: &str, _r: &str, _e: i64) -> Result<bool, String> {
            // Upsert succeeds so the bloom bit can be set before the read probe.
            Ok(false)
        }
        async fn list_tenant_tombstones(&self, _t: &str) -> Result<Vec<String>, String> {
            // Reload-seed errors (degraded path): the bloom keeps only its
            // carry-forward bits + the just_reloaded fall-through stays authoritative.
            Err("d1 fault".to_owned())
        }
    }

    /// A long staleness window so a freshly-loaded tenant bloom stays "fresh"
    /// for the whole test (the just-reloaded fall-through fires only on the
    /// FIRST touch of a tenant).
    const LONG_WINDOW: Duration = Duration::from_secs(3600);

    /// Warm a tenant's bloom past the one-shot just-reloaded epoch so the fast
    /// path is armed: touch any digest once (that first touch falls through),
    /// after which fresh `definitely-absent` lookups skip the inner store.
    async fn warm(store: &BloomTombstoneStore, tenant: &str) {
        let _ = store.is_tombstoned(tenant, "warm-up-digest").await;
    }

    /// (a) A non-tombstoned digest → `Ok(false)` with ZERO inner calls on the
    /// fast path (after the tenant bloom is warmed past its first touch).
    #[tokio::test]
    async fn bloom_fast_path_skips_inner_for_absent_digest() {
        let inner = Arc::new(CountingInner::new());
        let store = BloomTombstoneStore::with_params(
            inner.clone(),
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        );
        warm(&store, TENANT).await; // first touch falls through (1 inner read)
        let before = inner.reads();
        let r = store.is_tombstoned(TENANT, DIGEST).await.expect("q");
        assert!(!r, "absent digest ⇒ Ok(false)");
        assert_eq!(
            inner.reads(),
            before,
            "fast path must NOT touch the inner store"
        );
    }

    /// (b) A digest tombstoned THROUGH the wrapper → `Ok(true)` and stays true.
    #[tokio::test]
    async fn bloom_through_write_is_true_and_stays_true() {
        let inner = Arc::new(CountingInner::new());
        let store = BloomTombstoneStore::with_params(
            inner.clone(),
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        );
        warm(&store, TENANT).await;
        assert!(store.upsert(TENANT, DIGEST, "dsr", 1).await.is_ok());
        // Bloom hit (we wrote it) ⇒ falls through to inner, which is authoritative.
        assert!(
            store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
            "tombstoned ⇒ true"
        );
        assert!(
            store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
            "stays true"
        );
    }

    /// (c) NO FALSE NEGATIVE: a digest tombstoned DIRECTLY in the inner store
    /// (another instance) is correctly reported `true` AFTER a refresh — and
    /// the bounded-window behaviour is asserted: with a ZERO window EVERY
    /// lookup is stale ⇒ always falls through ⇒ always authoritative.
    #[tokio::test]
    async fn bloom_no_false_negative_cross_instance_after_refresh() {
        let inner = Arc::new(CountingInner::new());
        // Zero window ⇒ every tenant_bloom call re-stamps ⇒ just_reloaded=true
        // ⇒ every lookup falls through to the authoritative inner store. This
        // is the worst case (window→0 = the wrapper is a pass-through, never a
        // false negative) and the boundary the bound is measured against.
        let store = BloomTombstoneStore::with_params(
            inner.clone(),
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            Duration::ZERO,
        );
        // Another instance writes the tombstone directly to D1 (not via bloom).
        inner.seed_inner(TENANT, DIGEST);
        // The wrapper must NEVER report this absent.
        assert!(
            store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
            "cross-instance tombstone must read true after refresh"
        );

        // And the WITHIN-window staleness bound: with a long window, the FIRST
        // touch of a tenant falls through (catches the cross-instance write);
        // subsequent fast-path lookups of OTHER absent digests skip D1, but a
        // cross-instance write that lands AFTER the bloom was loaded is only
        // guaranteed visible after the window elapses (next reload). Assert the
        // first-touch catch:
        let inner2 = Arc::new(CountingInner::new());
        let store2 = BloomTombstoneStore::with_params(
            inner2.clone(),
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        );
        inner2.seed_inner(TENANT, DIGEST); // present before first touch
        assert!(
            store2.is_tombstoned(TENANT, DIGEST).await.expect("q"),
            "first-touch reload catches the cross-instance tombstone"
        );
    }

    /// (c2) F-010 REGRESSION — WITHIN-WINDOW false-negative is closed by the D1
    /// reload-seed. A digest tombstoned by a SEPARATE writer (the erase route's
    /// own `D1TombstoneStore`, which does NOT share this bloom) is present in D1
    /// before the bloom's first (re)load. With a LONG window the bloom is
    /// (re)loaded exactly once on first touch; the OLD code seeded the fresh bloom
    /// from in-process writes ONLY, so the FIRST read fell through (410) but
    /// STAMPED an EMPTY bloom — every subsequent read within the ~30s window then
    /// hit the empty-bloom fast path and returned `Ok(false)` WITHOUT consulting
    /// D1 (the 410→404/200 GDPR downgrade). The fix seeds the reload from the
    /// authoritative D1 set, so the SECOND (and every) within-window read still
    /// sees the digest in the bloom → falls through → stays `true`. This asserts
    /// the in-code invariant #1 ("NO FALSE NEGATIVE … within at most one window").
    #[tokio::test]
    async fn bloom_no_within_window_false_negative_for_cross_writer_tombstone() {
        let inner = Arc::new(CountingInner::new());
        // LONG window: the bloom is (re)loaded ONCE, so the only thing standing
        // between a second read and a false negative is the reload SEED (F-010).
        let store = BloomTombstoneStore::with_params(
            inner.clone(),
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        );
        // A SEPARATE writer (not via this bloom) tombstones the digest in D1.
        inner.seed_inner(TENANT, DIGEST);
        // First read: triggers the one-shot (re)load → seeds the bloom from D1 →
        // falls through → true.
        assert!(
            store.is_tombstoned(TENANT, DIGEST).await.expect("q1"),
            "first within-window read of a cross-writer tombstone must be true"
        );
        // SECOND read, still WITHIN the (long) window — the regression point. The
        // bloom is NOT re-stamped (window not elapsed), so the fast path runs; it
        // MUST still see the seeded bit and fall through to D1 → true. Pre-fix
        // this returned Ok(false) (false negative / GDPR 410 bypass).
        assert!(
            store.is_tombstoned(TENANT, DIGEST).await.expect("q2"),
            "F-010: second within-window read MUST NOT be a false negative"
        );
        // And a THIRD, to be thorough.
        assert!(
            store.is_tombstoned(TENANT, DIGEST).await.expect("q3"),
            "F-010: stays true for every within-window read"
        );
        // A genuinely-absent OTHER digest still fast-paths to false (the seed did
        // not over-set membership for unrelated digests).
        assert!(
            !store.is_tombstoned(TENANT, "00absent00").await.expect("q4"),
            "an un-tombstoned digest still reads absent"
        );
    }

    /// (d) False-positive path: a bloom HIT on a digest that is NOT actually
    /// tombstoned falls through to the inner store and returns its authoritative
    /// `Ok(false)` — never a wrongful 410. We force a "hit" by writing the
    /// digest through the wrapper (sets the bits) but NOT into the inner set
    /// (simulating a bloom bit set with no real tombstone, e.g. an upsert whose
    /// inner write later rolled back). Here we use the erroring-free inner and
    /// assert the authoritative answer wins.
    #[tokio::test]
    async fn bloom_false_positive_falls_through_to_authoritative_inner() {
        let inner = Arc::new(CountingInner::new());
        let store = BloomTombstoneStore::with_params(
            inner.clone(),
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        );
        warm(&store, TENANT).await;
        // Set the bloom bit directly (a "false positive": bit set, no inner row).
        let tb = store
            .fresh_tenant_bloom(TENANT)
            .expect("warmed bloom is fresh");
        tb.bloom
            .insert(&BloomTombstoneStore::bloom_key(TENANT, DIGEST));
        let before = inner.reads();
        let r = store.is_tombstoned(TENANT, DIGEST).await.expect("q");
        assert!(
            !r,
            "bloom false-positive must defer to the authoritative inner Ok(false)"
        );
        assert_eq!(
            inner.reads(),
            before + 1,
            "maybe-present must consult the inner store exactly once"
        );
    }

    /// (e) Inner error propagates UNCHANGED on the maybe-present path
    /// (preserves today's fail-OPEN semantics — invariant 3).
    #[tokio::test]
    async fn bloom_inner_error_propagates_on_maybe_path() {
        let inner: Arc<dyn TombstoneStore> = Arc::new(ErroringInner);
        let store = BloomTombstoneStore::with_params(
            inner,
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        );
        warm(&store, TENANT).await; // first touch also errors, fine
                                    // Write through to set a bloom bit → guarantees a maybe-present probe.
        let _ = store.upsert(TENANT, DIGEST, "r", 1).await;
        let err = store.is_tombstoned(TENANT, DIGEST).await;
        assert_eq!(
            err,
            Err("d1 fault".to_owned()),
            "inner Err must propagate unchanged"
        );
    }

    /// (f) Concurrency: many tasks read + write concurrently; no panic, no torn
    /// state, and every through-the-wrapper write is observed true afterwards.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn bloom_concurrent_reads_and_writes() {
        let inner = Arc::new(CountingInner::new());
        let store = Arc::new(BloomTombstoneStore::with_params(
            inner,
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        ));
        let mut handles = Vec::new();
        for i in 0..64u32 {
            let s = store.clone();
            handles.push(tokio::spawn(async move {
                let d = format!("digest-{i:04}");
                // half write, all read.
                if i % 2 == 0 {
                    let _ = s.upsert(TENANT, &d, "r", i64::from(i)).await;
                }
                let _ = s.is_tombstoned(TENANT, &d).await;
            }));
        }
        for h in handles {
            h.await.expect("task");
        }
        // Every even digest was written THROUGH the wrapper ⇒ must read true.
        for i in (0..64u32).step_by(2) {
            let d = format!("digest-{i:04}");
            assert!(
                store.is_tombstoned(TENANT, &d).await.expect("q"),
                "written digest {d} must be tombstoned"
            );
        }
    }

    /// Bloom unit: one-sided error guarantee — once inserted, `contains` is true.
    #[test]
    fn bloom_unit_no_false_negative_and_bounded() {
        let b = Bloom::new(DEFAULT_BLOOM_BITS, DEFAULT_BLOOM_HASHES);
        for i in 0..1000 {
            b.insert(&format!("k{i}"));
        }
        for i in 0..1000 {
            assert!(
                b.contains(&format!("k{i}")),
                "inserted key must always be present"
            );
        }
        // Bounded: the bit-array size is fixed regardless of element count.
        assert_eq!(b.words.len(), DEFAULT_BLOOM_BITS / 64);
    }

    // ──────────────────────────────────────────────────────────────────────
    // F-004 — TombstoneGatedCasHandler (the centralized shared-seam erase gate)
    // ──────────────────────────────────────────────────────────────────────

    use corelink_handler_cas::{
        CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasReadHandler,
        CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest, CasWriteResponse,
    };

    /// Minimal CAS handler fake: read/exists always succeed, write always
    /// commits, delete always succeeds — so any 404/410/503 in the tests below
    /// can ONLY come from the tombstone gate, not from the underlying handler.
    #[derive(Debug, Default)]
    struct AlwaysOkCas {
        wrote: Mutex<Vec<(String, String)>>,
        deleted: Mutex<Vec<(String, String)>>,
    }
    impl CasReadHandler for AlwaysOkCas {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            Ok(CasReadResponse::new(b"live-bytes".to_vec(), req.hash))
        }
    }
    impl CasWriteHandler for AlwaysOkCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            lock_or_recover(&self.wrote).push((req.tenant.clone(), req.claimed_hash.clone()));
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }
    impl CasDeleteHandler for AlwaysOkCas {
        fn delete(&self, req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
            lock_or_recover(&self.deleted).push((req.tenant.clone(), req.hash.clone()));
            Ok(CasDeleteResponse::new(true))
        }
    }

    fn gated_with(
        tombstones: Arc<dyn TombstoneStore>,
    ) -> (TombstoneGatedCasHandler, Arc<AlwaysOkCas>) {
        let backing = Arc::new(AlwaysOkCas::default());
        let gated = TombstoneGatedCasHandler::new(
            backing.clone() as Arc<dyn CasReadHandler>,
            backing.clone() as Arc<dyn CasWriteHandler>,
            backing.clone() as Arc<dyn CasDeleteHandler>,
            tombstones,
        );
        (gated, backing)
    }

    fn read_req() -> CasReadRequest {
        CasReadRequest::new(
            TENANT.to_owned(),
            DIGEST.to_owned(),
            format!("anon@{TENANT}"),
            TENANT.to_owned(),
            0,
        )
    }
    fn write_req() -> CasWriteRequest {
        CasWriteRequest::new(
            TENANT.to_owned(),
            DIGEST.to_owned(),
            b"resurrect".to_vec(),
            format!("anon@{TENANT}"),
            TENANT.to_owned(),
            0,
        )
    }

    /// A tombstoned read is refused at the SHARED seam (NotFound) — so NO
    /// surface (cargo/brew/npm/pip/bazel/turbo/oci) can serve erased bytes,
    /// even though only the native route has the inline 410 gate. (F-004 read.)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_read_of_tombstoned_is_notfound() {
        let ts = Arc::new(InMemoryTombstoneStore::new());
        ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
        let (gated, _backing) = gated_with(ts);
        let r = gated.read(read_req());
        assert!(
            matches!(r, Err(CasHandlerError::NotFound { .. })),
            "tombstoned read must be refused at the shared seam, got {r:?}"
        );
        // exists likewise reports absent.
        assert!(!gated.exists(read_req()).expect("exists"));
    }

    /// A non-tombstoned read delegates to the backing handler (live bytes).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_read_of_live_blob_delegates() {
        let ts = Arc::new(InMemoryTombstoneStore::new());
        let (gated, _backing) = gated_with(ts);
        let resp = gated.read(read_req()).expect("live read");
        assert_eq!(resp.bytes, b"live-bytes".to_vec());
        assert!(gated.exists(read_req()).expect("exists"));
    }

    /// A re-PUT of a tombstoned (tenant, hash) is REFUSED at the shared seam with
    /// the GONE sentinel — so erased bytes cannot be resurrected at the same
    /// content address via ANY write surface (the prior write path was ungated).
    /// (F-004 write — the resurrection vector.)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_write_of_tombstoned_is_refused_gone() {
        let ts = Arc::new(InMemoryTombstoneStore::new());
        ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
        let (gated, backing) = gated_with(ts);
        let r = gated.write(write_req());
        match r {
            Err(CasHandlerError::Internal(msg)) => {
                assert!(
                    msg.starts_with(TOMBSTONE_GONE_SENTINEL),
                    "re-PUT must carry the GONE sentinel, got {msg}"
                );
            }
            other => panic!("re-PUT of erased blob must be refused, got {other:?}"),
        }
        // The backing handler was NEVER reached — no resurrection.
        assert!(
            lock_or_recover(&backing.wrote).is_empty(),
            "no bytes were written"
        );
    }

    /// A write to a LIVE (non-tombstoned) hash delegates and commits normally.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_write_of_live_hash_delegates() {
        let ts = Arc::new(InMemoryTombstoneStore::new());
        let (gated, backing) = gated_with(ts);
        let resp = gated.write(write_req()).expect("live write");
        assert!(resp.durable);
        assert_eq!(lock_or_recover(&backing.wrote).len(), 1);
    }

    /// A tombstone-gate transport FAULT fails CLOSED on both read and write
    /// (UNAVAILABLE sentinel → 503) — never serve/commit when the erasure gate
    /// cannot be consulted (PEN-2/REV-S1).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_gate_fault_fails_closed() {
        let ts: Arc<dyn TombstoneStore> = Arc::new(ErroringInner);
        let (gated, backing) = gated_with(ts);
        match gated.read(read_req()) {
            Err(CasHandlerError::Internal(msg)) => {
                assert!(
                    msg.starts_with(TOMBSTONE_UNAVAILABLE_SENTINEL),
                    "read fail-closed, got {msg}"
                );
            }
            other => panic!("read must fail closed on gate fault, got {other:?}"),
        }
        match gated.write(write_req()) {
            Err(CasHandlerError::Internal(msg)) => {
                assert!(
                    msg.starts_with(TOMBSTONE_UNAVAILABLE_SENTINEL),
                    "write fail-closed, got {msg}"
                );
            }
            other => panic!("write must fail closed on gate fault, got {other:?}"),
        }
        assert!(
            lock_or_recover(&backing.wrote).is_empty(),
            "no write on gate fault"
        );
    }

    /// DELETE is a pass-through even for a tombstoned blob (idempotent erase
    /// must never be blocked by its own tombstone).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_delete_passes_through_even_when_tombstoned() {
        let ts = Arc::new(InMemoryTombstoneStore::new());
        ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
        let (gated, backing) = gated_with(ts);
        let req = CasDeleteRequest::new(
            TENANT.to_owned(),
            DIGEST.to_owned(),
            format!("anon@{TENANT}"),
            TENANT.to_owned(),
            0,
        );
        gated.delete(req).expect("delete passes through");
        assert_eq!(lock_or_recover(&backing.deleted).len(), 1);
    }

    // ──────────────────────────────────────────────────────────────────────
    // H3 — resurrection guard (authoritative write gate) + bloom-map eviction
    // ──────────────────────────────────────────────────────────────────────

    /// H3 (resurrection): after a SEPARATE writer (another container instance /
    /// region) erases a hash STRAIGHT to D1 — invisible to THIS instance's warm
    /// bloom within the staleness window — a re-PUT of that hash MUST be REFUSED
    /// (GONE), never silently resurrected. The READ fast-path is (tolerably)
    /// within-window blind (the bytes are already gone from R2), but the WRITE
    /// gate is authoritative. FAILS before the fix (write trusted the stale
    /// bloom `Ok(false)` → resurrection); PASSES after.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_write_after_cross_writer_erase_within_window_is_refused() {
        let inner = Arc::new(CountingInner::new());
        // LONG window: once warm, the bloom is NOT re-stamped, so a cross-writer
        // erase that lands AFTER the load is invisible to the fast path — the
        // exact H3 resurrection window.
        let bloom = Arc::new(BloomTombstoneStore::with_params(
            inner.clone(),
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        ));
        // Warm the tenant bloom past its one-shot just-reloaded epoch (the bloom
        // was seeded while the inner set was still empty).
        warm(&bloom, TENANT).await;
        // Another instance erases the hash DIRECTLY in D1 (never through this
        // bloom), AFTER this bloom was loaded.
        inner.seed_inner(TENANT, DIGEST);
        // The READ fast path is fooled within the window (the tolerable case:
        // R2 bytes already gone, a slipped GET 404s).
        assert!(
            !bloom.is_tombstoned(TENANT, DIGEST).await.expect("read q"),
            "read fast-path is within-window blind (expected)"
        );
        // The AUTHORITATIVE write gate is NOT fooled — the re-PUT is refused GONE.
        let (gated, backing) = gated_with(bloom);
        match gated.write(write_req()) {
            Err(CasHandlerError::Internal(msg)) => assert!(
                msg.starts_with(TOMBSTONE_GONE_SENTINEL),
                "H3: re-PUT after a cross-writer erase must be GONE, got {msg}"
            ),
            other => {
                panic!("H3: re-PUT of a cross-writer-erased blob must be refused, got {other:?}")
            }
        }
        assert!(
            lock_or_recover(&backing.wrote).is_empty(),
            "H3: erased bytes must NOT be resurrected"
        );
    }

    /// H3 (resurrection, control): a genuinely-LIVE hash still writes normally
    /// through the authoritative gate (the fix does not block legitimate PUTs).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gated_write_of_live_hash_through_bloom_still_commits() {
        let inner = Arc::new(CountingInner::new());
        let bloom = Arc::new(BloomTombstoneStore::with_params(
            inner,
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
        ));
        warm(&bloom, TENANT).await;
        let (gated, backing) = gated_with(bloom);
        let resp = gated.write(write_req()).expect("live write commits");
        assert!(resp.durable);
        assert_eq!(lock_or_recover(&backing.wrote).len(), 1);
    }

    /// H3 (OOM): the per-tenant bloom map is LRU-BOUNDED. Touching far more
    /// distinct tenants than the cap never grows the map past the cap, and the
    /// live-count gauge tracks it. Pre-fix the map grew one 128 KiB bloom per
    /// tenant forever (≈1.3 GB @ 10k tenants ⇒ OOM on the capped container).
    #[tokio::test]
    async fn bloom_map_is_lru_bounded_under_many_tenants() {
        const CAP: usize = 8;
        let inner = Arc::new(CountingInner::new());
        let store = BloomTombstoneStore::with_params_capped(
            inner,
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
            CAP,
        );
        // Touch 200 distinct tenants — each first touch mints (reloads) a bloom.
        for i in 0..200u32 {
            let tenant = format!("tenant-{i:04}");
            let _ = store.is_tombstoned(&tenant, DIGEST).await.expect("q");
        }
        // The map (and its gauge) never exceeds the cap despite 200 tenants.
        assert!(
            store.tenant_bloom_count() <= CAP as u64,
            "bloom map must stay LRU-bounded: {} > {CAP}",
            store.tenant_bloom_count()
        );
        // The gauge is live and the map fills exactly to the cap (evict-one-per
        // -new-insert steady state).
        assert_eq!(
            store.tenant_bloom_count(),
            CAP as u64,
            "map fills to — and holds at — the cap"
        );
    }

    /// H3 (eviction correctness): evicting a tenant's bloom is safe — a
    /// subsequent lookup for an evicted tenant reloads AUTHORITATIVELY from D1,
    /// so a tombstone written before eviction is still reported `true` (no
    /// false-negative introduced by eviction; invariant 1 preserved).
    #[tokio::test]
    async fn evicted_tenant_reload_still_sees_its_tombstone() {
        const CAP: usize = 2;
        let inner = Arc::new(CountingInner::new());
        // A tombstone for TENANT lives durably in D1 (any writer).
        inner.seed_inner(TENANT, DIGEST);
        let store = BloomTombstoneStore::with_params_capped(
            inner,
            DEFAULT_BLOOM_BITS,
            DEFAULT_BLOOM_HASHES,
            LONG_WINDOW,
            CAP,
        );
        // Prime TENANT (loads its bloom, seeded from D1 → reports true).
        assert!(store.is_tombstoned(TENANT, DIGEST).await.expect("q0"));
        // Churn other tenants past the cap to force TENANT's bloom out.
        for i in 0..8u32 {
            let t = format!("other-{i:04}");
            let _ = store.is_tombstoned(&t, DIGEST).await.expect("q");
        }
        assert_eq!(store.tenant_bloom_count(), CAP as u64, "still bounded");
        // TENANT's bloom was evicted; the next lookup reloads from D1 and MUST
        // still report the tombstone (authoritative — no eviction false-negative).
        assert!(
            store.is_tombstoned(TENANT, DIGEST).await.expect("q1"),
            "evicted tenant's durable tombstone must survive reload"
        );
    }
}
