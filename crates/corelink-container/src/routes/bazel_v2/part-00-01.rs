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
