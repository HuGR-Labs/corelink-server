//! Adapter configuration assembled by the binary entrypoint and
//! handed to [`crate::cargo::run_cargo_adapter`].
//!
//! Environment-variable mapping (consumed by the binary that wires
//! this adapter into the production server):
//!
//! | Env var | Field | Default |
//! |---|---|---|
//! | `HUGR_CARGO_ADAPTER_BIND_ADDR` | `bind_addr` | `0.0.0.0:8085` |
//! | `HUGR_CARGO_ADAPTER_BODY_SIZE_LIMIT_BYTES` | `body_size_limit_bytes` | `16_777_216` (16 MiB) |

use std::net::SocketAddr;
use std::sync::Arc;

use crate::cargo::ports::{SharedCasStore, SharedTenantResolver};

/// Default PUT body cap (16 MiB) — per spec §8 adversarial row 2.
/// sccache build artifacts are typically 1-8 MiB; 16 MiB provides
/// ample headroom for large LTO artifacts while bounding DoS surface.
pub const DEFAULT_BODY_SIZE_LIMIT_BYTES: u64 = 16 * 1024 * 1024;

/// All runtime dependencies of the cargo adapter.
///
/// `#[non_exhaustive]` so the binary's struct-literal construction
/// continues to compile when new fields are added at the adapter
/// boundary.
#[non_exhaustive]
pub struct CargoAdapterConfig {
    /// TCP listener bind address. Production: `0.0.0.0:8085`; tests
    /// typically use `127.0.0.1:0` to bind an ephemeral port.
    pub bind_addr: SocketAddr,

    /// Maximum allowed PUT body size. Requests producing a body above
    /// this limit fail with HTTP 413.
    pub body_size_limit_bytes: u64,

    /// Per-tenant CAS store.
    pub cas: SharedCasStore,

    /// PAT → tenant resolver.
    pub tenant_resolver: SharedTenantResolver,

    /// Audit chokepoint (sync trait per Wave-33 Stage 0 sub-step 4).
    pub auditor: Arc<dyn corelink_audit::ports::AuditEmitter>,
}

impl CargoAdapterConfig {
    /// Construct a fully wired [`CargoAdapterConfig`]. Provided so
    /// external callers (tests, the production binary) can build the
    /// struct despite `#[non_exhaustive]`.
    #[must_use]
    pub fn new(
        bind_addr: SocketAddr,
        body_size_limit_bytes: u64,
        cas: SharedCasStore,
        tenant_resolver: SharedTenantResolver,
        auditor: Arc<dyn corelink_audit::ports::AuditEmitter>,
    ) -> Self {
        Self {
            bind_addr,
            body_size_limit_bytes,
            cas,
            tenant_resolver,
            auditor,
        }
    }
}

impl std::fmt::Debug for CargoAdapterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CargoAdapterConfig")
            .field("bind_addr", &self.bind_addr)
            .field("body_size_limit_bytes", &self.body_size_limit_bytes)
            .field("cas", &"Arc<dyn CasStore>")
            .field("tenant_resolver", &"Arc<dyn TenantResolver>")
            .field("auditor", &"Arc<dyn AuditEmitter>")
            .finish()
    }
}
