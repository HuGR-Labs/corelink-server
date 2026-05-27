//! Pip adapter configuration.
//!
//! The config struct is `#[non_exhaustive]` and intentionally takes
//! `Arc<dyn …>` handles for every collaborator so the adapter can be
//! booted with either production or in-memory test wiring. Env-var
//! overrides follow the `HUGR_PIP_ADAPTER_*` naming convention (per
//! spec §4) and only cover the scalar tunables; the trait handles
//! must be constructed by the caller because they have no env-var
//! representation.

use std::net::SocketAddr;
use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use url::Url;

use crate::pip::ports::{CasStoreHandle, KvStoreHandle, TenantResolverHandle};

/// Default upstream PyPI base URL (`https://pypi.org`).
pub const DEFAULT_UPSTREAM_PYPI: &str = "https://pypi.org";

/// Default index TTL (5 minutes; spec §5).
pub const DEFAULT_INDEX_TTL_SECONDS: u64 = 300;

/// Default wheel size limit (1 GiB; covers the largest scientific
/// wheels such as `torch` + `nvidia-cudnn-cu*`). Spec §5.
pub const DEFAULT_WHEEL_SIZE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

/// Pip adapter configuration.
///
/// `#[non_exhaustive]` so we can extend the boot-time tunables (e.g.
/// per-region upstream pool, mTLS client cert for private mirrors)
/// without breaking call sites.
#[derive(Clone)]
#[non_exhaustive]
pub struct PipAdapterConfig {
    /// Socket address the axum router binds to.
    pub bind_addr: SocketAddr,
    /// Upstream PyPI base URL (default [`DEFAULT_UPSTREAM_PYPI`]).
    pub upstream_pypi: Url,
    /// TTL after which the cached PEP 691 index payload is refreshed
    /// from upstream (default [`DEFAULT_INDEX_TTL_SECONDS`]).
    pub index_ttl_seconds: u64,
    /// Maximum permitted wheel/sdist byte length (default
    /// [`DEFAULT_WHEEL_SIZE_LIMIT_BYTES`]).
    pub wheel_size_limit_bytes: u64,
    /// Prefer PEP 691 JSON index over PEP 503 HTML when the client
    /// accepts both (default `true`).
    pub prefer_json_index: bool,
    /// Content-addressable blob store handle.
    pub cas: CasStoreHandle,
    /// Tenant-scoped KV store handle for the PEP 691 JSON index
    /// cache.
    pub metadata_kv: KvStoreHandle,
    /// PAT → tenant resolver handle.
    pub tenant_resolver: TenantResolverHandle,
    /// Audit emitter handle. Every state-mutating CAS / KV write
    /// invokes [`AuditEmitter::emit`] BEFORE returning success
    /// (audit-fail-CLOSED contract).
    pub auditor: Arc<dyn AuditEmitter>,
}

impl PipAdapterConfig {
    /// Construct a new [`PipAdapterConfig`]. The struct is
    /// `#[non_exhaustive]`; this helper exists so external callers
    /// (integration tests + production boot wiring) can build values
    /// without depending on the internal field layout.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        bind_addr: SocketAddr,
        upstream_pypi: Url,
        index_ttl_seconds: u64,
        wheel_size_limit_bytes: u64,
        prefer_json_index: bool,
        cas: CasStoreHandle,
        metadata_kv: KvStoreHandle,
        tenant_resolver: TenantResolverHandle,
        auditor: Arc<dyn AuditEmitter>,
    ) -> Self {
        Self {
            bind_addr,
            upstream_pypi,
            index_ttl_seconds,
            wheel_size_limit_bytes,
            prefer_json_index,
            cas,
            metadata_kv,
            tenant_resolver,
            auditor,
        }
    }
}

impl std::fmt::Debug for PipAdapterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipAdapterConfig")
            .field("bind_addr", &self.bind_addr)
            .field("upstream_pypi", &self.upstream_pypi.as_str())
            .field("index_ttl_seconds", &self.index_ttl_seconds)
            .field("wheel_size_limit_bytes", &self.wheel_size_limit_bytes)
            .field("prefer_json_index", &self.prefer_json_index)
            .field("cas", &"<Arc<dyn CasStore>>")
            .field("metadata_kv", &"<Arc<dyn KvStore>>")
            .field("tenant_resolver", &"<Arc<dyn TenantResolver>>")
            .field("auditor", &"<Arc<dyn AuditEmitter>>")
            .finish()
    }
}

/// Parse the upstream PyPI URL with a structured error on malformed
/// input. Public so callers can validate env-var overrides before
/// constructing the full [`PipAdapterConfig`].
///
/// # Errors
///
/// Returns the `url::ParseError` rendered as a `String` so the caller
/// does not need to import the `url` crate at the boot layer.
pub fn parse_upstream(url_str: &str) -> Result<Url, String> {
    Url::parse(url_str).map_err(|e| format!("invalid HUGR_PIP_ADAPTER_UPSTREAM `{url_str}`: {e}"))
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
        assert_eq!(DEFAULT_UPSTREAM_PYPI, "https://pypi.org");
        assert_eq!(DEFAULT_INDEX_TTL_SECONDS, 300);
        assert_eq!(DEFAULT_WHEEL_SIZE_LIMIT_BYTES, 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_upstream_accepts_default() {
        let parsed = parse_upstream(DEFAULT_UPSTREAM_PYPI);
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
