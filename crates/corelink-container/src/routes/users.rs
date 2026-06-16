//! `GET /v1/users/me` — caller identity reflection.
//!
//! Returns a JSON document describing the **authenticated caller** as
//! resolved by the Worker's PAT validation. The Worker injects three
//! correlation headers BEFORE forwarding to the Durable Object:
//!
//! - `x-corelink-tenant-id`     — PAT-resolved tenant (DO trusted; never
//!                                 a client-supplied value).
//! - `x-corelink-token-prefix`  — token prefix for audit correlation
//!                                 (NOT the raw PAT secret).
//! - `x-corelink-route-kind`    — route family (informational).
//!
//! This handler is the canonical "who am I" endpoint clients use to
//! verify their PAT is wired correctly without exercising any cache
//! data plane (CAS / AC) surface. Per the REAPI v1 convention the
//! tenant is implicit in the PAT and never appears in the URL.
//!
//! # Security
//!
//! - No request body is read or logged (INV-NO-BODY-IN-LOGS).
//! - The tenant is extracted fail-CLOSED via the shared
//!   [`crate::auth_tenant::AuthTenant`] extractor — a missing or
//!   sentinel `x-corelink-tenant-id` is rejected with 401, exactly
//!   like every other v1 surface (cas/ac/turbo). The endpoint never
//!   echoes a sentinel tenant.
//! - The tenant id and token prefix are echoed back to the caller; the
//!   caller already proved possession of these via the PAT, so this is
//!   not an information disclosure.
//! - The raw PAT is NEVER reflected (the Worker strips it from the
//!   forwarded headers AFTER validation in the v1 path; the DO never
//!   sees it for /v1/users/me anyway).

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};

use crate::auth_tenant::AuthTenant;

/// Canonical /v1/users/me route path.
pub const USERS_ME_ROUTE: &str = "/v1/users/me";

/// The `x-corelink-token-prefix` value the Worker stamps for a Clerk-session
/// caller (see `worker/src/index.ts`). A Clerk caller is edge-verified and
/// carries NO bearer PAT, so the PAT Argon2id backstop is skipped for it —
/// mirroring the customer plane (`super::customer`).
const CLERK_TOKEN_PREFIX: &str = "clerk";

/// Shared route state for `/v1/users/me`.
///
/// Carries the optional native PAT possession gate (cycle-2 nuclear red-team,
/// cluster A). `Some` in production (`PAT_SIGNING_KEY` + D1) — re-runs the full
/// Argon2id Option-B verify on the bearer PAT against the resolved tenant
/// BEFORE the identity is reflected, so a forged-HMAC PAT cannot confirm a
/// victim tenant's identity. Skipped for Clerk-session callers (edge-verified,
/// no bearer). `None` in dev/CI (skipped). Mirrors
/// [`super::cas::CasRouteState::pat_gate`].
#[derive(Clone, Default)]
pub struct UsersRouteState {
    /// Optional native PAT possession gate; see the struct docs.
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
}

impl core::fmt::Debug for UsersRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UsersRouteState").finish_non_exhaustive()
    }
}

/// Build the axum `Router` exposing `GET /v1/users/me`. `Router`
/// already carries `#[must_use]`, so the function itself does not
/// need the attribute (clippy::double_must_use).
pub fn router(state: UsersRouteState) -> Router {
    Router::new()
        .route(USERS_ME_ROUTE, get(handle_me))
        .with_state(state)
}

/// Read the value of `name` from `headers`, trimmed; return `default`
/// when the header is absent or empty. Header values that contain
/// non-ASCII bytes fall through to `default` (the Worker only sets
/// ASCII values). Used ONLY for echo-only correlation metadata
/// (token prefix / route kind) — the tenant goes through the
/// fail-CLOSED [`AuthTenant`] extractor instead.
fn header_or(headers: &HeaderMap, name: &str, default: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// `GET /v1/users/me` handler.
///
/// The tenant comes from the fail-CLOSED [`AuthTenant`] extractor (the
/// same one every other v1 surface uses): a missing/sentinel
/// `x-corelink-tenant-id` rejects with 401 BEFORE this body runs. The
/// remaining headers are echo-only correlation metadata, so they keep
/// soft defaults.
async fn handle_me(
    State(state): State<UsersRouteState>,
    auth: AuthTenant,
    headers: HeaderMap,
) -> impl IntoResponse {
    let tenant_id = auth.0;
    let token_prefix = header_or(&headers, "x-corelink-token-prefix", "_unknown");
    let route_kind = header_or(&headers, "x-corelink-route-kind", "reapi_v1");

    // Native PAT possession backstop (cluster A): re-verify the bearer PAT
    // (Argon2id, full Option-B) resolves to the claimed tenant BEFORE reflecting
    // the identity, so a forged-HMAC PAT cannot confirm a victim tenant. Skipped
    // for Clerk-session callers (edge-verified, no bearer) and in dev/CI (gate
    // absent). 401 forged/wrong-tenant; 503 verifier fault.
    if let Some(gate) = state.pat_gate.as_ref() {
        if token_prefix != CLERK_TOKEN_PREFIX {
            let bearer = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if let Err(resp) = gate.verify(&tenant_id, bearer).await {
                return resp.into_response();
            }
        }
    }

    // JSON construction by hand to avoid pulling serde_json into the
    // routes crate's dep surface (it's already transitive via other
    // routes, but the body is small enough that string concat keeps
    // the handler dep-light and easy to audit).
    let body = format!(
        r#"{{"tenant_id":"{}","token_prefix":"{}","route_kind":"{}"}}"#,
        json_escape(&tenant_id),
        json_escape(&token_prefix),
        json_escape(&route_kind),
    );

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response()
}

/// Escape the four characters JSON requires: `"`, `\`, and ASCII
/// control codes via `\u00XX`. The header path only sets values that
/// the Worker constructed from D1-stored data, so in practice only
/// the four characters below ever appear; we encode them defensively
/// regardless.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    #[test]
    fn route_constant_is_canonical() {
        assert_eq!(USERS_ME_ROUTE, "/v1/users/me");
        assert!(!USERS_ME_ROUTE.contains('{'));
    }

    #[tokio::test]
    async fn returns_200_with_tenant_from_header() {
        let app = router(UsersRouteState::default());
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .header("x-corelink-tenant-id", "tenant-abc")
            .header("x-corelink-token-prefix", "clpat_abcd")
            .header("x-corelink-route-kind", "reapi_v1")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let body = String::from_utf8(bytes.to_vec()).expect("utf8");
        assert!(body.contains("\"tenant_id\":\"tenant-abc\""));
        assert!(body.contains("\"token_prefix\":\"clpat_abcd\""));
        assert!(body.contains("\"route_kind\":\"reapi_v1\""));
    }

    #[tokio::test]
    async fn missing_tenant_header_is_401() {
        // Fail-CLOSED parity with every other v1 surface: no authenticated
        // tenant ⇒ 401 from the AuthTenant extractor, never a sentinel echo.
        let app = router(UsersRouteState::default());
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sentinel_tenant_header_is_401() {
        // Sentinels (_unknown/_anonymous/_system/_pending) are non-tenant
        // traffic markers — the extractor rejects them fail-CLOSED.
        for sentinel in ["_unknown", "_anonymous", "_system", "_pending", ""] {
            let app = router(UsersRouteState::default());
            let req = Request::builder()
                .uri("/v1/users/me")
                .method("GET")
                .header("x-corelink-tenant-id", sentinel)
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(req).await.expect("oneshot");
            assert_eq!(
                resp.status(),
                StatusCode::UNAUTHORIZED,
                "sentinel tenant {sentinel:?} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn echo_only_metadata_keeps_soft_defaults() {
        // With a REAL tenant, the echo-only correlation headers (token
        // prefix / route kind) keep their soft defaults when absent.
        let app = router(UsersRouteState::default());
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .header("x-corelink-tenant-id", "tenant-abc")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let body = String::from_utf8(bytes.to_vec()).expect("utf8");
        assert!(body.contains("\"tenant_id\":\"tenant-abc\""));
        assert!(body.contains("\"token_prefix\":\"_unknown\""));
        assert!(body.contains("\"route_kind\":\"reapi_v1\""));
    }

    #[test]
    fn json_escape_handles_quote_and_backslash() {
        assert_eq!(json_escape(r#"abc"def"#), r#"abc\"def"#);
        assert_eq!(json_escape(r"a\b"), r"a\\b");
        assert_eq!(json_escape("ab\nc"), r"ab\nc");
    }

    // ── Native PAT possession backstop (cluster A) ────────────────────────────

    use crate::adapter_pat::PatRow as VerifierPatRow;
    use crate::native_pat_gate::testing::verifier_with_row;
    use crate::native_pat_gate::NativePatGate;
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_RW,
    };
    use uuid::Uuid;

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("32-byte key"))
    }

    /// Mint a real PAT for `tenant_u128`; return `(plaintext, token_id, pat_hash, tenant_string)`.
    fn mint_pat(key: &PatSigningKey, tenant_u128: u128) -> (String, String, String, String) {
        let tenant_id = TenantId(Uuid::from_u128(tenant_u128));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(tenant_u128 + 1)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            key,
            1,
        )
        .expect("mint");
        (
            plaintext.into_string(),
            pat.token_id.as_str().to_owned(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    fn state_with_gate(
        token_id: String,
        pat_hash: String,
        tenant: String,
        key: Arc<PatSigningKey>,
    ) -> UsersRouteState {
        let row = VerifierPatRow {
            tenant_id: tenant,
            pat_hash,
            scope: "cas:rw".to_owned(),
        };
        let verifier = verifier_with_row(token_id, row, key);
        UsersRouteState {
            pat_gate: Some(Arc::new(NativePatGate::new_for_test(verifier))),
        }
    }

    /// A forged-HMAC PAT is REJECTED 401 on `/v1/users/me` when the gate is
    /// wired — it cannot confirm a victim tenant's identity.
    #[tokio::test]
    async fn forged_pat_rejected_401_on_users_me() {
        let key = test_key();
        let (pt, tid, _hash, tenant) = mint_pat(&key, 200);
        let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 201);
        let app = router(state_with_gate(tid, other_hash, tenant.clone(), key));
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .header("x-corelink-tenant-id", tenant)
            .header("x-corelink-token-prefix", "clpat_forged")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// A genuine PAT for its own tenant PASSES the backstop (200).
    #[tokio::test]
    async fn genuine_pat_passes_backstop_on_users_me() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 202);
        let app = router(state_with_gate(tid, hash, tenant.clone(), key));
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .header("x-corelink-tenant-id", tenant)
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// A Clerk-session caller (no bearer) SKIPS the backstop and gets 200 even
    /// with the gate wired.
    #[tokio::test]
    async fn clerk_caller_skips_backstop_on_users_me() {
        let key = test_key();
        let (_pt, tid, hash, gated_tenant) = mint_pat(&key, 203);
        let app = router(state_with_gate(tid, hash, gated_tenant, key));
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .header("x-corelink-tenant-id", "dashboard-tenant")
            .header("x-corelink-token-prefix", "clerk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// `None`-gate preserves dev behavior: 200 with no bearer, no gate.
    #[tokio::test]
    async fn none_gate_preserves_dev_behavior_on_users_me() {
        let app = router(UsersRouteState::default());
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .header("x-corelink-tenant-id", "dev-tenant")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
