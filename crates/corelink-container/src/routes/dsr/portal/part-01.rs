async fn handle_access(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(state, headers, DsrRequestKind::Access, unwrap_body(body)).await
}

async fn handle_portability(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(
        state,
        headers,
        DsrRequestKind::Portability,
        unwrap_body(body),
    )
    .await
}

async fn handle_rectification(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(
        state,
        headers,
        DsrRequestKind::Rectification,
        unwrap_body(body),
    )
    .await
}

async fn handle_erasure(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(state, headers, DsrRequestKind::Erasure, unwrap_body(body)).await
}

async fn handle_restriction(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(
        state,
        headers,
        DsrRequestKind::Restriction,
        unwrap_body(body),
    )
    .await
}

async fn handle_objection(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(state, headers, DsrRequestKind::Objection, unwrap_body(body)).await
}

fn unwrap_body(body: Option<Json<DsrSubmitBody>>) -> DsrSubmitBody {
    body.map(|Json(b)| b).unwrap_or_default()
}

/// The shared submit pipeline: auth → rate-limit → receipt → dispatch → persist.
async fn submit(
    state: PrivacyDsrRouteState,
    headers: HeaderMap,
    action: DsrRequestKind,
    body: DsrSubmitBody,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let now = now_ms();

    // Rate limit (LGPD Art.20 humane cap) — fail-CLOSED on a store fault.
    match state
        .tickets
        .count_since(&tenant, now.saturating_sub(DAY_MS))
    {
        Ok(n) if n >= DSR_DAILY_LIMIT => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "daily DSR request limit reached",
            )
                .into_response()
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!(error = %e, "dsr/submit: rate-limit count failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "rate-limit check failed").into_response();
        }
    }

    let jurisdiction = jurisdiction_from(&headers);
    let request_id = Uuid::now_v7().to_string();
    let sla_deadline_ms = sla_for(jurisdiction, now);
    let receipt = issue_receipt(
        &state.receipt_key,
        &request_id,
        action,
        jurisdiction,
        sla_deadline_ms,
        now,
        &tenant,
    );

    let Some(pipeline) = state.pipeline.as_ref() else {
        // Fail-CLOSED: the live pipeline is not wired (dev/CI / unconfigured).
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "DSR pipeline not configured",
        )
            .into_response();
    };

    let mut ticket = DsrTicket {
        tenant_id: tenant.clone(),
        request_id: request_id.clone(),
        action,
        status: TicketStatus::Pending,
        jurisdiction,
        reason: body.reason.clone(),
        mfa_required: false,
        submitted_at_ms: now,
        sla_deadline_ms,
        receipt: receipt.clone(),
        data_download_url: None,
        timeline: vec![TimelineEvent {
            at_ms: now,
            from: None,
            to: TicketStatus::Pending,
            note: Some("received".to_owned()),
        }],
        updated_at_ms: now,
    };

    match action {
        DsrRequestKind::Access => match pipeline.access(&tenant, &request_id, now) {
            Ok(_export) => complete(&mut ticket, now, None, "access gathered"),
            Err(e) => return pipeline_error(&e, "access"),
        },
        DsrRequestKind::Portability => match pipeline.portability(&tenant, &request_id, now) {
            Ok((_export, handle)) => complete(&mut ticket, now, handle, "portability export ready"),
            Err(e) => return pipeline_error(&e, "portability"),
        },
        DsrRequestKind::Rectification | DsrRequestKind::Erasure => {
            // Destructive arms: gate on the Worker-trusted MFA freshness header.
            if !mfa_fresh(&headers) {
                ticket.mfa_required = true;
                transition(
                    &mut ticket,
                    now,
                    TicketStatus::Pending,
                    "awaiting MFA step-up",
                );
            } else {
                match run_destructive(
                    pipeline.as_ref(),
                    action,
                    &tenant,
                    &request_id,
                    &body,
                    &mut ticket,
                    now,
                ) {
                    // Ticket mutated in place (completed / no-op) → fall through to
                    // the shared insert + 200 OK below.
                    Destructive::Continue => {}
                    // Rejected: the ticket carries a durable Rejected disposition
                    // that MUST be persisted (compliance record) BEFORE the 4xx is
                    // surfaced — the shared insert below is skipped on this return.
                    Destructive::RejectPersist(resp) => {
                        if let Err(e) = state.tickets.insert(&ticket) {
                            tracing::error!(
                                error = %e,
                                request_id = %request_id,
                                "dsr/submit: rejected ticket insert failed",
                            );
                            return (StatusCode::INTERNAL_SERVER_ERROR, "ticket persist failed")
                                .into_response();
                        }
                        return resp;
                    }
                    // Pipeline fault → fail-CLOSED: return the error WITHOUT persisting.
                    Destructive::Abort(resp) => return resp,
                }
            }
        }
        DsrRequestKind::Restriction | DsrRequestKind::Objection => {
            // Policy-only: durably recorded for operator disposition (no auto op).
            transition(
                &mut ticket,
                now,
                TicketStatus::Pending,
                "recorded for operator review",
            );
        }
        // `DsrRequestKind` is `#[non_exhaustive]`.
        _ => return (StatusCode::BAD_REQUEST, "unsupported DSR action").into_response(),
    }

    if let Err(e) = state.tickets.insert(&ticket) {
        tracing::error!(error = %e, request_id = %request_id, "dsr/submit: ticket insert failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "ticket persist failed").into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "request_id": request_id,
            "action": action.as_str(),
            "jurisdiction": jurisdiction.as_str(),
            "sla_deadline": ms_to_iso8601(clamp_i64(sla_deadline_ms)),
            "jwt_receipt": receipt,
        })),
    )
        .into_response()
}

/// Outcome of an inline destructive arm (the ticket is mutated in place).
enum Destructive {
    /// Completed / no-op — the caller persists the ticket and returns 200 OK.
    Continue,
    /// Rejected — the caller PERSISTS the (Rejected) ticket, then returns this 4xx.
    RejectPersist(Response),
    /// Pipeline fault — the caller returns this WITHOUT persisting (fail-CLOSED).
    Abort(Response),
}

/// Execute a destructive arm inline (MFA already fresh). The returned
/// [`Destructive`] tells the caller whether to persist the mutated ticket and
/// which response to surface.
fn run_destructive(
    pipeline: &dyn DsrPipeline,
    action: DsrRequestKind,
    tenant: &str,
    request_id: &str,
    body: &DsrSubmitBody,
    ticket: &mut DsrTicket,
    now: u64,
) -> Destructive {
    transition(ticket, now, TicketStatus::InProgress, "executing");
    match action {
        DsrRequestKind::Erasure => match pipeline.erasure(tenant) {
            Ok(()) => {
                complete(ticket, now, None, "erasure requested");
                Destructive::Continue
            }
            Err(e) => Destructive::Abort(pipeline_error(&e, "erasure")),
        },
        DsrRequestKind::Rectification => {
            let email = body
                .rectification
                .as_ref()
                .and_then(|r| r.email.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(email) = email else {
                // No live-rectifiable field supplied → record as completed no-op
                // (only the contact email is correctable in the live pipeline).
                complete(ticket, now, None, "no live-rectifiable field supplied");
                return Destructive::Continue;
            };
            match pipeline.rectification(tenant, request_id, email, now) {
                Ok(Ok(())) => {
                    complete(ticket, now, None, "rectification applied");
                    Destructive::Continue
                }
                Ok(Err(reject)) => {
                    reject_ticket(ticket, now, &reject);
                    // Rejected ticket must be persisted (durable disposition) by the
                    // caller BEFORE the 4xx is surfaced.
                    Destructive::RejectPersist(
                        (StatusCode::UNPROCESSABLE_ENTITY, reject).into_response(),
                    )
                }
                Err(e) => Destructive::Abort(pipeline_error(&e, "rectification")),
            }
        }
        _ => Destructive::Abort((StatusCode::BAD_REQUEST, "not a destructive arm").into_response()),
    }
}

/// `GET /v1/privacy/dsr/{request_id}/status`
async fn handle_status(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    match state.tickets.get(&tenant, &request_id) {
        // Constant-time confidentiality: a miss AND a cross-tenant lookup both
        // 404 — never disclose whether a request_id exists for another tenant.
        Ok(Some(ticket)) => (StatusCode::OK, Json(ticket.detail_json())).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dsr/status: lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "status lookup failed").into_response()
        }
    }
}

/// `GET /v1/privacy/dsr`
async fn handle_list(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    match state.tickets.list(&tenant, limit) {
        Ok(items) => (
            StatusCode::OK,
            Json(json!({
                "items": items.iter().map(DsrTicket::summary_json).collect::<Vec<_>>(),
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dsr/list: lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "list failed").into_response()
        }
    }
}

/// `POST /v1/privacy/dsr/{request_id}/verify-mfa`
async fn handle_verify_mfa(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
    _body: Option<Json<VerifyMfaBody>>,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let mut ticket = match state.tickets.get(&tenant, &request_id) {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dsr/verify-mfa: lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    // Already terminal → idempotent detail (a repeat verify is a no-op).
    if matches!(
        ticket.status,
        TicketStatus::Completed | TicketStatus::Rejected
    ) {
        return (StatusCode::OK, Json(ticket.detail_json())).into_response();
    }
    // Fail-CLOSED: the destructive op runs only under a fresh Worker-trusted
    // MFA step-up.
    if !mfa_fresh(&headers) {
        return (StatusCode::UNAUTHORIZED, "MFA step-up required").into_response();
    }
    let Some(pipeline) = state.pipeline.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "DSR pipeline not configured",
        )
            .into_response();
    };
    let now = now_ms();
    ticket.mfa_required = false;
    let body = DsrSubmitBody {
        reason: ticket.reason.clone(),
        rectification: None,
    };
    let outcome = run_destructive(
        pipeline.as_ref(),
        ticket.action,
        &tenant,
        &request_id,
        &body,
        &mut ticket,
        now,
    );
    // Persist the transition regardless of the op outcome (durable evidence): the
    // ticket already exists (Pending), so this is an UPDATE — a Rejected or faulted
    // disposition is recorded either way.
    if let Err(e) = state.tickets.update(&ticket) {
        tracing::error!(error = %e, request_id = %request_id, "dsr/verify-mfa: ticket update failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "ticket persist failed").into_response();
    }
    match outcome {
        Destructive::Continue => (StatusCode::OK, Json(ticket.detail_json())).into_response(),
        Destructive::RejectPersist(err_resp) | Destructive::Abort(err_resp) => err_resp,
    }
}

// ─── Auth + helpers ────────────────────────────────────────────────────────────

/// Resolve the authenticated tenant (fail-CLOSED) and enforce the Clerk-session
/// + PAT-backstop gates. `Err(resp)` ⇒ the caller returns that response.
async fn authed_tenant(
    state: &PrivacyDsrRouteState,
    headers: &HeaderMap,
) -> Result<String, Response> {
    let tenant = tenant(headers).map_err(|()| {
        (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
    })?;
    // Clerk-session ONLY: a data-plane cache PAT must never drive a DSR request
    // (mirrors `/v1/customer/account/delete`).
    if principal(headers) != CLERK_TOKEN_PREFIX {
        return Err((
            StatusCode::FORBIDDEN,
            "data-subject-rights requests require a dashboard (Clerk) session",
        )
            .into_response());
    }
    // Native PAT possession backstop (skipped for Clerk callers + dev/CI).
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        gate.verify(&tenant, bearer).await?;
    }
    Ok(tenant)
}

/// Read the authenticated `x-corelink-tenant-id`, fail-CLOSED (mirrors
/// `routes/customer.rs::tenant`).
fn tenant(headers: &HeaderMap) -> Result<String, ()> {
    let raw = headers
        .get("x-corelink-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    if raw.is_empty() || TENANT_SENTINELS.contains(&raw) {
        return Err(());
    }
    Ok(raw.to_owned())
}

/// Read the Worker-set `x-corelink-token-prefix` (fail-CLOSED to `_unknown`).
fn principal(headers: &HeaderMap) -> String {
    headers
        .get("x-corelink-token-prefix")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "_unknown".to_owned())
}

/// Whether the Worker-trusted MFA freshness marker is set (`x-corelink-mfa-
/// verified: 1`). The header is in the Worker strip list, so a client can never
/// forge it.
fn mfa_fresh(headers: &HeaderMap) -> bool {
    headers
        .get(MFA_VERIFIED_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        == Some("1")
}

/// Resolve the SLA jurisdiction from the optional Worker header; default GDPR.
fn jurisdiction_from(headers: &HeaderMap) -> DsrJurisdiction {
    headers
        .get(JURISDICTION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .map(parse_jurisdiction)
        .unwrap_or(DsrJurisdiction::Gdpr)
}

fn parse_jurisdiction(s: &str) -> DsrJurisdiction {
    match s.to_ascii_lowercase().as_str() {
        "lgpd" => DsrJurisdiction::Lgpd,
        "ccpa" => DsrJurisdiction::Ccpa,
        // Default (incl. "gdpr") is GDPR — the strictest calendar-month SLA.
        _ => DsrJurisdiction::Gdpr,
    }
}
