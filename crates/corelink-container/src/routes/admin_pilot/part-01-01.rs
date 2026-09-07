// -----------------------------------------------------------------------------
// Query + body shapes
// -----------------------------------------------------------------------------

/// Query string for `GET /v1/admin/pilots`.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct ListPilotsQuery {
    /// Required state filter (uppercase canonical form).
    pub state: String,
    /// Optional 0-based offset (default 0).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Optional page size (default `DEFAULT_PAGE_SIZE`, max
    /// `MAX_PAGE_SIZE`).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Response body for `GET /v1/admin/pilots`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ListPilotsResponse {
    /// Echoed state filter.
    pub state: PilotState,
    /// Returned rows.
    pub rows: Vec<PilotTenant>,
    /// 0-based offset applied.
    pub offset: usize,
    /// Page size applied.
    pub limit: usize,
}

/// Request body for `POST /v1/admin/pilots` (create a pilot tenant).
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct CreatePilotBody {
    /// Display slug (lowercase, hyphen-delimited). Required, non-empty,
    /// `≤ MAX_SLUG_LEN` chars — validated at the route boundary (L4).
    pub slug: String,
    /// Optional initial storage cap in bytes (decimal-GB convention).
    /// Pre-grant tenants default to 0; the operator stamps the real cap
    /// via `grant-tier`.
    #[serde(default)]
    pub cap_bytes: Option<u64>,
}

/// Response body for `POST /v1/admin/pilots`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreatePilotResponse {
    /// The newly-created pilot tenant (state `NEW`, tier `free`).
    pub tenant: PilotTenant,
}

/// Request body for `POST /v1/admin/pilots/{tenant_id}/grant-tier`.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct GrantTierBody {
    /// Tier to grant — MUST be `"pilot"` (parity with the wave-27
    /// `grant-pilot-tier.sh` script).
    pub tier: String,
    /// Storage cap in bytes (decimal-GB convention).
    pub cap_bytes: u64,
}

/// Response body for `POST /v1/admin/pilots/{tenant_id}/grant-tier`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GrantTierResponse {
    /// Post-mutation tenant record.
    pub tenant: PilotTenant,
}

/// Response body for `POST /v1/admin/pilots/{tenant_id}/checkin`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CheckinResponse {
    /// Tenant id checked.
    pub tenant_id: Uuid,
    /// `true` when the tenant is in the `no-blob silent-fail`
    /// window (24h post-grant with `first_blob_at_ms = None`).
    pub alert_emitted: bool,
    /// Time-since-grant in ms; `None` pre-grant.
    pub age_ms: Option<u64>,
}

// -----------------------------------------------------------------------------
// Router builder
// -----------------------------------------------------------------------------

/// Build the axum sub-router for the pilot-admin surface.
///
/// Each handler is bound twice: at its canonical `/v1/admin/pilots…`
/// path AND at the `/_internal/admin/pilots…` alias (#218 §2.1,
/// ratified Q3) — the alias rides the Worker's existing `/_internal/*`
/// forwarding channel (constant-time internal-auth verification +
/// client-trust-header strip + `_system`-DO forward, path preserved),
/// making the surface operator-reachable from the public edge with
/// zero Worker changes. Both paths run the IDENTICAL gate
/// ([`require_admin_scope`]) — the alias is reachability, not a
/// privilege change.
pub fn router(state: PilotAdminRouteState) -> Router {
    Router::new()
        .route(PILOTS_LIST_ROUTE, get(handle_list).post(handle_create))
        .route(PILOTS_GRANT_TIER_ROUTE, post(handle_grant_tier))
        .route(PILOTS_CHECKIN_ROUTE, post(handle_checkin))
        .route(
            INTERNAL_PILOTS_LIST_ROUTE,
            get(handle_list).post(handle_create),
        )
        .route(INTERNAL_PILOTS_GRANT_TIER_ROUTE, post(handle_grant_tier))
        .route(INTERNAL_PILOTS_CHECKIN_ROUTE, post(handle_checkin))
        .with_state(state)
}

/// Build the canonical
/// `(Arc<dyn PilotStore>, Arc<dyn PilotAuditSink>)` pair backed by
/// in-memory fakes — used by dev/CI + the default `build()` path in
/// `routes.rs`.
#[must_use]
pub fn build_handlers() -> (Arc<dyn PilotStore>, Arc<dyn PilotAuditSink>) {
    let store: Arc<dyn PilotStore> = Arc::new(InMemoryPilotStore::new());
    let audit: Arc<dyn PilotAuditSink> = Arc::new(InMemoryPilotAuditSink::new());
    (store, audit)
}

// -----------------------------------------------------------------------------
// Handlers
// -----------------------------------------------------------------------------

async fn handle_list(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    Query(query): Query<ListPilotsQuery>,
) -> Response {
    let scope = match require_admin_scope(&state, &headers, None) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let parsed_state = match PilotState::parse(query.state.as_str()) {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "invalid state filter").into_response(),
    };
    let offset = query.offset.unwrap_or(0);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let rows = match state.store.list_by_state(parsed_state, offset, limit) {
        Ok(r) => r,
        Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "store unavailable").into_response(),
    };
    let now_ms = state.wall_clock.now_ms();
    let payload = serde_json::json!({
        "state": parsed_state.as_str(),
        "rows_returned": rows.len(),
        "offset": offset,
        "limit": limit,
    });
    let row = PilotAuditRow {
        event_type: "corelink.admin.pilot_list.v1".to_string(),
        principal: scope.principal,
        tenant_id: None,
        at_unix_ms: now_ms,
        exit_status: "ok".to_string(),
        payload,
    };
    if state.audit_sink.emit(row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    let body = ListPilotsResponse {
        state: parsed_state,
        rows,
        offset,
        limit,
    };
    (StatusCode::OK, Json(body)).into_response()
}

async fn handle_create(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    // M3 (F16 pattern): body taken as raw `Bytes` so the `HeaderMap`
    // `FromRequestParts` extractor resolves BEFORE the body is buffered.
    // The admin gate runs FIRST; an unauthenticated caller is rejected
    // without the container ever JSON-parsing an arbitrarily-large body.
    // JSON deserialisation runs only AFTER the gate passes — mirrors
    // `admin::handle_mutate`.
    body: axum::body::Bytes,
) -> Response {
    // No target tenant — create mints a NEW id, so the gate runs at the
    // global-operator scope (mirrors `handle_list`).
    let scope = match require_admin_scope(&state, &headers, None) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    // Body parsed ONLY after the auth gate passes (M3).
    let body: CreatePilotBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid_body").into_response(),
    };
    // L4 input validation.
    let slug = body.slug.trim();
    if slug.is_empty() {
        return (StatusCode::BAD_REQUEST, "slug must be non-empty").into_response();
    }
    if slug.chars().count() > MAX_SLUG_LEN {
        return (StatusCode::BAD_REQUEST, "slug too long").into_response();
    }
    // L4 charset validation (F-03): enforce the documented "lowercase,
    // hyphen-delimited" slug contract — ASCII alphanumeric / `-` / `_` only.
    // Blocks null/control chars and other non-printables from reaching D1 /
    // audit logs (log-injection / row-shape hardening). SQLi is already
    // impossible here (all SQL is parameterised), so this is defence-in-depth.
    if !slug
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return (
            StatusCode::BAD_REQUEST,
            "slug must be lowercase alphanumeric/hyphen/underscore",
        )
            .into_response();
    }
    let cap_bytes = body.cap_bytes.unwrap_or(0);
    let tenant_id = Uuid::now_v7();
    let now_ms = state.wall_clock.now_ms();

    // Fail-CLOSED: emit audit BEFORE the mutation. If audit fails, abort.
    let pre_row = PilotAuditRow {
        event_type: EVENT_TYPE_CREATED.to_string(),
        principal: scope.principal.clone(),
        tenant_id: Some(tenant_id),
        at_unix_ms: now_ms,
        exit_status: "attempt".to_string(),
        payload: json!({
            "slug": slug,
            "cap_bytes": cap_bytes,
        }),
    };
    if state.audit_sink.emit(pre_row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    let tenant = match state.store.create(tenant_id, slug, cap_bytes, now_ms) {
        Ok(t) => t,
        Err(msg) => {
            let status = if msg == "tenant already exists" {
                StatusCode::CONFLICT
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            return (status, msg).into_response();
        }
    };

    let post_row = PilotAuditRow {
        event_type: EVENT_TYPE_CREATED.to_string(),
        principal: scope.principal,
        tenant_id: Some(tenant_id),
        at_unix_ms: now_ms,
        exit_status: "ok".to_string(),
        payload: json!({
            "slug": tenant.slug,
            "cap_bytes": tenant.cap_bytes,
            "pilot_state": tenant.pilot_state.as_str(),
        }),
    };
    if state.audit_sink.emit(post_row).is_err() {
        // The row already landed in the store, but the post-emit failed:
        // surface 503 so the operator retries. The pre-emit `attempt`
        // row is the SEC team's record that the create was issued.
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    (StatusCode::CREATED, Json(CreatePilotResponse { tenant })).into_response()
}

async fn handle_grant_tier(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    // M3 (F16 pattern): raw `Bytes` body. `HeaderMap` + `Path` are
    // `FromRequestParts` extractors and resolve before the body buffer,
    // so the admin gate runs BEFORE any JSON parse of an attacker-supplied
    // body. JSON deserialisation happens only after the gate passes.
    body: axum::body::Bytes,
) -> Response {
    let tenant_uuid = match parse_tenant_uuid(&tenant_id) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let scope = match require_admin_scope(&state, &headers, Some(tenant_uuid)) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    // Body parsed ONLY after the auth gate passes (M3).
    let body: GrantTierBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid_body").into_response(),
    };
    if body.tier != "pilot" {
        return (StatusCode::BAD_REQUEST, "tier must be 'pilot'").into_response();
    }
    if body.cap_bytes == 0 {
        return (StatusCode::BAD_REQUEST, "cap_bytes must be > 0").into_response();
    }
    let now_ms = state.wall_clock.now_ms();

    // Fail-CLOSED: emit audit BEFORE mutation. If audit fails, abort.
    let pre_row = PilotAuditRow {
        event_type: EVENT_TYPE_TIER_GRANTED.to_string(),
        principal: scope.principal.clone(),
        tenant_id: Some(tenant_uuid),
        at_unix_ms: now_ms,
        exit_status: "attempt".to_string(),
        payload: serde_json::json!({
            "tier": body.tier,
            "cap_bytes": body.cap_bytes,
        }),
    };
    if state.audit_sink.emit(pre_row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    let tenant = match state
        .store
        .apply_grant_tier(tenant_uuid, &body.tier, body.cap_bytes, now_ms)
    {
        Ok(t) => t,
        Err(msg) => {
            let status = if msg == "tenant not found" {
                StatusCode::NOT_FOUND
            } else if msg == "tenant not in grant-eligible state" {
                StatusCode::CONFLICT
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            return (status, msg).into_response();
        }
    };

    let post_row = PilotAuditRow {
        event_type: EVENT_TYPE_TIER_GRANTED.to_string(),
        principal: scope.principal,
        tenant_id: Some(tenant_uuid),
        at_unix_ms: now_ms,
        exit_status: "ok".to_string(),
        payload: serde_json::json!({
            "tier": tenant.tier,
            "cap_bytes": tenant.cap_bytes,
            "pilot_state": tenant.pilot_state.as_str(),
        }),
    };
    if state.audit_sink.emit(post_row).is_err() {
        // The mutation already landed in the store, but the
        // post-emit failed: surface 503 so the operator retries.
        // The post-emit failure is the SEC team's hard signal that
        // the audit pipeline is degraded; the pre-emit landed so
        // the SEC team still has the `attempt` row.
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    (StatusCode::OK, Json(GrantTierResponse { tenant })).into_response()
}

async fn handle_checkin(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Response {
    let tenant_uuid = match parse_tenant_uuid(&tenant_id) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let scope = match require_admin_scope(&state, &headers, Some(tenant_uuid)) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let tenant = match state.store.get(tenant_uuid) {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, "tenant not found").into_response(),
        Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "store unavailable").into_response(),
    };
    let now_ms = state.wall_clock.now_ms();
    let (alert, age_ms) = match tenant.tier_granted_at_ms {
        Some(granted_at) => {
            let age = now_ms.saturating_sub(granted_at);
            let alert = tenant.pilot_state == PilotState::Active
                && age >= CHECKIN_WINDOW_MS
                && tenant.first_blob_at_ms.is_none();
            (alert, Some(age))
        }
        None => (false, None),
    };

    let row = PilotAuditRow {
        event_type: EVENT_TYPE_CHECKIN.to_string(),
        principal: scope.principal,
        tenant_id: Some(tenant_uuid),
        at_unix_ms: now_ms,
        exit_status: if alert {
            "alert".to_string()
        } else {
            "ok".to_string()
        },
        payload: serde_json::json!({
            "alert_emitted": alert,
            "age_ms": age_ms,
            "first_blob_at_ms": tenant.first_blob_at_ms,
            "pilot_state": tenant.pilot_state.as_str(),
        }),
    };
    if state.audit_sink.emit(row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    (
        StatusCode::OK,
        Json(CheckinResponse {
            tenant_id: tenant_uuid,
            alert_emitted: alert,
            age_ms,
        }),
    )
        .into_response()
}

// -----------------------------------------------------------------------------
// 5-Layer Defense helpers
// -----------------------------------------------------------------------------
