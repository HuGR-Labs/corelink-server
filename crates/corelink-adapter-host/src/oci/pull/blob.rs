//! `GET`/`HEAD /v2/<name>/blobs/<digest>` handlers.
//!
//! Both routes are tenant-scoped via the bearer token's `scope`
//! grant. `name` MUST match the scope's repository and the scope MUST
//! include the `pull` action.

use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;

use corelink_core::TenantId;

use crate::oci::digest::OciDigest;
use crate::oci::error::OciAdapterError;
use crate::oci::ports::BlobStore;

/// Common name-validation + scope-check before the actual fetch.
fn check_repo_pull(repo: &str, scope: &crate::oci::auth::OciScope) -> Result<(), OciAdapterError> {
    crate::oci::server::validate_repo_name(repo)?;
    if !scope.allows(repo, "pull") {
        return Err(OciAdapterError::Auth(format!(
            "scope does not grant pull on {repo}"
        )));
    }
    Ok(())
}

/// `GET /v2/<repo>/blobs/<digest>` body. Returns the raw blob bytes on
/// `200`, `404` not-found via the [`OciAdapterError::NotFound`] arm.
pub async fn get(
    cas: &dyn BlobStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    digest_str: &str,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_pull(repo, scope)?;
    let digest = OciDigest::parse(digest_str)?;
    let blob_key = digest.to_wire();
    let bytes = cas
        .get_blob(tenant, &blob_key)
        .await
        .map_err(OciAdapterError::Cas)?
        .ok_or(OciAdapterError::NotFound)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/octet-stream"
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        digest
            .to_wire()
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("digest header parse")))?,
    );
    Ok((StatusCode::OK, headers, Body::from(bytes)).into_response())
}

/// `HEAD /v2/<repo>/blobs/<digest>` body. Returns `200` (no body) if
/// the blob exists, `404` otherwise.
pub async fn head(
    cas: &dyn BlobStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    digest_str: &str,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_pull(repo, scope)?;
    let digest = OciDigest::parse(digest_str)?;
    // A HEAD must carry the SAME Content-Type + Content-Length as the GET (just
    // no body). OCI Distribution Spec v1.1 §5.2 requires the blob size in
    // Content-Length; without the explicit header the empty body yields
    // `Content-Length: 0`, and containerd/skopeo/crane pre-allocate the download
    // buffer from it (a zero makes them reject the descriptor). Use the
    // metadata-only `blob_size` port (`None` ⇒ 404), mirroring the manifest HEAD.
    let size = cas
        .blob_size(tenant, &digest.to_wire())
        .await
        .map_err(OciAdapterError::Cas)?
        .ok_or(OciAdapterError::NotFound)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/octet-stream"
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("header parse")))?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        size.to_string()
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("len header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        digest
            .to_wire()
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("digest header parse")))?,
    );
    Ok((StatusCode::OK, headers, Body::empty()).into_response())
}
