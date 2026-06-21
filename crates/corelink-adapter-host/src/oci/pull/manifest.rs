//! `GET`/`HEAD /v2/<name>/manifests/<reference>` handlers.
//!
//! The KV key shape per oci.md §3 is
//! `oci_manifest:<repo>:<reference>` (tenant scoping implicit via the
//! `tenant` arg to the port — every `ManifestKvStore` impl per-tenant
//! prefixes at the bottom).
//!
//! Tag references resolve through the same key as digest references
//! because the `PUT` side writes both `oci_manifest:<repo>:<tag>` and
//! `oci_manifest:<repo>:<digest>` slots on every push.

use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;

use corelink_core::TenantId;

use crate::oci::error::OciAdapterError;
use crate::oci::ports::ManifestKvStore;

/// Canonical KV key (callers pass the result straight to
/// [`ManifestKvStore::get`]).
#[must_use]
pub fn manifest_key(repo: &str, reference: &str) -> String {
    format!("oci_manifest:{repo}:{reference}")
}

/// Companion: per-manifest content-type slot (so we can return the
/// SAME `Content-Type` the client supplied on `PUT`, per OCI
/// Distribution Spec v1.1 §pull).
#[must_use]
pub fn manifest_ct_key(repo: &str, reference: &str) -> String {
    format!("oci_manifest_ct:{repo}:{reference}")
}

fn check_repo_pull(repo: &str, scope: &crate::oci::auth::OciScope) -> Result<(), OciAdapterError> {
    crate::oci::server::validate_repo_name(repo)?;
    if !scope.allows(repo, "pull") {
        return Err(OciAdapterError::Auth(format!(
            "scope does not grant pull on {repo}"
        )));
    }
    Ok(())
}

/// `GET /v2/<repo>/manifests/<reference>`.
pub async fn get(
    kv: &dyn ManifestKvStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    reference: &str,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_pull(repo, scope)?;
    let key = manifest_key(repo, reference);
    let body = kv
        .get(tenant, &key)
        .await
        .map_err(OciAdapterError::Kv)?
        .ok_or(OciAdapterError::NotFound)?;
    // Companion content-type slot. Falls back to canonical OCI v1
    // manifest mediaType if missing (older pushes from before the CT
    // slot was added — defensive, all NEW pushes write CT explicitly).
    let ct_key = manifest_ct_key(repo, reference);
    let ct_bytes = kv.get(tenant, &ct_key).await.map_err(OciAdapterError::Kv)?;
    let ct_str = match ct_bytes {
        Some(b) => String::from_utf8(b.to_vec())
            .unwrap_or_else(|_| String::from("application/vnd.oci.image.manifest.v1+json")),
        None => String::from("application/vnd.oci.image.manifest.v1+json"),
    };
    let manifest_digest =
        crate::oci::digest::OciDigest::compute(crate::oci::digest::OciDigestAlgo::Sha256, &body)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        ct_str
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("ct header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        manifest_digest
            .to_wire()
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("digest header parse")))?,
    );
    Ok((StatusCode::OK, headers, Body::from(body)).into_response())
}

/// `HEAD /v2/<repo>/manifests/<reference>`.
pub async fn head(
    kv: &dyn ManifestKvStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    reference: &str,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_pull(repo, scope)?;
    let key = manifest_key(repo, reference);
    let body = kv
        .get(tenant, &key)
        .await
        .map_err(OciAdapterError::Kv)?
        .ok_or(OciAdapterError::NotFound)?;
    // A HEAD must carry the SAME Content-Type + Content-Length as the GET (just
    // no body). Without the explicit Content-Length the empty body yields
    // `Content-Length: 0`, and docker pull rejects the descriptor with
    // "content size of zero: invalid argument" — so tag resolution (HEAD
    // manifests/<tag>) breaks every pull. Mirror the GET's content-type slot.
    let ct_key = manifest_ct_key(repo, reference);
    let ct_bytes = kv.get(tenant, &ct_key).await.map_err(OciAdapterError::Kv)?;
    let ct_str = match ct_bytes {
        Some(b) => String::from_utf8(b.to_vec())
            .unwrap_or_else(|_| String::from("application/vnd.oci.image.manifest.v1+json")),
        None => String::from("application/vnd.oci.image.manifest.v1+json"),
    };
    let manifest_digest =
        crate::oci::digest::OciDigest::compute(crate::oci::digest::OciDigestAlgo::Sha256, &body)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        ct_str
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("ct header parse")))?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        body.len()
            .to_string()
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("len header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        manifest_digest
            .to_wire()
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("digest header parse")))?,
    );
    Ok((StatusCode::OK, headers, Body::empty()).into_response())
}
