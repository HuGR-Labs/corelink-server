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
use crate::oci::ports::{ManifestKvStore, ManifestResolver, ResolvedManifest};

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

/// Default media type served when the companion content-type slot is
/// absent (older pushes from before the CT slot was added — defensive;
/// all NEW pushes write CT explicitly).
const DEFAULT_MANIFEST_MEDIA_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";

/// Build the `GET` response headers + body for a served manifest from its
/// content-type, canonical digest wire form, and bytes. Shared by the KV
/// hit and the upstream-on-miss resolution paths so both emit an identical
/// header shape.
fn manifest_get_response(
    ct_str: &str,
    digest_wire: &str,
    body: axum::body::Bytes,
) -> Result<axum::response::Response, OciAdapterError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        ct_str
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("ct header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        digest_wire
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("digest header parse")))?,
    );
    Ok((StatusCode::OK, headers, Body::from(body)).into_response())
}

/// Build the `HEAD` response (same headers as [`manifest_get_response`]
/// plus `Content-Length`, no body). Shared by the KV hit and the
/// upstream-on-miss paths.
fn manifest_head_response(
    ct_str: &str,
    digest_wire: &str,
    body_len: usize,
) -> Result<axum::response::Response, OciAdapterError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        ct_str
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("ct header parse")))?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        body_len
            .to_string()
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("len header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        digest_wire
            .parse()
            .map_err(|_| OciAdapterError::Kv(String::from("digest header parse")))?,
    );
    Ok((StatusCode::OK, headers, Body::empty()).into_response())
}

/// Fetch the companion content-type slot for a stored manifest, defaulting
/// to the canonical OCI v1 media type when absent / non-UTF-8.
async fn stored_content_type(
    kv: &dyn ManifestKvStore,
    tenant: &TenantId,
    repo: &str,
    reference: &str,
) -> Result<String, OciAdapterError> {
    let ct_key = manifest_ct_key(repo, reference);
    let ct_bytes = kv.get(tenant, &ct_key).await.map_err(OciAdapterError::Kv)?;
    Ok(match ct_bytes {
        Some(b) => String::from_utf8(b.to_vec())
            .unwrap_or_else(|_| String::from(DEFAULT_MANIFEST_MEDIA_TYPE)),
        None => String::from(DEFAULT_MANIFEST_MEDIA_TYPE),
    })
}

/// Resolve a per-tenant KV manifest miss through the upstream-on-miss
/// `resolver` (M1, `OCI_UPSTREAM_ON_MISS`). Returns the resolved manifest
/// when the flag is ON, a resolver is wired, and the upstream serves +
/// verifies; `None` (⇒ the caller 404s) when no resolver is wired or the
/// resolver fails open. Errors only propagate a genuine internal
/// (`PortResult`) fault.
async fn resolve_miss(
    resolver: Option<&dyn ManifestResolver>,
    tenant: &TenantId,
    repo: &str,
    reference: &str,
) -> Result<Option<ResolvedManifest>, OciAdapterError> {
    match resolver {
        Some(r) => r
            .resolve_on_miss(tenant, repo, reference)
            .await
            .map_err(OciAdapterError::Kv),
        None => Ok(None),
    }
}

/// `GET /v2/<repo>/manifests/<reference>`.
pub async fn get(
    kv: &dyn ManifestKvStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    reference: &str,
    resolver: Option<&dyn ManifestResolver>,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_pull(repo, scope)?;
    let key = manifest_key(repo, reference);
    match kv.get(tenant, &key).await.map_err(OciAdapterError::Kv)? {
        Some(body) => {
            let ct_str = stored_content_type(kv, tenant, repo, reference).await?;
            let manifest_digest = crate::oci::digest::OciDigest::compute(
                crate::oci::digest::OciDigestAlgo::Sha256,
                &body,
            )?;
            manifest_get_response(&ct_str, &manifest_digest.to_wire(), body)
        }
        // Per-tenant KV miss: on-miss upstream resolution (M1) if wired,
        // else the historical 404 (buildkit fails open to upstream).
        None => match resolve_miss(resolver, tenant, repo, reference).await? {
            Some(rm) => manifest_get_response(&rm.content_type, &rm.digest, rm.bytes),
            None => Err(OciAdapterError::NotFound),
        },
    }
}

/// `HEAD /v2/<repo>/manifests/<reference>`.
pub async fn head(
    kv: &dyn ManifestKvStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    reference: &str,
    resolver: Option<&dyn ManifestResolver>,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_pull(repo, scope)?;
    let key = manifest_key(repo, reference);
    // A HEAD must carry the SAME Content-Type + Content-Length as the GET
    // (just no body). Without the explicit Content-Length the empty body
    // yields `Content-Length: 0`, and docker pull rejects the descriptor
    // with "content size of zero: invalid argument" — so tag resolution
    // (HEAD manifests/<tag>) breaks every pull.
    match kv.get(tenant, &key).await.map_err(OciAdapterError::Kv)? {
        Some(body) => {
            let ct_str = stored_content_type(kv, tenant, repo, reference).await?;
            let manifest_digest = crate::oci::digest::OciDigest::compute(
                crate::oci::digest::OciDigestAlgo::Sha256,
                &body,
            )?;
            manifest_head_response(&ct_str, &manifest_digest.to_wire(), body.len())
        }
        None => match resolve_miss(resolver, tenant, repo, reference).await? {
            Some(rm) => manifest_head_response(&rm.content_type, &rm.digest, rm.bytes.len()),
            None => Err(OciAdapterError::NotFound),
        },
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::http::StatusCode;
    use bytes::Bytes;

    use corelink_core::TenantId;

    use crate::oci::auth::OciScope;
    use crate::oci::ports::testing::InMemoryKv;
    use crate::oci::ports::{ManifestKvStore, PortResult};

    use super::*;

    const REPO: &str = "library/alpine";

    fn pull_scope() -> OciScope {
        OciScope::new(REPO, vec![String::from("pull")])
    }

    fn tenant() -> TenantId {
        TenantId::from_uuid(uuid::Uuid::nil())
    }

    /// A resolver that returns a canned manifest and counts its calls.
    #[derive(Debug, Default)]
    struct CannedResolver {
        bytes: Vec<u8>,
        content_type: String,
        digest: String,
        calls: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl ManifestResolver for CannedResolver {
        async fn resolve_on_miss(
            &self,
            _tenant: &TenantId,
            _repo: &str,
            _reference: &str,
        ) -> PortResult<Option<ResolvedManifest>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ResolvedManifest {
                bytes: Bytes::from(self.bytes.clone()),
                content_type: self.content_type.clone(),
                digest: self.digest.clone(),
            }))
        }
    }

    #[tokio::test]
    async fn get_miss_with_no_resolver_is_404_exactly_as_today() {
        // Flag OFF (resolver `None`): a KV miss 404s, and the resolver is never
        // consulted — byte-identical to the pre-M1 behavior.
        let kv = InMemoryKv::default();
        let err = get(&kv, &tenant(), &pull_scope(), REPO, "3.20", None)
            .await
            .expect_err("a KV miss with no resolver must 404");
        assert!(matches!(err, OciAdapterError::NotFound));
    }

    #[tokio::test]
    async fn head_miss_with_no_resolver_is_404_exactly_as_today() {
        let kv = InMemoryKv::default();
        let err = head(&kv, &tenant(), &pull_scope(), REPO, "3.20", None)
            .await
            .expect_err("a KV HEAD miss with no resolver must 404");
        assert!(matches!(err, OciAdapterError::NotFound));
    }

    #[tokio::test]
    async fn get_miss_with_resolver_serves_the_resolved_manifest() {
        // Flag ON (resolver `Some`): a KV miss is resolved on-miss and served
        // with the resolver's content-type + digest.
        let kv = InMemoryKv::default();
        let resolver = CannedResolver {
            bytes: b"resolved-manifest-bytes".to_vec(),
            content_type: String::from("application/vnd.oci.image.index.v1+json"),
            digest: String::from("sha256:deadbeef"),
            calls: AtomicUsize::new(0),
        };
        let resp = get(&kv, &tenant(), &pull_scope(), REPO, "3.20", Some(&resolver))
            .await
            .expect("resolver serves the manifest");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("Docker-Content-Digest").unwrap(),
            "sha256:deadbeef"
        );
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/vnd.oci.image.index.v1+json"
        );
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn get_hit_does_not_consult_the_resolver() {
        // A KV HIT is served straight from KV; the resolver is never called
        // (the on-miss path is strictly a miss-only fallback).
        let kv = InMemoryKv::default();
        let body = Bytes::from_static(b"stored-manifest");
        kv.put(&tenant(), &manifest_key(REPO, "3.20"), body.clone())
            .await
            .unwrap();
        let resolver = CannedResolver::default();
        let resp = get(&kv, &tenant(), &pull_scope(), REPO, "3.20", Some(&resolver))
            .await
            .expect("KV hit serves");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    }
}
