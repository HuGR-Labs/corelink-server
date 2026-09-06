use super::*;

// ── Team routes ───────────────────────────────────────────────────────────

#[tokio::test]
async fn team_invite_returns_201_with_member() {
    let (state, _) = fixture();
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "email": "alice@example.com",
        "role": "member"
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/team/invite")
        .method("POST")
        .header("x-corelink-tenant-id", "t7")
        .header("x-corelink-token-prefix", "clpat_t7")
        .header("x-corelink-role", "owner")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["member"]["email"], "alice@example.com");
    assert_eq!(v["member"]["role"], "member");
    assert_eq!(v["member"]["status"], "invited");
}

/// RBAC hardening (#103): the escalation chain the account-delete audit found
/// — a member invites an `owner` seat → accepts → resolves owner → deletes the
/// tenant — is closed at the source: an `owner` invite is rejected 403 for ANY
/// caller (even the owner), so no second owner can ever be minted.
#[tokio::test]
async fn team_invite_owner_role_is_always_rejected() {
    let (state, _) = fixture();
    let app = router(state);
    for caller in ["owner", "admin", "member", "viewer", ""] {
        let body = serde_json::to_string(&serde_json::json!({
            "email": "evil@example.com", "role": "Owner"
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/team/invite")
            .method("POST")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("x-corelink-role", caller)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "an owner invite must be rejected for caller role {caller:?}"
        );
    }
}

/// RBAC hardening (#103): a plain `member`/`viewer` cannot invite ANY seat,
/// and only the OWNER can invite a privileged `admin` (an admin cannot mint
/// another admin). Owner→member/admin and admin→member succeed.
#[tokio::test]
async fn team_invite_is_owner_admin_gated() {
    let (state, _) = fixture();
    let app = router(state);
    let invite = |caller: &str, role: &str| {
        let body = serde_json::to_string(&serde_json::json!({ "email": "x@e.com", "role": role }))
            .unwrap();
        Request::builder()
            .uri("/v1/customer/team/invite")
            .method("POST")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("x-corelink-role", caller)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap()
    };
    // member / viewer / unknown cannot invite at all → 403.
    for caller in ["member", "viewer", ""] {
        let r = app.clone().oneshot(invite(caller, "member")).await.unwrap();
        assert_eq!(
            r.status(),
            StatusCode::FORBIDDEN,
            "caller {caller:?} cannot invite"
        );
    }
    // admin can invite a non-privileged member → 201, but NOT a privileged admin → 403.
    assert_eq!(
        app.clone()
            .oneshot(invite("admin", "member"))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        app.clone()
            .oneshot(invite("admin", "admin"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN,
        "only the owner may invite a privileged admin"
    );
    // owner can invite an admin → 201.
    assert_eq!(
        app.clone()
            .oneshot(invite("owner", "admin"))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
}

/// RBAC hardening (#103): a plain `member` (cache-write scope) can no longer
/// remove teammates / revoke their PATs — owner/admin only.
#[tokio::test]
async fn team_remove_member_role_is_403() {
    let (state, _) = fixture();
    let app = router(state);
    for caller in ["member", "viewer", ""] {
        let req = Request::builder()
            .uri("/v1/customer/team/user_victim")
            .method("DELETE")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-role", caller)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "caller role {caller:?} must not remove a team member"
        );
    }
}

#[tokio::test]
async fn team_remove_without_write_scope_is_403() {
    // A read-only caller (no cache-write scope) must NOT remove a seat —
    // the privilege gate fires 403 (intra-tenant member-lockout defense).
    let (state, _) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/team/user_victim")
        .method("DELETE")
        .header("x-corelink-tenant-id", "t7")
        .header("x-corelink-token-prefix", "clpat_t7")
        .header("x-corelink-scope", "read-only")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn team_remove_with_write_scope_flips_seat() {
    // Seed a member via invite, then remove it with a write-scoped caller.
    let (state, _) = fixture();
    let app = router(state);
    let invite_body = serde_json::to_string(&serde_json::json!({
        "email": "bob@example.com", "role": "member"
    }))
    .unwrap();
    let invite = Request::builder()
        .uri("/v1/customer/team/invite")
        .method("POST")
        .header("x-corelink-tenant-id", "t7")
        .header("x-corelink-token-prefix", "clpat_t7")
        .header("x-corelink-role", "owner")
        .header("content-type", "application/json")
        .body(Body::from(invite_body))
        .unwrap();
    let iresp = app.clone().oneshot(invite).await.expect("invite oneshot");
    assert_eq!(iresp.status(), StatusCode::CREATED);
    let ibytes = to_bytes(iresp.into_body(), 1 << 20).await.expect("body");
    let iv: serde_json::Value = serde_json::from_slice(&ibytes).expect("json");
    let user_id = iv["member"]["user_id"]
        .as_str()
        .expect("user_id")
        .to_owned();

    let remove = Request::builder()
        .uri(format!("/v1/customer/team/{user_id}"))
        .method("DELETE")
        .header("x-corelink-tenant-id", "t7")
        .header("x-corelink-token-prefix", "clpat_t7")
        .header("x-corelink-scope", "read-write")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(remove).await.expect("remove oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["member"]["status"], "removed");
    assert!(v["revoked_pats"].is_number());
}

// ── Audit query route ─────────────────────────────────────────────────────

#[tokio::test]
async fn audit_route_returns_empty_rows_for_new_tenant() {
    let (state, _) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/audit")
        .method("GET")
        // F-018 (H17): the audit log carries security/account PII — requires
        // the billing capability (the Worker's non-viewer dashboard scope).
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "t8")
        .header("x-corelink-token-prefix", "clpat_t8")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["rows"].as_array().expect("rows").len(), 0);
}

// ── Error mapping ─────────────────────────────────────────────────────────

#[test]
fn map_err_unauthorized_is_401() {
    let resp = map_err(CustomerHandlerError::Unauthorized("no session".into()));
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn map_err_cross_tenant_is_403() {
    let resp = map_err(CustomerHandlerError::CrossTenantDenied {
        caller: "a".into(),
        requested_tenant: "b".into(),
    });
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn map_err_not_found_is_404() {
    let resp = map_err(CustomerHandlerError::NotFound { what: "x".into() });
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn map_err_audit_failed_is_503() {
    let resp = map_err(CustomerHandlerError::AuditFailed("pipe down".into()));
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn map_err_internal_is_500() {
    let resp = map_err(CustomerHandlerError::Internal("boom".into()));
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn map_err_not_implemented_is_501() {
    // The generic NotImplemented arm maps to an explicit 501. Team
    // invites are now fully implemented (D1 + signup-worker accept);
    // this arm is the defensive fallback for any future unimplemented
    // customer surface, so the copy is generic (not team-specific).
    let resp = map_err(CustomerHandlerError::NotImplemented(
        "some future endpoint".into(),
    ));
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
}

#[test]
fn build_handlers_from_env_falls_back_to_in_memory_without_d1_env() {
    // Dev/CI (no StorageEnv): the env-gated factory must still
    // produce a usable state (InMemory fallback) without panicking.
    // (If a developer machine exports the full StorageEnv this still
    // constructs — the D1 handler does no I/O at build time.)
    let state = build_handlers_from_env();
    let _router = router(state);
}
