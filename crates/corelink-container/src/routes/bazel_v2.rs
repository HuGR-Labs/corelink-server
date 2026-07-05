//! REAPI v2 Bazel remote-cache routes: `/bazel/v2/*`
//!
//! # Purpose
//!
//! Mounts the five REAPI v2 REST cache endpoints (the ByteStream-style
//! `/bazel/v2/:instance/blobs/:hash/:size` scheme — see the mapping
//! below) backed by the same R2 blobs the native CAS/AC endpoints serve.
//!
//! # Client contract (read before assuming `--remote_cache` works)
//!
//! This surface implements the CoreLink REAPI scheme and requires a
//! compatible client (a REAPI/ByteStream client, or `bazel` configured
//! against this scheme). It is NOT stock Bazel's plain HTTP cache: vanilla
//! `--remote_cache=http(s)://…` sends `/<cache>/<hash>` (e.g. `/cas/<hash>`,
//! `/ac/<hash>` — no `:instance` segment, no `:size`), which does NOT match
//! the routes below and 404s here. A stock-Bazel-HTTP `/cas|/ac` alias is a
//! tracked enhancement (TODO/DEFERRED), not a current capability.
//!
//! # Endpoint mapping
//!
//! | HTTP | Route | Operation |
//! |------|-------|-----------|
//! | `GET`  | `/bazel/v2/:instance/blobs/:hash/:size` | CAS read |
//! | `PUT`  | `/bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size` | CAS write |
//! | `GET`  | `/bazel/v2/:instance/blobs/ac/:hash/:size` | AC read |
//! | `PUT`  | `/bazel/v2/:instance/blobs/ac/:hash/:size` | AC write |
//! | `POST` | `/bazel/v2/:instance/findMissingBlobs` | batch find-missing |
//!
//! # Tenant isolation
//!
//! The `:instance` path segment MUST equal the `x-corelink-tenant-id`
//! header injected by the Cloudflare Worker post-auth. Mismatches produce
//! HTTP 403 and an audit log entry via the underlying handler's audit
//! surface (not duplicated here — the adapter already emits).
//!
//! # Route-level responsibilities
//!
//! 1. Extract `:instance`, `:hash`, `:size`, `:uuid` from path params.
//! 2. Construct the REAPI [`Digest`] from `:hash`/`:size`.
//! 3. Read caller metadata from Worker-injected headers.
//! 4. Call the appropriate [`BazelAdapter`] or [`FindMissingHandler`]
//!    method.
//! 5. Map [`BazelBridgeError`] to the canonical HTTP status code.
//!
//! # Note on matchit 0.7 syntax
//!
//! All routes use the `:name` capture form, NOT `{name}`. matchit 0.7.3
//! (pinned transitively via `axum = "0.7"`) treats `{name}` as a literal
//! path segment — this was tracked as DEBT-029 and closed on the CAS/AC
//! surfaces. The same rule is enforced here.

#![forbid(unsafe_code)]

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
    /// Bazel write path buffers the full ~10 MiB body before any gate and had
    /// NO per-tenant concurrency cap, so a single authenticated tenant could
    /// open N concurrent PUTs and consume N × body-limit of heap). Mirrors the
    /// Turbo `put_inflight` guard: a `FromRequestParts` extractor
    /// ([`BazelPutGuard`]) declared AHEAD of `body: Bytes` increments this
    /// BEFORE the body is buffered and rejects the over-cap PUT 429; the RAII
    /// [`BazelPutSlot`] releases on return. `Arc<Mutex<..>>` shared across the
    /// per-request state clones.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
}

/// Maximum concurrent in-flight Bazel CAS/AC writes for a single tenant. Excess
/// writes are rejected 429 BEFORE the (up to 10 MiB) body is buffered. Bounds
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

#[axum::async_trait]
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
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent uploads",
                )
                    .into_response());
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
/// In production, callers of `build_with_factory` (in `routes.rs`) will
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
    }
}

/// Build a standalone [`BazelRouteState`] with fresh `InMemory` handlers.
///
/// Used by tests and dev paths where no shared CAS/AC state is needed.
/// The production `build_with_factory` in `routes.rs` uses
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
/// # matchit 0.7 note
///
/// Path params use `:name` syntax (never `{name}`). See module-level
/// doc for rationale.
pub fn router(state: BazelRouteState) -> Router {
    Router::new()
        // CAS read:  GET  /bazel/v2/:instance/blobs/:hash/:size
        .route(
            "/bazel/v2/:instance/blobs/:hash/:size",
            get(handle_cas_read),
        )
        // AC read:   GET  /bazel/v2/:instance/blobs/ac/:hash/:size
        // NOTE: axum resolves /blobs/ac/… before /blobs/:hash/… because
        // "ac" is a literal segment that wins over the wildcard `:hash`.
        // We mount the AC read/write on the same path, differentiated by
        // HTTP method.
        .route(
            "/bazel/v2/:instance/blobs/ac/:hash/:size",
            get(handle_ac_read).put(handle_ac_write),
        )
        // CAS write: PUT  /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size
        .route(
            "/bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size",
            put(handle_cas_write),
        )
        // Find missing: POST /bazel/v2/:instance/findMissingBlobs
        .route(
            "/bazel/v2/:instance/findMissingBlobs",
            post(handle_find_missing),
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
async fn quota_reject(
    state: &BazelRouteState,
    tenant: &str,
) -> Option<axum::response::Response> {
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
        Ok(bytes) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            bytes,
        )
            .into_response(),
        Err(e) => map_bridge_err(e),
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
    match state
        .adapter
        .cas_put(
            &instance,
            &digest,
            body.to_vec(),
            corelink_bazel_bridge::adapter::WriteCtx {
                principal: &p,
                caller_tenant: &tenant,
                at_unix_ms: now_ms(),
                storage_quota_bytes: crate::byte_accounting::storage_quota_from_headers(&headers),
            },
        )
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
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
        Ok(bytes) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            bytes,
        )
            .into_response(),
        Err(e) => map_bridge_err(e),
    }
}

/// `PUT /bazel/v2/:instance/blobs/ac/:hash/:size` — AC write.
async fn handle_ac_write(
    State(state): State<BazelRouteState>,
    Path((instance, hash, size)): Path<(String, String, String)>,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: HeaderMap,
    // cluster F: pre-body per-tenant concurrency reservation (see handle_cas_write).
    _concurrency: BazelPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): AC update is a cache WRITE; require `cas:rw`
    // (or `admin`) BEFORE any storage access. A read-only token is rejected.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // cf-multitenant WP5b (fail-CLOSED): mirror the native `ac.rs` AC-write gate on
    // the Bazel REAPI v2 AC surface. A narrowed runner-job PAT with a pinned AC key
    // may write ONLY that exact key — otherwise a per-job credential could escape its
    // narrowing by routing an arbitrary AC write through `/bazel/v2/.../blobs/ac/:hash`
    // instead of `/v1/ac/...`. A `"*"` pin (the launch default) or no pin ⇒ no key
    // restriction (NO-OP for current traffic). CAS is content-addressed so it needs no
    // such gate; only the AC (action-cache) surface is key-poisonable.
    if !runner_job.ac_key_allowed(&hash) {
        return (
            StatusCode::FORBIDDEN,
            "ac write outside the job's allowed key",
        )
            .into_response();
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
        .ac_put(
            &instance,
            &digest,
            body.to_vec(),
            corelink_bazel_bridge::adapter::WriteCtx {
                principal: &p,
                caller_tenant: &tenant,
                at_unix_ms: now_ms(),
                storage_quota_bytes: crate::byte_accounting::storage_quota_from_headers(&headers),
            },
        )
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_bridge_err(e),
    }
}

/// `POST /bazel/v2/:instance/findMissingBlobs` — batch find-missing.
///
/// Accepts JSON body `{"blobDigests":[{"hash":"…","sizeBytes":N},…]}`.
/// Returns `{"missingBlobDigests":[…]}`.
async fn handle_find_missing(
    State(state): State<BazelRouteState>,
    Path(instance): Path<String>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): findMissingBlobs is a read/existence probe
    // over the CAS — require `cas:r` (or `cas:rw`/`admin`) BEFORE any storage
    // access. The body extractor (`Bytes`) stays LAST per axum 0.7 ordering;
    // `CacheScope` is `FromRequestParts` so it precedes the body.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any body parse or storage access.
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession gate (finding #4) — AFTER scope+tenant, BEFORE any
    // body parse or storage access.
    if let Some(resp) = pat_gate_reject(&state, &tenant, &headers).await {
        return resp;
    }
    let p = principal(&headers);

    // Parse + validate the JSON body BEFORE the quota gate so we know the
    // digest count (F12: charge proportional to batch fan-out).
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "request body is not valid UTF-8").into_response()
        }
    };
    let digests = match parse_find_missing_request(body_str) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };

    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    // F12 + CAA-360 #14/#18: charge one op-cost unit per digest (proportional
    // to the full batch fan-out, uncapped) in a single atomic check-and-accrue
    // so the ceiling trips at the right dollar amount with no per-digest
    // round-trips and no iteration cap to dilute large batches.
    if let Some(resp) = quota_reject_batch(&state, &tenant, digests.len()).await {
        return resp;
    }

    // Delegate to the find-missing handler.
    let missing = match state
        .find_missing
        .find_missing(&instance, &p, &tenant, now_ms(), &digests)
    {
        Ok(m) => m,
        Err(e) => return map_bridge_err(e),
    };

    // Serialise the response.
    match build_find_missing_response(missing) {
        Ok(json) => (StatusCode::OK, [("content-type", "application/json")], json).into_response(),
        Err(e) => map_bridge_err(e),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

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
        body::{to_bytes, Body},
        http::Request,
    };
    use corelink_bazel_bridge::digest::sha256_hex;
    use tower::ServiceExt;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TENANT: &str = "acme";

    /// Build a `HeaderMap` carrying the given `x-corelink-tenant-id` value.
    fn headers_with_tenant(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            "x-corelink-tenant-id",
            axum::http::HeaderValue::from_str(value).expect("valid header value"),
        );
        h
    }

    // ── Reserved-sentinel tenant rejection (fix-#4 parity) ────────────────────
    //
    // The REAPI surface has NO `AuthTenant` extractor; `caller_tenant` is the
    // backstop and MUST reject the SAME reserved set as `auth_tenant`, incl.
    // `_oci` and `_public` (the shared cross-tenant dedup namespace). A `_public`
    // claim reaching storage poisons the shared namespace.

    #[tokio::test]
    async fn caller_tenant_rejects_oci_sentinel() {
        let h = headers_with_tenant("_oci");
        assert!(
            caller_tenant(&h).is_err(),
            "_oci must never be accepted as a tenant on the REAPI surface"
        );
    }

    #[tokio::test]
    async fn caller_tenant_rejects_public_namespace_sentinel() {
        let h = headers_with_tenant(crate::adapter_cache::PUBLIC_NAMESPACE);
        assert!(
            caller_tenant(&h).is_err(),
            "_public (shared dedup namespace) must never be accepted as a tenant"
        );
    }

    #[tokio::test]
    async fn caller_tenant_accepts_concrete_tenant() {
        let h = headers_with_tenant("acme");
        assert_eq!(
            caller_tenant(&h).expect("concrete tenant must be accepted"),
            "acme"
        );
    }

    fn make_state() -> BazelRouteState {
        build_handlers()
    }

    /// Shared fixture: PUT a blob via the route, return the hash used.
    async fn seed_cas_via_route(app: &axum::Router, tenant: &str, hash: &str, payload: &[u8]) {
        let size = payload.len();
        let uuid = "test-uuid-0000";
        let uri = format!("/bazel/v2/{tenant}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", tenant)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload.to_vec()))
            .unwrap();
        // Must clone the router for oneshot; caller holds the arc in state.
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::NO_CONTENT,
            "seed PUT should return 204"
        );
    }

    // ── CAS read happy path ───────────────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_hit_returns_200_with_bytes() {
        let state = make_state();
        let payload = b"hello bazel".to_vec();
        let hash = sha256_hex(&payload);
        let app = router(state.clone());

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    // ── CAS read miss ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_miss_returns_404() {
        let state = make_state();
        let app = router(state);
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── CAS write + read round trip ───────────────────────────────────────────

    #[tokio::test]
    async fn cas_write_then_read_round_trip() {
        let state = make_state();
        let payload = b"round trip data".to_vec();
        let hash = sha256_hex(&payload);
        let app = router(state);

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let size = payload.len();
        let read_uri = format!("/bazel/v2/{TENANT}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&read_uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    // ── AC write + read round trip ────────────────────────────────────────────

    #[tokio::test]
    async fn ac_write_then_read_round_trip() {
        let state = make_state();
        let hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let payload = b"action_result_payload".to_vec();
        let size = payload.len();
        let app = router(state);

        // AC write (PUT)
        let write_uri = format!("/bazel/v2/{TENANT}/blobs/ac/{hash}/{size}");
        let put_req = Request::builder()
            .uri(&write_uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload.clone()))
            .unwrap();
        let put_resp = app.clone().oneshot(put_req).await.expect("PUT oneshot");
        assert_eq!(put_resp.status(), StatusCode::NO_CONTENT);

        // AC read (GET)
        let get_req = Request::builder()
            .uri(&write_uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let get_resp = app.oneshot(get_req).await.expect("GET oneshot");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let got = to_bytes(get_resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    /// WP5b parity with the native `ac.rs` gate: a narrowed runner-job PAT with a
    /// PINNED AC key may NOT write a DIFFERENT key through the Bazel AC surface
    /// (403 before any storage), and the `"*"` wildcard (the launch default) is a
    /// NO-OP that still writes. Closes the pin-bypass where a per-job credential
    /// could route an arbitrary AC write via `/bazel/v2/.../blobs/ac/:hash`.
    #[tokio::test]
    async fn runner_job_ac_pin_denies_bazel_write_to_other_key() {
        let write_hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let pinned_key = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let payload = b"result".to_vec();
        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/blobs/ac/{write_hash}/{size}");

        // Pinned to a DIFFERENT key → 403 before storage.
        let app = router(make_state());
        let put = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, pinned_key)
            .body(Body::from(payload.clone()))
            .unwrap();
        let resp = app.oneshot(put).await.expect("PUT oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "pinned key mismatch must 403");
        let body = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(body.as_ref(), b"ac write outside the job's allowed key");

        // Wildcard `"*"` (launch default) → NO-OP, the write proceeds.
        let app2 = router(make_state());
        let put2 = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
            .body(Body::from(payload))
            .unwrap();
        let resp2 = app2.oneshot(put2).await.expect("PUT oneshot");
        assert_eq!(resp2.status(), StatusCode::NO_CONTENT, "wildcard write must pass");
    }

    // ── findMissingBlobs — all absent ────────────────────────────────────────

    #[tokio::test]
    async fn find_missing_all_absent_returns_both_digests() {
        let state = make_state();
        let app = router(state);
        let hash_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let body = serde_json::json!({
            "blobDigests": [
                {"hash": HASH_A, "sizeBytes": 5},
                {"hash": hash_b, "sizeBytes": 5}
            ]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v["missingBlobDigests"]
                .as_array()
                .expect("missingBlobDigests")
                .len(),
            2
        );
    }

    // ── findMissingBlobs — one present, one absent ────────────────────────────

    #[tokio::test]
    async fn find_missing_partial_returns_only_absent() {
        let state = make_state();
        let payload = b"present blob".to_vec();
        let hash = sha256_hex(&payload);
        let app = router(state);

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let hash_absent = HASH_A;
        let body = serde_json::json!({
            "blobDigests": [
                {"hash": &hash, "sizeBytes": payload.len()},
                {"hash": hash_absent, "sizeBytes": 5}
            ]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let missing = v["missingBlobDigests"].as_array().expect("array");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0]["hash"], hash_absent);
    }

    // ── findMissingBlobs — quota charged for EVERY digest (CAA-360 #14/#18) ────

    /// Build a `BazelRouteState` whose quota gate is seeded so the tenant has
    /// `budget_micros` of headroom at a flat `1` micro-USD per op. Mirrors
    /// `cas::fixture_over_ceiling`'s wiring (in-memory store + fake clock).
    fn make_state_with_quota(tenant: &str, budget_micros: i64) -> BazelRouteState {
        use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaState, QuotaStore};
        use crate::wall_clock::InMemoryFakeWallClock;

        let mut state = build_handlers();
        let store = InMemoryQuotaStore::new();
        store.seed(
            tenant,
            QuotaState {
                monthly_budget_usd_micros: budget_micros,
                accrued_usd_micros: 0,
                cycle_anchor_ms: 1_700_000_000_000,
            },
        );
        let store: Arc<dyn QuotaStore> = Arc::new(store);
        // Clock pinned AT the anchor so the cycle never rolls mid-test.
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let guard = Arc::new(QuotaGuard::new(store, clock));
        state.quota = Some(crate::routes::QuotaGate::new_for_test(guard, 1));
        state
    }

    /// A `findMissingBlobs` batch of N > 64 digests must be charged for ALL N
    /// digests, not the legacy 64-iteration cap. With a budget of 100 micro-USD
    /// at 1 µ$/op, a 150-digest batch costs 150 µ$ — over the ceiling → 402.
    /// Under the old `BATCH_QUOTA_ITERS_CAP = 64`, only 64 µ$ would have been
    /// charged (64 < 100), so the batch would have WRONGLY returned 200. This
    /// test fails on that regression and passes on the proportional charge.
    #[tokio::test]
    async fn find_missing_charges_every_digest_beyond_cap_trips_402() {
        let state = make_state_with_quota(TENANT, 100);
        let app = router(state);

        // 150 distinct, canonical 64-hex digests (> the old 64 cap).
        let blob_digests: Vec<serde_json::Value> = (0..150u32)
            .map(|i| serde_json::json!({ "hash": format!("{i:064x}"), "sizeBytes": 5 }))
            .collect();
        let body = serde_json::json!({ "blobDigests": blob_digests }).to_string();

        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::PAYMENT_REQUIRED,
            "150-digest batch (150 µ$) must exceed the 100 µ$ ceiling — proves \
             every digest is charged, not a 64-capped subset"
        );
    }

    /// Control: a batch that fits under the ceiling proceeds (200), proving the
    /// batch gate is proportional, not a blanket block. 80 digests = 80 µ$ < 100.
    #[tokio::test]
    async fn find_missing_under_ceiling_proceeds() {
        let state = make_state_with_quota(TENANT, 100);
        let app = router(state);

        let blob_digests: Vec<serde_json::Value> = (0..80u32)
            .map(|i| serde_json::json!({ "hash": format!("{i:064x}"), "sizeBytes": 5 }))
            .collect();
        let body = serde_json::json!({ "blobDigests": blob_digests }).to_string();

        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "80-digest batch (80 µ$) is under the 100 µ$ ceiling — must proceed"
        );
    }

    // ── Cross-tenant denial — CAS read ────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_cross_tenant_returns_403() {
        let state = make_state();
        let app = router(state);
        // instance = "victim" in URL, but caller header = "attacker"
        let uri = format!("/bazel/v2/victim/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", "attacker")
            .header("x-corelink-token-prefix", "tok_attacker")
            // Carry a valid cache scope so the request CLEARS the scope gate
            // and reaches the adapter's cross-tenant check (the SUT here).
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── Cross-tenant denial — AC write ────────────────────────────────────────

    #[tokio::test]
    async fn ac_write_cross_tenant_returns_403() {
        let state = make_state();
        let app = router(state);
        let hash = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
        let uri = format!("/bazel/v2/victim/blobs/ac/{hash}/8");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", "attacker")
            .header("x-corelink-token-prefix", "tok_attacker")
            // Valid write scope so the request clears the scope gate and the
            // adapter's cross-tenant check is what produces the 403.
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"payload".to_vec()))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── CAS write size mismatch returns 422 ───────────────────────────────────

    #[tokio::test]
    async fn cas_write_size_mismatch_returns_422() {
        let state = make_state();
        let app = router(state);
        let payload = b"five!".to_vec(); // 5 bytes
        let hash = sha256_hex(&payload);
        // Claim size 99 but send 5 bytes.
        let uuid = "test-uuid-mismatch";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/99");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ── CAS write with a MISMATCHED sha256 digest returns 422 ─────────────────

    /// REAPI v2 content-addresses with SHA-256: a write whose body does NOT
    /// hash to the client-supplied digest is rejected 422 at the REAPI
    /// boundary (`verify_sha256`) BEFORE delegating to the shared handler —
    /// poisoned bytes never enter the `bazel/sha256/` keyspace.
    #[tokio::test]
    async fn cas_write_sha256_mismatch_returns_422() {
        let state = make_state();
        let app = router(state);
        let payload = b"honest bazel bytes".to_vec();
        // A well-formed but WRONG sha256 digest (correct length, wrong value).
        let wrong_hash = "0".repeat(64);
        let size = payload.len();
        let uuid = "test-uuid-poison";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{wrong_hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ── Invalid digest in URL returns 400 ─────────────────────────────────────

    #[tokio::test]
    async fn cas_read_bad_digest_returns_400() {
        let state = make_state();
        let app = router(state);
        // hash is too short → invalid digest
        let uri = format!("/bazel/v2/{TENANT}/blobs/badhash/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── build_handlers smoke test ─────────────────────────────────────────────

    #[test]
    fn build_handlers_constructs_without_panic() {
        let state = build_handlers();
        let _r = router(state);
    }

    // ── Route constants use matchit-0.7 colon syntax ──────────────────────────

    #[test]
    fn route_paths_use_colon_syntax_not_curly_braces() {
        // Regression net per DEBT-029: `{name}` is a LITERAL in matchit 0.7.
        let paths = [
            "/bazel/v2/:instance/blobs/:hash/:size",
            "/bazel/v2/:instance/blobs/ac/:hash/:size",
            "/bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size",
            "/bazel/v2/:instance/findMissingBlobs",
        ];
        for p in paths {
            assert!(
                !p.contains('{'),
                "path {p:?} must use :name syntax, not {{name}}"
            );
        }
    }

    // ── Cache-scope enforcement (x-corelink-scope) ────────────────────────────
    //
    // The Bazel REAPI surface was the one PAT-reachable cache surface left
    // UNGATED after PR #159 wired the scope gate into CAS/AC/Turbo. These
    // tests pin the gate fail-CLOSED on the Bazel handlers so a read-only
    // (`cas:r`) token can no longer write blobs via Bazel. They mirror the
    // CAS/AC/Turbo scope tests: missing scope → 403, read-only on a write →
    // 403, `cas:rw`/`admin` → passes the gate.

    /// Read the 403 response body as bytes (helper for the assertions below).
    async fn body_bytes(resp: axum::response::Response) -> Vec<u8> {
        to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("body")
            .to_vec()
    }

    /// Fail-CLOSED: a CAS read with NO `x-corelink-scope` header is rejected
    /// 403 "insufficient scope" BEFORE any storage access.
    #[tokio::test]
    async fn cas_read_missing_scope_returns_403_insufficient_scope() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            // no x-corelink-scope header → fail-CLOSED
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// Fail-CLOSED: a CAS write with NO scope header is rejected 403 BEFORE
    /// the adapter `cas_put` is reached (the gap the cold review flagged).
    #[tokio::test]
    async fn cas_write_missing_scope_returns_403_insufficient_scope() {
        let app = router(make_state());
        let payload = b"no-scope-write".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uuid = "test-uuid-noscope";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            // no x-corelink-scope header → fail-CLOSED
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// A read-only (`cas:r`) token on a CAS write is rejected 403 BEFORE the
    /// adapter — the core privilege-escalation the gate prevents.
    #[tokio::test]
    async fn cas_write_read_only_scope_returns_403() {
        let app = router(make_state());
        let payload = b"read-only-cannot-write".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uuid = "test-uuid-readonly";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// A read-only (`cas:r`) token on an AC write is rejected 403 BEFORE the
    /// adapter (AC update is a cache write).
    #[tokio::test]
    async fn ac_write_read_only_scope_returns_403() {
        let app = router(make_state());
        let hash = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let uri = format!("/bazel/v2/{TENANT}/blobs/ac/{hash}/7");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(b"payload".to_vec()))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// Fail-CLOSED: findMissingBlobs (a read/existence probe) with NO scope
    /// header is rejected 403 BEFORE the find-missing handler runs.
    #[tokio::test]
    async fn find_missing_missing_scope_returns_403() {
        let app = router(make_state());
        let body = serde_json::json!({
            "blobDigests": [{"hash": HASH_A, "sizeBytes": 5}]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header("content-type", "application/json")
            // no x-corelink-scope header → fail-CLOSED
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// A read-only (`cas:r`) token PASSES the read gate on a CAS read: the
    /// blob is absent so the route returns 404 — proving the gate let the
    /// read through (a denied scope would 403 before storage).
    #[tokio::test]
    async fn cas_read_read_only_scope_passes_gate_then_404() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// An `admin` token (the prod superset, pre back-fill) passes the write
    /// gate: a CAS write succeeds (204) — proving `admin` grants cache rw and
    /// the gate is a NO-OP for live admin-scoped traffic.
    #[tokio::test]
    async fn cas_write_admin_scope_succeeds() {
        let app = router(make_state());
        let payload = b"admin-writes-ok".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uuid = "test-uuid-admin";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "admin")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // ── F-defense: fail-CLOSED on missing / sentinel tenant header ─────────────
    //
    // The previous `caller_tenant` fell back to the `"_unknown"` sentinel, so
    // an unauthenticated request flowed into the bridge. These prove every
    // Bazel surface now rejects 401 (and BEFORE any storage / body parse) when
    // the authenticated-tenant header is absent or a sentinel — even with a
    // valid cache scope.

    #[tokio::test]
    async fn cas_read_missing_tenant_header_returns_401() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            // No x-corelink-tenant-id; valid scope so the 401 is the tenant
            // gate, not the scope gate.
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cas_read_sentinel_tenant_returns_401() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/_unknown/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            // Sentinel tenant header must be rejected even though instance
            // == header (so the cross-tenant check would otherwise pass).
            .header("x-corelink-tenant-id", "_unknown")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cas_write_missing_tenant_header_returns_401() {
        let app = router(make_state());
        let payload = b"no-tenant".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/uploads/u1/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ac_read_missing_tenant_header_returns_401() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/ac/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn find_missing_missing_tenant_header_returns_401() {
        let app = router(make_state());
        // A valid JSON body — the 401 must fire BEFORE the body is parsed.
        let body = serde_json::json!({
            "blobDigests": [{"hash": HASH_A, "sizeBytes": 5}]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── finding #4: native PAT possession gate ───────────────────────────────

    /// A Bazel CAS read with the PAT gate wired but NO bearer Authorization
    /// header is rejected 401 — the native gate fails CLOSED on a missing token
    /// even though the Worker-set tenant header is present (defense-in-depth on
    /// the previously HMAC-only Bazel REAPI surface).
    #[tokio::test]
    async fn pat_gate_missing_bearer_returns_401() {
        use crate::adapter_pat::PatRow;
        use crate::native_pat_gate::testing::verifier_with_row;
        use crate::native_pat_gate::NativePatGate;
        use corelink_pat::PatSigningKey;

        let mut state = make_state();
        let key = Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("key"));
        let verifier = verifier_with_row(
            "no-such-token".to_owned(),
            PatRow {
                tenant_id: TENANT.to_owned(),
                pat_hash: String::new(),
                scope: "cas:rw".to_owned(),
            },
            key,
        );
        state.pat_gate = Some(Arc::new(NativePatGate::new_for_test(verifier)));
        let app = router(state);
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/5"))
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            // No Authorization header — the gate rejects.
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── cluster F: per-tenant pre-body write concurrency cap ──────────────────

    /// With the tenant AT `BAZEL_WRITE_CONCURRENCY_LIMIT` in-flight writes, the
    /// next CAS write is rejected 429 BEFORE its body is buffered — the
    /// `BazelPutGuard` `FromRequestParts` extractor runs ahead of `body: Bytes`.
    /// Proven deterministically by sending a body LARGER than the global 10 MiB
    /// limit: if the body were buffered first, the body-limit layer would reject
    /// it; because the concurrency guard runs first, we get 429 and the oversized
    /// body is never read.
    #[tokio::test]
    async fn cas_write_at_concurrency_limit_returns_429_before_body() {
        let state = make_state();
        {
            let mut g = state.put_inflight.lock().unwrap();
            g.insert(TENANT.to_owned(), BAZEL_WRITE_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]); // > 10 MiB
        let uri = format!("/bazel/v2/{TENANT}/uploads/u1/blobs/{HASH_A}/5");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(oversized)
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit Bazel write must be rejected 429 by the pre-body concurrency guard"
        );
    }

    /// A write BELOW the limit succeeds and releases its slot (counter back to 0).
    #[tokio::test]
    async fn cas_write_below_limit_releases_slot() {
        let state = make_state();
        let app = router(state.clone());
        let payload = b"slot-release".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/uploads/u1/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let g = state.put_inflight.lock().unwrap();
        assert_eq!(
            g.get(TENANT),
            None,
            "the concurrency slot must be released after the write completes"
        );
    }
}
