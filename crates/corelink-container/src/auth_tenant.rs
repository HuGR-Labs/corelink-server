//! Authenticated-tenant extractor. The ONLY trustworthy tenant source inside
//! the container is the DO-injected `x-corelink-tenant-id` header (the Worker
//! resolves it from the PAT; the DO forwards path/query unchanged). Fail-CLOSED.
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// The PAT-resolved tenant for this request. Construction is only possible from
/// a concrete, non-sentinel `x-corelink-tenant-id`; handlers that take this as
/// an argument cannot run without an authenticated tenant.
#[derive(Debug, Clone)]
pub struct AuthTenant(
    /// The authenticated, PAT-resolved tenant identifier (never a sentinel).
    pub String,
);

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
const SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending", ""];

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthTenant {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty() || SENTINELS.contains(&raw) {
            // Fail CLOSED: no authenticated tenant ⇒ deny. Do not leak which.
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        Ok(AuthTenant(raw.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// Build a `Parts` carrying the given `x-corelink-tenant-id` header value.
    fn parts_with_header(value: &str) -> Parts {
        axum::http::Request::builder()
            .header("x-corelink-tenant-id", value)
            .body(axum::body::Body::empty())
            .unwrap()
            .into_parts()
            .0
    }

    /// Build a `Parts` with no `x-corelink-tenant-id` header at all.
    fn parts_without_header() -> Parts {
        axum::http::Request::builder()
            .body(axum::body::Body::empty())
            .unwrap()
            .into_parts()
            .0
    }

    #[tokio::test]
    async fn concrete_header_is_accepted() {
        let mut parts = parts_with_header("tenant_abc123");
        let extracted = AuthTenant::from_request_parts(&mut parts, &())
            .await
            .expect("concrete tenant header must be accepted");
        assert_eq!(extracted.0, "tenant_abc123");
    }

    #[tokio::test]
    async fn concrete_header_is_trimmed() {
        let mut parts = parts_with_header("  tenant_abc123  ");
        let extracted = AuthTenant::from_request_parts(&mut parts, &())
            .await
            .expect("trimmed concrete tenant header must be accepted");
        assert_eq!(extracted.0, "tenant_abc123");
    }

    #[tokio::test]
    async fn anonymous_sentinel_is_rejected() {
        assert_sentinel_rejected("_anonymous").await;
    }

    #[tokio::test]
    async fn unknown_sentinel_is_rejected() {
        assert_sentinel_rejected("_unknown").await;
    }

    #[tokio::test]
    async fn system_sentinel_is_rejected() {
        assert_sentinel_rejected("_system").await;
    }

    #[tokio::test]
    async fn pending_sentinel_is_rejected() {
        assert_sentinel_rejected("_pending").await;
    }

    #[tokio::test]
    async fn empty_header_is_rejected() {
        assert_sentinel_rejected("").await;
    }

    #[tokio::test]
    async fn missing_header_is_rejected() {
        let mut parts = parts_without_header();
        let rejection = AuthTenant::from_request_parts(&mut parts, &())
            .await
            .expect_err("missing tenant header must be rejected");
        assert_eq!(rejection.status(), StatusCode::UNAUTHORIZED);
    }

    /// Assert the given header value is rejected with `401 UNAUTHORIZED`.
    async fn assert_sentinel_rejected(value: &str) {
        let mut parts = parts_with_header(value);
        let rejection = AuthTenant::from_request_parts(&mut parts, &())
            .await
            .expect_err("sentinel tenant header must be rejected");
        assert_eq!(rejection.status(), StatusCode::UNAUTHORIZED);
    }
}
