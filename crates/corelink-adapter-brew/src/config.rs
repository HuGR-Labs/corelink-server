//! Adapter configuration assembled by the binary entrypoint and
//! handed to [`crate::run_brew_adapter`].
//!
//! Environment-variable mapping (consumed by the binary that wires
//! this adapter into the production server):
//!
//! | Env var | Field | Default |
//! |---|---|---|
//! | `HUGR_BREW_ADAPTER_BIND_ADDR` | `bind_addr` | `0.0.0.0:8084` |
//! | `HUGR_BREW_ADAPTER_UPSTREAM_DOMAIN` | `upstream_domain` | `https://ghcr.io` |
//! | `HUGR_BREW_ADAPTER_BOTTLE_SIZE_LIMIT_BYTES` | `bottle_size_limit_bytes` | `2_147_483_648` (2 GiB) |

use std::net::SocketAddr;
use std::sync::Arc;

use url::Url;

use crate::ports::{SharedCasStore, SharedTenantResolver};

/// Default bottle size cap (2 GiB) — chosen to fit the largest
/// scientific-computing bottles (e.g. `tensorflow`, `gcc`) observed in
/// production. Per spec §5.
pub const DEFAULT_BOTTLE_SIZE_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// All runtime dependencies of the brew adapter.
///
/// Mirrors `BrewAdapterConfig` in `specs/_proposals/adapters/brew.md`
/// §5 exactly; `#[non_exhaustive]` so the binary's struct-literal
/// construction continues to compile when new fields are added at the
/// adapter boundary.
#[non_exhaustive]
pub struct BrewAdapterConfig {
    /// TCP listener bind address. Production: `0.0.0.0:8084`; tests
    /// typically use `127.0.0.1:0` to bind an ephemeral port.
    pub bind_addr: SocketAddr,

    /// Upstream bottle host. Defaults to GitHub Packages
    /// (`https://ghcr.io`). MUST be a parsed [`Url`] so trailing-slash
    /// / scheme / port quirks are normalized at construction time.
    pub upstream_domain: Url,

    /// Maximum allowed bottle size. Requests producing a Content-Length
    /// (or streamed byte total) above this limit fail with HTTP 413.
    pub bottle_size_limit_bytes: u64,

    /// Per-tenant CAS store.
    pub cas: SharedCasStore,

    /// PAT → tenant resolver.
    pub tenant_resolver: SharedTenantResolver,

    /// Audit chokepoint (sync trait per Wave-33 Stage 0 sub-step 4).
    pub auditor: Arc<dyn corelink_audit::ports::AuditEmitter>,
}

impl BrewAdapterConfig {
    /// Construct a fully wired [`BrewAdapterConfig`]. Provided so
    /// external callers (tests, the production binary) can build the
    /// struct despite `#[non_exhaustive]`.
    #[must_use]
    pub fn new(
        bind_addr: SocketAddr,
        upstream_domain: Url,
        bottle_size_limit_bytes: u64,
        cas: SharedCasStore,
        tenant_resolver: SharedTenantResolver,
        auditor: Arc<dyn corelink_audit::ports::AuditEmitter>,
    ) -> Self {
        Self {
            bind_addr,
            upstream_domain,
            bottle_size_limit_bytes,
            cas,
            tenant_resolver,
            auditor,
        }
    }
}

impl std::fmt::Debug for BrewAdapterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Trait objects can't auto-derive Debug; print only the
        // non-sensitive scalar fields plus the trait pointer type tag.
        f.debug_struct("BrewAdapterConfig")
            .field("bind_addr", &self.bind_addr)
            .field("upstream_domain", &self.upstream_domain.as_str())
            .field("bottle_size_limit_bytes", &self.bottle_size_limit_bytes)
            .field("cas", &"Arc<dyn CasStore>")
            .field("tenant_resolver", &"Arc<dyn TenantResolver>")
            .field("auditor", &"Arc<dyn AuditEmitter>")
            .finish()
    }
}
