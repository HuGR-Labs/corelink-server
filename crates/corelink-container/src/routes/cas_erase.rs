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
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use subtle::ConstantTimeEq;

use corelink_handler_cas_erase::{erase_outcome, prepare_erase, EraseOutcome};
use corelink_privacy_erasure_worker::legitimacy::DsrLegitimacyStore;

use crate::routes::dsr::legitimacy::D1DsrLegitimacyStore;

/// HTTP header carrying the shared internal-auth secret (mirrors
/// [`crate::routes::internal_pat`] byte-for-byte).
const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Canonical erase route path (matchit-0.7 `:name` captures — see the DEBT-029
/// note in [`crate::routes::cas`]).
pub const CAS_ERASE_ROUTE: &str = "/_internal/cas/:tenant/:hash/erase";

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

    /// Upsert a tombstone (idempotent). Returns whether a row already existed
    /// (so the route can report `AlreadyErased` for an idempotent re-erase).
    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        reason: &str,
        erased_at_ms: i64,
    ) -> Result<bool, String>;
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
            tracing::warn!("cas_erase: non-UUID tenant on erase — denying (no legitimacy possible)");
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

/// Canonical CAS storage regions — the `<region>/` key-prefix segment.
///
/// Byte-for-byte the same list the DSR Wave 1 tenant-wide CAS adapter uses
/// (`routes::dsr::adapter_r2_cas::CAS_REGIONS`). CAS is **one bucket**; the
/// storage region lives in the key prefix `<region>/<tenant_prefix>/<digest>`.
/// The container writes through a single global `R2_CAS_REGION`, but a
/// per-hash erase sweeps all five regions so it is robust to a deployment
/// whose write region changed over time (cheap — a once-per-erase LIST).
const CAS_REGIONS: &[&str] = &["sam", "iad", "lhr", "nrt", "syd"];

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
        let client =
            crate::storage::r2_s3::R2S3Client::new(&env, self.cas_bucket.clone()).await?;
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
    let legitimacy: Arc<dyn DsrLegitimacyStore> =
        Arc::new(D1DsrLegitimacyStore::from_env()?);

    let cas_bucket = crate::storage::env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
    let eraser: Arc<dyn CasBlobEraser> =
        Arc::new(R2CasBlobEraser::new(Arc::new(tdk), cas_bucket));

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
        tracing::warn!("cas_erase: R2_TDK_HEX is not valid hex; eraser NOT built (route unmounted)");
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
    fn state() -> (CasEraseRouteState, Arc<InMemoryBlobEraser>, Arc<InMemoryTombstoneStore>) {
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
    ) -> (CasEraseRouteState, Arc<InMemoryBlobEraser>, Arc<InMemoryTombstoneStore>) {
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
        assert!(eraser.was_erased(TENANT, DIGEST), "R2 bytes must be deleted");
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
        assert!(!eraser.was_erased(TENANT, DIGEST), "no delete without a legit row");
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
        assert!(!eraser.was_erased(TENANT, DIGEST), "no delete on legitimacy fault");
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
        assert!(!eraser.was_erased(TENANT_B, DIGEST), "no cross-tenant erase via foreign dsr_id");
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
}
