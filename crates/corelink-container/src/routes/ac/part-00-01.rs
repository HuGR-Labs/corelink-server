    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify(tenant, bearer).await.err()
}

/// Write-path variant of [`pat_gate_reject`]: independently re-derives the PAT's
/// D1-stored `can_write` capability at the container (via
/// `NativePatGate::verify_write`), not just tenant possession — upholding the
/// Option-B invariant ("a compromised or misconfigured Worker cannot grant write
/// on its own") on the AC update/delete paths, matching the sibling surfaces
/// (Bazel/Turbo/cargo/OCI). A read-only PAT on a write path ⇒ `403` even if the
/// Worker-set scope header claimed write (deep-audit money/auth F-1).
async fn pat_gate_reject_write(
    state: &AcRouteState,
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

/// `GET /v1/ac/:tenant/:action_digest` handler — lookup.
async fn handle_lookup(
    State(state): State<AcRouteState>,
    Path((tenant, action_digest)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // The authenticated tenant (DO-injected `x-corelink-tenant-id`,
    // PAT-resolved by the Worker) is the SOLE isolation key. The path
    // `:tenant` is a client-controllable echo that MUST match it;
    // mismatch is a cross-tenant attempt and is denied 403 BEFORE any
    // storage access (no tenant quoted in the body). Mirrors bazel_v2.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed action_digest BEFORE it derives an R2 key.
    if !is_canonical_digest(&action_digest) {
        return (StatusCode::BAD_REQUEST, "malformed action_digest").into_response();
    }
    // Scope gate (fail-CLOSED): AC lookup is a cache READ — require
    // `cas:rw` or `cas:r`. BEFORE any storage access. NO-OP for `cas:rw`.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost, AFTER the scope gate, BEFORE storage. 402 over-ceiling /
    // 503 fail-CLOSED. `None` in dev/CI ⇒ not enforced.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    // CAA-360 #6: real audit-event timestamp from the production SystemWallClock
    // (was hardcoded `0u64`; see cas.rs for the same fix).
    let now_ms = SystemWallClock.now_ms();
    let req = AcLookupRequest::new(
        auth.0.clone(),
        action_digest,
        // Principal is filled by an auth middleware in production;
        // demo wire-up mirrors cas.rs.
        format!("anon@{}", auth.0),
        auth.0,
        now_ms,
    );
    match state.lookup.lookup(req) {
        Ok(resp) => (StatusCode::OK, resp.result_payload).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /v1/ac/:tenant/:action_digest` handler — update.
struct AcUpdateParts {
    tenant: String,
    action_digest: String,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: axum::http::HeaderMap,
    _concurrency: AcPutGuard,
}

impl FromRequestParts<AcRouteState> for AcUpdateParts {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AcRouteState,
    ) -> Result<Self, Self::Rejection> {
        let Path((tenant, action_digest)) =
            Path::<(String, String)>::from_request_parts(parts, state)
                .await
                .map_err(IntoResponse::into_response)?;
        let auth = crate::auth_tenant::AuthTenant::from_request_parts(parts, state).await?;
        let scope = match crate::scope::CacheScope::from_request_parts(parts, state).await {
            Ok(value) => value,
            Err(never) => match never {},
        };
        let runner_job = match crate::scope::RunnerJob::from_request_parts(parts, state).await {
            Ok(value) => value,
            Err(never) => match never {},
        };
        let _concurrency = AcPutGuard::from_request_parts(parts, state).await?;
        Ok(Self {
            tenant,
            action_digest,
            auth,
            scope,
            runner_job,
            headers: parts.headers.clone(),
            _concurrency,
        })
    }
}

async fn handle_update(
    State(state): State<AcRouteState>,
    AcUpdateParts {
        tenant,
        action_digest,
        auth,
        scope,
        runner_job,
        headers,
        _concurrency,
    }: AcUpdateParts,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // See `handle_lookup`: the authenticated tenant is the sole
    // isolation key; the path `:tenant` is a client echo that must
    // match. Deny 403 BEFORE any storage access on mismatch.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed action_digest BEFORE it derives an R2 key.
    if !is_canonical_digest(&action_digest) {
        return (StatusCode::BAD_REQUEST, "malformed action_digest").into_response();
    }
    // Scope gate (fail-CLOSED): AC update is a cache WRITE — require
    // `cas:rw`. A read-only (`cas:r`) token is rejected here. NO-OP for
    // current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // cf-multitenant WP5b (fail-CLOSED): a narrowed runner-job PAT with a pinned
    // AC key may write ONLY that exact key — a stolen per-job credential must not
    // write results outside the job it was minted for. A `"*"` pin (the launch
    // default) or no pin ⇒ no key restriction (create allowed at any key; overwrite
    // still 409 by INV-AC-RESULT-HASH-IMMUTABLE; delete still denied separately).
    if !runner_job.ac_key_allowed(&action_digest) {
        return (
            StatusCode::FORBIDDEN,
            "ac write outside the job's allowed key",
        )
            .into_response();
    }
    // Native PAT write-capability gate (deep-audit money/auth F-1): re-derive the
    // PAT's D1 `can_write` at the container — an AC update is a mutation, so a
    // read-only PAT ⇒ 403 even if the Worker-set scope header claimed write.
    if let Some(resp) = pat_gate_reject_write(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) — see
    // `handle_lookup`. AFTER the scope gate, BEFORE storage.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = AcUpdateRequest::new(
        auth.0.clone(),
        action_digest,
        body.to_vec(),
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    )
    // Thread the Worker-resolved per-tier storage cap so the decorator seeds a
    // FRESH `tenant_storage_state` row with the REAL cap (not uncapped `0`).
    // Absent ⇒ `None` ⇒ fail-CLOSED on an unseeded tenant.
    .with_storage_quota_bytes(crate::byte_accounting::storage_quota_from_headers(&headers));
    // Storage byte accounting (finding #1 / cluster B+C) is enforced INSIDE
    // `state.update` by the [`crate::byte_accounting::AccountingAcHandler`]
    // decorator (reserve→commit→release at the AC update trait object, shared
    // with the Bazel AC write surface). An over-cap / accounting-fault write
    // surfaces here as a sentinel-tagged `Internal` error mapped to 402 / 503.
    match state.update.update(req) {
        Ok(resp) => {
            // AC create-only (deny-overwrite) — anti AC-squat, the AC analog of
            // deny-DELETE. The store's `durable` flag is an ATOMIC put-if-absent
            // signal: `true` = the row was FRESHLY INSERTED (the entry was
            // absent); `false` = the `(tenant, action_digest)` entry ALREADY
            // EXISTED and this was a byte-identical idempotent no-op (NOT a
            // destructive overwrite — the store is content-immutable). A
            // create-only cred may CREATE but never touch an existing entry, so a
            // non-durable result is rejected 409. NO TOCTOU window: the decision
            // rides the store's OWN conditional insert (the same atomic point
            // that backs INV-AC-RESULT-HASH-IMMUTABLE), not a separate look-up.
            if runner_job.ac_create_only() && !resp.durable {
                return ac_create_only_conflict();
            }
            let code = if resp.durable {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (code, resp.action_digest).into_response()
        }
        // A create-only cred whose write hit an existing entry with DIVERGENT
        // bytes: the store already refused the overwrite (DivergentBody, the
        // INV-AC-RESULT-HASH-IMMUTABLE gate). Surface it with the consistent
        // AC_CREATE_ONLY body rather than the generic divergent-body 409 — both
        // are 409 rejections of the same overwrite attempt by this cred.
        Err(AcHandlerError::DivergentBody { .. }) if runner_job.ac_create_only() => {
            ac_create_only_conflict()
        }
        Err(e) => map_err(e),
    }
}

/// `DELETE /v1/ac/:tenant/:action_digest` handler — delete a ref (D-1).
///
/// Idempotent: returns 204 No Content whether the ref existed or not.
/// Requires a WRITE-capable PAT (mirrors `handle_update`).
async fn handle_delete(
    State(state): State<AcRouteState>,
    Path((tenant, action_digest)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Cross-tenant: deny 403 BEFORE any storage access (mirrors lookup).
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed action_digest BEFORE it derives an R2 key.
    if !is_canonical_digest(&action_digest) {
        return (StatusCode::BAD_REQUEST, "malformed action_digest").into_response();
    }
    // cf-multitenant WP5b (fail-CLOSED): a narrowed runner-job PAT may NEVER
    // delete an AC ref — a stolen per-job credential must not be able to EVICT
    // the tenant's action cache. Denied BEFORE the scope gate (the Worker
    // forwards the job's write scope, so the write bit alone would let it through).
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
    // Native PAT write-capability gate (deep-audit money/auth F-1): an AC delete
    // is a mutation — re-derive the PAT's D1 `can_write` at the container, not
    // just possession. Read-only PAT on this write path ⇒ 403.
    if let Some(resp) = pat_gate_reject_write(&state, &auth.0, &headers).await {
        return resp;
    }
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = AcDeleteRequest::new(
        auth.0.clone(),
        action_digest,
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    );
    // Storage byte accounting (finding #1 / cluster-C): the reclaimed bytes are
    // RELEASED inside `state.delete` by the
    // [`crate::byte_accounting::AccountingAcHandler`] decorator (reads the
    // size-bearing `AcDeleteResponse::reclaimed_bytes` the R2 delete handler now
    // populates via a pre-delete HEAD). Idempotent: 204 either way.
    match state.delete.delete(req) {
        Ok(resp) => {
            let _ = resp.existed;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_err(e),
    }
}
