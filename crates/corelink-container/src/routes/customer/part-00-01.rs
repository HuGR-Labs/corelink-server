
// ─── Query param shapes ───────────────────────────────────────────────────────

/// `GET /v1/customer/usage` query params.
#[non_exhaustive]
#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    /// Optional billing period filter (e.g. `"2026-05"`).
    pub period: Option<String>,
}

/// `GET /v1/customer/audit` query params.
#[non_exhaustive]
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    /// Optional ISO-8601 since filter (maps to `AuditQueryRequest::since`).
    pub from: Option<String>,
    /// Ignored in this surface (InMemory has no `to` filter); preserved
    /// for forward-compatibility with the admin-ui contract.
    pub to: Option<String>,
    /// Optional event-type filter (comma-separated string).
    pub kind: Option<String>,
}

// ─── POST body shapes ─────────────────────────────────────────────────────────

/// `POST /v1/customer/keys` request body.
#[non_exhaustive]
#[derive(Debug, Deserialize)]
pub struct CreatePatBody {
    /// Human-readable name for the new PAT.
    pub name: String,
    /// Scopes to grant (e.g. `["cache:read", "cache:write"]`).
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// `POST /v1/pats` request body.
///
/// The public contract calls the human-readable key name a `label`; the
/// dashboard's older `/v1/customer/keys` form calls the same field `name`.
/// Both feed the same mint implementation so their tenant, principal, scope,
/// audit, and shown-once-token guarantees cannot drift.
#[non_exhaustive]
#[derive(Debug, Deserialize)]
pub struct PatIssueBody {
    /// Human-readable label for the new PAT.
    pub label: String,
    /// Scopes to grant (e.g. `["cache:read", "cache:write"]`).
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// `POST /v1/customer/team/invite` request body.
#[non_exhaustive]
#[derive(Debug, Deserialize)]
pub struct InviteBody {
    /// Email address to invite.
    pub email: String,
    /// Role to assign (`"admin"` / `"member"` / `"viewer"`).
    pub role: String,
}

// ─── Handler fns ─────────────────────────────────────────────────────────────

/// `GET /v1/customer/overview`
async fn handle_overview(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession backstop (cluster A) — reject a forged-HMAC / wrong-
    // tenant PAT BEFORE any storage access. Skipped for Clerk callers + dev/CI.
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Billing/PII gate (F-018): the overview carries billing status + amount-due
    // + financial PII — an owner/admin surface, not cache access. A read-only
    // (`cas:r`) cache token must not read it.
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    let req = OverviewRequest::new(t, p, now_ms());
    match state.overview.overview(req) {
        Ok(resp) => {
            let body: Value = json!({
                "tenant_id":       resp.tenant_id,
                "tenant_name":     resp.tenant_name,
                "plan":            resp.plan,
                "usage": {
                    "period":      resp.usage.period,
                    "cas_bytes":   resp.usage.cas_bytes,
                    "quota_bytes": resp.usage.quota_bytes,
                    "reads":       resp.usage.reads,
                    "writes":      resp.usage.writes,
                },
                "billing": {
                    "status":             resp.billing.status,
                    "next_invoice_at":    resp.billing.next_invoice_at,
                    "amount_due_cents":   resp.billing.amount_due_cents,
                    "currency":           resp.billing.currency,
                },
                "byok": {
                    "status":         resp.byok.status,
                    "cmk_id":         resp.byok.cmk_id,
                    "last_rotated_at": resp.byok.last_rotated_at,
                },
                "recent_activity": resp.recent_activity.iter().map(|e| json!({
                    "event_id":   e.event_id,
                    "ts":         e.ts,
                    "event_type": e.event_type,
                    "severity":   e.severity,
                    "actor":      e.actor,
                    "summary":    e.summary,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/usage?period=`
async fn handle_usage(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Query(q): Query<UsageQuery>,
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
    let req = UsageRequest::new(t, p, q.period, now_ms());
    match state.usage.usage(req) {
        Ok(resp) => {
            let body: Value = json!({
                "period":        resp.period,
                "cas_bytes":     resp.cas_bytes,
                "reads":         resp.reads,
                "writes":        resp.writes,
                "request_count": resp.request_count,
                "quota_bytes":   resp.quota_bytes,
                // `hit_rate` serializes as JSON `null` when None (no cache reads).
                "hit_rate":            resp.hit_rate,
                "time_saved_seconds":  resp.time_saved_seconds,
                "dollars_saved_cents": resp.dollars_saved_cents,
                "daily": resp.daily.iter().map(|d| json!({
                    "day":       d.day,
                    "reads":     d.reads,
                    "writes":    d.writes,
                    "cas_bytes": d.cas_bytes,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/audit?from=&to=&kind=`
async fn handle_audit(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
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
    // Billing/PII gate (F-018): the audit log surfaces security/account PII
    // events — an owner/admin surface, not cache access. A read-only (`cas:r`)
    // cache token must not read it.
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    // `kind` is comma-separated; split into event_types Vec.
    let event_types: Vec<String> = q
        .kind
        .as_deref()
        .map(|k| {
            k.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    // `from` maps to `since` in the handler surface; `to` has no
    // analogue in AuditQueryRequest (InMemory doesn't filter by end-date).
    let req = AuditQueryRequest::new(t, p, q.from, event_types, now_ms());
    match state.audit.query(req) {
        Ok(resp) => {
            let body: Value = json!({
                "rows": resp.rows.iter().map(|r| json!({
                    "event_id":   r.event_id,
                    "ts":         r.ts,
                    "event_type": r.event_type,
                    "severity":   r.severity,
                    "actor":      r.actor,
                    "summary":    r.summary,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/billing`
async fn handle_billing(
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
    // Billing/PII gate (F-018): billing detail (status / plan / invoices /
    // amounts) is financial PII — an owner/admin surface, not cache access. A
    // read-only (`cas:r`) cache token must not read it.
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    let req = BillingRequest::new(t, p, now_ms());
    match state.billing.billing(req) {
        Ok(resp) => {
            let body: Value = json!({
                "status":                resp.status,
                "plan":                  resp.plan,
                "current_period_start":  resp.current_period_start,
                "current_period_end":    resp.current_period_end,
                "amount_due_cents":      resp.amount_due_cents,
                "currency":              resp.currency,
                "invoices": resp.invoices.iter().map(|i| json!({
                    "invoice_id":  i.invoice_id,
                    "issued_at":   i.issued_at,
                    "amount_cents": i.amount_cents,
                    "status":      i.status,
                    "hosted_url":  i.hosted_url,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/billing/portal`
async fn handle_billing_portal(
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
    // Billing-management gate (F-018): minting the Stripe billing-portal session
    // lets the caller manage/remove payment methods + CANCEL the subscription —
    // an owner/admin op, not cache access. A read-only (`cas:r`) cache token must
    // not open it (cluster A + CK-4 cross-tenant via F-006).
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    let req = PortalRequest::new(t, p, now_ms());
    match state.billing.portal_url(req) {
        Ok(resp) => {
            let body: Value = json!({ "portal_url": resp.portal_url });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}
