/// Parse + validate the `{tenant_id}` path capture as a lowercase
/// hyphenated UUID.
fn parse_tenant_uuid(raw: &str) -> Result<Uuid, Box<Response>> {
    Uuid::parse_str(raw)
        .map_err(|_| Box::new((StatusCode::BAD_REQUEST, "invalid tenant_id").into_response()))
}

/// Constant-time verification of the operator shared secret.
///
/// Fail-CLOSED:
/// - `expected` is `None` (key not configured at boot) → `false`.
/// - header absent / wrong → `false`.
///
/// Pads the provided value to the expected length and runs a single
/// `ct_eq`, then folds in the real length-equality — so the secret LENGTH
/// is not leaked via an early-return short-circuit.
#[must_use]
fn internal_auth_ok(expected: Option<&Arc<str>>, headers: &HeaderMap) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let provided = headers
        .get(ADMIN_INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected_bytes = expected.as_bytes();
    let provided_bytes = provided.as_bytes();
    let provided_padded: Vec<u8> = if provided_bytes.len() >= expected_bytes.len() {
        provided_bytes
            .get(..expected_bytes.len())
            .unwrap_or(&[])
            .to_vec()
    } else {
        let mut v = provided_bytes.to_vec();
        v.resize(expected_bytes.len(), 0);
        v
    };
    let content_ok = expected_bytes.ct_eq(&provided_padded).unwrap_u8();
    let len_ok = u8::from(expected_bytes.len() == provided_bytes.len());
    (content_ok & len_ok) == 1
}

/// L2 + L3 + L5: enforce that the caller carries
/// `corelink:admin:pilots` in the validated scope claim, and that
/// (if tenant-scoped) the bound tenant matches the target tenant.
/// On failure: emit the canonical audit row BEFORE returning the
/// 403/401/503.
///
/// Returns the parsed [`PilotAdminScope`] on success; otherwise a
/// fully-formed `Response`.
fn require_admin_scope(
    state: &PilotAdminRouteState,
    headers: &HeaderMap,
    target_tenant: Option<Uuid>,
) -> Result<PilotAdminScope, Box<Response>> {
    let now_ms = state.wall_clock.now_ms();

    // PRIMARY boundary (fail-CLOSED): the operator-only shared secret.
    // This replaces the previous design where the client-forgeable
    // `x-admin-scope` header was the SOLE gate — any tenant PAT could
    // set that header and self-assert admin. Now the request must clear
    // the `x-corelink-internal-auth` constant-time gate first; absent /
    // wrong secret, or unconfigured key → emit the canonical
    // unauthorized audit row BEFORE the 403.
    if !internal_auth_ok(state.internal_auth_key.as_ref(), headers) {
        let row = PilotAuditRow {
            event_type: EVENT_TYPE_UNAUTHORIZED.to_string(),
            principal: String::new(),
            tenant_id: target_tenant,
            at_unix_ms: now_ms,
            exit_status: "forbidden".to_string(),
            payload: serde_json::json!({
                "reason": "missing/invalid x-corelink-internal-auth",
            }),
        };
        return Err(Box::new(emit_or_503(
            state,
            row,
            (StatusCode::FORBIDDEN, "operator auth required").into_response(),
        )));
    }

    let principal = headers
        .get(ADMIN_PRINCIPAL_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let route_kind = headers
        .get(ROUTE_KIND_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    // Internal-edge identity synthesis (#218 §2.2, ratified Q3
    // 2026-06-10): the Worker's `/_internal/*` path strips the
    // client-suppliable `x-admin-principal` / `x-admin-scope` headers
    // (CLIENT_TRUST_HEADERS) and re-injects ONLY internal-auth /
    // request-id / route-kind / tenant / token-prefix — so an operator
    // call through the public edge arrives with NO principal and NO
    // scope label. After the PRIMARY internal-auth gate above has
    // passed, an empty principal PLUS the server-set
    // `x-corelink-route-kind: internal` (the Worker unconditionally
    // overwrites that header on every forward — not client-forgeable)
    // synthesizes the audit identity [`INTERNAL_EDGE_PRINCIPAL`] with
    // the pilots scope treated as granted, so audit rows keep a
    // principal. `bound_tenant` stays `None` (global operator): the
    // Worker strips `x-admin-tenant` on this path too. Matches the
    // `/_internal/pat/mint` precedent (internal-auth-only gating).
    //
    // Requests with an explicit principal keep today's behavior
    // byte-identical; an empty principal WITHOUT the internal
    // route-kind still falls through to the 403 below.
    let scope = if principal.is_empty() && route_kind == ROUTE_KIND_INTERNAL {
        PilotAdminScope {
            principal: INTERNAL_EDGE_PRINCIPAL.to_string(),
            bound_tenant: None,
        }
    } else {
        let scope_header = headers
            .get(ADMIN_SCOPE_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // SECONDARY label (NOT the sole gate): the internal-auth check above
        // is the boundary. We keep the scope-string + principal checks as a
        // defence-in-depth label so audit rows still carry a principal and a
        // misconfigured operator forward (no scope) is observable.
        let has_scope = scope_header
            .split_whitespace()
            .any(|s| s == REQUIRED_ADMIN_SCOPE);
        if principal.is_empty() || !has_scope {
            let row = PilotAuditRow {
                event_type: EVENT_TYPE_UNAUTHORIZED.to_string(),
                principal: principal.to_string(),
                tenant_id: target_tenant,
                at_unix_ms: now_ms,
                exit_status: "forbidden".to_string(),
                payload: serde_json::json!({
                    "reason": if principal.is_empty() {
                        "missing X-Admin-Principal"
                    } else {
                        "missing corelink:admin:pilots scope"
                    },
                }),
            };
            return Err(Box::new(emit_or_503(
                state,
                row,
                (StatusCode::FORBIDDEN, "admin scope required").into_response(),
            )));
        }
        let bound_tenant = headers
            .get(ADMIN_TENANT_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| Uuid::parse_str(s).ok());
        PilotAdminScope {
            principal: principal.to_string(),
            bound_tenant,
        }
    };
    if let Some(target) = target_tenant {
        if !scope.allows_tenant(target) {
            let row = PilotAuditRow {
                event_type: EVENT_TYPE_CROSS_TENANT.to_string(),
                principal: scope.principal.clone(),
                tenant_id: Some(target),
                at_unix_ms: now_ms,
                exit_status: "forbidden".to_string(),
                payload: serde_json::json!({
                    "bound_tenant": scope.bound_tenant.map(|u| u.to_string()),
                    "target_tenant": target.to_string(),
                }),
            };
            return Err(Box::new(emit_or_503(
                state,
                row,
                (StatusCode::FORBIDDEN, "cross-tenant admin probe").into_response(),
            )));
        }
    }
    Ok(scope)
}
