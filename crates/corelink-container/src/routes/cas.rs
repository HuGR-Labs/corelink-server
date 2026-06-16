//! `GET /v1/cas/:tenant/:hash` — CAS read route wired against
//! [`corelink_handler_cas::CasReadHandler`].
//!
//! This is the **example end-to-end wire-up** demonstrating how the
//! R-prep handler-crate skeleton plugs into `apps/server`. The
//! production CF-Worker R2-bound handler would replace
//! `InMemoryCasHandler` under the `#[cfg(target_arch = "wasm32")]`
//! branch in the trait-object construction site (see crate-level
//! `corelink_handler_cas` doc).
//!
//! # Why a trait object
//!
//! The route handler accepts `Arc<dyn CasReadHandler>` so the route
//! table stays binary-shape stable as we swap the in-memory fake for
//! the wasm32 CF-Worker impl. The same shape is used for the
//! Stripe webhook route's
//! [`corelink_stripe_real::webhook_dispatch::StateMaterializer`]
//! collaborator (wave 16 unification —
//! `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`
//! §unification).
//!
//! # SLO emit
//!
//! Every entry into this route emits one `Sli::AvailCasGet` + one
//! `Sli::LatencyCasGetP99` observation through the
//! `SliObserver` collaborator on the handler. Per the audit
//! 2026-05-14 closure list, this transitions `SLO-AVAIL-CAS-GET`
//! from "Sli-bound deferred" to "Sli emitted at handler layer".

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use crate::wall_clock::{SystemWallClock, WallClock};
use corelink_handler_cas::{
    CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasListHandler,
    CasListRequest, CasListResponse, CasReadHandler, CasReadRequest, CasReadResponse,
    CasWriteHandler, CasWriteRequest, CasWriteResponse, InMemoryAuditSink, InMemoryCasHandler,
    InMemorySliObserver,
};

/// Canonical CAS list route path — `GET /v1/cas/:tenant` (D-8).
pub const CAS_LIST_ROUTE: &str = "/v1/cas/:tenant";

/// Default page size for the CAS list route when `?limit` is absent.
const DEFAULT_LIST_LIMIT: u32 = 200;
/// Hard cap on the CAS list page size (contract: `1..=1000`).
const MAX_LIST_LIMIT: u32 = 1000;

/// Query parameters for the paginated CAS list route (`?limit=&cursor=`).
#[derive(Debug, Default, serde::Deserialize)]
pub struct ListQuery {
    /// Requested page size; clamped to `1..=1000` (default 200).
    pub limit: Option<u32>,
    /// Opaque continuation cursor from a prior page.
    pub cursor: Option<String>,
}

/// Clamp a requested `?limit` into the `1..=1000` contract window,
/// defaulting to 200 when absent or zero.
fn clamp_limit(requested: Option<u32>) -> u32 {
    match requested {
        None | Some(0) => DEFAULT_LIST_LIMIT,
        Some(n) => n.min(MAX_LIST_LIMIT),
    }
}

/// Canonical CAS read route path (matchit-0.7 / axum-0.7 `:name` captures).
///
/// MUST use the `:name` form — matchit 0.7.3 (the version transitively
/// pinned via `axum = "0.7"`) parses `{name}` as **literal path bytes**
/// rather than a capture, which would silently route every real request
/// to a router-level 404. This was DEBT-029 (closed wave-30 stream-1)
/// on `ac.rs` + `admin.rs`; DEBT-029-cas closes the same surface here.
pub const CAS_READ_ROUTE: &str = "/v1/cas/:tenant/:hash";

/// Canonical CAS write route path. The write path reuses the same
/// template; axum disambiguates by HTTP method (GET vs PUT).
pub const CAS_WRITE_ROUTE: &str = "/v1/cas/:tenant/:hash";

/// Shared route state — distinct trait objects for read and write.
#[derive(Clone)]
pub struct CasRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see [`build_handlers`].
    pub read: Arc<dyn CasReadHandler>,
    /// Write handler (separate trait object — see crate-level docs).
    pub write: Arc<dyn CasWriteHandler>,
    /// Delete handler (D-8) — write-capable. In production this points at
    /// the same shared handler instance as `read`/`write`.
    pub delete: Arc<dyn CasDeleteHandler>,
    /// List handler (D-8) — read-capable paginated blob enumeration.
    pub list: Arc<dyn CasListHandler>,
    /// Optional 410-Gone tombstone store (hugit-P2 seam B, WP-B). When present,
    /// `GET` consults it FIRST: an erased `(tenant, hash)` short-circuits to
    /// HTTP 410 Gone (never 404, never 200) before the R2 lookup. `None` (the
    /// default — in-memory / test builds, and prod until PR #254's R2 erase
    /// adapter lands) ⇒ no tombstone gate, classic 200/404 behaviour. Backed by
    /// [`crate::routes::cas_erase::D1TombstoneStore`] in prod.
    pub tombstones: Option<Arc<dyn crate::routes::cas_erase::TombstoneStore>>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed) — checked at the TOP of each billable
    /// handler, AFTER the scope gate, BEFORE storage. `None` in dev/CI (no D1)
    /// — the ceiling is simply not enforced. See [`crate::tenant_quota`].
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production (PAT_SIGNING_KEY + D1) — re-runs the full
    /// Argon2id Option-B verify at the TOP of each billable handler, AFTER the
    /// scope+tenant gate, BEFORE storage, so a leaked `PAT_SIGNING_KEY` cannot
    /// serve a forged tenant's PAT. `None` in dev/CI (skipped). See
    /// [`crate::native_pat_gate`].
    pub pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    /// Optional per-tenant storage byte accountant (red-team finding #1 —
    /// storage-cap enforcement). `Some` in production (D1-backed) — `accrue`d
    /// after a successful write (fail-CLOSED 503 / over-cap reject) and
    /// `release`d after a delete. `None` in dev/CI (not enforced). See
    /// [`crate::byte_accounting`].
    pub bytes: Option<std::sync::Arc<crate::byte_accounting::ByteAccountant>>,
}

impl core::fmt::Debug for CasRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasRouteState").finish_non_exhaustive()
    }
}

/// Sentinel carried in [`CasHandlerError::Internal`] by
/// [`UnavailableCasHandler`] so [`map_err`] can map the storage-unavailable
/// condition to **503** (rather than the generic 500 the `Internal` wildcard
/// arm yields). The handler-error enum lives in a sibling crate and is
/// `#[non_exhaustive]` without a dedicated `Unavailable` variant, so we thread
/// the distinction through `Internal` with this exact marker prefix.
const STORAGE_UNAVAILABLE_SENTINEL: &str = "storage-unavailable: ";

/// Fail-CLOSED stand-in handler mounted in place of `InMemoryCasHandler` when
/// storage credentials ARE present but the R2 handler refused to build —
/// today exactly the "R2_TDK_HEX required" case (F1, CAA-360).
///
/// A silent `InMemory` fallback there would serve a NON-durable cache with no
/// alarm (fail-OPEN-ish). Instead every read/write through this handler fails
/// CLOSED + LOUD: it returns [`CasHandlerError::Internal`] carrying the
/// [`STORAGE_UNAVAILABLE_SENTINEL`], which [`map_err`] maps to HTTP 503 — the
/// route is unavailable until `R2_TDK_HEX` is set. Distinct from the
/// creds-ABSENT path (`None` ⇒ dev/CI `InMemory`, which is fine).
#[derive(Debug)]
struct UnavailableCasHandler;

impl CasReadHandler for UnavailableCasHandler {
    fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasWriteHandler for UnavailableCasHandler {
    fn write(&self, _req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasDeleteHandler for UnavailableCasHandler {
    fn delete(&self, _req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasListHandler for UnavailableCasHandler {
    fn list(&self, _req: CasListRequest) -> Result<CasListResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

/// Build the canonical `Arc<dyn CasReadHandler>` for the current
/// build target and runtime environment.
///
/// # Runtime selection (WP-S1 Phase 1)
///
/// On native targets the function probes for storage credentials at
/// runtime:
///
/// - When `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`,
///   `R2_S3_ENDPOINT`, `CLOUDFLARE_ACCOUNT_ID`, `CF_API_TOKEN`, and
///   `D1_DATABASE_ID` are all present in the environment,
///   [`R2CasHandler`](crate::storage::r2_s3::R2CasHandler) is
///   constructed against `corelink-cas-prod` (override via
///   `R2_CAS_BUCKET`). This is the **production path**.
///
/// - When credentials are absent (unit tests, local dev, CI) the
///   function falls back to `InMemoryCasHandler`. No network I/O
///   occurs.
///
/// - When credentials ARE present but the R2 handler refuses to build
///   (today exactly the "`R2_TDK_HEX` required" case — F1, CAA-360) the
///   function does NOT silently degrade to `InMemoryCasHandler` (that
///   would serve a non-durable cache with no alarm). It instead mounts
///   the fail-CLOSED [`UnavailableCasHandler`], whose every read/write
///   maps to HTTP 503 until `R2_TDK_HEX` is set, and emits a loud,
///   structured `tracing::error!`.
///
/// On `wasm32-unknown-unknown` (Cloudflare Worker target) a
/// compile-error placeholder is emitted per the
/// `trait-abstraction-defer` rule; the wasm32 binding is out of
/// scope for WP-S1.
///
/// # Panics
///
/// Does not panic. If the R2 client cannot be constructed with creds
/// present (malformed endpoint URL, missing `R2_TDK_HEX`, etc.) an
/// error is logged and the route is served by the fail-CLOSED
/// [`UnavailableCasHandler`] (HTTP 503), never the silent `InMemory`
/// fallback.
#[must_use]
#[allow(
    clippy::type_complexity,
    reason = "builder returns a fixed read/write/delete/list (D-8) handler tuple; a named alias would orphan this function's doc block"
)]
pub fn build_handlers() -> (
    Arc<dyn CasReadHandler>,
    Arc<dyn CasWriteHandler>,
    Arc<dyn CasDeleteHandler>,
    Arc<dyn CasListHandler>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_s3, StorageEnv};

        // Probe for storage credentials.
        if StorageEnv::from_env().is_some() {
            // Credentials are present — try to build the real handler.
            // Empty-or-absent → default (see storage::env_or; mirrors the
            // AC fix for the 2026-06-05 prod incident — latent here).
            let bucket = crate::storage::env_or("R2_CAS_BUCKET", "corelink-cas-prod");
            // F7 (2026-06-13 CAA-360 audit) — CAS residency. The CAS storage
            // region is keyed per-env from `R2_CAS_REGION` (FROZEN CONTRACT),
            // set by `[env.prod-<region>].vars` in `wrangler.toml` and forwarded
            // by the DO `container.start({env})` list (F8). This is what makes a
            // regional env (sam/lhr/nrt/syd) key its CAS objects under its OWN
            // region instead of the US default — closing the Schrems-II / GDPR
            // Art. 44 gap for CAS content. The default `"iad"` applies only to
            // the IAD env; a regional env that lacks the binding silently
            // degrades to IAD, so the binding is asserted by the residency
            // invariant test (`residency_invariant_every_regional_env_sets_cas_region`)
            // that fails the build if any `[env.prod-<r>]` omits `R2_CAS_REGION`.
            let region = crate::storage::env_or("R2_CAS_REGION", "iad");

            // Construction is now sync-only (commit ead0f37a removed the
            // aws_config::defaults() IMDS probe). We keep block_in_place +
            // block_on around the async signature in case future R2 init
            // grows network work; the cost is zero when the inner future
            // resolves immediately.
            let handle = tokio::runtime::Handle::current();
            let built = tokio::task::block_in_place(|| {
                handle.block_on(r2_s3::build_r2_cas_handler_from_env(&bucket, &region))
            });
            match built {
                Some(Ok(handler)) => {
                    tracing::info!(
                        bucket = %bucket,
                        region = %region,
                        "CAS handler: R2S3 (real storage)"
                    );
                    // R2CasHandler implements both CasReadHandler and
                    // CasWriteHandler against the same R2 bucket; share
                    // one Arc behind both trait objects.
                    let shared: Arc<r2_s3::R2CasHandler> = Arc::new(handler);
                    let read: Arc<dyn CasReadHandler> = shared.clone();
                    let write: Arc<dyn CasWriteHandler> = shared.clone();
                    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
                    let list: Arc<dyn CasListHandler> = shared;
                    return (read, write, delete, list);
                }
                Some(Err(e)) => {
                    // F1 (CAA-360) fail-CLOSED + LOUD: storage creds ARE present
                    // (this is the production data plane), but the R2 handler
                    // refused to build — today exactly "R2_TDK_HEX required". We
                    // MUST NOT silently fall back to the non-durable `InMemory`
                    // cache (that is fail-OPEN-ish: a broken cache with no
                    // alarm). Mount the fail-CLOSED `UnavailableCasHandler`
                    // instead: every read/write returns 503 until `R2_TDK_HEX` is
                    // set. Distinct from the creds-ABSENT case below (`None` ⇒
                    // dev/CI `InMemory`, which is fine).
                    tracing::error!(
                        error = %e,
                        bucket = %bucket,
                        region = %region,
                        "CAS handler: R2S3 build REFUSED with storage creds present \
                         (R2_TDK_HEX unset/invalid?) — mounting fail-CLOSED 503 \
                         handler, NOT InMemory (F1 INV-TENANT-ISOLATION)"
                    );
                    let shared: Arc<UnavailableCasHandler> = Arc::new(UnavailableCasHandler);
                    let read: Arc<dyn CasReadHandler> = shared.clone();
                    let write: Arc<dyn CasWriteHandler> = shared.clone();
                    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
                    let list: Arc<dyn CasListHandler> = shared;
                    return (read, write, delete, list);
                }
                None => {
                    // Should not happen: we already checked is_some().
                }
            }
        }

        // Fallback: InMemory — creds ABSENT (unit tests, local dev, CI). This is
        // the dev/CI path; production always has storage creds and takes the R2
        // branch above (which now fails CLOSED on a missing TDK).
        tracing::info!("CAS handler: InMemory (no storage credentials configured)");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryCasHandler> = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        (read, write, delete, list)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Placeholder for `corelink_handler_cas::cf_worker::CfWorkerCasHandler`
        // (deferred per trait-abstraction-defer; see crate-level docs).
        // Until that impl lands, the wasm32 build does not include
        // this route — the binary entry asserts the cfg at link time.
        compile_error!(
            "wasm32 CF-Worker CAS handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Build the axum `Router` exposing the CAS read + write routes.
///
/// Both routes share the `/v1/cas/:tenant/:hash` template; axum
/// disambiguates by HTTP method (GET vs PUT).
pub fn router(state: CasRouteState) -> Router {
    Router::new()
        .route(
            CAS_READ_ROUTE,
            get(handle_read).put(handle_write).delete(handle_delete),
        )
        .route(CAS_LIST_ROUTE, get(handle_list))
        .with_state(state)
}

/// Run the native PAT possession gate (finding #4) when it is wired.
///
/// Reads the bearer PAT from the `Authorization` header and re-verifies it
/// (Argon2id, full Option-B pipeline) against the claimed `tenant`. `Some(resp)`
/// ⇒ REJECT (401 forged/wrong-tenant / 503 verifier fault); `None` ⇒ proceed (or
/// when the gate is absent in dev/CI). Called at the TOP of each billable
/// handler, AFTER the scope+tenant gate, BEFORE storage.
async fn pat_gate_reject(
    state: &CasRouteState,
    tenant: &str,
    headers: &axum::http::HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match gate.verify(tenant, bearer).await {
        Ok(()) => None,
        Err(resp) => Some(resp),
    }
}

/// `GET /v1/cas/:tenant/:hash` handler.
///
/// The authenticated tenant (DO-injected `x-corelink-tenant-id`,
/// PAT-resolved by the Worker) is bound by the
/// [`crate::auth_tenant::AuthTenant`] extractor and is the SOLE
/// isolation key. The path `:tenant` is a client-controllable echo
/// that MUST match it (mismatch ⇒ 403 before any storage access).
async fn handle_read(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // The path `:tenant` is a client echo that MUST equal the
    // authenticated tenant; mismatch is a cross-tenant attempt and is
    // denied 403 BEFORE any storage access (no tenant quoted in the
    // body). Mirrors bazel_v2 / ac.rs.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed CAS hash BEFORE it derives an R2 key
    // (defense-in-depth alongside the AC gate; reuses the shared validator).
    if !super::ac::is_canonical_digest(&hash) {
        return (StatusCode::BAD_REQUEST, "malformed hash").into_response();
    }
    // Scope gate (fail-CLOSED): the PAT must carry a cache-READ capability
    // (`cas:rw` or `cas:r`) in the Worker-trusted `x-corelink-scope` header.
    // BEFORE any storage access. Today every prod PAT is `cas:rw` so this is
    // a NO-OP for current traffic; it establishes the gate for tiered tokens.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4 — defense-in-depth): re-verify the
    // bearer PAT (Argon2id) resolves to the claimed tenant, AFTER the scope gate,
    // BEFORE storage. 401 forged/wrong-tenant; 503 verifier fault.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost, AFTER the scope gate, BEFORE storage. Over-ceiling ⇒ 402;
    // store/clock fault ⇒ 503 (fail-CLOSED). `None` in dev/CI ⇒ not enforced.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    // 410-Gone tombstone gate (hugit-P2 seam B, WP-B). When a tombstone store is
    // wired, an erased `(tenant, hash)` short-circuits to HTTP 410 Gone — BEFORE
    // the R2 GET, so it is a single keyed D1 lookup off the hot path. An erased
    // artifact MUST return 410 (never 404 "never existed", never 200 resurrect).
    // A lookup-transport error fails OPEN to the normal read path: a transient
    // D1 blip must not 410 a live blob (the erase write-side is the source of
    // truth; the read gate is advisory). `None` ⇒ classic 200/404.
    if let Some(tombstones) = state.tombstones.as_ref() {
        match tombstones.is_tombstoned(&auth.0, &hash).await {
            Ok(true) => return (StatusCode::GONE, "erased").into_response(),
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "cas: tombstone gate lookup failed; serving normally");
            }
        }
    }
    // CAA-360 #6: real audit-event timestamp from the production SystemWallClock
    // (was hardcoded `0u64`, which stamped every audit event with epoch 0 and
    // made the audit log un-orderable / un-correlatable).
    let now_ms = SystemWallClock.now_ms();
    let req = CasReadRequest::new(
        auth.0.clone(),
        hash,
        // Principal is filled in by the auth middleware in
        // production; demo wire-up mirrors ac.rs.
        format!("anon@{}", auth.0),
        auth.0,
        now_ms,
    );
    match state.read.read(req) {
        Ok(resp) => (StatusCode::OK, resp.bytes).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /v1/cas/:tenant/:hash` handler.
///
/// Accepts raw bytes in the body; the client claims the content hash
/// via the URL path. The handler enforces hash equality before
/// committing to durable storage. On success: 201 Created (fresh
/// insert) or 200 OK (idempotent re-write).
async fn handle_write(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // See `handle_read`: the authenticated tenant is the sole
    // isolation key; the path `:tenant` is a client echo that must
    // match. Deny 403 BEFORE any storage access on mismatch.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed CAS hash BEFORE it derives an R2 key.
    if !super::ac::is_canonical_digest(&hash) {
        return (StatusCode::BAD_REQUEST, "malformed hash").into_response();
    }
    // Scope gate (fail-CLOSED): the PAT must carry a cache-WRITE capability
    // (`cas:rw`) BEFORE any storage access. A read-only (`cas:r`) token is
    // rejected here. NO-OP for current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) — see
    // `handle_read`. AFTER the scope gate, BEFORE storage.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let byte_len = i64::try_from(body.len()).unwrap_or(i64::MAX);
    let req = CasWriteRequest::new(
        auth.0.clone(),
        hash,
        body.to_vec(),
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    );
    match state.write.write(req) {
        Ok(resp) => {
            // Storage byte accounting (finding #1): accrue ONLY a fresh durable
            // insert's bytes (an idempotent re-write stored nothing new, so
            // `durable == false` ⇒ no double-count) AFTER the write committed,
            // BEFORE returning success. Over-cap ⇒ 402 (the tenant is over its
            // storage cap); a transport fault ⇒ 503 fail-CLOSED (we will not
            // return success for a write we could not account).
            if resp.durable {
                if let Some(acc) = state.bytes.as_ref() {
                    match acc.accrue(&auth.0, byte_len).await {
                        Ok(crate::byte_accounting::AccrueOutcome::Accrued) => {}
                        Ok(crate::byte_accounting::AccrueOutcome::OverCap) => {
                            return (
                                StatusCode::PAYMENT_REQUIRED,
                                "storage quota exceeded",
                            )
                                .into_response();
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "cas: byte accrual failed; failing closed");
                            return (
                                StatusCode::SERVICE_UNAVAILABLE,
                                "storage accounting unavailable",
                            )
                                .into_response();
                        }
                    }
                }
            }
            let code = if resp.durable {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (code, resp.content_hash).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `DELETE /v1/cas/:tenant/:hash` handler — delete a blob (D-8).
///
/// Idempotent: returns 204 No Content whether the blob existed or not.
/// Requires a WRITE-capable PAT (mirrors `handle_write`).
async fn handle_delete(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Cross-tenant: deny 403 BEFORE any storage access (mirrors read).
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed CAS hash BEFORE it derives an R2 key.
    if !super::ac::is_canonical_digest(&hash) {
        return (StatusCode::BAD_REQUEST, "malformed hash").into_response();
    }
    // Scope gate (fail-CLOSED): delete is a cache WRITE — require `cas:rw`.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = CasDeleteRequest::new(
        auth.0.clone(),
        hash,
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    );
    match state.delete.delete(req) {
        // Idempotent: 204 No Content for both deleted-existing and absent.
        // Storage byte accounting (finding #1): release the reclaimed bytes from
        // the tenant's counter when a blob actually existed. The byte SIZE of the
        // deleted blob is sourced from the size-bearing delete response if/when
        // the handler crate surfaces it (`reclaimed_bytes`); today `CasDeleteResponse`
        // reports only `existed`, so the release plumbing
        // ([`crate::byte_accounting::ByteAccountant::release`]) is in place and
        // unit-tested, awaiting that size on the response. A failed release
        // over-counts (conservative), never under-counts — so it never widens the
        // cap; we therefore log + continue rather than fail the (succeeded) delete.
        Ok(resp) => {
            let _ = resp.existed; // size not yet on the response; see comment above.
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/cas/:tenant` handler — paginated blob enumeration (D-8).
///
/// Requires a READ-capable PAT. Returns
/// `{ "blobs": [...], "next_cursor": <opaque|null> }`.
async fn handle_list(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    // Cross-tenant: deny 403 BEFORE any storage access.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // Scope gate (fail-CLOSED): list is a cache READ — require read cap.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = CasListRequest::new(
        auth.0.clone(),
        format!("anon@{}", auth.0),
        auth.0,
        clamp_limit(q.limit),
        q.cursor,
        now_ms,
    );
    match state.list.list(req) {
        Ok(resp) => {
            let blobs: Vec<_> = resp
                .blobs
                .into_iter()
                .map(|b| {
                    serde_json::json!({
                        "hash": b.hash,
                        "size": b.size,
                        "created_at": b.created_at,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "blobs": blobs,
                    "next_cursor": resp.next_cursor,
                })),
            )
                .into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Map a [`CasHandlerError`] to the canonical HTTP response.
fn map_err(e: CasHandlerError) -> axum::response::Response {
    // Log the full error before mapping so operators retain a
    // diagnostic record (R2/S3 detail) without exposing internals to
    // the caller.
    tracing::warn!(error = ?e, "CAS handler error");
    match e {
        CasHandlerError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found").into_response(),
        CasHandlerError::HashMismatch { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "content hash mismatch").into_response()
        }
        CasHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        CasHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve
            // bytes / commit writes without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // F1 (CAA-360) fail-CLOSED: the route was mounted with storage creds
        // present but the R2 handler refused to build (R2_TDK_HEX unset/invalid),
        // so it is serving the `UnavailableCasHandler`. Surface that as 503
        // "storage unavailable" — the cache is unavailable until R2_TDK_HEX is
        // set, NOT a generic 500. See [`UnavailableCasHandler`].
        CasHandlerError::Internal(ref msg) if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL) => {
            (StatusCode::SERVICE_UNAVAILABLE, "storage unavailable").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
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
    use corelink_handler_cas::handler::fake_hash;

    /// `map_err` must map an `Internal` error to 503 ONLY when it carries the
    /// storage-unavailable sentinel; any other `Internal` is a generic 500.
    /// Kills the cargo-mutants "replace match guard with true" mutant on the
    /// `if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL)` guard (mirrors the
    /// ac.rs test) — with the guard forced to `true`, the non-sentinel case
    /// below would wrongly become 503.
    #[test]
    fn map_err_internal_is_503_only_for_storage_sentinel() {
        let storage = map_err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2_TDK_HEX unset"
        )));
        assert_eq!(
            storage.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "sentinel-tagged Internal must be 503"
        );

        let generic = map_err(CasHandlerError::Internal(
            "lock poisoned: unrelated failure".to_string(),
        ));
        assert_eq!(
            generic.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "non-sentinel Internal must be 500, not 503 (guard must not be `true`)"
        );
    }

    fn fixture() -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            bytes: None,
        }
    }

    /// Fixture with a tombstone store pre-seeded with `(tenant, hash)` so the
    /// read gate returns 410 Gone (WP-B).
    fn fixture_with_tombstone(tenant: &str, hash: &str) -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let store = Arc::new(crate::routes::cas_erase::InMemoryTombstoneStore::new());
        store.seed(tenant, hash);
        let tombstones: Arc<dyn crate::routes::cas_erase::TombstoneStore> = store;
        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: Some(tombstones),
            quota: None,
            pat_gate: None,
            bytes: None,
        }
    }

    /// Route state backed by the fail-CLOSED [`UnavailableCasHandler`] — the
    /// shape `build_handlers` returns when storage creds are present but the R2
    /// handler refuses to build (F1: `R2_TDK_HEX` unset/invalid).
    fn fixture_unavailable() -> CasRouteState {
        let shared = Arc::new(UnavailableCasHandler);
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            bytes: None,
        }
    }

    #[test]
    fn route_constants_match_canonical_path() {
        assert_eq!(CAS_READ_ROUTE, "/v1/cas/:tenant/:hash");
        assert_eq!(CAS_WRITE_ROUTE, "/v1/cas/:tenant/:hash");
    }

    /// F1 (CAA-360) fail-CLOSED: the `UnavailableCasHandler`'s error maps to
    /// HTTP 503 "storage unavailable" (NOT the generic 500) so a forgotten
    /// `R2_TDK_HEX` is loud, not a silent non-durable cache.
    #[test]
    fn unavailable_handler_maps_to_503() {
        let err = UnavailableCasHandler
            .read(CasReadRequest::new("t1", "h", "anon@t1", "t1", 0))
            .expect_err("unavailable");
        assert!(matches!(err, CasHandlerError::Internal(_)));
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn route_constant_uses_matchit_0_7_colon_syntax_not_curly_braces() {
        // DEBT-029-cas regression net: matchit 0.7.3 parses `{name}`
        // as LITERAL path bytes, not a capture. Any reintroduction of
        // the `{name}` form would silently route every real request
        // to a router-level 404. Pin both the absence of `{` and the
        // presence of `:tenant` + `:hash` so renames don't drift.
        assert!(
            !CAS_READ_ROUTE.contains('{'),
            "CAS_READ_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
        );
        assert!(CAS_READ_ROUTE.contains(":tenant"));
        assert!(CAS_READ_ROUTE.contains(":hash"));
    }

    /// F7 (2026-06-13 CAA-360 audit) — CAS data-residency invariant.
    ///
    /// Every non-IAD regional prod env (`[env.prod-sam|lhr|nrt|syd]`) MUST set
    /// `R2_CAS_REGION` in its `.vars` so the container keys that region's CAS
    /// objects under its OWN region instead of silently defaulting to the US
    /// `"iad"` (the Schrems-II / GDPR Art. 44 gap). This test fails the build if
    /// any regional env omits the binding — the fail-CLOSED, fail-LOUD guard
    /// that stops a future env-block edit from re-opening the residency hole.
    ///
    /// We assert the binding *exists* per region (the F7 contract). The bucket
    /// stays `corelink-cas-prod` until per-region CAS buckets are provisioned
    /// (infra follow-up), so we do not assert the bucket here.
    #[test]
    fn residency_invariant_every_regional_env_sets_cas_region() {
        // wrangler.toml lives at the repo root; this crate is at
        // crates/corelink-container, so climb two parents.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let wrangler_path = std::path::Path::new(manifest_dir)
            .join("..")
            .join("..")
            .join("wrangler.toml");
        let toml = std::fs::read_to_string(&wrangler_path).unwrap_or_else(|e| {
            panic!("cannot read {}: {e}", wrangler_path.display());
        });

        // Each non-IAD regional env must declare its CAS region. The IAD env is
        // exempt: its `R2_CAS_REGION` default ("iad") is the residency-correct
        // value, so an absent binding there is not a cross-border leak.
        for region in ["sam", "lhr", "nrt", "syd"] {
            let header = format!("[env.prod-{region}.vars]");
            let start = toml.find(&header).unwrap_or_else(|| {
                panic!("wrangler.toml missing `{header}` env block");
            });
            // Bound the search to this env block: from its header to the next
            // top-level `[` section after the header line.
            let after_header = start + header.len();
            let block_end = toml[after_header..]
                .find("\n[")
                .map_or(toml.len(), |rel| after_header + rel);
            let block = &toml[start..block_end];
            // Every residency-bearing key space must be pinned to this region.
            // CAS was the original F7 invariant; AC objects (which embed output
            // digests + command metadata) and chunk objects are equally
            // residency-bearing, and key under R2_AC_REGION / R2_CHUNK_REGION
            // (both default to "iad" in cas.rs/ac.rs `env_or`). A regional env
            // that drops ANY of the three silently keys that space under the US
            // default — the same cross-border leak the CAS check guards. Assert
            // all three so the build-time guard covers the full key surface.
            for var in ["R2_CAS_REGION", "R2_AC_REGION", "R2_CHUNK_REGION"] {
                let expected = format!("{var} = \"{region}\"");
                assert!(
                    block.contains(&expected),
                    "[env.prod-{region}] must set `{expected}` (residency \
                     invariant): a regional env without {var} keys that object \
                     space under the US default \"iad\" — a cross-border leak. \
                     Add the binding to wrangler.toml."
                );
            }
        }
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        // Smoke test the build_handlers shape on the native target.
        let (read, _write, _delete, _list) = build_handlers();
        let bytes = b"hello".to_vec();
        let hash = fake_hash(&bytes);
        // We don't have a seed entry point on the trait alone, so
        // we drive the read against a known-empty handler and assert
        // it returns NotFound. The full wire-up is exercised via
        // the handler-crate's own unit tests.
        let res = read.read(CasReadRequest::new("t1", hash, "anon", "t1", 0));
        assert!(matches!(res, Err(CasHandlerError::NotFound { .. })));
        // Build state to satisfy the router constructor.
        let _router = router(fixture());
    }

    /// PUT then GET round-trip via the route state drives both trait
    /// objects against the shared InMemory backing store — pins that
    /// the new write trait object writes to the SAME backing store the
    /// read trait object reads from (regression net for a future
    /// refactor that accidentally splits the backing store).
    #[test]
    fn put_then_get_round_trip_through_route_state() {
        let st = fixture();
        let bytes = b"corelink-cas-put-then-get".to_vec();
        let hash = fake_hash(&bytes);

        // PUT
        let upd = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
        let upd_resp = st.write.write(upd).expect("write");
        assert!(upd_resp.durable, "fresh insert must be durable=true");

        // GET — must return the same bytes
        let rd = CasReadRequest::new("t1", hash.clone(), "anon@t1", "t1", 2);
        let rd_resp = st.read.read(rd).expect("read hit");
        assert_eq!(rd_resp.bytes, bytes);
        assert_eq!(rd_resp.content_hash, hash);
    }

    /// Idempotent retry on the write path: a second PUT with the same
    /// bytes returns durable=false (mirrors AC + handler-crate
    /// semantics).
    #[test]
    fn idempotent_write_retry_returns_durable_false() {
        let st = fixture();
        let bytes = b"r".to_vec();
        let hash = fake_hash(&bytes);
        let req1 = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
        assert!(st.write.write(req1).expect("first").durable);
        let req2 = CasWriteRequest::new("t1", hash, bytes, "anon@t1", "t1", 2);
        assert!(!st.write.write(req2).expect("retry").durable);
    }

    // ── scope enforcement (x-corelink-scope) ─────────────────────────────────

    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::ServiceExt; // for `.oneshot()`

    /// Authenticated tenant injected via `x-corelink-tenant-id` so the
    /// `AuthTenant` extractor admits the request; the scope header then
    /// gates read vs write.
    const TEST_TENANT: &str = "t1";

    /// A `cas:rw` PUT writes successfully (current prod scope — happy path).
    #[tokio::test]
    async fn put_with_rw_scope_succeeds() {
        let app = router(fixture());
        let bytes = b"cas-scope-rw".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// A `cas:r` (read-only) PUT is rejected 403 "insufficient scope"
    /// BEFORE storage.
    #[tokio::test]
    async fn put_with_read_only_scope_returns_403_insufficient_scope() {
        let app = router(fixture());
        let bytes = b"cas-scope-ro".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"insufficient scope");
    }

    /// F1 (CAA-360) end-to-end: a route mounted on the fail-CLOSED
    /// `UnavailableCasHandler` (creds present, `R2_TDK_HEX` missing) returns
    /// HTTP 503 for an in-scope GET — the cache refuses to serve, not a silent
    /// 200/404 from a non-durable InMemory fallback.
    #[tokio::test]
    async fn get_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// F1 (CAA-360) end-to-end: an in-scope PUT against the fail-CLOSED
    /// `UnavailableCasHandler` returns 503 — writes are refused (never a silent
    /// non-durable commit) until `R2_TDK_HEX` is set.
    #[tokio::test]
    async fn put_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let bytes = b"cas-unavailable".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// A `cas:r` GET is allowed (read-only token reads). The artifact is
    /// absent so the route returns 404 — proving the scope gate PASSED
    /// (a denied scope would 403 before storage).
    #[tokio::test]
    async fn get_with_read_only_scope_passes_gate_then_404() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Fail-CLOSED: a GET with NO scope header is rejected 403.
    #[tokio::test]
    async fn get_with_missing_scope_returns_403() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_malformed_hash_returns_400() {
        // CAA-360 #9: a non-canonical :hash is rejected with 400 BEFORE storage,
        // even WITH valid auth + scope (the digest gate runs before the scope
        // gate). Covers the gate against a mutant that disables/inverts it.
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/not-a-canonical-hash"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── 410-Gone tombstone read gate (hugit-P2 seam B, WP-B) ─────────────────

    /// A GET for an ERASED `(tenant, hash)` returns HTTP 410 Gone — NOT 404,
    /// NOT 200. The tombstone gate runs after the scope gate and before the R2
    /// read.
    #[tokio::test]
    async fn get_erased_hash_returns_410_gone() {
        const ERASED: &str = "1111111111111111111111111111111111111111111111111111111111111111";
        let app = router(fixture_with_tombstone(TEST_TENANT, ERASED));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/{ERASED}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::GONE);
    }

    /// With a tombstone store wired, a NON-erased hash still 404s (the gate is
    /// per-hash, not a blanket block).
    #[tokio::test]
    async fn get_non_erased_hash_with_store_still_404() {
        let app = router(fixture_with_tombstone(TEST_TENANT, "some-other-hash"));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) ─────────

    /// Fixture whose state carries a `QuotaGate` backed by an in-memory quota
    /// store seeded so the NEXT billable op trips the ceiling. The flat per-op
    /// cost is `1` micro-USD; the seeded tenant is already AT its budget, so any
    /// charge projects over the cap.
    fn fixture_over_ceiling(tenant: &str) -> CasRouteState {
        use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaState, QuotaStore};
        use crate::wall_clock::InMemoryFakeWallClock;

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;

        let store = InMemoryQuotaStore::new();
        store.seed(
            tenant,
            QuotaState {
                monthly_budget_usd_micros: 5_000_000,
                accrued_usd_micros: 5_000_000, // already at the $5 cap
                cycle_anchor_ms: 1_700_000_000_000,
            },
        );
        let store: Arc<dyn QuotaStore> = Arc::new(store);
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let guard = Arc::new(QuotaGuard::new(store, clock));
        let gate = crate::routes::QuotaGate::new_for_test(guard, 1);

        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: Some(gate),
            pat_gate: None,
            bytes: None,
        }
    }

    /// Once accrued spend has reached the monthly ceiling, a billable CAS read
    /// is rejected with HTTP 402 Payment Required — BEFORE storage. This is the
    /// load-bearing wiring assertion: the previously-dead `QuotaGuard` is now
    /// mounted and trips on a real HTTP request.
    #[tokio::test]
    async fn billable_request_over_ceiling_returns_402() {
        let app = router(fixture_over_ceiling(TEST_TENANT));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    /// Control: with room under the ceiling the same request passes the gate
    /// and proceeds to storage (404 for the absent blob — NOT 402). Proves the
    /// gate is not a blanket block.
    #[tokio::test]
    async fn billable_request_under_ceiling_proceeds() {
        use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaStore};
        use crate::wall_clock::InMemoryFakeWallClock;

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        // Empty store ⇒ fresh tenant at the $5 tripwire with 0 accrued.
        let store: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let guard = Arc::new(QuotaGuard::new(store, clock));
        let st = CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: Some(crate::routes::QuotaGate::new_for_test(guard, 1_000)),
            pat_gate: None,
            bytes: None,
        };
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── finding #1: storage byte accounting (cap enforcement) ────────────────

    /// Fixture whose state carries a [`ByteAccountant`] over an in-memory byte
    /// store seeded AT a tiny cap, so the next durable write trips the cap.
    fn fixture_over_storage_cap(tenant: &str) -> CasRouteState {
        use crate::byte_accounting::{testing::InMemoryByteStore, testing::Row, ByteAccountant, ByteStore};

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;

        let store = Arc::new(InMemoryByteStore::new());
        // Cap of 4 bytes, already at 4 ⇒ ANY further byte is over-cap.
        store.seed(tenant, "iad", Row { used: 4, quota: 4 });
        let store_dyn: Arc<dyn ByteStore> = store;
        let acc = Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned()));

        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            bytes: Some(acc),
        }
    }

    /// The KILLING finding-#1 test: with the storage counter AT the cap, a PUT
    /// that would store new bytes is rejected 402 — the previously-inert storage
    /// cap now trips on a real HTTP write.
    #[tokio::test]
    async fn put_over_storage_cap_returns_402() {
        let app = router(fixture_over_storage_cap(TEST_TENANT));
        let bytes = b"more-bytes-than-the-cap-allows".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    /// Control: an uncapped tenant's PUT accrues its bytes and succeeds (201) —
    /// proving the counter MOVES (it never did before finding #1) and the gate
    /// is not a blanket block.
    #[tokio::test]
    async fn put_under_storage_cap_accrues_and_succeeds() {
        use crate::byte_accounting::{testing::InMemoryByteStore, ByteAccountant, ByteStore};

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let store = Arc::new(InMemoryByteStore::new()); // empty ⇒ uncapped fresh row
        let store_dyn: Arc<dyn ByteStore> = store.clone();
        let st = CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            bytes: Some(Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned()))),
        };
        let app = router(st);
        let body = b"hello-cas".to_vec();
        let body_len = body.len() as i64;
        let hash = fake_hash(&body);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(
            store.used(TEST_TENANT, "iad"),
            body_len,
            "a durable write must accrue its bytes into the storage counter"
        );
    }

    // ── finding #4: native PAT possession gate ───────────────────────────────

    /// A request with the PAT gate wired but NO bearer Authorization header is
    /// rejected 401 — the native gate fails CLOSED on a missing token even when
    /// the Worker-set tenant header is present (defense-in-depth).
    #[tokio::test]
    async fn pat_gate_missing_bearer_returns_401() {
        use crate::native_pat_gate::testing::verifier_with_row;
        use crate::native_pat_gate::NativePatGate;
        use crate::adapter_pat::PatRow;
        use corelink_pat::PatSigningKey;

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let key = Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("key"));
        let verifier = verifier_with_row(
            "no-such-token".to_owned(),
            PatRow {
                tenant_id: TEST_TENANT.to_owned(),
                pat_hash: String::new(),
                scope: "cas:rw".to_owned(),
            },
            key,
        );
        let st = CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: Some(Arc::new(NativePatGate::new_for_test(verifier))),
            bytes: None,
        };
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            // No Authorization header — the gate must reject.
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
