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
//! - The tenant id and token prefix are echoed back to the caller; the
//!   caller already proved possession of these via the PAT, so this is
//!   not an information disclosure.
//! - The raw PAT is NEVER reflected (the Worker strips it from the
//!   forwarded headers AFTER validation in the v1 path; the DO never
//!   sees it for /v1/users/me anyway).

use axum::{
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};

/// Canonical /v1/users/me route path.
pub const USERS_ME_ROUTE: &str = "/v1/users/me";

/// Build the axum `Router` exposing `GET /v1/users/me`. `Router`
/// already carries `#[must_use]`, so the function itself does not
/// need the attribute (clippy::double_must_use).
pub fn router() -> Router {
    Router::new().route(USERS_ME_ROUTE, get(handle_me))
}

/// Read the value of `name` from `headers`, trimmed; return `default`
/// when the header is absent or empty. Header values that contain
/// non-ASCII bytes fall through to `default` (the Worker only sets
/// ASCII values; an invalid value indicates client tampering on a path
/// the DO did not expect — fail-CLOSED to "_unknown").
fn header_or(headers: &HeaderMap, name: &str, default: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// `GET /v1/users/me` handler.
async fn handle_me(headers: HeaderMap) -> impl IntoResponse {
    let tenant_id = header_or(&headers, "x-corelink-tenant-id", "_unknown");
    let token_prefix = header_or(&headers, "x-corelink-token-prefix", "_unknown");
    let route_kind = header_or(&headers, "x-corelink-route-kind", "reapi_v1");

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
        let app = router();
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
    async fn returns_unknown_when_headers_missing() {
        let app = router();
        let req = Request::builder()
            .uri("/v1/users/me")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let body = String::from_utf8(bytes.to_vec()).expect("utf8");
        // Fail-CLOSED default — both tenant + token-prefix unknown.
        assert!(body.contains("\"tenant_id\":\"_unknown\""));
        assert!(body.contains("\"token_prefix\":\"_unknown\""));
    }

    #[test]
    fn json_escape_handles_quote_and_backslash() {
        assert_eq!(json_escape(r#"abc"def"#), r#"abc\"def"#);
        assert_eq!(json_escape(r"a\b"), r"a\\b");
        assert_eq!(json_escape("ab\nc"), r"ab\nc");
    }
}
