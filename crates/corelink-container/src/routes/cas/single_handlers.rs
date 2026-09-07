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
#[allow(
    clippy::too_many_arguments,
    reason = "axum supplies independent request extractors to the route handler"
)]
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
