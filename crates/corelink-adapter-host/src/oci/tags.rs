//! `GET /v2/<name>/tags/list` handler.
//!
//! The KV layout per oci.md §3 stores the canonical tag-list body at
//! `oci_tags:<repo>` (written by [`crate::oci::push::manifest`] on every
//! manifest push). We return that body verbatim as
//! `Content-Type: application/json`.
//!
//! Pagination: NOT implemented at v1. The OCI Distribution Spec v1.1
//! §pagination defines `?n=<n>&last=<tag>` query parameters; we
//! ignore them and return the full list. Customers with >10k tags
//! per repo can request pagination via the post-GA admin track.

use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use bytes::Bytes;

use corelink_core::TenantId;

use crate::oci::error::OciAdapterError;
use crate::oci::ports::ManifestKvStore;

/// `GET /v2/<repo>/tags/list`.
pub async fn list(
    kv: &dyn ManifestKvStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
) -> Result<axum::response::Response, OciAdapterError> {
    crate::oci::server::validate_repo_name(repo)?;
    if !scope.allows(repo, "pull") {
        return Err(OciAdapterError::Auth(format!(
            "scope does not grant pull on {repo}"
        )));
    }
    let key = crate::oci::push::manifest::tag_list_key(repo);
    let body = match kv.get(tenant, &key).await.map_err(OciAdapterError::Kv)? {
        Some(b) => b,
        None => {
            // Empty tag list — return canonical empty body.
            let v = serde_json::json!({ "name": repo, "tags": [] });
            let bytes = serde_json::to_vec(&v)
                .map_err(|e| OciAdapterError::Kv(format!("tag-list serialize: {e}")))?;
            Bytes::from(bytes)
        }
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("ct header parse")))?,
    );
    Ok((StatusCode::OK, headers, Body::from(body)).into_response())
}
