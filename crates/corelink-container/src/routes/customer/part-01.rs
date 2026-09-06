/// `GET /v1/customer/keys`
async fn handle_keys_list(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Privilege gate (rt-nuclear cycle-2 #7): key MANAGEMENT (enumerating the
    // tenant's PATs / BYOK status) is an admin op, not cache access. A read-only
    // (`cas:r`) cache token must not enumerate other principals' credentials —
    // info-disclosure + the recon step of the revoke attack. Mirror the
    // mint/revoke gate. Dashboard (`read-write`) + `cas:rw` pass; `cas:r` → 403.
    let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
    if !crate::scope::requires_cache_write(&caller_scope) {
        return (
            StatusCode::FORBIDDEN,
            "insufficient scope to list credentials",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = KeysListRequest::new(t, p, now_ms());
    match state.keys.list(req) {
        Ok(resp) => {
            let body: Value = json!({
                "pats": resp.pats.iter().map(|row| json!({
                    "pat_id":      row.pat_id,
                    "name":        row.name,
                    "scopes":      row.scopes,
                    "created_at":  row.created_at,
                    "last_used_at": row.last_used_at,
                    "revoked_at":  row.revoked_at,
                })).collect::<Vec<_>>(),
                "byok": {
                    "status":         resp.byok.status,
                    "cmk_id":         resp.byok.cmk_id,
                    "last_rotated_at": resp.byok.last_rotated_at,
                },
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/keys`
async fn handle_keys_create(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<CreatePatBody>,
) -> axum::response::Response {
    create_pat_response(state, headers, body.name, body.scopes).await
}

/// `POST /v1/pats` — customer-authorized PAT issuance.
///
/// This deliberately shares the dashboard implementation instead of calling
/// `/_internal/pat/mint`: only the Worker-authenticated customer principal may
/// select the tenant, and the operator mint credential never reaches this path.
async fn handle_pats_create(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<PatIssueBody>,
) -> axum::response::Response {
    create_pat_response(state, headers, body.label, body.scopes).await
}

/// Common customer-authorized PAT mint flow for the dashboard and public API.
async fn create_pat_response(
    state: CustomerRouteState,
    headers: HeaderMap,
    name: String,
    scopes: Vec<String>,
) -> axum::response::Response {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Privilege-escalation gate (cluster A): a read-only principal must NOT be
    // able to MINT a write/admin credential (which would let a `cas:r` PAT
    // bootstrap a `cas:rw` PAT for itself — a self-escalation that survives key
    // rotation). The caller's capability comes from the Worker-trusted
    // `x-corelink-scope` header (the same `cache:write` capability the native
    // CAS/AC write handlers enforce via `CacheScope::can_write`). If the
    // requested scopes ask for any write/admin capability AND the caller lacks
    // cache-write, reject 403 BEFORE the mint. Clerk-session callers carry the
    // dashboard `read-write` scope (Worker-set), so they are unaffected.
    if mint_requests_write(&scopes) {
        let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
        if !crate::scope::requires_cache_write(&caller_scope) {
            return (
                StatusCode::FORBIDDEN,
                "insufficient scope to mint a write/admin credential",
            )
                .into_response();
        }
    }
    // Production authority is the tenant Durable Object. It stamps one
    // request-scoped lease only after its serialized durable decision. A
    // directly reached/recycled container must fail closed rather than fall
    // back to an in-process limiter that can reset or diverge between hosts.
    if headers
        .get(PAT_ISSUE_AUTHORIZED_HEADER)
        .and_then(|value| value.to_str().ok())
        != Some("1")
    {
        #[cfg(test)]
        {
            let bucket_tenant = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, t.as_bytes());
            let bucket_key = corelink_ratelimit::BucketKey::per_tenant_per_endpoint(
                bucket_tenant,
                PAT_ISSUE_ENDPOINT_ID,
            );
            let limiter_now_ms = crate::wall_clock::default_wall_clock().now_ms();
            if limiter_now_ms == 0 {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "rate-limit clock unavailable",
                )
                    .into_response();
            }
            match state.pat_issue_rate_limiter.try_acquire(
                bucket_tenant,
                bucket_key,
                1,
                limiter_now_ms,
            ) {
                Ok(outcome) => match outcome.decision {
                    RateLimitDecision::Allow { .. } => {}
                    RateLimitDecision::Deny429 {
                        retry_after_secs, ..
                    } => {
                        let mut response = (
                            StatusCode::TOO_MANY_REQUESTS,
                            "PAT issuance rate limit exceeded",
                        )
                            .into_response();
                        if let Ok(value) = retry_after_secs.to_string().parse() {
                            response
                                .headers_mut()
                                .insert(axum::http::header::RETRY_AFTER, value);
                        }
                        return response;
                    }
                    _ => {
                        return (
                            StatusCode::TOO_MANY_REQUESTS,
                            "PAT issuance rate limit exceeded",
                        )
                            .into_response();
                    }
                },
                Err(_) => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "rate-limit pipeline failed",
                    )
                        .into_response();
                }
            }
        }
        #[cfg(not(test))]
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "PAT issuance rate-limit lease unavailable",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = KeyCreateRequest::new(t, p, name, scopes, now_ms());
    match state.keys.create(req) {
        Ok(resp) => {
            let json_body: Value = json!({
                "pat": {
                    "pat_id":      resp.pat.pat_id,
                    "name":        resp.pat.name,
                    "scopes":      resp.pat.scopes,
                    "created_at":  resp.pat.created_at,
                    "last_used_at": resp.pat.last_used_at,
                    "revoked_at":  resp.pat.revoked_at,
                },
                "token": resp.token,
            });
            (StatusCode::CREATED, Json(json_body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/keys/:pat_id/revoke`
async fn handle_keys_revoke(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Path(pat_id): Path<String>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Role gate (B-144): revoking a credential is a destructive team-admin
    // operation, not a cache capability. A plain member's `read-write` scope
    // must not grant intra-tenant credential-DoS / owner-lockout authority.
    // The Worker is the sole setter of this role header; missing/unknown roles
    // fail CLOSED. Owners may revoke any PAT in their tenant. Admins may revoke
    // non-owner PATs; the D1 handler enforces the owner-target safeguard.
    if !caller_is_owner_or_admin(&headers) {
        return (
            StatusCode::FORBIDDEN,
            "only the owner or an admin may revoke a credential",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = KeyRevokeRequest::with_role(t, p, caller_role(&headers), pat_id, now_ms());
    match state.keys.revoke(req) {
        Ok(resp) => {
            let body: Value = json!({
                "pat": {
                    "pat_id":      resp.pat.pat_id,
                    "name":        resp.pat.name,
                    "scopes":      resp.pat.scopes,
                    "created_at":  resp.pat.created_at,
                    "last_used_at": resp.pat.last_used_at,
                    "revoked_at":  resp.pat.revoked_at,
                },
            });
            let mut headers = HeaderMap::new();
            if let Some(token_id) = resp.cache_token_id {
                if token_id.is_empty() {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "revocation cache handle missing",
                    )
                        .into_response();
                }
                let Ok(header) = HeaderValue::from_str(&token_id) else {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "revocation cache handle invalid",
                    )
                        .into_response();
                };
                headers.insert("x-corelink-pat-cache-invalidate", header);
            }
            (StatusCode::OK, headers, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/team`
async fn handle_team_list(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    let p = principal(&headers);
    let req = TeamListRequest::new(t, p, now_ms());
    match state.team.list(req) {
        Ok(resp) => {
            let body: Value = json!({
                "members": resp.members.iter().map(|m| json!({
                    "user_id":   m.user_id,
                    "email":     m.email,
                    "role":      m.role,
                    "joined_at": m.joined_at,
                    "status":    m.status,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/team/invite`
async fn handle_team_invite(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<InviteBody>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Team-management RBAC (owner/admin only). The coarse `x-corelink-scope`
    // granted EVERY non-viewer `read-write billing`, so the prior cache-write gate
    // let a plain `member` invite seats — including a privileged `admin`, and (via
    // the member→owner escalation the account-delete audit flagged) an `owner`
    // seat. Gate on the D1-resolved role instead (migration 0074):
    //   1. `owner` is NEVER self-serve-invitable — there is exactly one owner (the
    //      tenant creator). Reject outright so the escalation chain (invite-owner →
    //      accept → resolve owner → delete tenant) is closed at the source.
    //      (`normalize_invite_role` also maps owner→admin as defense-in-depth.)
    //   2. inviting a privileged `admin` requires the caller be the OWNER.
    //   3. any invite at all requires the caller be owner OR admin (a plain
    //      member/viewer cannot add seats).
    if body.role.trim().eq_ignore_ascii_case("owner") {
        return (
            StatusCode::FORBIDDEN,
            "the owner role is not grantable via a team invite",
        )
            .into_response();
    }
    if !caller_is_owner_or_admin(&headers) {
        return (
            StatusCode::FORBIDDEN,
            "only the owner or an admin may invite team members",
        )
            .into_response();
    }
    if role_is_privileged(&body.role) && caller_role(&headers) != "owner" {
        return (
            StatusCode::FORBIDDEN,
            "only the owner may invite a privileged (admin) role",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = TeamInviteRequest::new(t, p, body.email, body.role, now_ms());
    match state.team.invite(req) {
        Ok(resp) => {
            let json_body: Value = json!({
                "member": {
                    "user_id":   resp.member.user_id,
                    "email":     resp.member.email,
                    "role":      resp.member.role,
                    "joined_at": resp.member.joined_at,
                    "status":    resp.member.status,
                },
                "invitation_token": resp.invitation_token,
            });
            (StatusCode::CREATED, Json(json_body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `DELETE /v1/customer/team/:user_id` — remove a member's seat. Owner/admin
/// only (cache-write scope); flips the seat to `removed` AND revokes the
/// member's PATs (the load-bearing security effect). Returns 200 with the
/// removed member + the revoked-PAT count.
async fn handle_team_remove(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401 first.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Team-management RBAC (owner/admin only). Removing a seat is a destructive
    // admin op that revokes another principal's credentials — a plain `member`
    // must NOT be able to remove teammates / revoke their PATs (intra-tenant
    // lockout / credential-DoS). The coarse cache-write gate granted every
    // non-viewer this; gate on the D1-resolved role instead (the owner row itself
    // is separately protected from removal in `customer_d1`).
    if !caller_is_owner_or_admin(&headers) {
        return (
            StatusCode::FORBIDDEN,
            "only the owner or an admin may remove a team member",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = TeamRemoveRequest::new(t, p, user_id, now_ms());
    match state.team.remove(req) {
        Ok(resp) => {
            let body: Value = json!({
                "member": {
                    "user_id":   resp.member.user_id,
                    "email":     resp.member.email,
                    "role":      resp.member.role,
                    "joined_at": resp.member.joined_at,
                    "status":    resp.member.status,
                },
                "revoked_pats": resp.revoked_pats,
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/account/delete` — self-serve GDPR account erasure (C-ACCTDEL).
///
/// A customer erases their OWN account: this is a **Clerk-session-only** surface
/// (`x-corelink-token-prefix: clerk`, set by the Worker after edge-verifying the
/// session and stripping the bearer). A cache PAT (`cas:r` / `cas:rw`) is a
/// data-plane credential and MUST NOT trigger account erasure — a PAT caller gets
/// 403 (mirrors the e2e contract that the erasure-request surface is session-auth'd).
///
/// On accept it mirrors the Clerk `user.deleted` path: build the canonical
/// `dsr.queued.v1` message + `INSERT OR IGNORE` a `dsr_requested` anchor row, then
/// enqueue — returning **202 Accepted**. Idempotent (the deterministic `dsr_id` +
/// `INSERT OR IGNORE` make a repeat request a no-op). Fail-CLOSED: when the
/// requester is unwired (dev/CI) → 503; on a D1/transport fault → 500 (never a
/// silent 202 we cannot honor).
async fn handle_account_delete(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401 first.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Clerk-session ONLY. A customer deletes their OWN account via the dashboard;
    // a cache PAT must not erase the account. (The Worker stamps the `clerk`
    // token-prefix for an edge-verified session and forwards NO bearer.)
    if principal(&headers) != CLERK_TOKEN_PREFIX {
        return (
            StatusCode::FORBIDDEN,
            "account deletion requires a dashboard (Clerk) session",
        )
            .into_response();
    }
    // OWNER-only (team RBAC, migration 0074). This erases the WHOLE tenant
    // (`request_erasure(&t)` below), so a non-owner seat — `admin`/`member`/
    // `viewer`, which all resolve to the OWNING tenant and all carry a Clerk
    // session — must NOT be able to nuke every teammate's account. The Worker
    // forwards the D1-resolved role as the server-trusted `x-corelink-role`
    // (client copies stripped); fail-CLOSED on anything but `owner`.
    if header_or(&headers, ROLE_HEADER, "") != "owner" {
        return (
            StatusCode::FORBIDDEN,
            "account deletion requires the tenant OWNER",
        )
            .into_response();
    }
    let Some(requester) = state.account_deletion.as_ref() else {
        // Fail-CLOSED: the erasure path is not wired (dev/CI / unconfigured).
        // NEVER ack a GDPR erasure we cannot durably honor.
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "account deletion not configured",
        )
            .into_response();
    };
    match requester.request_erasure(&t) {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(json!({ "ok": true, "status": "erasure_requested" })),
        )
            .into_response(),
        // No provisioned account to erase (already deleted / never provisioned).
        // Idempotent ACK (202) — mirrors the webhook's "no tenant → ack" no-op so
        // the client UX is uniform and a double-submit is safe.
        Err(AccountDeletionError::NotFound) => (
            StatusCode::ACCEPTED,
            Json(json!({ "ok": true, "status": "no_account" })),
        )
            .into_response(),
        Err(AccountDeletionError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "account delete: erasure request failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "erasure request failed").into_response()
        }
    }
}

/// `POST /v1/customer/account/export` — self-serve tenant bulk export (SEAM).
///
/// Streams the full portability bundle (content-addressed NDJSON): the tenant's
/// CAS + AC blob BYTES + the D1 governance records (RBAC/team, DPA/consent, audit
/// slice). Unblocks the CLI `corelink tenant export` (PR #708) and serves GDPR
/// Art.20 data portability for the whole tenant.
///
/// Auth: owner/admin only. Accepts EITHER a dashboard Clerk session OR a
/// write-capable (`cas:rw`) PAT — the bundle exposes the tenant's team PII, DPA
/// records, audit log AND every blob, so a read-only (`cas:r`) cache token is NOT
/// sufficient (gated on `requires_cache_write`, exactly like the billing/keys/team
/// surfaces). The native PAT-possession backstop runs first (a leaked
/// `PAT_SIGNING_KEY` cannot forge a bearer for a victim tenant). Rate-limited
/// per-tenant. Audited: a durable `account.export` row is written BEFORE any bytes
/// are disclosed. Fail-CLOSED: unwired source → 503; a gather/audit fault → 5xx —
/// never a partial 200.
async fn handle_account_export(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> axum::response::Response {
    use crate::routes::customer_export::TenantExportError;

    // 1. Fail-CLOSED tenant resolution (401 on missing/sentinel).
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // 2. Native PAT possession backstop (forged / wrong-tenant PAT → 401/503).
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // 3. Owner/admin + PII gate: the bundle carries team PII / DPA / audit / all
    //    blobs — a read-only cache token must not export it (F-018 sibling).
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    // 4. Require the export source (fail-CLOSED 503 when unwired — dev/CI).
    let Some(source) = state.export.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "tenant export not configured",
        )
            .into_response();
    };
    // 5. Per-tenant rate limit (heavy full-tenant read). The tenant may be a
    //    non-UUID fixture id, so bucket on a deterministic v5 UUID derived from it.
    let export_now_ms = crate::wall_clock::default_wall_clock().now_ms();
    let bucket_tenant = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, t.as_bytes());
    let bucket_key = corelink_ratelimit::BucketKey::per_tenant_per_endpoint(
        bucket_tenant,
        crate::routes::customer_export::EXPORT_ENDPOINT_ID,
    );
    match state
        .export_rate_limiter
        .try_acquire(bucket_tenant, bucket_key, 1, export_now_ms)
    {
        Ok(outcome) => match outcome.decision {
            corelink_ratelimit::RateLimitDecision::Allow { .. } => {}
            corelink_ratelimit::RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let body = format!("rate-limited; retry after {retry_after_secs}s");
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                if let Ok(val) = format!("{retry_after_secs}").parse() {
                    resp.headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, val);
                }
                return resp;
            }
            // The decision enum is `#[non_exhaustive]`; any future non-Allow arm
            // is treated as a deny so the route never streams under an unknown
            // decision shape.
            _ => return (StatusCode::TOO_MANY_REQUESTS, "rate-limited").into_response(),
        },
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "rate-limit pipeline failed",
            )
                .into_response();
        }
    }
    // 6. Gather governance + the blob index UP FRONT (fail-CLOSED — never a
    //    partial-looking 200). Small: D1 rows + a (digest,size) index.
    let metadata = match source.metadata_records(&t) {
        Ok(m) => m,
        Err(TenantExportError::Unavailable(e)) => {
            tracing::warn!(error = %e, tenant = %t, "tenant export: source unavailable");
            return (StatusCode::SERVICE_UNAVAILABLE, "export source unavailable").into_response();
        }
        Err(TenantExportError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "tenant export: metadata gather failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "export gather failed").into_response();
        }
    };
    let blobs = match source.blob_index(&t) {
        Ok(b) => b,
        Err(TenantExportError::Unavailable(e)) => {
            tracing::warn!(error = %e, tenant = %t, "tenant export: blob index unavailable");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "export storage unavailable",
            )
                .into_response();
        }
        Err(TenantExportError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "tenant export: blob index failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "export enumerate failed").into_response();
        }
    };
    // 7. Audit BEFORE disclosure (durable `account.export` row). A fault here
    //    aborts fail-CLOSED so a disclosure is never unlogged.
    if let Err(e) = source.record_export_audit(&t, blobs.len(), metadata.len()) {
        tracing::error!(error = %e, tenant = %t, "tenant export: audit write failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "export audit failed").into_response();
    }
    tracing::info!(
        event = "customer.account.export",
        tenant = %t,
        blob_count = blobs.len(),
        metadata_count = metadata.len(),
        "tenant export streaming"
    );
    // 8. Stream the content-addressed NDJSON bundle. Blob bytes are fetched
    //    lazily one at a time (memory-bounded).
    let body_stream = crate::routes::customer_export::build_export_stream(
        source,
        t,
        export_now_ms,
        metadata,
        blobs,
    );
    let body = axum::body::Body::new(http_body_util::StreamBody::new(body_stream));
    let mut resp = (StatusCode::OK, body).into_response();
    if let Ok(val) = axum::http::HeaderValue::from_str("application/x-ndjson") {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, val);
    }
    if let (Ok(name), Ok(val)) = (
        axum::http::HeaderName::from_bytes(b"x-corelink-export-schema"),
        axum::http::HeaderValue::from_str(crate::routes::customer_export::EXPORT_SCHEMA),
    ) {
        resp.headers_mut().insert(name, val);
    }
    resp
}

// ─── Account-deletion (DSR erasure) collaborators (C-ACCTDEL) ──────────────────

/// Failure modes of a self-serve account-deletion request.
#[derive(Debug)]
pub enum AccountDeletionError {
    /// No account/tenant row exists to erase (already deleted / never
    /// provisioned) — the route maps this to an idempotent 202 no-op.
    NotFound,
    /// A D1 / transport / configuration fault — the route fails CLOSED (500) so
    /// the obligation is retried, never silently dropped.
    Internal(String),
}

/// Self-serve account-erasure requester (route collaborator). The production
/// impl is [`D1AccountDeletionRequester`]; tests supply a mock. Wired into
/// [`CustomerRouteState::account_deletion`] by `routes.rs` (the erasure sink is a
/// cross-module collaborator built alongside the DSR worker), exactly as
/// `pat_gate` is wired there.
pub trait AccountDeletionRequester: Send + Sync + core::fmt::Debug {
    /// Durably anchor + enqueue a GDPR erasure for `tenant_id`. Idempotent
    /// (deterministic `dsr_id` + `INSERT OR IGNORE`). Uses a real wall clock for
    /// the SLA-anchor `queued_at_ms` (the route's logical clock is 0).
    ///
    /// # Errors
    ///
    /// [`AccountDeletionError::NotFound`] when no tenant row exists;
    /// [`AccountDeletionError::Internal`] on any D1 / transport / config fault.
    fn request_erasure(&self, tenant_id: &str) -> Result<(), AccountDeletionError>;
}

/// Transport seam for the built `dsr.queued.v1` message. The genuinely
/// cross-service piece (the container has no CF Queue producer binding): the
/// production sink is wired in `routes.rs` over the in-process DSR erasure worker
/// (or a queue producer). Kept behind a trait so [`D1AccountDeletionRequester`]
/// owns the message construction + the `dsr_requested` anchor (the C-ACCTDEL
/// D1-observable effects) hermetically, with the transport injected.
pub trait DsrErasureSink: Send + Sync + core::fmt::Debug {
    /// Enqueue an already-anchored `dsr.queued.v1` erasure message.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any transport failure (the requester maps it to
    /// [`AccountDeletionError::Internal`] → 500 fail-CLOSED).
    fn enqueue(&self, message: &Value) -> Result<(), String>;
}

/// `dev.hugr.corelink.dsr.queued.v1` schema id (FROZEN — mirrors
/// `apps/signup-worker/src/webhooks/clerk.ts buildErasureQueueMessage`).
const DSR_QUEUED_SCHEMA: &str = "dev.hugr.corelink.dsr.queued.v1";
