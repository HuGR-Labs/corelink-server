// REAPI v2 Bazel remote-cache routes: `/bazel/v2/*`
//
// # Purpose
//
// Mounts the five REAPI v2 REST cache endpoints (the ByteStream-style
// `/bazel/v2/:instance/blobs/:hash/:size` scheme — see the mapping
// below) backed by the same R2 blobs the native CAS/AC endpoints serve.
//
// # Client contract (two schemes, one store)
//
// This module serves TWO wire schemes onto the SAME [`BazelAdapter`]/handlers
// and the same per-tenant R2 store:
//
// 1. The CoreLink **REAPI ByteStream REST scheme**
//    (`/bazel/v2/:instance/blobs/:hash/:size`) — for a REAPI/ByteStream client
//    (or `bazel` configured against this scheme). The tenant is the `:instance`
//    path segment.
// 2. The **stock-Bazel HTTP cache alias** (`/bazel/cache/{cas,ac}/:hash`) — what
//    vanilla `bazel --remote_cache=https://host/bazel/cache` actually sends:
//    `GET`/`PUT` on `/cas/<hash>` and `/ac/<hash>` with **no `:instance` segment
//    and no `:size`**. Buck2 is NOT a client of this alias — it speaks REAPI over
//    **gRPC** only and has no plain-HTTP cache backend. Here the tenant
//    (== REAPI `instance`) is derived from the Worker-injected
//    `x-corelink-tenant-id` header, so a missing/sentinel tenant → 401 and
//    isolation is by the per-tenant namespace (a cross-tenant hash is a uniform
//    404, never another tenant's bytes). Previously this alias was DEFERRED and
//    stock `--remote_cache=http` 404'd; it is now BUILT.
//
// # Endpoint mapping
//
// | HTTP | Route | Operation |
// |------|-------|-----------|
// | `GET`  | `/bazel/v2/:instance/blobs/:hash/:size` | CAS read (REST) |
// | `PUT`  | `/bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size` | CAS write (REST) |
// | `GET`  | `/bazel/v2/:instance/blobs/ac/:hash/:size` | AC read (REST) |
// | `PUT`  | `/bazel/v2/:instance/blobs/ac/:hash/:size` | AC write (REST) |
// | `POST` | `/bazel/v2/:instance/findMissingBlobs` | batch find-missing |
// | `GET`  | `/bazel/cache/cas/:hash` | CAS read (stock-Bazel HTTP alias) |
// | `PUT`  | `/bazel/cache/cas/:hash` | CAS write (stock-Bazel HTTP alias) |
// | `GET`  | `/bazel/cache/ac/:hash` | AC read (stock-Bazel HTTP alias) |
// | `PUT`  | `/bazel/cache/ac/:hash` | AC write (stock-Bazel HTTP alias) |
//
// # Tenant isolation
//
// The `:instance` path segment MUST equal the `x-corelink-tenant-id`
// header injected by the Cloudflare Worker post-auth. Mismatches produce
// HTTP 403 and an audit log entry via the underlying handler's audit
// surface (not duplicated here — the adapter already emits).
//
// # Route-level responsibilities
//
// 1. Extract `:instance`, `:hash`, `:size`, `:uuid` from path params.
// 2. Construct the REAPI [`Digest`] from `:hash`/`:size`.
// 3. Read caller metadata from Worker-injected headers.
// 4. Call the appropriate [`BazelAdapter`] or [`FindMissingHandler`]
//    method.
// 5. Map [`BazelBridgeError`] to the canonical HTTP status code.
//
// # Note on matchit 0.8 syntax
//
// All routes use the `{name}` brace capture form, NOT `:name`. The
// workspace is pinned to axum 0.8 / matchit 0.8, where `:name` is now a
// literal path segment (silent 404) and `{name}` is the capture — the
// inverse of matchit 0.7. Tracked as DEBT-029, closed on the CAS/AC surfaces too.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{FromRequestParts, Path, State},
    http::{request::Parts, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Router,
};
use corelink_bazel_bridge::{
    adapter::BazelAdapter,
    digest::Digest,
    error::BazelBridgeError,
    find_missing::{
        build_find_missing_response, parse_find_missing_request, FindMissingHandler,
        InMemoryFindMissing,
    },
};
use corelink_handler_ac::{
    AcLookupHandler, AcUpdateHandler, InMemoryAcHandler, InMemoryAuditSink as AcAuditSink,
    InMemorySliObserver as AcSliObserver,
};
use corelink_handler_cas::{
    CasReadHandler, CasWriteHandler, InMemoryAuditSink as CasAuditSink, InMemoryCasHandler,
    InMemorySliObserver as CasSliObserver,
};
use corelink_hash::CACHE_ENTRY_MAX_BYTES;

// ─── State ───────────────────────────────────────────────────────────────────

/// Shared state for all `/bazel/v2/*` routes.
///
/// Holds the [`BazelAdapter`] (wraps CAS + AC trait objects) and the
/// [`FindMissingHandler`] (backed by the same CAS read surface).
///
/// Cloning increments Arc ref-counts; no data is copied.
#[derive(Clone)]
pub struct BazelRouteState {
    /// REAPI CAS / AC adapter.
    pub adapter: BazelAdapter,
    /// `findMissingBlobs` handler.
    pub find_missing: Arc<dyn FindMissingHandler>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed); checked at the TOP of each billable
    /// REAPI handler, AFTER the scope gate + tenant resolve, BEFORE storage.
    /// `None` in dev/CI (not enforced). Set by `routes::build_with_factory`.
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production; re-runs the full Argon2id Option-B verify
    /// at the TOP of each billable REAPI handler, AFTER scope+tenant, BEFORE
    /// storage. `None` in dev/CI (skipped). See [`crate::native_pat_gate`].
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
    /// Per-tenant in-flight CAS/AC write concurrency counter (cluster F — the
    /// Bazel write path buffers the full ~64 MiB body before any gate and had
    /// NO per-tenant concurrency cap, so a single authenticated tenant could
    /// open N concurrent PUTs and consume N × body-limit of heap). Mirrors the
    /// Turbo `put_inflight` guard: a `FromRequestParts` extractor
    /// ([`BazelPutGuard`]) declared AHEAD of `body: Bytes` increments this
    /// BEFORE the body is buffered and rejects the over-cap PUT 429; the RAII
    /// [`BazelPutSlot`] releases on return. `Arc<Mutex<..>>` shared across the
    /// per-request state clones.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Display usage aggregator (usage-metering-roi). The REAPI CAS/AC read-HIT /
    /// read-MISS / write decision points `record` fire-and-forget into this meter
    /// — a cheap lock+increment, NEVER a DB call / `await` on the hot path. Inert
    /// in dev/CI; `build_with_factory` sets the ONE shared, D1-backed meter.
    pub usage_meter: Arc<crate::usage_meter::UsageMeter>,
}

/// Maximum concurrent in-flight Bazel CAS/AC writes for a single tenant. Excess
/// writes are rejected 429 BEFORE the (up to 64 MiB) body is buffered. Bounds
/// peak per-tenant write heap; mirrors [`super::turbo_v8::TURBO_PUT_CONCURRENCY_LIMIT`].
pub const BAZEL_WRITE_CONCURRENCY_LIMIT: usize = 8;

impl core::fmt::Debug for BazelRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BazelRouteState").finish_non_exhaustive()
    }
}

/// RAII release of one per-tenant in-flight Bazel write slot (decrements on
/// every return path — success, error, panic). See [`BazelPutGuard`].
pub(crate) struct BazelPutSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for BazelPutSlot {
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

/// `FromRequestParts` extractor reserving a per-tenant in-flight Bazel write slot
/// BEFORE the body is buffered (cluster F — pre-buffer OOM guard). Declared ahead
/// of `body: Bytes` in the write handlers so axum 0.7 runs it first: a tenant
/// already at [`BAZEL_WRITE_CONCURRENCY_LIMIT`] is rejected 429 with no body
/// read. Mirrors `turbo_v8::PutConcurrencyGuard`.
pub(crate) struct BazelPutGuard {
    _slot: BazelPutSlot,
}

impl FromRequestParts<BazelRouteState> for BazelPutGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &BazelRouteState,
    ) -> Result<Self, Self::Rejection> {
        // Reserve per AUTHENTICATED tenant (fail-CLOSED on missing/sentinel —
        // mirrors `caller_tenant`): an unauthenticated request never reserves a
        // slot and never buffers a body.
        let raw = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if crate::auth_tenant::is_reserved_sentinel(raw) {
            return Err(unauthenticated_tenant());
        }
        let tenant_key = raw.to_owned();
        {
            let mut inflight = match state.put_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(tenant_id = %tenant_key, error = %e, "bazel write concurrency tracker poisoned; failing closed");
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= BAZEL_WRITE_CONCURRENCY_LIMIT {
                tracing::warn!(tenant_id = %tenant_key, in_flight = *count, limit = BAZEL_WRITE_CONCURRENCY_LIMIT, "bazel write concurrency limit reached; 429 BEFORE body buffering");
                return Err(
                    (StatusCode::TOO_MANY_REQUESTS, "too many concurrent uploads").into_response(),
                );
            }
            *count += 1;
        }
        Ok(Self {
            _slot: BazelPutSlot {
                inflight: Arc::clone(&state.put_inflight),
                tenant_key,
            },
        })
    }
}

// ─── Handler factory ─────────────────────────────────────────────────────────

/// Build the canonical [`BazelRouteState`] from the CAS and AC handler
/// pairs that are already wired in the container.
///
/// This function reuses the SAME trait-object pattern as [`super::cas::build_handlers`]
/// and [`super::ac::build_handlers`]: on native targets with R2 credentials
/// it constructs the real R2-backed handlers; otherwise it falls back to
/// `InMemory`. The `BazelAdapter` wraps these four trait objects so the
/// `/bazel/v2/*` routes share the same backing store as the native
/// CAS/AC routes.
///
/// # Production path
///
/// In production, callers of `build_with_factory` (in `routes/build.rs`) will
/// call `cas::build_handlers()` and `ac::build_handlers()` first and
/// PASS those trait objects here, ensuring a single shared handler
/// instance. This overload is used by that composition; the
/// `build_handlers()` function below is for standalone use (tests, dev).
#[must_use]
pub fn build_handlers_from(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    ac_lookup: Arc<dyn AcLookupHandler>,
    ac_update: Arc<dyn AcUpdateHandler>,
) -> BazelRouteState {
    // The find-missing handler delegates to the same CAS read trait object
    // so it shares the backing store with CAS reads.
    let find_missing: Arc<dyn FindMissingHandler> =
        Arc::new(InMemoryFindMissing::new(cas_read.clone()));
    let adapter = BazelAdapter::new(cas_read, cas_write, ac_lookup, ac_update);
    BazelRouteState {
        adapter,
        find_missing,
        // Default OFF; `routes::build_with_factory` sets the D1-backed gate.
        quota: None,
        pat_gate: None,
        put_inflight: Arc::new(Mutex::new(HashMap::new())),
        // Inert placeholder; `build_with_factory` sets the shared, D1-backed meter.
        usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

/// Build a standalone [`BazelRouteState`] with fresh `InMemory` handlers.
///
/// Used by tests and dev paths where no shared CAS/AC state is needed.
/// The production `build_with_factory` in `routes/build.rs` uses
/// [`build_handlers_from`] to share the existing CAS/AC trait objects.
#[must_use]
pub fn build_handlers() -> BazelRouteState {
    let cas_audit = Arc::new(CasAuditSink::new());
    let cas_sli = Arc::new(CasSliObserver::new());
    let cas: Arc<InMemoryCasHandler> = Arc::new(InMemoryCasHandler::new(cas_audit, cas_sli));
    let ac_audit = Arc::new(AcAuditSink::new());
    let ac_sli = Arc::new(AcSliObserver::new());
    let ac: Arc<InMemoryAcHandler> = Arc::new(InMemoryAcHandler::new(ac_audit, ac_sli));
    build_handlers_from(
        cas.clone() as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        ac.clone() as Arc<dyn AcLookupHandler>,
        ac as Arc<dyn AcUpdateHandler>,
    )
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Build the axum `Router` mounting all five `/bazel/v2/*` REAPI routes.
///
/// All routes share the single [`BazelRouteState`]; axum disambiguates
/// operations by HTTP method and path template.
///
/// # matchit 0.8 note
///
/// Path params use `{name}` brace syntax (never `:name`, which is a literal
/// in matchit 0.8). See module-level doc for rationale.
pub fn router(state: BazelRouteState) -> Router {
    Router::new()
        // CAS read:  GET  /bazel/v2/:instance/blobs/:hash/:size
        .route(
            "/bazel/v2/{instance}/blobs/{hash}/{size}",
            get(handle_cas_read),
        )
        // AC read:   GET  /bazel/v2/:instance/blobs/ac/:hash/:size
        // NOTE: axum resolves /blobs/ac/… before /blobs/:hash/… because
        // "ac" is a literal segment that wins over the wildcard `:hash`.
        // We mount the AC read/write on the same path, differentiated by
        // HTTP method.
        .route(
            "/bazel/v2/{instance}/blobs/ac/{hash}/{size}",
            get(handle_ac_read)
                .put(handle_ac_write)
                .layer(axum::extract::DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)),
        )
        // CAS write: PUT  /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size
        .route(
            "/bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}",
            put(handle_cas_write)
                .layer(axum::extract::DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)),
        )
        // Find missing: POST /bazel/v2/:instance/findMissingBlobs
        .route(
            "/bazel/v2/{instance}/findMissingBlobs",
            post(handle_find_missing),
        )
        // ── Stock-Bazel HTTP cache alias ─────────────────────────────────────
        // Stock `bazel --remote_cache=https://host/bazel/cache` speaks the plain
        // HTTP-cache scheme: GET/PUT on `/{cas,ac}/<hash>` with NO `:instance`
        // segment and NO `:size`. (Buck2 cannot use these routes — it speaks
        // REAPI over gRPC only.) These routes map that shape onto the SAME
        // adapter/handlers/store as the REST scheme above; the tenant
        // (== REAPI `instance`) is derived from the Worker-injected
        // `x-corelink-tenant-id` header (fail-CLOSED via
        // `caller_tenant`), so isolation is by the per-tenant namespace. `cas`
        // and `ac` are literal segments (no conflict with each other).
        .route(
            "/bazel/cache/cas/{hash}",
            get(handle_http_cas_read)
                .put(handle_http_cas_write)
                .layer(axum::extract::DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)),
        )
        .route(
            "/bazel/cache/ac/{hash}",
            get(handle_http_ac_read)
                .put(handle_http_ac_write)
                .layer(axum::extract::DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)),
        )
        .with_state(state)
}

// ─── Header helpers ───────────────────────────────────────────────────────────

/// Read a header value; return `default` when absent or non-ASCII.
fn header_str(headers: &HeaderMap, name: &str, default: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Extract the authenticated `x-corelink-tenant-id`, **fail-CLOSED**.
///
/// F-defense: the previous version fell back to the `"_unknown"` sentinel on a
/// missing/empty header, so an unauthenticated request silently flowed into the
/// bridge under a sentinel tenant. We now mirror `auth_tenant::AuthTenant`:
/// a missing/empty/sentinel value is an `Err(())` that the handler maps to a
/// hard `401`. Only a concrete, non-sentinel tenant is returned. (The bridge's
/// `instance == caller_tenant` cross-tenant check still applies on top of this.)
///
/// Rejection routes through [`crate::auth_tenant::is_reserved_sentinel`] — the
/// SAME shared source-of-truth `AuthTenant` uses — so this REAPI surface (which
/// has no `AuthTenant` extractor) rejects the full reserved set, incl. `_oci`
/// and [`crate::adapter_cache::PUBLIC_NAMESPACE`] (`_public`). Otherwise a
/// `_public` claim would land in the shared cross-tenant dedup namespace
/// (`storage/r2_s3.rs`) → cache poisoning if the Worker's header-strip regresses.
fn caller_tenant(headers: &HeaderMap) -> Result<String, ()> {
    let raw = headers
        .get("x-corelink-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    if crate::auth_tenant::is_reserved_sentinel(raw) {
        return Err(());
    }
    Ok(raw.to_owned())
}

/// Canonical fail-CLOSED 401 for a missing/sentinel authenticated tenant.
/// Does not leak which condition tripped.
fn unauthenticated_tenant() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
}

/// Extract `x-corelink-token-prefix` as principal; fail-CLOSED to `"_unknown"`.
fn principal(headers: &HeaderMap) -> String {
    header_str(headers, "x-corelink-token-prefix", "_unknown")
}

/// Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) for the REAPI
/// surface: charge the flat per-op cost when the gate is wired. `Some(resp)` ⇒
/// REJECT (402 over-ceiling / 503 fail-CLOSED); `None` ⇒ proceed (and `None`
/// when the gate is absent in dev/CI). Called at the TOP of each billable
/// handler, AFTER the scope gate + tenant resolve, BEFORE storage.
async fn quota_reject(state: &BazelRouteState, tenant: &str) -> Option<axum::response::Response> {
    match state.quota.as_ref() {
        Some(gate) => gate.check(tenant).await,
        None => None,
    }
}

/// Run the native PAT possession gate (finding #4) when it is wired.
///
/// Reads the bearer PAT from the `Authorization` header and re-verifies it
/// (Argon2id, full Option-B pipeline) against the resolved `tenant`. `Some(resp)`
/// ⇒ REJECT (401 forged/wrong-tenant / 503 verifier fault); `None` ⇒ proceed (or
/// when the gate is absent in dev/CI). Called at the TOP of each billable
/// handler, AFTER the scope gate + tenant resolve, BEFORE storage.
async fn pat_gate_reject(
    state: &BazelRouteState,
    tenant: &str,
    headers: &HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify(tenant, bearer).await.err()
}

/// Like [`pat_gate_reject`] but for WRITE handlers: additionally enforces the
/// PAT's D1-derived `can_write` capability (not just the Worker-set
/// `x-corelink-scope` header), matching the two-layer enforcement cargo/OCI
/// already do (deep-audit B/F-1). A read-only PAT presented on a write path is
/// rejected `403` even if the scope header claimed write.
async fn pat_gate_reject_write(
    state: &BazelRouteState,
    tenant: &str,
    headers: &HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify_write(tenant, bearer).await.err()
}

/// Batch variant of [`quota_reject`] for `findMissingBlobs` (F12 fix —
/// quota-bypass-by-batching; closed completely by CAA-360 #14/#18).
///
/// `findMissingBlobs` accepts up to `FIND_MISSING_BLOB_CAP` (4096) digests
/// per request. Charging a single flat op cost for the whole batch allows a
/// tenant to drive up to 4096× more backend CAS work per accrued dollar than
/// the per-request cost model assumes. This charges one op cost unit per
/// digest (`n × cost`), making the cost proportional to the actual fan-out,
/// consistent with the ADR-0068 coarse-tripwire intent.
///
/// The charge is a SINGLE [`crate::routes::QuotaGate::check_batch`] call,
/// which issues one atomic check-and-accrue statement (`n × cost` in one D1
/// round-trip). This replaces the prior per-digest loop that was capped at 64
/// iterations: that cap under-charged every batch over 64 digests (CAA-360
/// #14/#18 — up to a 64× dilution at the 4096-digest cap), while also paying
/// one D1 round-trip per digest. Charging the whole batch in one statement
/// removes BOTH the enforcement gap and the round-trip cost, so there is no
/// iteration cap to leak through.
///
/// `n = 0` (empty batch) charges nothing and returns `None` (allow).
async fn quota_reject_batch(
    state: &BazelRouteState,
    tenant: &str,
    n: usize,
) -> Option<axum::response::Response> {
    let gate = state.quota.as_ref()?;
    gate.check_batch(tenant, n).await
}

/// Real wall-clock millis for audit-event timestamps (CAA-360 #6). Previously a
/// `const fn` returning `0`, which stamped every Bazel REAPI audit event with
/// epoch 0 — making the audit log un-orderable/un-correlatable. Uses the
/// production [`crate::wall_clock::SystemWallClock`]; CAS/AC/Turbo were fixed the same way.
fn now_ms() -> u64 {
    use crate::wall_clock::WallClock as _;
    crate::wall_clock::SystemWallClock.now_ms()
}

// ─── Error mapping ────────────────────────────────────────────────────────────

/// Map a [`BazelBridgeError`] to the canonical HTTP response.
///
/// HTTP status codes per the task spec:
/// - `NotFound`           → 404
/// - `InvalidDigest`      → 400
/// - `SizeMismatch`       → 422
/// - `CrossTenantDenied`  → 403
/// - `AuditFailed`        → 503
/// - `BatchTooLarge`      → 413
/// - `Internal`           → 500
fn map_bridge_err(e: BazelBridgeError) -> axum::response::Response {
    tracing::warn!(error = ?e, "bazel-bridge error");
    let (code, body): (StatusCode, &str) = match e {
        BazelBridgeError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found"),
        BazelBridgeError::InvalidDigest { .. } => (StatusCode::BAD_REQUEST, "invalid digest"),
        BazelBridgeError::InvalidRequest { .. } => {
            (StatusCode::BAD_REQUEST, "invalid request body")
        }
        BazelBridgeError::SizeMismatch { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "size mismatch")
        }
        BazelBridgeError::DigestMismatch { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "digest mismatch")
        }
        BazelBridgeError::CrossTenantDenied { .. } => (StatusCode::FORBIDDEN, "cross-tenant"),
        BazelBridgeError::AuditFailed(_) => (StatusCode::SERVICE_UNAVAILABLE, "audit closed"),
        BazelBridgeError::BatchTooLarge { .. } => {
            (StatusCode::PAYLOAD_TOO_LARGE, "batch too large")
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    (code, body).into_response()
}

/// Parse a `{hash}/{size}` digest pair from path params.
///
/// Returns `Err(BazelBridgeError::InvalidDigest)` when the digest is invalid.
fn parse_digest(hash: &str, size_str: &str) -> Result<Digest, BazelBridgeError> {
    let digest_str = format!("{hash}/{size_str}");
    Digest::parse(&digest_str)
}

// ─── Route handlers ───────────────────────────────────────────────────────────

/// `GET /bazel/v2/:instance/blobs/:hash/:size` — CAS read.
async fn handle_cas_read(
    State(state): State<BazelRouteState>,
    Path((instance, hash, size)): Path<(String, String, String)>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): the PAT must carry a cache-READ capability
    // (`cas:rw` or `cas:r`) in the Worker-trusted `x-corelink-scope` header
    // BEFORE any storage access. Mirrors the CAS/AC/Turbo surfaces; this
    // closes the previously-UNGATED Bazel REAPI cache surface. NO-OP for
    // current `cas:rw`/`admin` prod traffic; establishes the gate for
    // tiered (`cas:r`) tokens.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage access — do not trust a `"_unknown"` default.
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession gate (finding #4) — AFTER scope+tenant, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &tenant, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state
        .adapter
        .cas_get(&instance, &digest, &p, &tenant, now_ms())
    {
        Ok(bytes) => {
            // usage-metering-roi: CAS read HIT (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::ReadHit);
            (
                StatusCode::OK,
                [("content-type", "application/octet-stream")],
                bytes,
            )
                .into_response()
        }
        Err(e) => {
            // usage-metering-roi: a genuine NotFound is a read MISS; other errors
            // are faults, not classified ops.
            if matches!(e, BazelBridgeError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_bridge_err(e)
        }
    }
}

/// `PUT /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size` — CAS write.
///
/// `uuid` is the resumable-upload correlation ID used by Bazel; this
/// bridge accepts single-shot writes and logs the `uuid` for tracing
/// but does not perform any resumable-upload protocol.
async fn handle_cas_write(
    State(state): State<BazelRouteState>,
    Path((instance, uuid, hash, size)): Path<(String, String, String, String)>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
    // cluster F: pre-body per-tenant concurrency reservation (declared AHEAD of
    // `body: Bytes`, so axum runs it BEFORE the ~10 MiB body is buffered). 429 on
    // over-cap; the RAII slot releases on return.
    _concurrency: BazelPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): the PAT must carry a cache-WRITE capability
    // (`cas:rw`) BEFORE any storage access. A read-only (`cas:r`) token is
    // rejected here — this is the gap the cold review flagged (a `cas:r`
    // token could write blobs via Bazel while blocked on CAS/AC/Turbo).
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage access.
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession gate (finding #4) — AFTER scope+tenant, BEFORE storage.
    if let Some(resp) = pat_gate_reject_write(&state, &tenant, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    // REAPI v2 content-addressing boundary check (defense-in-depth + early
    // clean 422): the uploaded bytes MUST hash (SHA-256, the REAPI default) to
    // the client digest BEFORE we delegate to the shared handler. The durable
    // gate re-verifies the SHA-256 keyspace independently on write AND read
    // (bitrot); this boundary rejection just gives a clean error and never
    // admits poisoned bytes into the `bazel/sha256/` keyspace. `DigestMismatch`
    // maps to 422 via `map_bridge_err`.
    if let Err(e) = corelink_bazel_bridge::digest::verify_sha256(&digest.hash, &body) {
        return map_bridge_err(e);
    }
    tracing::debug!(
        instance = %instance,
        upload_id = %uuid,
        hash = %hash,
        "bazel CAS write"
    );
    match state.adapter.cas_put(
        &instance,
        &digest,
        body.to_vec(),
        corelink_bazel_bridge::adapter::WriteCtx {
            principal: &p,
            caller_tenant: &tenant,
            at_unix_ms: now_ms(),
            storage_quota_bytes: crate::byte_accounting::storage_quota_from_headers(&headers),
        },
    ) {
        Ok(()) => {
            // usage-metering-roi: CAS write (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::Write);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_bridge_err(e),
    }
}

/// `GET /bazel/v2/:instance/blobs/ac/:hash/:size` — AC read.
async fn handle_ac_read(
    State(state): State<BazelRouteState>,
    Path((instance, hash, size)): Path<(String, String, String)>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): AC lookup is a cache READ; require `cas:r`
    // (or `cas:rw`/`admin`) BEFORE any storage access.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage access.
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession gate (finding #4) — AFTER scope+tenant, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &tenant, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state
        .adapter
        .ac_get(&instance, &digest, &p, &tenant, now_ms())
    {
        Ok(bytes) => {
            // usage-metering-roi: AC read HIT (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::ReadHit);
            (
                StatusCode::OK,
                [("content-type", "application/octet-stream")],
                bytes,
            )
                .into_response()
        }
        Err(e) => {
            // usage-metering-roi: a genuine NotFound is a read MISS.
            if matches!(e, BazelBridgeError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_bridge_err(e)
        }
    }
}
