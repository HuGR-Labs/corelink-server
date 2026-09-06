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
            get(handle_read)
                .put(handle_write)
                .delete(handle_delete)
                .layer(axum::extract::DefaultBodyLimit::max(corelink_hash::CACHE_ENTRY_MAX_BYTES)),
        )
        .route(CAS_LIST_ROUTE, get(handle_list))
        // Bulk routes. These are SIBLINGS of `/v1/cas/:tenant/:hash`, not
        // captures of it: matchit-0.7 ranks the static `batch` / `batch-read` /
        // `batch-exists` literals ABOVE the `:hash` wildcard, so `POST
        // /v1/cas/t/batch` matches this route and `GET /v1/cas/t/<hash>` still
        // matches the read route (no method collision either — these are POST).
        .route(
            CAS_BATCH_ROUTE,
            post(handle_batch_write).layer(axum::extract::DefaultBodyLimit::max(
                BATCH_REQUEST_BODY_LIMIT_BYTES,
            )),
        )
        .route(
            CAS_BATCH_READ_ROUTE,
            post(handle_batch_read).layer(axum::extract::DefaultBodyLimit::max(
                BATCH_REQUEST_BODY_LIMIT_BYTES,
            )),
        )
        .route(
            CAS_BATCH_EXISTS_ROUTE,
            post(handle_batch_exists).layer(axum::extract::DefaultBodyLimit::max(
                BATCH_REQUEST_BODY_LIMIT_BYTES,
            )),
        )
        .with_state(state)
}

/// `true` if `headers` declares a `Content-Type` whose media type (ignoring any
/// `; charset=…` suffix and ASCII case) is one of `accepted`. Used by the batch
/// routes for the FROZEN 415 gate: a wrong/absent type is rejected BEFORE the
/// body is parsed (the length-framed wire format must not be misread as a
/// generic octet-stream).
fn content_type_is(headers: &axum::http::HeaderMap, accepted: &[&str]) -> bool {
    let ct = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Strip parameters (`application/x-ndjson; charset=utf-8`) and trim.
    let media = ct.split(';').next().unwrap_or("").trim();
    accepted.iter().any(|a| media.eq_ignore_ascii_case(a))
}

/// 413 over-cap body shared by `/batch` and `/batch-read` (FROZEN contract).
fn batch_too_large() -> axum::response::Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(serde_json::json!({
            "error": "batch_too_large",
            "limit_objects": BATCH_MAX_OBJECTS,
            "limit_bytes": BATCH_MAX_BYTES,
        })),
    )
        .into_response()
}

/// One manifest line of a `/batch` upload: a claimed content hash + the exact
/// byte length that follows for that object in the concatenated payload.
#[derive(serde::Deserialize)]
struct BatchUploadManifestEntry {
    /// Claimed blake3 content hash (validated canonical per object).
    hash: String,
    /// Exact byte length of this object in the length-framed payload.
    len: u64,
}

/// One NDJSON request line of `/batch-read` and `/batch-exists`.
#[derive(serde::Deserialize)]
struct BatchHashRequest {
    /// Requested content hash.
    hash: String,
}

/// Split a length-framed batch body into its manifest text and the raw payload
/// bytes at the FIRST blank line (`\n\n`). The manifest is the bytes before the
/// terminating blank line; the payload is everything after it. `None` ⇒ no blank
/// line terminator present (a framing error ⇒ 400 for the whole request).
fn split_manifest(body: &[u8]) -> Option<(&[u8], &[u8])> {
    // The manifest is newline-delimited JSON terminated by a SINGLE blank line.
    // After the last manifest line's `\n` there is one more `\n` (the blank
    // line), so the separator is `\n\n`. An empty manifest (zero objects) is
    // still framed by a leading blank line, i.e. the body begins with `\n`.
    let sep = body.windows(2).position(|w| w == b"\n\n")?;
    // `sep` is the index of the manifest-terminating `\n`; `sep + 1` is the
    // blank line's `\n`. The manifest is `..=sep` (includes the terminating
    // newline) and the payload is everything after the blank line (`sep + 2..`).
    // `position` guarantees `sep + 1 < body.len()`, so `sep + 2 <= body.len()`
    // and both `split_at` indices are in bounds (no panic — clippy-safe).
    let (manifest, rest) = body.split_at(sep + 1);
    let payload = rest.get(1..).unwrap_or(&[]);
    Some((manifest, payload))
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
    gate.verify(tenant, bearer).await.err()
}

/// Write-path variant of [`pat_gate_reject`]: independently re-derives the
/// PAT's D1-stored `can_write` capability at the container (via
/// `NativePatGate::verify_write`), not just tenant possession. This upholds the
/// Option-B invariant — "a compromised or misconfigured Worker cannot grant
/// write on its own" — on the CAS write path, matching the sibling build
/// surfaces (Bazel `pat_gate_reject_write`, Turbo/cargo/OCI `verify_write`).
/// A read-only PAT presented on a write path is rejected `403` even if the
/// Worker-set scope header claimed write (deep-audit money/auth F-1).
async fn pat_gate_reject_write(
    state: &CasRouteState,
    tenant: &str,
    headers: &axum::http::HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify_write(tenant, bearer).await.err()
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
    // B-052: reserve a per-tenant in-flight read slot BEFORE the handler runs,
    // so a burst of concurrent GETs cannot each buffer a full object into the
    // heap. Shares the SAME `read_inflight` pool as `handle_batch_read` on
    // purpose — see the note on `CAS_READ_CONCURRENCY_LIMIT`. The RAII slot
    // releases on every return path.
    _read_concurrency: CasReadConcurrencyGuard,
    // B-077: reserve process-wide bytes before storage buffers the object.
    _global_read_budget: GlobalCasReadBudgetGuard,
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
    // A lookup-transport error fails CLOSED to 503 (PEN-2/REV-S1): an erased
    // artifact is GDPR/DSR-deleted, and confidentiality of legally-erased data
    // outweighs availability of a live blob during a transient D1 blip — never
    // resurrect (200) erased bytes just because the gate couldn't be consulted.
    // `None` ⇒ no gate wired ⇒ classic 200/404.
    if let Some(tombstones) = state.tombstones.as_ref() {
        match tombstones.is_tombstoned(&auth.0, &hash).await {
            Ok(true) => return (StatusCode::GONE, "erased").into_response(),
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "cas: tombstone gate lookup failed; failing CLOSED (503)");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "tombstone gate unavailable",
                )
                    .into_response();
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
        Ok(resp) => {
            // usage-metering-roi: read HIT (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::ReadHit);
            // Keep the weighted permit alive until the response body has been
            // consumed or dropped. Returning `Bytes` directly would release
            // the guard at handler return while the response buffer remained
            // live, allowing concurrent reads to exceed the measured peak.
            let body_stream = async_stream::stream! {
                // The tenant slot follows the bytes for the same lifetime as
                // the process-wide permit; a slow client cannot open another
                // GET while this response is still consuming its reservation.
                let _read_slot = _read_concurrency;
                let _permit = _global_read_budget;
                yield Ok::<Frame<axum::body::Bytes>, std::convert::Infallible>(
                    Frame::data(axum::body::Bytes::from(resp.bytes)),
                );
            };
            (StatusCode::OK, Body::new(StreamBody::new(body_stream))).into_response()
        }
        Err(e) => {
            // usage-metering-roi: a genuine "not found" is a read MISS; every
            // other error is a fault, not a classified cache op — don't count it.
            if matches!(e, CasHandlerError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_err(e)
        }
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
    // cluster F: pre-body per-tenant concurrency reservation (declared AHEAD of
    // `body: Bytes`, so axum runs it BEFORE the body is buffered). 429 on over-cap;
    // the RAII slot releases on return. Mirrors `bazel_v2::BazelPutGuard`.
    _concurrency: CasPutGuard,
    // B-077: process-wide write bytes are reserved before body buffering.
    _global_write_budget: GlobalCasWriteBudgetGuard,
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
    // Native PAT write-capability gate (deep-audit money/auth F-1): re-derive
    // the PAT's D1 `can_write` at the container, not just tenant possession —
    // AFTER scope, BEFORE storage. A read-only PAT on a write path ⇒ 403 even
    // if the Worker-set scope header claimed write.
    if let Some(resp) = pat_gate_reject_write(&state, &auth.0, &headers).await {
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
    let req = CasWriteRequest::new(
        auth.0.clone(),
        hash,
        body.to_vec(),
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    )
    // Thread the Worker-resolved per-tier storage cap (server-trusted header)
    // into the reservation so the decorator seeds a FRESH `tenant_storage_state`
    // row with the REAL cap (not the legacy uncapped `0`). Absent ⇒ `None` ⇒
    // fail-CLOSED on an unseeded tenant (never treated as unlimited).
    .with_storage_quota_bytes(crate::byte_accounting::storage_quota_from_headers(&headers));
    // Storage byte accounting (finding #1 / cluster B+C) is enforced INSIDE
    // `state.write` by the [`crate::byte_accounting::AccountingCasHandler`]
    // decorator (wired in `routes::build_with_factory` when D1 is present):
    // reserve→commit→release happens AT the write trait object — the SINGLE
    // chokepoint every CAS write surface (native / Bazel / OCI / adapters)
    // shares. So the cap is enforced atomically BEFORE the R2 PUT, and an
    // over-cap or accounting-fault write surfaces here as a sentinel-tagged
    // `Internal` error that `map_err` maps to 402 / 503. The route no longer
    // accrues (that would double-count through the decorated handler).
    match state.write.write(req) {
        Ok(resp) => {
            // usage-metering-roi: write (both fresh 201 and idempotent 200 are a
            // WRITE op) — fire-and-forget, no await/I/O on the hot path.
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::Write);
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
