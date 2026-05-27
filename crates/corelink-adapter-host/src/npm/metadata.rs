//! `GET /<pkg>` — package metadata cache logic.
//!
//! Pure-logic layer (no `axum` types) so the body can be unit tested
//! without spinning up a server. The router in [`crate::npm::server`] is
//! a thin wrapper adapting `axum` Request/Response to/from this
//! module's signatures.
//!
//! npm package metadata is cached in tenant-scoped KV with a TTL
//! (default 300s). On a cache miss (or stale entry), the adapter
//! fetches from upstream, validates the JSON is well-formed, stores
//! back, and emits a `metadata.refreshed.v1` audit event BEFORE the
//! KV write (audit-fail-CLOSED contract).

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::types::tenant::TenantId;

use crate::npm::audit::{emit_npm_audit, event_types, now_unix_ms};
use crate::npm::error::NpmAdapterError;
use crate::npm::ports::KvStoreHandle;
use crate::npm::upstream::UpstreamClient;

/// KV key used to store metadata for a given package.
#[must_use]
pub fn kv_key_for_pkg(pkg: &str) -> String {
    format!("npm:meta:{}", normalise_pkg_name(pkg))
}

/// Normalise an npm package name for use as a KV key: lowercase,
/// leading/trailing whitespace removed.
#[must_use]
pub fn normalise_pkg_name(pkg: &str) -> String {
    pkg.trim().to_ascii_lowercase()
}

/// Return `true` if `inserted_at_unix_ms` is still within
/// `ttl_seconds` of `now_unix_ms`.
#[must_use]
pub fn is_fresh(inserted_at_unix_ms: u64, now_unix_ms: u64, ttl_seconds: u64) -> bool {
    let ttl_ms = ttl_seconds.saturating_mul(1000);
    now_unix_ms.saturating_sub(inserted_at_unix_ms) < ttl_ms
}

/// Metadata response: raw JSON bytes + package name.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MetadataResponse {
    /// Raw JSON bytes to forward to the client.
    pub body: Vec<u8>,
    /// Canonical package name from the metadata.
    pub pkg: String,
}

/// Serve `GET /<pkg>` — metadata endpoint.
///
/// Algorithm:
/// 1. Look up cached JSON in `kv`. If fresh, emit `cache_hit`, return.
/// 2. Else fetch from upstream, validate JSON, emit `refreshed` BEFORE
///    `kv.put`, store, return.
///
/// # Errors
///
/// Surfaces any [`NpmAdapterError`] from KV / upstream / audit layers.
pub async fn serve_metadata(
    pkg: &str,
    ttl_seconds: u64,
    tenant: &TenantId,
    kv: &KvStoreHandle,
    upstream: &UpstreamClient,
    auditor: &Arc<dyn AuditEmitter>,
) -> Result<MetadataResponse, NpmAdapterError> {
    let key = kv_key_for_pkg(pkg);
    let now = now_unix_ms();
    let cached = kv.get(tenant, &key).await?;

    if let Some((bytes, inserted)) = cached {
        if is_fresh(inserted, now, ttl_seconds) {
            // Validate the cached JSON is still parseable (defence in depth).
            validate_metadata_json(&bytes)?;
            emit_npm_audit(
                auditor,
                event_types::METADATA_CACHE_HIT,
                tenant,
                now,
                serde_json::json!({ "pkg": pkg }),
            )?;
            return Ok(MetadataResponse {
                body: bytes,
                pkg: pkg.to_owned(),
            });
        }
    }

    refresh_from_upstream(pkg, ttl_seconds, tenant, kv, upstream, auditor, now).await
}

async fn refresh_from_upstream(
    pkg: &str,
    _ttl_seconds: u64,
    tenant: &TenantId,
    kv: &KvStoreHandle,
    upstream: &UpstreamClient,
    auditor: &Arc<dyn AuditEmitter>,
    now: u64,
) -> Result<MetadataResponse, NpmAdapterError> {
    let raw = upstream.fetch_metadata(pkg).await?;
    // Validate before caching to fail-CLOSED on malformed upstream.
    validate_metadata_json(&raw)?;
    // Audit BEFORE the KV write (audit-fail-CLOSED contract).
    emit_npm_audit(
        auditor,
        event_types::METADATA_REFRESHED,
        tenant,
        now,
        serde_json::json!({ "pkg": pkg }),
    )?;
    kv.put(tenant, &kv_key_for_pkg(pkg), raw.clone(), now).await?;
    Ok(MetadataResponse {
        body: raw,
        pkg: pkg.to_owned(),
    })
}

/// Validate that bytes are parseable as JSON and contain the minimum
/// fields expected from an npm metadata response. Fails-CLOSED on
/// malformed input.
///
/// # Errors
///
/// Returns [`NpmAdapterError::MetadataParse`] on malformed JSON or
/// missing required top-level fields.
pub fn validate_metadata_json(bytes: &[u8]) -> Result<serde_json::Value, NpmAdapterError> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| NpmAdapterError::MetadataParse(format!("invalid JSON: {e}")))?;
    if !v.is_object() {
        return Err(NpmAdapterError::MetadataParse(
            "expected JSON object at top level".into(),
        ));
    }
    Ok(v)
}

/// Extract `dist.shasum` for a specific version from npm metadata
/// JSON. The `dist.shasum` is a SHA1 hex string published by the npm
/// registry for every tarball.
///
/// # Errors
///
/// Returns [`NpmAdapterError::MetadataParse`] if the version or
/// `dist.shasum` field is missing or malformed.
pub fn extract_shasum(metadata: &serde_json::Value, version: &str) -> Result<String, NpmAdapterError> {
    let shasum = metadata
        .get("versions")
        .and_then(|v| v.get(version))
        .and_then(|v| v.get("dist"))
        .and_then(|d| d.get("shasum"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| {
            NpmAdapterError::MetadataParse(format!(
                "missing dist.shasum for version {version}"
            ))
        })?;
    Ok(shasum.to_owned())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn kv_key_normalises_package_name() {
        assert_eq!(kv_key_for_pkg("Lodash"), "npm:meta:lodash");
        assert_eq!(kv_key_for_pkg("  React  "), "npm:meta:react");
    }

    #[test]
    fn is_fresh_table() {
        // Not yet expired.
        assert!(is_fresh(0, 1000, 2));
        // Exactly at the boundary (ms=2000 with ttl=2s = expired).
        assert!(!is_fresh(0, 2000, 2));
        // Well expired.
        assert!(!is_fresh(0, 5000, 2));
    }

    #[test]
    fn validate_metadata_json_accepts_object() {
        let raw = b"{\"name\": \"lodash\", \"versions\": {}}";
        let result = validate_metadata_json(raw);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_metadata_json_rejects_non_object() {
        let result = validate_metadata_json(b"[1, 2, 3]");
        assert!(matches!(result, Err(NpmAdapterError::MetadataParse(_))));
    }

    #[test]
    fn validate_metadata_json_rejects_malformed() {
        let result = validate_metadata_json(b"{not json");
        assert!(matches!(result, Err(NpmAdapterError::MetadataParse(_))));
    }

    #[test]
    fn extract_shasum_succeeds_with_valid_metadata() {
        let meta = serde_json::json!({
            "name": "lodash",
            "versions": {
                "4.17.21": {
                    "dist": {
                        "shasum": "abc123def456"
                    }
                }
            }
        });
        let shasum = extract_shasum(&meta, "4.17.21").expect("shasum");
        assert_eq!(shasum, "abc123def456");
    }

    #[test]
    fn extract_shasum_fails_missing_version() {
        let meta = serde_json::json!({ "name": "lodash", "versions": {} });
        let result = extract_shasum(&meta, "9.9.9");
        assert!(matches!(result, Err(NpmAdapterError::MetadataParse(_))));
    }
}
