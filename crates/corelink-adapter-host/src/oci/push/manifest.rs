//! `PUT /v2/<repo>/manifests/<reference>` — validate + persist a
//! manifest.
//!
//! Schema validation surface (per oci.md §8 row 4):
//!
//! - `mediaType` MUST be one of:
//!   - `application/vnd.oci.image.manifest.v1+json`
//!   - `application/vnd.oci.image.index.v1+json`
//!   - `application/vnd.docker.distribution.manifest.v2+json`
//!   - `application/vnd.docker.distribution.manifest.list.v2+json`
//! - `schemaVersion` MUST be `2` (we reject v1 manifests entirely;
//!   they're a deprecated Docker format pre-OCI).
//! - For image manifests: `config` MUST exist + carry a `digest`.
//! - For indexes: `manifests` MUST exist and be a non-empty array.
//!
//! We deliberately do NOT validate that every referenced blob digest
//! is already present in CAS. The OCI Distribution Spec v1.1 leaves
//! that as an OPTIONAL "linked blob check" most registries skip;
//! garbage collection of orphan references is delegated to
//! `corelink-gc`.
//!
//! Audit emission per oci.md §1 (audit BEFORE state mutation):
//!
//! 1. Emit `oci.manifest.push.v1` BEFORE the `kv.put(...)`.
//! 2. If `<reference>` is a tag (not a digest), emit
//!    `oci.tag.update.v1` BEFORE updating the tag-list slot.

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use bytes::Bytes;

use corelink_audit::ports::AuditEmitter;
use corelink_core::TenantId;

use crate::oci::audit::{emit as audit_emit, OciAuditEvent};
use crate::oci::digest::{OciDigest, OciDigestAlgo};
use crate::oci::error::OciAdapterError;
use crate::oci::ports::ManifestKvStore;

const ACCEPTED_MEDIA_TYPES: &[&str] = &[
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
];

/// Validate a manifest body. Returns the `(mediaType, computed-digest)`
/// pair on success.
///
/// # Errors
///
/// Returns [`OciAdapterError::ManifestInvalid`] on any schema failure.
pub fn validate(body: &[u8]) -> Result<(String, OciDigest), OciAdapterError> {
    // JSON parse.
    let v: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| OciAdapterError::ManifestInvalid(format!("json: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| OciAdapterError::ManifestInvalid(String::from("not a json object")))?;
    let schema_version = obj
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| OciAdapterError::ManifestInvalid(String::from("missing schemaVersion")))?;
    if schema_version != 2 {
        return Err(OciAdapterError::ManifestInvalid(format!(
            "unsupported schemaVersion: {schema_version}"
        )));
    }
    let media_type = obj
        .get("mediaType")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OciAdapterError::ManifestInvalid(String::from("missing mediaType")))?;
    if !ACCEPTED_MEDIA_TYPES.contains(&media_type) {
        return Err(OciAdapterError::ManifestInvalid(format!(
            "unsupported mediaType: {media_type}"
        )));
    }
    let is_index = media_type.contains("index") || media_type.contains("manifest.list");
    if is_index {
        let arr = obj
            .get("manifests")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                OciAdapterError::ManifestInvalid(String::from(
                    "index manifest missing `manifests` array",
                ))
            })?;
        if arr.is_empty() {
            return Err(OciAdapterError::ManifestInvalid(String::from(
                "index manifests must be non-empty",
            )));
        }
    } else {
        let config = obj
            .get("config")
            .ok_or_else(|| OciAdapterError::ManifestInvalid(String::from("missing config")))?;
        config
            .as_object()
            .ok_or_else(|| OciAdapterError::ManifestInvalid(String::from("config must be object")))?
            .get("digest")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                OciAdapterError::ManifestInvalid(String::from("config.digest missing"))
            })?;
    }
    let digest = OciDigest::compute(OciDigestAlgo::Sha256, body)?;
    Ok((media_type.to_string(), digest))
}

/// Is `reference` a tag (not a digest)?
///
/// OCI tags match `[A-Za-z0-9_][A-Za-z0-9._-]{0,127}`; digests
/// contain a `:` and the algorithm prefix. Cheap heuristic: presence
/// of `:` ⇒ digest reference.
#[must_use]
pub fn is_tag_reference(reference: &str) -> bool {
    !reference.contains(':')
}

fn check_repo_push(repo: &str, scope: &crate::oci::auth::OciScope) -> Result<(), OciAdapterError> {
    crate::oci::server::validate_repo_name(repo)?;
    if !scope.allows(repo, "push") {
        return Err(OciAdapterError::Auth(format!(
            "scope does not grant push on {repo}"
        )));
    }
    Ok(())
}

/// Tag-list KV key (per oci.md §3).
#[must_use]
pub fn tag_list_key(repo: &str) -> String {
    format!("oci_tags:{repo}")
}

/// `PUT /v2/<repo>/manifests/<reference>`.
#[allow(
    clippy::too_many_arguments,
    reason = "wire-shape constructor — every arg is necessary"
)]
pub async fn put(
    kv: &dyn ManifestKvStore,
    auditor: &dyn AuditEmitter,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    reference: &str,
    media_type_header: &str,
    body: Bytes,
    now_unix_ms: u64,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_push(repo, scope)?;
    let (validated_media_type, manifest_digest) = validate(&body)?;
    // Digest-form reference fail-CLOSED (mirrors the blob path's
    // `verify_against_bytes`, `oci/push/upload.rs:282`). A `PUT
    // .../manifests/sha256:<hex>` is content-addressed: the supplied digest MUST
    // hash the body, else the registry would serve a manifest under a digest
    // address that does not hash to it (digest confusion, F-009). Tag-form
    // references (no `:`) carry no digest assertion and are stored verbatim.
    if !is_tag_reference(reference) && reference != manifest_digest.to_wire() {
        return Err(OciAdapterError::ManifestInvalid(format!(
            "digest-form reference {reference} does not match manifest body digest {}",
            manifest_digest.to_wire()
        )));
    }
    // The header `Content-Type` MUST match (or be empty / `application/json`)
    // — be permissive: accept any of the canonical OCI/Docker types
    // that we already validated, plus a generic json content-type.
    if !media_type_header.is_empty()
        && media_type_header != validated_media_type
        && !media_type_header.starts_with("application/json")
    {
        return Err(OciAdapterError::ManifestInvalid(format!(
            "Content-Type {media_type_header} does not match body mediaType {validated_media_type}"
        )));
    }
    // Audit emit BEFORE the KV put (mutation).
    audit_emit(
        auditor,
        tenant,
        &OciAuditEvent::ManifestPush {
            repo,
            reference,
            manifest_digest: &manifest_digest.to_wire(),
            media_type: &validated_media_type,
        },
        now_unix_ms,
    )?;
    // Write both reference-form (tag or digest) AND digest-form slots
    // so future `GET .../manifests/<digest>` works even if the
    // original push was tag-keyed.
    let key_by_reference = crate::oci::pull::manifest::manifest_key(repo, reference);
    let key_by_digest = crate::oci::pull::manifest::manifest_key(repo, &manifest_digest.to_wire());
    kv.put(tenant, &key_by_reference, body.clone())
        .await
        .map_err(OciAdapterError::Kv)?;
    kv.put(tenant, &key_by_digest, body.clone())
        .await
        .map_err(OciAdapterError::Kv)?;
    // Companion content-type slots.
    let ct_bytes = Bytes::from(validated_media_type.clone().into_bytes());
    kv.put(
        tenant,
        &crate::oci::pull::manifest::manifest_ct_key(repo, reference),
        ct_bytes.clone(),
    )
    .await
    .map_err(OciAdapterError::Kv)?;
    kv.put(
        tenant,
        &crate::oci::pull::manifest::manifest_ct_key(repo, &manifest_digest.to_wire()),
        ct_bytes,
    )
    .await
    .map_err(OciAdapterError::Kv)?;
    // Tag-list update — only if reference was a tag.
    if is_tag_reference(reference) {
        audit_emit(
            auditor,
            tenant,
            &OciAuditEvent::TagUpdate {
                repo,
                tag: reference,
                manifest_digest: &manifest_digest.to_wire(),
            },
            now_unix_ms,
        )?;
        update_tag_list(kv, tenant, repo, reference).await?;
    }
    let mut headers = HeaderMap::new();
    let location = format!("/v2/{repo}/manifests/{}", manifest_digest.to_wire());
    headers.insert(
        axum::http::header::LOCATION,
        location
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("location header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        manifest_digest
            .to_wire()
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("digest header parse")))?,
    );
    Ok((StatusCode::CREATED, headers, Body::empty()).into_response())
}

/// Read-modify-write the tag-list slot at `oci_tags:<repo>`.
///
/// Storage shape: JSON-serialized `{"name": "<repo>", "tags": ["..."]}`
/// per OCI Distribution Spec v1.1 §pagination (we don't paginate, so
/// the body is a single object). Idempotent: re-pushing an existing
/// tag does not duplicate.
async fn update_tag_list(
    kv: &dyn ManifestKvStore,
    tenant: &TenantId,
    repo: &str,
    new_tag: &str,
) -> Result<(), OciAdapterError> {
    let key = tag_list_key(repo);
    let existing = kv.get(tenant, &key).await.map_err(OciAdapterError::Kv)?;
    let mut tags: Vec<String> = match existing {
        Some(bytes) => {
            let v: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| OciAdapterError::Kv(format!("tag-list json: {e}")))?;
            v.get("tags")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        }
        None => Vec::new(),
    };
    if !tags.iter().any(|t| t == new_tag) {
        tags.push(new_tag.to_string());
    }
    tags.sort();
    let new_body = serde_json::json!({
        "name": repo,
        "tags": tags,
    });
    let bytes = serde_json::to_vec(&new_body)
        .map_err(|e| OciAdapterError::Kv(format!("tag-list serialize: {e}")))?;
    kv.put(tenant, &key, Bytes::from(bytes))
        .await
        .map_err(OciAdapterError::Kv)?;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn validates_oci_image_manifest() {
        let body = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "size": 0
            },
            "layers": []
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let (mt, _d) = validate(&bytes).expect("valid manifest");
        assert_eq!(mt, "application/vnd.oci.image.manifest.v1+json");
    }

    #[test]
    fn validates_oci_image_index() {
        let body = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                { "mediaType": "x", "digest": "sha256:00", "size": 0,
                  "platform": {"architecture": "amd64", "os": "linux"} }
            ]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let (mt, _d) = validate(&bytes).expect("valid index");
        assert_eq!(mt, "application/vnd.oci.image.index.v1+json");
    }

    #[test]
    fn rejects_schema_v1() {
        let body = serde_json::json!({ "schemaVersion": 1, "mediaType": "x" });
        let bytes = serde_json::to_vec(&body).unwrap();
        assert!(matches!(
            validate(&bytes),
            Err(OciAdapterError::ManifestInvalid(_))
        ));
    }

    #[test]
    fn rejects_unknown_media_type() {
        let body = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/x-evil",
            "config": {"digest": "sha256:00"},
            "layers": []
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        assert!(matches!(
            validate(&bytes),
            Err(OciAdapterError::ManifestInvalid(_))
        ));
    }

    #[test]
    fn is_tag_reference_distinguishes_digest() {
        assert!(is_tag_reference("latest"));
        assert!(is_tag_reference("v1.2.3"));
        assert!(!is_tag_reference("sha256:abc"));
    }
}
