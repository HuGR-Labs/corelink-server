/// Parse an NDJSON `{"hash":"…"}` request body into an ordered list of hashes.
/// Blank lines are skipped; any malformed line ⇒ `Err((400, msg))` for the whole
/// request (a framing error — same discipline as the upload manifest). The error
/// is the small `(StatusCode, &str)` tuple (not a built `Response`) so the
/// `Result` stays cheap (clippy `result_large_err`); the caller turns it into a
/// response with `.into_response()`.
fn parse_ndjson_hashes(body: &[u8]) -> Result<Vec<String>, (StatusCode, &'static str)> {
    let text =
        std::str::from_utf8(body).map_err(|_| (StatusCode::BAD_REQUEST, "request not utf-8"))?;
    let mut hashes = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if hashes.len() >= BATCH_MAX_OBJECTS {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, "too many request lines"));
        }
        if line.len() > BATCH_MAX_LINE_BYTES {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, "request line too long"));
        }
        match serde_json::from_str::<BatchHashRequest>(line) {
            Ok(r) if r.hash.len() <= BATCH_MAX_HASH_BYTES => hashes.push(r.hash),
            Ok(_) => return Err((StatusCode::PAYLOAD_TOO_LARGE, "hash too long")),
            Err(_) => return Err((StatusCode::BAD_REQUEST, "malformed request line")),
        }
    }
    Ok(hashes)
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
    runner_job: crate::scope::RunnerJob,
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
    // cf-multitenant WP5b (fail-CLOSED): a narrowed runner-job PAT may NEVER
    // delete — a stolen per-job credential must not be able to EVICT the
    // tenant's cache. Denied BEFORE the scope gate (the Worker forwards the
    // job's write scope, so the write bit alone would let it through).
    if runner_job.is_runner_job() {
        return (
            StatusCode::FORBIDDEN,
            "delete not permitted for a runner-job credential",
        )
            .into_response();
    }
    // Scope gate (fail-CLOSED): delete is a cache WRITE — require `cas:rw`.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT write-capability gate (deep-audit money/auth F-1): a delete is
    // a mutation — re-derive the PAT's D1 `can_write` at the container, not just
    // possession. Read-only PAT on this write path ⇒ 403.
    if let Some(resp) = pat_gate_reject_write(&state, &auth.0, &headers).await {
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
    // Storage byte accounting (finding #1 / cluster-C): the reclaimed bytes are
    // RELEASED inside `state.delete` by the
    // [`crate::byte_accounting::AccountingCasHandler`] decorator (it reads the
    // size-bearing `CasDeleteResponse::reclaimed_bytes` the R2 delete handler now
    // populates via a pre-delete HEAD) so `bytes_used` drops. Idempotent: 204 No
    // Content for both deleted-existing and absent.
    match state.delete.delete(req) {
        Ok(resp) => {
            let _ = resp.existed;
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
    // Length-cap the opaque continuation cursor: a clean 400 reject rather than
    // letting an over-length value ride the edge query limit into the backend.
    if q.cursor
        .as_deref()
        .is_some_and(|c| c.len() > MAX_CURSOR_LEN)
    {
        return (StatusCode::BAD_REQUEST, "cursor too long").into_response();
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
        // B-051: the object exists and is intact; it is the SIZE that is
        // refused, so this is 413 and not 404 (which would tell the client to
        // re-upload bytes we already hold) and not 500 (which invites a retry
        // that cannot succeed).
        CasHandlerError::ObjectTooLarge { .. } => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "object exceeds the read-size ceiling",
        )
            .into_response(),
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
        // Storage byte-accounting decorator (finding #1 / cluster B+C): an
        // over-cap reservation ⇒ 402 (the tenant is over its storage cap); an
        // accounting-backend fault ⇒ 503 fail-CLOSED. Both ride a sentinel-tagged
        // `Internal` from `AccountingCasHandler` so they are distinct from a
        // generic 500.
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::OVER_CAP_SENTINEL) =>
        {
            (StatusCode::PAYMENT_REQUIRED, "storage quota exceeded").into_response()
        }
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::ACCT_UNAVAILABLE_SENTINEL) =>
        {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "storage accounting unavailable",
            )
                .into_response()
        }
        // F-004 — the shared tombstone gate (`TombstoneGatedCasHandler`): a
        // write that re-PUTs an erased `(tenant, hash)` is refused 410 Gone (an
        // erased artifact must never be resurrected at the same content address),
        // and a gate-lookup transport fault fails CLOSED 503 (never serve/commit
        // when the erasure gate is unconsultable). These ride sentinel-tagged
        // `Internal` errors from the gate decorator so they are distinct from a
        // generic 500.
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::routes::cas_erase::TOMBSTONE_GONE_SENTINEL) =>
        {
            (StatusCode::GONE, "erased").into_response()
        }
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::routes::cas_erase::TOMBSTONE_UNAVAILABLE_SENTINEL) =>
        {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "tombstone gate unavailable",
            )
                .into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}
