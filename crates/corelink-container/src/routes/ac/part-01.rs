/// `GET /v1/ac/:tenant` handler — paginated ref enumeration (D-7).
///
/// Requires a READ-capable PAT. Returns
/// `{ "refs": [...], "next_cursor": <opaque|null> }`.
async fn handle_list_refs(
    State(state): State<AcRouteState>,
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
    let req = AcListRequest::new(
        auth.0.clone(),
        format!("anon@{}", auth.0),
        auth.0,
        clamp_limit(q.limit),
        q.cursor,
        now_ms,
    );
    match state.list.list(req) {
        Ok(resp) => {
            let refs: Vec<_> = resp
                .refs
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "ref_key": r.ref_key,
                        "updated_at": r.updated_at,
                        "size": r.size,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "refs": refs,
                    "next_cursor": resp.next_cursor,
                })),
            )
                .into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Canonical **409 CONFLICT** for a create-only (deny-overwrite) runner-job cred
/// that tried to OVERWRITE an existing `(tenant, action_digest)` AC entry — the
/// anti AC-squat rejection (see [`crate::scope::RunnerJob::ac_create_only`]). The
/// FIRST write of a key proceeds; any later write by a create-only cred is
/// refused, so the AC is append-only per tenant for these creds.
fn ac_create_only_conflict() -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "error": "AC_CREATE_ONLY",
            "message": "create-only cred cannot overwrite an existing action-cache entry",
        })),
    )
        .into_response()
}

/// Map an [`AcHandlerError`] to the canonical HTTP response.
fn map_err(e: AcHandlerError) -> axum::response::Response {
    match e {
        AcHandlerError::Miss { .. } => (StatusCode::NOT_FOUND, "ac miss").into_response(),
        AcHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        AcHandlerError::DivergentBody { .. } => {
            // Same (tenant, action_digest), different bytes: a proven AC
            // result must never be silently overwritten. 409 Conflict.
            (StatusCode::CONFLICT, "divergent body").into_response()
        }
        AcHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve
            // bytes / commit updates without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // F1 (CAA-360) fail-CLOSED: the route was mounted with storage creds
        // present but the R2 handler refused to build (R2_TDK_HEX unset/invalid),
        // so it is serving the `UnavailableAcHandler`. Surface that as 503
        // "storage unavailable" — the cache is unavailable until R2_TDK_HEX is
        // set, NOT a generic 500. See [`UnavailableAcHandler`].
        AcHandlerError::Internal(ref msg) if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL) => {
            (StatusCode::SERVICE_UNAVAILABLE, "storage unavailable").into_response()
        }
        // Storage byte-accounting decorator (finding #1 / cluster B+C): over-cap
        // ⇒ 402, accounting-backend fault ⇒ 503 fail-CLOSED. Both ride a
        // sentinel-tagged `Internal` from `AccountingAcHandler`.
        AcHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::OVER_CAP_SENTINEL) =>
        {
            (StatusCode::PAYMENT_REQUIRED, "storage quota exceeded").into_response()
        }
        AcHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::ACCT_UNAVAILABLE_SENTINEL) =>
        {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "storage accounting unavailable",
            )
                .into_response()
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
    include!("tests-00-00.rs");
    include!("tests-00-01.rs");
}
