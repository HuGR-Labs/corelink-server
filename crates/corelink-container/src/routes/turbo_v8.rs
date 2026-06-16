//! `GET/PUT /v8/artifacts/:hash` + `POST /v8/artifacts/events` +
//! `POST /v8/artifacts/status` — Vercel Turborepo remote-cache protocol.
//!
//! # Why this module exists
//!
//! Turborepo (by Vercel) supports any HTTP server that implements the
//! Vercel Remote Cache `/v8/artifacts` API. Wiring CoreLink as that server
//! lets teams replace Vercel's paid remote cache with their own CAS-backed
//! store by setting:
//!
//! ```text
//! TURBO_API=https://corelink-api.humangr.com
//! TURBO_TOKEN=<their CoreLink PAT>
//! ```
//!
//! # Backing store (Phase 0 / v1)
//!
//! The route state is backed by [`corelink_turbo_bridge::adapter::InMemoryKvStore`]
//! which stores artifacts in RAM and does NOT persist across container restarts.
//! This is the correct starting point — the backing store is a swappable port
//! trait ([`CasReadStore`] / [`CasWriteStore`]).
//!
//! **TODO(v2):** Replace `InMemoryKvStore` with a thin `R2KvStore` impl backed
//! by R2 directly (separate bucket / prefix — e.g. `corelink-turbo-cache-prod`).
//! See `specs/TODO-turbo-r2-backing-store.md` for the migration plan.
//! The route handlers and audit surface are unchanged by the swap.
//!
//! # Tenant isolation
//!
//! The isolation tenant is the DO-injected, PAT-resolved authenticated tenant
//! (`AuthTenant` / `x-corelink-tenant-id`), threaded in as `caller_tenant`. The
//! `teamId` query parameter is required on PUT and GET (missing `teamId` → 400)
//! but is NOT a security boundary: it is a Turborepo team label demoted to a
//! logical sub-namespace WITHIN the authenticated tenant. Storage is keyed
//! `tenant = auth.0`, `key = "<teamId>/<hash>"`, so teams under one tenant stay
//! partitioned while cross-tenant access is impossible (no authenticated tenant
//! ⇒ 401 fail-CLOSED at the extractor).
//!
//! # Hash semantics
//!
//! Turbo's artifact hash is OPAQUE. It is stored verbatim as the KV key without
//! any hash-integrity verification. Hashes longer than
//! [`corelink_turbo_bridge::MAX_HASH_LEN`] (128 chars) are rejected with 400
//! as a DoS guard — see [`corelink_turbo_bridge::TurboBridgeError::HashTooLong`].
//!
//! # Route constants
//!
//! MUST use the matchit-0.7 `:name` capture syntax.  `{name}` form is silently
//! treated as a literal path segment in matchit 0.7.3 — see DEBT-029 comment in
//! `routes/cas.rs` for the full rationale.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::wall_clock::{SystemWallClock, WallClock};
use corelink_turbo_bridge::{
    adapter::{CasAdapterTurboHandler, InMemoryKvStore},
    audit::InMemoryTurboAuditSink,
    error::validate_team_id,
    TurboArtifactHandler, TurboBridgeError, TurboEventsRequest, TurboEventsResponse,
    TurboGetRequest, TurboGetResponse, TurboPutRequest, TurboPutResponse, TurboStatusRequest,
    TurboStatusResponse,
};

// ── Route constants ───────────────────────────────────────────────────────────

/// `GET /v8/artifacts/:hash` — retrieve a Turborepo build artifact.
///
/// MUST use matchit-0.7 `:name` syntax (not `{name}`).
pub const TURBO_GET_ROUTE: &str = "/v8/artifacts/:hash";

/// `PUT /v8/artifacts/:hash` — store a Turborepo build artifact.
///
/// Same template as GET; axum disambiguates by HTTP method.
pub const TURBO_PUT_ROUTE: &str = "/v8/artifacts/:hash";

/// `POST /v8/artifacts/events` — accept-and-drop Turborepo telemetry.
pub const TURBO_EVENTS_ROUTE: &str = "/v8/artifacts/events";

/// `POST /v8/artifacts/status` — static remote-cache enabled check.
pub const TURBO_STATUS_ROUTE: &str = "/v8/artifacts/status";

/// Per-route request-body cap for the `/v8/artifacts/*` routes (100 MiB).
///
/// Turbo build artifacts are legitimately larger than the 10 MiB global body
/// limit applied in `main.rs`, so the Turbo router layers this larger
/// `DefaultBodyLimit` (which axum honours as the innermost limit) — while still
/// bounding the body so an authenticated PAT cannot OOM the shared container.
pub const TURBO_BODY_LIMIT_BYTES: usize = 100 * 1024 * 1024;

/// Maximum concurrent in-flight `PUT /v8/artifacts/:hash` requests for a
/// single tenant. Excess PUTs are rejected with 429 (Too Many Requests).
///
/// Rationale: each Turbo PUT buffers up to `TURBO_BODY_LIMIT_BYTES` (100 MiB)
/// in memory. Without a concurrency cap, a single authenticated tenant can open
/// N concurrent PUTs and consume N × 100 MiB of heap, OOM-ing their container.
/// Capping at 4 bounds peak per-tenant working set to ~400 MiB while still
/// allowing realistic parallel builds (Turborepo's default parallelism is 2–4
/// concurrent tasks). Excess requests get 429, not 503 — the client retries.
pub const TURBO_PUT_CONCURRENCY_LIMIT: usize = 4;

// ── Query parameters ──────────────────────────────────────────────────────────

/// Query parameters for `GET /v8/artifacts/:hash` and
/// `PUT /v8/artifacts/:hash`.
///
/// `teamId` is **required**; missing value → 400 (axum rejects the
/// extraction before the handler is called).  `slug` is optional and
/// informational.
#[derive(Debug, Deserialize)]
pub struct ArtifactQuery {
    /// Vercel team identifier — a logical sub-namespace WITHIN the authenticated
    /// tenant, NOT the isolation tenant. Forms the storage key prefix
    /// (`"<teamId>/<hash>"`); the isolation tenant is `AuthTenant`.
    #[serde(rename = "teamId")]
    pub team_id: String,
    /// Informational slug (repo name, etc.) — logged but not partitioned.
    #[serde(default)]
    pub slug: String,
}

// ── Response DTOs ─────────────────────────────────────────────────────────────

/// Serialised body for `PUT /v8/artifacts/:hash` success response.
///
/// Vercel spec: `{"urls": ["..."]}`
#[derive(Debug, Serialize)]
pub struct PutArtifactResponse {
    /// Storage URL(s) for the stored artifact.  Turbo treats 200 + this body
    /// as "uploaded successfully"; the URL is informational.
    pub urls: Vec<String>,
}

// ── Route state ───────────────────────────────────────────────────────────────

/// Shared state for `/v8/artifacts/*` routes.
///
/// Holds a single `Arc<dyn TurboArtifactHandler>` so the backing store is
/// swappable behind the port trait without changing the route layer.  See
/// [`build_handlers`] for Phase 0 wiring using `InMemoryKvStore`.
#[derive(Clone)]
pub struct TurboRouteState {
    /// Handler implementing all four Turbo API verbs.
    pub handler: Arc<dyn TurboArtifactHandler>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed); checked at the TOP of the billable
    /// artifact GET/PUT handlers, AFTER the scope gate, BEFORE storage. `None`
    /// in dev/CI (not enforced). The telemetry `events`/static `status` verbs
    /// are NOT billable and are not gated. Set by `routes::build_with_factory`.
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production; re-runs the full Argon2id Option-B verify
    /// at the TOP of the billable artifact handlers, AFTER scope, BEFORE storage.
    /// `None` in dev/CI (skipped). See [`crate::native_pat_gate`].
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
    /// Optional per-tenant storage byte accountant (red-team finding #1).
    /// `Some` in production — Turbo writes to R2 directly (via the `R2KvStore`
    /// handler), so a successful PUT `accrue`s its body bytes (over-cap ⇒ 402 /
    /// fault ⇒ 503). `None` in dev/CI. See [`crate::byte_accounting`].
    pub bytes: Option<Arc<crate::byte_accounting::ByteAccountant>>,
    /// Per-tenant in-flight PUT concurrency counter (finding #2 — pre-buffer
    /// self-DoS guard).
    ///
    /// Maps `tenant_id → count` of PUT requests currently in-flight (body
    /// buffered in memory). The reservation is taken by the
    /// [`PutConcurrencyGuard`] `FromRequestParts` extractor — which axum runs
    /// BEFORE the `body: Bytes` extractor — so a 6th concurrent PUT is rejected
    /// 429 BEFORE its (up to 100 MiB) body is read into the heap. The RAII guard
    /// the extractor yields releases the slot when the handler returns.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
}

impl core::fmt::Debug for TurboRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TurboRouteState")
            .field("put_inflight", &"Arc<Mutex<HashMap<..>>>")
            .finish_non_exhaustive()
    }
}

// ── Pre-buffer PUT concurrency guard (finding #2) ──────────────────────────────

/// RAII release of one per-tenant in-flight PUT slot.
///
/// Decrements the tenant's `put_inflight` count on `Drop`, so the slot is freed
/// on EVERY return path (success, handler error, panic). Carried out of the
/// [`PutConcurrencyGuard`] extractor into the handler so the slot stays held for
/// the whole request lifetime (including the body buffer + storage write).
pub(crate) struct PutSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for PutSlot {
    fn drop(&mut self) {
        if let Ok(mut g) = self.inflight.lock() {
            if let Some(c) = g.get_mut(&self.tenant_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    g.remove(&self.tenant_key);
                }
            }
        }
    }
}

/// `FromRequestParts` extractor that reserves a per-tenant in-flight PUT slot.
///
/// # Why an extractor (finding #2 — pre-buffer OOM guard)
///
/// The slot reservation USED to live inside `handle_put`, AFTER the
/// `body: axum::body::Bytes` extractor had already buffered up to
/// [`TURBO_BODY_LIMIT_BYTES`] (100 MiB) into the heap. So a burst of concurrent
/// PUTs each buffered ~100 MiB BEFORE the cap-check ran — `burst × 100 MiB` of
/// transient heap could OOM the shared container regardless of the cap.
///
/// As a [`FromRequestParts`] extractor this runs while only request **parts**
/// (headers/method/uri) are available — axum 0.7 runs every `FromRequestParts`
/// extractor BEFORE the single `FromRequest` body extractor (`Bytes`). Declaring
/// it AHEAD of `body: Bytes` in the handler signature therefore does the
/// lock+count+increment BEFORE a single body byte is read: an over-cap request
/// is rejected 429 with no buffering. The yielded [`PutSlot`] RAII-releases the
/// slot when the handler returns.
pub(crate) struct PutConcurrencyGuard {
    /// The reserved slot — released on drop. Held by the handler for the whole
    /// request (it is NOT dropped at the end of extraction).
    _slot: PutSlot,
}

#[axum::async_trait]
impl axum::extract::FromRequestParts<TurboRouteState> for PutConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        // The isolation tenant is the DO-injected, PAT-resolved authenticated
        // tenant. A missing/empty header fails CLOSED (401) — the same gate the
        // `AuthTenant` extractor enforces; we mirror it here so the reservation
        // is per AUTHENTICATED tenant (an unauthenticated request never reserves
        // a slot, and never buffers a body).
        let tenant = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        // Sentinels the Worker/DO use for non-tenant traffic — never a real
        // tenant (mirrors `auth_tenant::AuthTenant`'s set).
        const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];
        if tenant.is_empty() || TENANT_SENTINELS.contains(&tenant) {
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        let tenant_key = tenant.to_owned();

        {
            let mut inflight = match state.put_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_key,
                        error = %e,
                        "turbo PUT concurrency tracker mutex poisoned; failing closed"
                    );
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= TURBO_PUT_CONCURRENCY_LIMIT {
                tracing::warn!(
                    tenant_id = %tenant_key,
                    in_flight = *count,
                    limit = TURBO_PUT_CONCURRENCY_LIMIT,
                    "turbo PUT concurrency limit reached; returning 429 BEFORE body buffering"
                );
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent artifact uploads",
                )
                    .into_response());
            }
            *count += 1;
        }

        Ok(Self {
            _slot: PutSlot {
                inflight: Arc::clone(&state.put_inflight),
                tenant_key,
            },
        })
    }
}

// ── Handler construction ──────────────────────────────────────────────────────

/// Build the [`TurboRouteState`] for the current build target and runtime.
///
/// # Runtime backing-store selection (v2)
///
/// When storage credentials are configured (`StorageEnv::from_env()`), the
/// handler is backed by the **durable** [`R2KvStore`](crate::storage::r2_kv::R2KvStore)
/// (closes the former `TODO(v2)`: artifacts now persist across container
/// restarts). Otherwise — dev / CI without secrets — it falls back to the
/// in-RAM [`InMemoryKvStore`]. Mirrors the `cas`/`ac` `build_handlers`
/// fallback convention; the route handlers and audit/validation logic are
/// identical for both backings (opaque-key semantics, no hash verify).
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn build_handlers() -> TurboRouteState {
    let audit = Arc::new(InMemoryTurboAuditSink::new());

    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_kv, StorageEnv};
        if StorageEnv::from_env().is_some() {
            // `block_in_place` rationale: build_handlers runs inside the
            // `#[tokio::main]` multi-thread runtime; a bare `block_on`
            // from a running future hangs. Mirrors `cas`/`ac` handlers.
            let handle = tokio::runtime::Handle::current();
            let built =
                tokio::task::block_in_place(|| handle.block_on(r2_kv::build_r2_kv_from_env()));
            match built {
                Some(Ok(store)) => {
                    let store = Arc::new(store);
                    let read: Arc<dyn corelink_turbo_bridge::adapter::CasReadStore> = store.clone();
                    let write: Arc<dyn corelink_turbo_bridge::adapter::CasWriteStore> = store;
                    tracing::info!("Turbo handler: R2KvStore (durable storage)");
                    let handler: Arc<dyn TurboArtifactHandler> =
                        Arc::new(CasAdapterTurboHandler::new(read, write, audit));
                    return TurboRouteState {
                        handler,
                        quota: None,
                        pat_gate: None,
                        bytes: None,
                        put_inflight: Arc::new(Mutex::new(HashMap::new())),
                    };
                }
                Some(Err(e)) => {
                    // CAA-360 #7: storage creds ARE present but R2KvStore refused
                    // to build → do NOT silently fall back to the non-durable
                    // InMemory store (fail-OPEN → silent data loss). Mount the
                    // fail-CLOSED UnavailableTurboHandler so every verb 503s LOUDLY
                    // until storage is fixed (mirrors cas.rs UnavailableCasHandler).
                    tracing::error!(error = %e, "Turbo R2KvStore build failed; mounting fail-CLOSED 503 handler (audit #7)");
                    return TurboRouteState {
                        handler: Arc::new(UnavailableTurboHandler),
                        quota: None,
                        pat_gate: None,
                        bytes: None,
                        put_inflight: Arc::new(Mutex::new(HashMap::new())),
                    };
                }
                None => {}
            }
        }
    }

    tracing::info!("Turbo handler: InMemoryKvStore (no storage credentials configured)");
    let store = Arc::new(InMemoryKvStore::new());
    let handler: Arc<dyn TurboArtifactHandler> =
        Arc::new(CasAdapterTurboHandler::new(store.clone(), store, audit));
    TurboRouteState {
        handler,
        quota: None,
        pat_gate: None,
        bytes: None,
        put_inflight: Arc::new(Mutex::new(HashMap::new())),
    }
}

/// Build the axum `Router` exposing all four `/v8/artifacts/*` routes.
///
/// Routes registered:
/// - `GET  /v8/artifacts/:hash`    → [`handle_get`]
/// - `PUT  /v8/artifacts/:hash`    → [`handle_put`]
/// - `POST /v8/artifacts/events`   → [`handle_events`]
/// - `POST /v8/artifacts/status`   → [`handle_status`]
///
/// The `events` and `status` routes must be registered BEFORE the `:hash`
/// wildcard route so matchit resolves the literals first.
pub fn router(state: TurboRouteState) -> Router {
    Router::new()
        // Fixed-path routes registered BEFORE the wildcard `:hash` routes so
        // matchit prefers the literal segments `events` / `status` over the
        // capture.
        .route(TURBO_EVENTS_ROUTE, post(handle_events))
        .route(TURBO_STATUS_ROUTE, post(handle_status))
        .route(TURBO_GET_ROUTE, get(handle_get).put(handle_put))
        // Per-route body cap: Turbo build artifacts are legitimately larger than
        // the 10 MiB global limit set in `main.rs`. This inner `DefaultBodyLimit`
        // layer overrides the outer global default for the `/v8/artifacts/*`
        // routes only (axum honours the innermost limit) while still bounding the
        // body at 100 MiB so a PAT cannot OOM the shared container.
        .layer(axum::extract::DefaultBodyLimit::max(TURBO_BODY_LIMIT_BYTES))
        .with_state(state)
}

// ── Route handlers ────────────────────────────────────────────────────────────

/// `GET /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Returns 200 + raw artifact bytes on hit, 404 on miss, 400 on bad params,
/// 403 on cross-tenant, 503 on audit-closed.
async fn handle_get(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): Turbo GET is a cache READ — require
    // `cas:rw` or `cas:r`. BEFORE any audit or storage. NO-OP for `cas:rw`.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Isolation tenant is the DO-injected, PAT-resolved authenticated tenant
    // (`auth.0`) — NOT the client `teamId`.  `teamId` is a Turborepo team label
    // demoted to a logical sub-namespace inside the authenticated tenant.
    let caller_tenant = auth.0;
    // DoS / key-aliasing guard: reject empty / overlong / `/`-bearing `teamId`
    // (it is interpolated into the storage key) BEFORE any audit or storage.
    // Mirrors the MAX_HASH_LEN guard; maps to 400 via `map_err`.
    if let Err(e) = validate_team_id(&params.team_id) {
        return map_err(e);
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Err(resp) = gate.verify(&caller_tenant, bearer).await {
            return resp;
        }
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost, AFTER the scope gate, BEFORE storage. 402 over-ceiling /
    // 503 fail-CLOSED. `None` in dev/CI ⇒ not enforced.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&caller_tenant).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = TurboGetRequest::new(
        hash,
        params.team_id,
        params.slug,
        // Principal is filled in by the auth middleware in production;
        // Phase 0 uses a placeholder.
        format!("anon@{caller_tenant}"),
        caller_tenant,
        now_ms,
    );
    match state.handler.get(req) {
        Ok(resp) => (StatusCode::OK, resp.bytes).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Accepts raw `application/octet-stream` body.  Returns 200 +
/// `{"urls": [...]}` on success.
async fn handle_put(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // finding #2: the concurrency reservation is a `FromRequestParts` extractor
    // declared AHEAD of `body: Bytes`, so axum runs it (lock+count+increment,
    // 429 on over-cap) BEFORE the body is buffered into the heap. Holding
    // `_concurrency` for the whole handler keeps the slot reserved until return;
    // its `PutSlot` RAII-releases on drop.
    _concurrency: PutConcurrencyGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): Turbo PUT is a cache WRITE — require
    // `cas:rw`. A read-only (`cas:r`) token is rejected here. NO-OP for
    // current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Isolation tenant is the authenticated `auth.0`, not the client `teamId`.
    let caller_tenant = auth.0;
    // DoS / key-aliasing guard on `teamId` (storage-key prefix) before any
    // audit or storage. Mirrors the MAX_HASH_LEN guard; maps to 400.
    if let Err(e) = validate_team_id(&params.team_id) {
        return map_err(e);
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Err(resp) = gate.verify(&caller_tenant, bearer).await {
            return resp;
        }
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) — see
    // `handle_get`. AFTER the scope gate, BEFORE storage.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&caller_tenant).await {
            return resp;
        }
    }

    let now_ms = SystemWallClock.now_ms();
    let byte_len = i64::try_from(body.len()).unwrap_or(i64::MAX);
    let req = TurboPutRequest::new(
        hash,
        params.team_id,
        params.slug,
        body.to_vec(),
        None, // duration_ms — Phase 0: not parsed from headers
        format!("anon@{caller_tenant}"),
        caller_tenant.clone(),
        now_ms,
    );
    match state.handler.put(req) {
        Ok(resp) => {
            // Storage byte accounting (finding #1): Turbo writes the artifact to
            // R2 directly, so accrue its bytes AFTER the write committed, BEFORE
            // returning success. Over-cap ⇒ 402; transport fault ⇒ 503
            // fail-CLOSED. (Turbo's KV is opaque-keyed with no durable/idempotent
            // bit on the response, so every stored PUT is charged.)
            if let Some(acc) = state.bytes.as_ref() {
                match acc.accrue(&caller_tenant, byte_len).await {
                    Ok(crate::byte_accounting::AccrueOutcome::Accrued) => {}
                    Ok(crate::byte_accounting::AccrueOutcome::OverCap) => {
                        return (StatusCode::PAYMENT_REQUIRED, "storage quota exceeded")
                            .into_response();
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "turbo: byte accrual failed; failing closed");
                        return (
                            StatusCode::SERVICE_UNAVAILABLE,
                            "storage accounting unavailable",
                        )
                            .into_response();
                    }
                }
            }
            let body = PutArtifactResponse { urls: resp.urls };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v8/artifacts/events`
///
/// Accept-and-drop: any body accepted, always returns 200.  Turbo telemetry
/// is not CoreLink's to own — no state mutation, no audit emit.
///
/// F3 (defense-in-depth): require the `AuthTenant` extractor — fail-CLOSED 401
/// on a missing/sentinel tenant. The Worker PAT-gates this path, but the
/// container must not trust that; an unauthenticated request never reaches the
/// handler. `AuthTenant` is `FromRequestParts`, so it precedes the `Bytes`
/// body extractor (axum 0.7 ordering rule).
async fn handle_events(
    State(state): State<TurboRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let principal = format!("anon@{}", auth.0);
    let req = TurboEventsRequest::new(body.to_vec(), principal, 0u64);
    match state.handler.events(req) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /v8/artifacts/status`
///
/// Returns static `{"status":"enabled"}`.  No request body required.
///
/// F3 (defense-in-depth): require the `AuthTenant` extractor — fail-CLOSED 401
/// on a missing/sentinel tenant. `AuthTenant` is `FromRequestParts`, so it
/// precedes the body extractor (axum 0.7 ordering rule).
async fn handle_status(
    State(state): State<TurboRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    _req: axum::extract::Request,
) -> impl IntoResponse {
    let principal = format!("anon@{}", auth.0);
    let _status_req = TurboStatusRequest::new(principal, 0u64);
    match state.handler.status() {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(e) => map_err(e),
    }
}

// ── Error mapping ─────────────────────────────────────────────────────────────

/// Sentinel prefix on a [`TurboBridgeError::Internal`] message that [`map_err`]
/// maps to HTTP **503** (storage unavailable) rather than the generic 500.
const TURBO_STORAGE_UNAVAILABLE_SENTINEL: &str = "turbo-storage-unavailable: ";

/// Fail-CLOSED stand-in mounted when storage creds ARE present but the durable
/// `R2KvStore` refused to build (CAA-360 #7). A silent `InMemoryKvStore` fallback
/// there would serve a NON-durable cache with no alarm (fail-OPEN → silent data
/// loss); every verb here instead returns a sentinel `Internal` error that
/// [`map_err`] maps to 503. Mirrors `cas.rs::UnavailableCasHandler`.
#[derive(Debug)]
struct UnavailableTurboHandler;

impl UnavailableTurboHandler {
    fn unavailable() -> TurboBridgeError {
        TurboBridgeError::Internal(format!(
            "{TURBO_STORAGE_UNAVAILABLE_SENTINEL}R2KvStore refused to build"
        ))
    }
}

impl TurboArtifactHandler for UnavailableTurboHandler {
    fn put(&self, _req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn get(&self, _req: TurboGetRequest) -> Result<TurboGetResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn events(&self, _req: TurboEventsRequest) -> Result<TurboEventsResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn status(&self) -> Result<TurboStatusResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
}

/// Map a [`TurboBridgeError`] to the canonical HTTP response.
///
/// All error details are logged before mapping so operators have a
/// diagnostic record without exposing internals to callers.
fn map_err(e: TurboBridgeError) -> axum::response::Response {
    tracing::warn!(error = ?e, "turbo bridge error");
    match e {
        TurboBridgeError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found").into_response(),
        TurboBridgeError::HashTooLong { .. } => {
            (StatusCode::BAD_REQUEST, "hash too long").into_response()
        }
        TurboBridgeError::TeamIdInvalid { .. } => {
            (StatusCode::BAD_REQUEST, "invalid teamId").into_response()
        }
        TurboBridgeError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        TurboBridgeError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve/commit
            // without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // CAA-360 #7: storage-unavailable (R2KvStore refused to build) → 503,
        // distinct from a generic 500, so clients retry rather than treat it as
        // a permanent server fault.
        TurboBridgeError::Internal(ref msg)
            if msg.starts_with(TURBO_STORAGE_UNAVAILABLE_SENTINEL) =>
        {
            (StatusCode::SERVICE_UNAVAILABLE, "storage unavailable").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use tower::ServiceExt; // for `.oneshot()`

    /// Fixed authenticated tenant injected via `x-corelink-tenant-id` so the
    /// `AuthTenant` extractor (fail-CLOSED) admits the request. PUT/GET in a
    /// round-trip MUST share this value or the GET reads a different tenant's
    /// namespace and 404s.
    const TEST_AUTH_TENANT: &str = "11111111-1111-1111-1111-111111111111";

    /// Read+write cache scope, mirroring every current prod PAT. Requests
    /// that exercise GET/PUT must carry this in `x-corelink-scope` or the
    /// fail-CLOSED scope gate rejects them 403.
    const TEST_SCOPE_RW: &str = "cas:rw";

    /// Build a test fixture using `InMemoryKvStore` + `InMemoryTurboAuditSink`.
    fn fixture() -> TurboRouteState {
        build_handlers()
    }

    /// Build the router from the fixture state.
    fn test_router() -> Router {
        router(fixture())
    }

    // ── Route constant sanity ─────────────────────────────────────────────────

    #[test]
    fn route_constants_use_colon_syntax_not_curly_braces() {
        // Regression net for DEBT-029 — matchit 0.7.3 treats `{name}` as a
        // literal path segment rather than a capture variable.
        assert!(
            !TURBO_GET_ROUTE.contains('{'),
            "TURBO_GET_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
        );
        assert!(TURBO_GET_ROUTE.contains(":hash"));
        assert!(TURBO_PUT_ROUTE.contains(":hash"));
        assert_eq!(TURBO_EVENTS_ROUTE, "/v8/artifacts/events");
        assert_eq!(TURBO_STATUS_ROUTE, "/v8/artifacts/status");
    }

    // ── build_handlers smoke test ─────────────────────────────────────────────

    #[test]
    fn build_handlers_returns_usable_state() {
        let state = build_handlers();
        // Status should always succeed.
        let resp = state.handler.status().expect("status");
        assert_eq!(resp.status, "enabled");
    }

    #[test]
    fn unavailable_turbo_handler_503s_loud_not_silent_inmemory() {
        // CAA-360 #7: when R2KvStore fails to build with creds present, the route
        // must 503 LOUDLY on every verb — never silently degrade to a non-durable
        // InMemory store. All verbs share one error path (Self::unavailable);
        // assert it carries the sentinel and that map_err turns it into 503.
        let err = UnavailableTurboHandler::unavailable();
        assert!(
            matches!(&err, TurboBridgeError::Internal(m) if m.starts_with(TURBO_STORAGE_UNAVAILABLE_SENTINEL)),
            "unavailable error must carry the storage-unavailable sentinel"
        );
        assert_eq!(map_err(err).status(), StatusCode::SERVICE_UNAVAILABLE);
        // status() routes through the same error → 503 (not 500, not a fake OK).
        let h = UnavailableTurboHandler;
        assert_eq!(
            map_err(h.status().expect_err("must be unavailable")).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    // ── GET happy path ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_returns_404_when_artifact_absent() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/deadbeef?teamId=team_a")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_then_get_round_trip() {
        let state = fixture();
        let app = router(state);

        // PUT
        let body_bytes = b"webpack-output-chunk".to_vec();
        let put_req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/abc123?teamId=team_x")
            .header("content-type", "application/octet-stream")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(body_bytes.clone()))
            .expect("put request");
        let put_resp = app.clone().oneshot(put_req).await.expect("put oneshot");
        assert_eq!(put_resp.status(), StatusCode::OK);

        // GET — same hash + teamId should return the stored bytes.
        let get_req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/abc123?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("get request");
        let get_resp = app.oneshot(get_req).await.expect("get oneshot");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let response_body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(response_body.as_ref(), body_bytes.as_slice());
    }

    // ── opaque hash round-trip ────────────────────────────────────────────────

    #[tokio::test]
    async fn opaque_hash_round_trip() {
        // Turbo may send any string as hash (xxhash, sha512-prefix, etc.).
        // CoreLink stores it verbatim without hash verification.
        let state = fixture();
        let app = router(state);

        let opaque_hash = "turboxxhash-5e7c3b2a1f";
        let artifact = b"turbo-build-artifact-data".to_vec();

        let put_req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v8/artifacts/{opaque_hash}?teamId=t1"))
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(artifact.clone()))
            .expect("put");
        let put_resp = app.clone().oneshot(put_req).await.expect("put");
        assert_eq!(put_resp.status(), StatusCode::OK);

        let get_req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v8/artifacts/{opaque_hash}?teamId=t1"))
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("get");
        let get_resp = app.oneshot(get_req).await.expect("get");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), artifact.as_slice());
    }

    // ── cross-tenant guard ────────────────────────────────────────────────────

    #[tokio::test]
    async fn cross_tenant_isolated_via_caller_tenant_miss_not_denied() {
        // Repurposed from `put_cross_tenant_returns_403`. The isolation tenant
        // is now `AuthTenant` (auth.0), NOT the `teamId` query param, so a
        // `team_id == caller_tenant` denial is structurally impossible on this
        // path — at the HTTP layer a missing/invalid tenant is rejected 401 at
        // the extractor (fail-CLOSED) before the handler runs. We assert the
        // NEW invariant at the handler the route is wired to: an artifact
        // written under caller_tenant="tenantA" is unreachable from
        // caller_tenant="tenantB" with the SAME teamId+hash — a NotFound MISS,
        // not a denial.
        let state = fixture();
        state
            .handler
            .put(corelink_turbo_bridge::TurboPutRequest::new(
                "h1",
                "shared_team", // teamId — sub-namespace, NOT the tenant
                "s",
                b"tenantA bytes".to_vec(),
                None,
                "userA",
                "tenantA", // caller_tenant — the isolation dimension
                1,
            ))
            .expect("put under tenantA");
        let err = state
            .handler
            .get(corelink_turbo_bridge::TurboGetRequest::new(
                "h1",
                "shared_team",
                "s",
                "userB",
                "tenantB", // different caller_tenant ⇒ isolated
                2,
            ))
            .expect_err("tenantB must not reach tenantA's artifact");
        assert!(matches!(
            err,
            corelink_turbo_bridge::TurboBridgeError::NotFound { .. }
        ));
        // Same-tenant round-trip still serves the bytes.
        let ok = state
            .handler
            .get(corelink_turbo_bridge::TurboGetRequest::new(
                "h1",
                "shared_team",
                "s",
                "userA",
                "tenantA",
                3,
            ))
            .expect("tenantA round-trip");
        assert_eq!(ok.bytes, b"tenantA bytes".to_vec());
    }

    // ── events endpoint ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn post_events_always_200() {
        let app = test_router();
        // Turbo sends arbitrary JSON telemetry; we accept and drop.
        // F3: events now requires the authenticated-tenant header.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("content-type", "application/json")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_events_empty_body_200() {
        let app = test_router();
        // F3: events now requires the authenticated-tenant header.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── F3: events/status are fail-CLOSED without an authenticated tenant ──────

    #[tokio::test]
    async fn post_events_without_tenant_header_is_401() {
        // F3 (defense-in-depth): the container must not trust the Worker's
        // PAT gate. No `x-corelink-tenant-id` ⇒ `AuthTenant` rejects 401
        // BEFORE the accept-and-drop handler runs.
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_events_sentinel_tenant_is_401() {
        // A sentinel tenant value is not a real authenticated tenant.
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", "_unknown")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_status_without_tenant_header_is_401() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/status")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_status_sentinel_tenant_is_401() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/status")
            .header("x-corelink-tenant-id", "_unknown")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── status endpoint ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn post_status_returns_enabled() {
        let app = test_router();
        // F3: status now requires the authenticated-tenant header.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/status")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["status"], "enabled");
    }

    // ── hash-too-long guard ───────────────────────────────────────────────────

    #[test]
    fn hash_too_long_maps_to_400() {
        let resp = map_err(TurboBridgeError::HashTooLong { len: 129, max: 128 });
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── GET missing teamId → 400 ──────────────────────────────────────────────

    #[tokio::test]
    async fn get_missing_team_id_returns_400() {
        let app = test_router();
        // No teamId query param — axum Query extractor rejects this.
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/somehash")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── PUT missing teamId → 400 ──────────────────────────────────────────────

    #[tokio::test]
    async fn put_missing_team_id_returns_400() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/somehash")
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── teamId DoS / key-aliasing guard → 400 ─────────────────────────────────

    #[test]
    fn team_id_invalid_maps_to_400() {
        let resp = map_err(TurboBridgeError::TeamIdInvalid {
            len: 0,
            max: 256,
            reason: "team_id must not be empty",
        });
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_slash_in_team_id_returns_400() {
        let app = test_router();
        // `teamId=a/b` is `teamId=a%2Fb` here — a `/`-bearing value that would
        // escape the team sub-namespace key prefix. Rejected at the route guard.
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/somehash?teamId=a%2Fb")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_empty_team_id_returns_400() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/somehash?teamId=")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_overlong_team_id_returns_400() {
        let app = test_router();
        let long = "a".repeat(corelink_turbo_bridge::error::MAX_TEAM_ID_LEN + 1);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v8/artifacts/somehash?teamId={long}"))
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_slash_in_team_id_returns_400() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/somehash?teamId=a%2Fb")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── scope enforcement (x-corelink-scope) ─────────────────────────────────

    #[tokio::test]
    async fn put_with_read_only_scope_returns_403_insufficient_scope() {
        // A `cas:r` (read-only) token must NOT be able to PUT (write).
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h1?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"insufficient scope");
    }

    #[tokio::test]
    async fn put_with_rw_scope_succeeds() {
        // `cas:rw` is the current prod scope — PUT must still succeed.
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h1?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_with_missing_scope_returns_403() {
        // Fail-CLOSED: no `x-corelink-scope` header ⇒ no cache read.
        let app = test_router();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/h1?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn put_valid_team_id_accepted() {
        // Sanity: a conventional slug-style teamId still round-trips 200.
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h1?teamId=team_AbC-123")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── F24: per-tenant PUT concurrency guard ─────────────────────────────────

    #[test]
    fn put_inflight_counter_is_shared_across_clones() {
        // `TurboRouteState::clone` shares the `Arc<Mutex<..>>` — so all
        // route handler invocations (axum clones state per request) see the
        // SAME counter. Verify the Arc is truly shared, not deep-copied.
        let state = build_handlers();
        let state2 = state.clone();
        {
            let mut g = state.put_inflight.lock().unwrap();
            g.insert(TEST_AUTH_TENANT.to_owned(), 3);
        }
        let g = state2.put_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            Some(&3),
            "cloned state must share the same inflight counter"
        );
    }

    #[tokio::test]
    async fn put_at_limit_returns_429() {
        // Simulate reaching TURBO_PUT_CONCURRENCY_LIMIT by pre-seeding the
        // counter, then issue one more PUT — must get 429.
        let state = fixture();
        {
            let mut g = state.put_inflight.lock().unwrap();
            g.insert(
                TEST_AUTH_TENANT.to_owned(),
                TURBO_PUT_CONCURRENCY_LIMIT,
            );
        }
        let app = router(state);
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h_limit?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "at-limit PUT must return 429"
        );
    }

    #[tokio::test]
    async fn put_below_limit_succeeds_and_decrements_counter() {
        // A PUT that succeeds must release its concurrency slot (counter goes
        // back to 0 after the request completes, not leaked).
        let state = fixture();
        let app = router(state.clone());
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h_decr?teamId=team_y")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        // After the handler returns, the guard must have decremented the counter.
        let g = state.put_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            None, // removed when count reaches 0
            "concurrency counter must be released after PUT completes"
        );
    }

    // ── finding #2: the cap is enforced PRE-BUFFER (FromRequestParts) ─────────

    /// The KILLING finding-#2 test: a 5th concurrent PUT is rejected 429 BEFORE
    /// its body is buffered into the heap.
    ///
    /// The `PutConcurrencyGuard` is a `FromRequestParts` extractor declared
    /// AHEAD of `body: Bytes`; axum runs every `FromRequestParts` extractor
    /// before the single body extractor. We prove the ordering deterministically:
    /// with the tenant AT the cap, this PUT carries a body LARGER than the route's
    /// 100 MiB `DefaultBodyLimit`. If the body were buffered first (the OLD,
    /// in-handler guard), axum's body-limit layer would reject the oversized body
    /// (413/400). Because the concurrency extractor runs FIRST, we get **429**
    /// instead — and the oversized body is never read (the `Body` is streamed
    /// lazily; the extractor short-circuits before any of it is consumed, so the
    /// test stays cheap).
    #[tokio::test]
    async fn fifth_concurrent_put_rejected_429_before_body_buffering() {
        let state = fixture();
        {
            let mut g = state.put_inflight.lock().unwrap();
            // Tenant already AT the limit (4 in-flight).
            g.insert(TEST_AUTH_TENANT.to_owned(), TURBO_PUT_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        // A body strictly larger than the route's 100 MiB DefaultBodyLimit. If the
        // body extractor ran before the concurrency guard, the body-limit layer
        // would reject the oversized body; the pre-buffer guard makes it 429
        // without reading the body.
        let oversized = Body::from(vec![0u8; TURBO_BODY_LIMIT_BYTES + 1]);
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h_oversized?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the 5th concurrent PUT must be rejected 429 by the FromRequestParts \
             guard BEFORE the body is buffered (else the oversized body would be \
             rejected by the body-limit layer instead)"
        );
    }
}
