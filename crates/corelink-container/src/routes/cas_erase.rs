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
//! fills. Until #254 lands the trait is wired in tests via
//! [`InMemoryBlobEraser`]; the env builder ([`build_state_from_env`]) returns
//! `None` (route fail-CLOSED / unmounted) so no half-built erase can run in
//! prod without the R2 transport.
//!
//! # Tombstone store
//!
//! [`TombstoneStore`] persists/queries the `cas_tombstone` D1 table (migration
//! `0067`). [`D1TombstoneStore`] backs it over [`crate::storage::d1_http`];
//! [`InMemoryTombstoneStore`] backs the unit tests.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use subtle::ConstantTimeEq;

use corelink_handler_cas_erase::{erase_outcome, prepare_erase, EraseOutcome};

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

/// Erase request body. `tenant` is the authenticated tenant; `reason` is a
/// bounded, free-form audit string.
#[derive(Debug, serde::Deserialize)]
struct EraseBody {
    tenant: String,
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

/// Build the route state from env.
///
/// Returns `Some` only when BOTH the D1 tombstone store AND the R2 blob eraser
/// build from env — i.e. once PR #254 lands and supplies the R2 eraser. Until
/// then this returns `None` and the route is NOT mounted (fail-CLOSED): the
/// container will never run a half-built erase that drops the tombstone without
/// deleting the bytes, or vice-versa. The orchestrator wires the real eraser at
/// the #254-merge integration point (see module docs).
#[must_use]
pub fn build_state_from_env(_internal_auth_key: Option<Arc<str>>) -> Option<CasEraseRouteState> {
    // The R2 blob eraser is the DSR Wave 1 (#254) adapter; not yet importable on
    // this baseline. Returning None keeps the route unmounted until #254 lands
    // and the orchestrator supplies the eraser at the integration seam.
    None
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

    const TEST_KEY: &str = "test-internal-auth-key-32-bytes-x";
    const TENANT: &str = "t1";
    const DIGEST: &str = "deadbeef0123";

    fn state() -> (CasEraseRouteState, Arc<InMemoryBlobEraser>, Arc<InMemoryTombstoneStore>) {
        let tombstones = Arc::new(InMemoryTombstoneStore::new());
        let eraser = Arc::new(InMemoryBlobEraser::new());
        let st = CasEraseRouteState {
            tombstones: tombstones.clone(),
            eraser: eraser.clone(),
            internal_auth_key: Arc::from(TEST_KEY),
        };
        (st, eraser, tombstones)
    }

    fn erase_req(auth: &str, tenant: &str, digest: &str, body_tenant: &str) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri(format!("/_internal/cas/{tenant}/{digest}/erase"));
        if !auth.is_empty() {
            b = b.header(INTERNAL_AUTH_HEADER, auth);
        }
        b.body(Body::from(
            serde_json::json!({ "tenant": body_tenant, "reason": "dsr" }).to_string(),
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
        // path tenant t1, body tenant t2 → cross-tenant.
        let resp = router(st)
            .oneshot(erase_req(TEST_KEY, "t1", DIGEST, "t2"))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(!eraser.was_erased("t2", DIGEST));
        assert!(!eraser.was_erased("t1", DIGEST));
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
}
