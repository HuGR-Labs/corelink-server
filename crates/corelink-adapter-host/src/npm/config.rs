//! npm adapter configuration.
//!
//! The config struct is `#[non_exhaustive]` and takes `Arc<dyn …>`
//! handles for every collaborator so the adapter can be booted with
//! either production or in-memory test wiring. Env-var overrides
//! follow the `HUGR_NPM_ADAPTER_*` naming convention and cover only
//! scalar tunables; the trait handles must be constructed by the
//! caller.

use std::net::SocketAddr;
use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use url::Url;

use crate::npm::ports::{CasStoreHandle, KvStoreHandle, TenantResolverHandle};

/// Default upstream npm registry URL.
pub const DEFAULT_UPSTREAM_REGISTRY: &str = "https://registry.npmjs.org";

/// Default metadata TTL (5 minutes; spec §3).
pub const DEFAULT_METADATA_TTL_SECONDS: u64 = 300;

/// Default tarball size limit (256 MiB; spec §5).
pub const DEFAULT_TARBALL_SIZE_LIMIT_BYTES: u64 = 256 * 1024 * 1024;

/// Maximum validated packument size (bytes) the adapter will attempt to WRITE
/// into the tenant metadata KV cache. Packuments larger than this are served
/// proxy-through (fetched + validated + returned to the client) but NOT cached.
///
/// Package metadata is a pure CACHE, so a write we cannot land must never break
/// `npm install`. The backing store is D1/CF-KV over HTTP, whose per-value size
/// limit a large packument (e.g. `react` ~6.8 MiB, `npm` ~25 MiB) exceeds — the
/// write then errors and, before this cap existed, that CACHE-WRITE failure was
/// mapped to a 503 that failed the READ. This is a deliberately CONSERVATIVE
/// 1 MiB cap: it sits comfortably ABOVE the largest packuments that cache fine
/// today (prod-verified: `express` ~805 KiB → 200/cached) and well BELOW the
/// observed backing-store failure threshold, so small/medium packuments still
/// cache + serve exactly as before while oversized ones skip the write proactively.
pub const DEFAULT_METADATA_CACHE_MAX_BYTES: usize = 1024 * 1024;

/// npm adapter configuration.
///
/// `#[non_exhaustive]` so we can extend boot-time tunables without
/// breaking call sites.
#[derive(Clone)]
#[non_exhaustive]
pub struct NpmAdapterConfig {
    /// Socket address the axum router binds to.
    pub bind_addr: SocketAddr,
    /// Upstream npm registry base URL (default
    /// [`DEFAULT_UPSTREAM_REGISTRY`]).
    pub upstream_registry: Url,
    /// TTL after which the cached metadata JSON is refreshed from
    /// upstream (default [`DEFAULT_METADATA_TTL_SECONDS`]).
    pub metadata_ttl_seconds: u64,
    /// Maximum permitted tarball byte length (default
    /// [`DEFAULT_TARBALL_SIZE_LIMIT_BYTES`]).
    pub tarball_size_limit_bytes: u64,
    /// Content-addressable blob store handle (tarballs).
    pub cas: CasStoreHandle,
    /// Tenant-scoped KV store handle for package metadata cache.
    pub metadata_kv: KvStoreHandle,
    /// PAT → tenant resolver handle.
    pub tenant_resolver: TenantResolverHandle,
    /// Audit emitter handle. Every state-mutating CAS / KV write
    /// invokes [`AuditEmitter::emit`] BEFORE returning success
    /// (audit-fail-CLOSED contract).
    pub auditor: Arc<dyn AuditEmitter>,
}

impl NpmAdapterConfig {
    /// Construct a new [`NpmAdapterConfig`].
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        bind_addr: SocketAddr,
        upstream_registry: Url,
        metadata_ttl_seconds: u64,
        tarball_size_limit_bytes: u64,
        cas: CasStoreHandle,
        metadata_kv: KvStoreHandle,
        tenant_resolver: TenantResolverHandle,
        auditor: Arc<dyn AuditEmitter>,
    ) -> Self {
        Self {
            bind_addr,
            upstream_registry,
            metadata_ttl_seconds,
            tarball_size_limit_bytes,
            cas,
            metadata_kv,
            tenant_resolver,
            auditor,
        }
    }
}

impl std::fmt::Debug for NpmAdapterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NpmAdapterConfig")
            .field("bind_addr", &self.bind_addr)
            .field("upstream_registry", &self.upstream_registry.as_str())
            .field("metadata_ttl_seconds", &self.metadata_ttl_seconds)
            .field("tarball_size_limit_bytes", &self.tarball_size_limit_bytes)
            .field("cas", &"<Arc<dyn CasStore>>")
            .field("metadata_kv", &"<Arc<dyn KvStore>>")
            .field("tenant_resolver", &"<Arc<dyn TenantResolver>>")
            .field("auditor", &"<Arc<dyn AuditEmitter>>")
            .finish()
    }
}

/// Parse the upstream registry URL with a structured error on
/// malformed input.
///
/// # Errors
///
/// Returns the `url::ParseError` rendered as a `String`.
pub fn parse_upstream(url_str: &str) -> Result<Url, String> {
    Url::parse(url_str).map_err(|e| format!("invalid HUGR_NPM_ADAPTER_UPSTREAM `{url_str}`: {e}"))
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
    fn defaults_match_spec() {
        assert_eq!(DEFAULT_UPSTREAM_REGISTRY, "https://registry.npmjs.org");
        assert_eq!(DEFAULT_METADATA_TTL_SECONDS, 300);
        assert_eq!(DEFAULT_TARBALL_SIZE_LIMIT_BYTES, 256 * 1024 * 1024);
        assert_eq!(DEFAULT_METADATA_CACHE_MAX_BYTES, 1024 * 1024);
        // The metadata-cache cap must sit BELOW the tarball limit (metadata is a
        // small cache surface, not a bulk artifact) and be non-trivial.
        assert!(
            (DEFAULT_METADATA_CACHE_MAX_BYTES as u64) < DEFAULT_TARBALL_SIZE_LIMIT_BYTES,
            "metadata cache cap must be well below the tarball limit"
        );
    }

    #[test]
    fn parse_upstream_accepts_default() {
        let parsed = parse_upstream(DEFAULT_UPSTREAM_REGISTRY);
        assert!(parsed.is_ok());
    }

    #[test]
    fn parse_upstream_rejects_garbage() {
        let parsed = parse_upstream("not a url");
        assert!(parsed.is_err());
        if let Err(msg) = parsed {
            assert!(msg.contains("not a url"));
        }
    }
}
