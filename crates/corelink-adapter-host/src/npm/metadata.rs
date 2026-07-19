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
//!
//! Metadata is a pure CACHE, so a write we cannot land must NEVER break the
//! read: a packument larger than
//! [`crate::npm::config::DEFAULT_METADATA_CACHE_MAX_BYTES`] is served
//! proxy-through (skip the write proactively), and a KV backend outage on an
//! in-limit packument is logged + swallowed. Both cases still return the
//! validated upstream body (200) and emit `metadata.cache_skipped_oversized.v1`.
//! This is the deliberate opposite of the tarball CAS path, which stays
//! fail-CLOSED because content-addressed bytes are the product, not a cache.

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::types::tenant::TenantId;

use crate::npm::audit::{emit_npm_audit, event_types, now_unix_ms};
use crate::npm::config::DEFAULT_METADATA_CACHE_MAX_BYTES;
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
            let parsed = validate_metadata_json(&bytes)?;
            // rt-nuclear #7 self-heal: only serve a cached entry whose canonical
            // identity matches the requested key. A poisoned/aliased entry (name
            // != requested) falls through to a fresh upstream fetch — which the
            // refresh path re-binds correctly — instead of being served.
            if require_metadata_name_matches(&parsed, pkg).is_ok() {
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
    let parsed = validate_metadata_json(&raw)?;
    // rt-nuclear #7: the upstream fetch uses the RAW path `pkg` while the KV key
    // is normalized (trim+lowercase), so a case/trim/encoding alias could fetch
    // DIFFERENT registry content yet store it under a popular package's SHARED
    // `_public` key — cross-tenant metadata poisoning. Bind the stored content's
    // canonical identity to the requested key before caching (fail-CLOSED).
    require_metadata_name_matches(&parsed, pkg)?;

    // Package metadata is a pure CACHE — a write we cannot land must NEVER break
    // the READ (an `npm install`). This is the ONLY divergence from the tarball
    // path, whose CAS write stays fail-CLOSED because content-addressed bytes are
    // the product, not a cache. Two guards, both proxy-through (200 + body):
    //
    //   (1) PROACTIVE size-skip — a validated packument larger than
    //       DEFAULT_METADATA_CACHE_MAX_BYTES exceeds the D1/CF-KV per-value limit,
    //       so we skip the write we know would fail (react ~6.8 MiB, npm ~25 MiB).
    //   (2) SWALLOW-on-error — an in-limit packument still writes, but a KV
    //       backend outage is logged + swallowed, not surfaced as a 503.
    //
    // Both emit `metadata.cache_skipped_oversized.v1` (best-effort — this branch
    // performs NO durable mutation, so the audit-fail-CLOSED contract that binds
    // the tarball/refresh WRITE does not apply; a down audit sink must not break
    // `npm install` either). Small/medium packuments keep the exact prior
    // behavior: `metadata.refreshed.v1` audit (fail-CLOSED) BEFORE the KV write.
    if !metadata_cache_eligible(raw.len()) {
        emit_metadata_cache_skip_best_effort(auditor, pkg, tenant, now, raw.len(), "oversized");
        return Ok(MetadataResponse {
            body: raw,
            pkg: pkg.to_owned(),
        });
    }

    // Audit BEFORE the KV write (audit-fail-CLOSED contract) — unchanged.
    emit_npm_audit(
        auditor,
        event_types::METADATA_REFRESHED,
        tenant,
        now,
        serde_json::json!({ "pkg": pkg }),
    )?;
    if let Err(cache_err) = kv.put(tenant, &kv_key_for_pkg(pkg), raw.clone(), now).await {
        // Cache-write failure is NON-FATAL on the metadata path: log the real
        // backend detail server-side, record a best-effort skip audit, and STILL
        // return the validated upstream body (proxy-through).
        tracing::warn!(
            pkg = %pkg,
            detail = %cache_err,
            "npm: metadata KV cache write failed; serving proxy-through (non-fatal)"
        );
        emit_metadata_cache_skip_best_effort(auditor, pkg, tenant, now, raw.len(), "kv_error");
    }
    Ok(MetadataResponse {
        body: raw,
        pkg: pkg.to_owned(),
    })
}

/// Whether a validated packument of `body_len` bytes is small enough to WRITE
/// into the tenant metadata KV cache without exceeding the backing-store value
/// limit. Larger packuments are served proxy-through (never cached).
///
/// Pure so the size-skip boundary is unit-testable without any I/O.
#[must_use]
pub fn metadata_cache_eligible(body_len: usize) -> bool {
    body_len <= DEFAULT_METADATA_CACHE_MAX_BYTES
}

/// Emit the `metadata.cache_skipped_oversized.v1` audit BEST-EFFORT: on the
/// metadata proxy-through path there is no durable mutation to pair the audit
/// with, so an emit failure is logged and swallowed rather than propagated — a
/// down audit sink must not turn a cacheable-miss into a failed `npm install`.
fn emit_metadata_cache_skip_best_effort(
    auditor: &Arc<dyn AuditEmitter>,
    pkg: &str,
    tenant: &TenantId,
    now: u64,
    body_len: usize,
    reason: &str,
) {
    if let Err(audit_err) = emit_npm_audit(
        auditor,
        event_types::METADATA_CACHE_SKIPPED_OVERSIZED,
        tenant,
        now,
        serde_json::json!({ "pkg": pkg, "body_len": body_len, "reason": reason }),
    ) {
        tracing::warn!(
            pkg = %pkg,
            detail = %audit_err,
            "npm: metadata cache-skip audit emit failed (non-fatal; body still served)"
        );
    }
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

/// Require that the upstream metadata's canonical top-level `name` matches the
/// requested package (both [`normalise_pkg_name`]-normalized).
///
/// The registry — not us — owns a package's canonical identity, so binding the
/// stored content to the requested key collapses case/trim/encoding aliasing
/// that would otherwise let one upstream identity be cached under another
/// package's SHARED `_public` KV key (rt-nuclear #7 cross-tenant poisoning).
///
/// # Errors
///
/// [`NpmAdapterError::MetadataNameMismatch`] when the names differ — including a
/// missing/blank `name` (fail-CLOSED).
fn require_metadata_name_matches(
    parsed: &serde_json::Value,
    pkg: &str,
) -> Result<(), NpmAdapterError> {
    let fetched = normalise_pkg_name(
        parsed
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default(),
    );
    let requested = normalise_pkg_name(pkg);
    if fetched != requested {
        return Err(NpmAdapterError::MetadataNameMismatch { requested, fetched });
    }
    Ok(())
}

/// Extract `dist.shasum` for a specific version from npm metadata
/// JSON. The `dist.shasum` is a SHA1 hex string published by the npm
/// registry for every tarball.
///
/// # Errors
///
/// Returns [`NpmAdapterError::MetadataParse`] if the version or
/// `dist.shasum` field is missing or malformed.
pub fn extract_shasum(
    metadata: &serde_json::Value,
    version: &str,
) -> Result<String, NpmAdapterError> {
    let shasum = metadata
        .get("versions")
        .and_then(|v| v.get(version))
        .and_then(|v| v.get("dist"))
        .and_then(|d| d.get("shasum"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| {
            NpmAdapterError::MetadataParse(format!("missing dist.shasum for version {version}"))
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
    fn require_metadata_name_matches_binds_identity_to_key() {
        let json = |name: &str| serde_json::json!({ "name": name, "versions": {} });
        // Exact + case/trim-normalized match → Ok (the realistic registry case:
        // a case-variant request returns the canonical package).
        assert!(require_metadata_name_matches(&json("lodash"), "lodash").is_ok());
        assert!(require_metadata_name_matches(&json("lodash"), "LoDash").is_ok());
        assert!(require_metadata_name_matches(&json("lodash"), "  lodash ").is_ok());
        // rt-nuclear #7: upstream returned content for a DIFFERENT identity than
        // the requested (normalized) key → REJECTED (would otherwise poison the
        // shared `_public` key with another package's metadata).
        let err = require_metadata_name_matches(&json("evil-pkg"), "lodash").unwrap_err();
        assert!(
            matches!(err, NpmAdapterError::MetadataNameMismatch { .. }),
            "{err:?}"
        );
        assert_eq!(err.status_code(), 502);
        // Missing / blank `name` is fail-CLOSED.
        assert!(require_metadata_name_matches(&serde_json::json!({}), "lodash").is_err());
        assert!(require_metadata_name_matches(&json(""), "lodash").is_err());
    }

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

    // ── oversized-packument proxy-through (metadata cache is non-fatal) ────────

    #[test]
    fn metadata_cache_eligible_boundary() {
        // At/under the cap → cacheable; one byte over → skipped (proxy-through).
        assert!(metadata_cache_eligible(0));
        assert!(metadata_cache_eligible(
            DEFAULT_METADATA_CACHE_MAX_BYTES - 1
        ));
        assert!(metadata_cache_eligible(DEFAULT_METADATA_CACHE_MAX_BYTES));
        assert!(!metadata_cache_eligible(
            DEFAULT_METADATA_CACHE_MAX_BYTES + 1
        ));
    }

    use std::sync::Mutex;

    use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};

    use crate::npm::ports::KvStore;

    /// KV fake that records every `put` and always succeeds. `get` always
    /// misses so `serve_metadata` takes the refresh path.
    #[derive(Debug, Default)]
    struct RecordingKv {
        puts: Mutex<Vec<(String, usize)>>,
    }

    #[async_trait::async_trait]
    impl KvStore for RecordingKv {
        async fn get(
            &self,
            _tenant: &TenantId,
            _key: &str,
        ) -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError> {
            Ok(None)
        }
        async fn put(
            &self,
            _tenant: &TenantId,
            key: &str,
            value: Vec<u8>,
            _inserted_at_unix_ms: u64,
        ) -> Result<(), NpmAdapterError> {
            self.puts
                .lock()
                .unwrap()
                .push((key.to_owned(), value.len()));
            Ok(())
        }
    }

    /// KV fake that simulates a backend OUTAGE: `get` misses, `put` errors.
    #[derive(Debug, Default)]
    struct FailingKv;

    #[async_trait::async_trait]
    impl KvStore for FailingKv {
        async fn get(
            &self,
            _tenant: &TenantId,
            _key: &str,
        ) -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError> {
            Ok(None)
        }
        async fn put(
            &self,
            _tenant: &TenantId,
            _key: &str,
            _value: Vec<u8>,
            _inserted_at_unix_ms: u64,
        ) -> Result<(), NpmAdapterError> {
            Err(NpmAdapterError::Kv("simulated KV outage".into()))
        }
    }

    /// Build a valid packument (JSON object, correct top-level `name`) padded to
    /// AT LEAST `min_len` bytes so it crosses the cache cap deterministically.
    fn packument_at_least(name: &str, min_len: usize) -> Vec<u8> {
        let head = format!("{{\"name\":\"{name}\",\"versions\":{{}},\"_pad\":\"");
        let tail = "\"}";
        let pad_needed = min_len.saturating_sub(head.len() + tail.len());
        let body = format!("{head}{}{tail}", "a".repeat(pad_needed));
        body.into_bytes()
    }

    async fn serve_via_mock(
        pkg: &str,
        body: Vec<u8>,
        kv: KvStoreHandle,
        sink: &InMemoryAuditEmitter,
    ) -> Result<MetadataResponse, NpmAdapterError> {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/{pkg}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&upstream)
            .await;

        let base = url::Url::parse(&upstream.uri()).unwrap();
        let client = UpstreamClient::new(base).unwrap();
        let auditor: Arc<dyn AuditEmitter> = Arc::new(sink.clone());
        let tenant = TenantId::from_uuid(uuid::Uuid::now_v7());
        serve_metadata(pkg, 300, &tenant, &kv, &client, &auditor).await
    }

    fn skip_events(sink: &InMemoryAuditEmitter) -> usize {
        sink.snapshot()
            .iter()
            .filter(|e| e.event_type == event_types::METADATA_CACHE_SKIPPED_OVERSIZED)
            .count()
    }

    fn refreshed_events(sink: &InMemoryAuditEmitter) -> usize {
        sink.snapshot()
            .iter()
            .filter(|e| e.event_type == event_types::METADATA_REFRESHED)
            .count()
    }

    #[tokio::test]
    async fn oversized_packument_is_served_proxy_through_not_503() {
        // A packument BIGGER than the cache cap must return 200 + the exact body
        // (proxy-through), never a 503 — and the KV write is skipped proactively.
        let body = packument_at_least("react", DEFAULT_METADATA_CACHE_MAX_BYTES + 4096);
        assert!(body.len() > DEFAULT_METADATA_CACHE_MAX_BYTES);
        let kv = Arc::new(RecordingKv::default());
        let sink = InMemoryAuditEmitter::default();
        let kv_handle: KvStoreHandle = kv.clone();
        let resp = serve_via_mock("react", body.clone(), kv_handle, &sink)
            .await
            .expect("oversized packument must be SERVED, not 503");
        assert_eq!(
            resp.body, body,
            "client must receive the full upstream body"
        );
        assert_eq!(resp.pkg, "react");
        // Proactive skip: no write attempted, skip audit fired, NO `refreshed`.
        assert!(
            kv.puts.lock().unwrap().is_empty(),
            "oversized write must be skipped"
        );
        assert_eq!(skip_events(&sink), 1, "cache-skip audit must fire");
        assert_eq!(refreshed_events(&sink), 0);
    }

    #[tokio::test]
    async fn kv_outage_on_normal_packument_degrades_to_proxy_through() {
        // A NORMAL-size packument whose KV write ERRORS (backend outage) must
        // still return 200 + body (swallow-on-error), not 503.
        let body = br#"{"name":"is-odd","versions":{}}"#.to_vec();
        let kv: KvStoreHandle = Arc::new(FailingKv);
        let sink = InMemoryAuditEmitter::default();
        let resp = serve_via_mock("is-odd", body.clone(), kv, &sink)
            .await
            .expect("KV outage must degrade to proxy-through, not 503");
        assert_eq!(resp.body, body);
        // In-limit: `refreshed` fired (fail-closed, before the write) AND the
        // swallowed error emitted a best-effort skip audit.
        assert_eq!(refreshed_events(&sink), 1);
        assert_eq!(
            skip_events(&sink),
            1,
            "swallowed KV error must emit skip audit"
        );
    }

    #[tokio::test]
    async fn small_packument_is_cached_and_refreshed_audit_fires() {
        // Existing behavior preserved: an in-limit packument caches (kv.put
        // called) + serves, with the `refreshed` audit and NO skip event.
        let body = br#"{"name":"is-odd","versions":{}}"#.to_vec();
        let kv = Arc::new(RecordingKv::default());
        let sink = InMemoryAuditEmitter::default();
        let kv_handle: KvStoreHandle = kv.clone();
        let resp = serve_via_mock("is-odd", body.clone(), kv_handle, &sink)
            .await
            .expect("small packument must be served");
        assert_eq!(resp.body, body);
        let puts = kv.puts.lock().unwrap();
        assert_eq!(puts.len(), 1, "small packument must be cached");
        assert_eq!(puts[0].0, kv_key_for_pkg("is-odd"));
        assert_eq!(puts[0].1, body.len());
        drop(puts);
        assert_eq!(refreshed_events(&sink), 1);
        assert_eq!(skip_events(&sink), 0);
    }
}
