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
///
/// This is the fail-CLOSED backstop: the Worker normally resolves the real
/// tenant and strips/overwrites `x-corelink-tenant-id`, but if any reserved
/// namespace prefix reaches a native `AuthTenant` handler it MUST be rejected,
/// never accepted as a tenant. Covers EVERY reserved prefix the routing layer
/// can emit:
///   - `_anonymous` / `_unknown` / `_system` / `_pending` — Worker routing /
///     fail-CLOSED defaults (worker/src/index.ts; routes/customer.rs et al.).
///   - `_oci` — the synthetic shared OCI Durable-Object tenant id
///     (`tenantId: "_oci"`, `idFromName("_oci")` — worker/src/index.ts:558/569);
///     all OCI `/v2/` traffic shares this id, so it is never a real tenant.
///   - [`crate::adapter_cache::PUBLIC_NAMESPACE`] (`_public`) — the shared
///     cross-tenant dedup namespace, explicitly NOT a tenant (see
///     `storage/r2_s3.rs`); accepting it would let a caller masquerade as the
///     public-share namespace.
///   - `""` — empty / missing header.
const SENTINELS: &[&str] = &[
    "_anonymous",
    "_unknown",
    "_system",
    "_pending",
    "_oci",
    crate::adapter_cache::PUBLIC_NAMESPACE,
    "",
];

/// Shared source-of-truth: is `raw` a reserved sentinel / non-tenant value that
/// must NEVER be accepted as a tenant on ANY cache surface?
///
/// Every surface's fail-CLOSED tenant backstop MUST funnel through this so the
/// reserved set (incl. `_oci` and [`crate::adapter_cache::PUBLIC_NAMESPACE`])
/// stays in lock-step instead of drifting per-surface. `caller` must pass the
/// already-trimmed header value; the empty string is itself a sentinel here, so
/// a missing/blank header is rejected without a separate `is_empty()` check.
pub fn is_reserved_sentinel(raw: &str) -> bool {
    SENTINELS.contains(&raw)
}

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
        if is_reserved_sentinel(raw) {
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
    async fn oci_sentinel_is_rejected() {
        // The synthetic shared OCI DO tenant id must never be accepted as a
        // real tenant if it reaches a native handler (defense-in-depth backstop
        // for the Worker's header strip on the OCI path).
        assert_sentinel_rejected("_oci").await;
    }

    #[tokio::test]
    async fn public_namespace_sentinel_is_rejected() {
        // The shared cross-tenant dedup namespace is NOT a tenant.
        assert_sentinel_rejected(crate::adapter_cache::PUBLIC_NAMESPACE).await;
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
