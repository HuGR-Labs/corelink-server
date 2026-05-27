//! Absorbed `corelink-adapter-oci` — OCI Distribution Spec v1.1 registry.
//!
//! Wave-35 Phase 2 absorbed the former `corelink-adapter-oci` crate into
//! this module. CoreLink becomes a full OCI Distribution Spec v1.1
//! registry: any standard OCI client (`docker`, `podman`, `buildah`,
//! `containerd`, `crane`, Kubernetes' image pull machinery, BuildKit
//! cache, Helm OCI artifacts) speaks to it as a vanilla registry.
//! Manifests (JSON) live in KV; blobs (layers + configs) live in CAS.
//!
//! ## Wire surface (per `specs/_proposals/adapters/oci.md` §2)
//!
//! ```text
//! GET    /v2/                              → liveness + auth probe
//! GET    /v2/_catalog                      → 401 (disabled by default)
//! GET    /token?service=...&scope=...      → PAT → bearer-token exchange
//! GET    /v2/<repo>/blobs/<digest>         → pull blob
//! HEAD   /v2/<repo>/blobs/<digest>         → head blob
//! POST   /v2/<repo>/blobs/uploads/         → open upload session
//! PATCH  /v2/<repo>/blobs/uploads/<uuid>   → append chunk
//! PUT    /v2/<repo>/blobs/uploads/<uuid>?digest=X
//!                                          → finalize blob (declared-digest verify)
//! GET    /v2/<repo>/manifests/<reference>  → pull manifest (tag or digest)
//! HEAD   /v2/<repo>/manifests/<reference>  → head manifest
//! PUT    /v2/<repo>/manifests/<reference>  → push manifest (schema validation)
//! GET    /v2/<repo>/tags/list              → tag list
//! ```
//!
//! ## Bridges
//!
//! See [`bridge`] for `OciBlobBridge`, `OciManifestKvBridge<K>`, and
//! `OciTenantBridge`.

pub mod audit;
pub mod auth;
pub mod bridge;
pub mod config;
pub mod digest;
pub mod error;
pub mod ports;
pub mod pull;
pub mod push;
pub mod server;
pub mod tags;

pub use bridge::{OciBlobBridge, OciManifestKvBridge, OciTenantBridge};
pub use config::{ConfigError, OciAdapterConfig};
pub use error::{OciAdapterError, OciErrorEntry, OciErrorEnvelope};
pub use server::{router, status_for, validate_repo_name, wallclock_unix_ms, AppState};

use std::sync::Arc;

/// Bind, serve, and run the OCI adapter until interrupted.
///
/// # Errors
///
/// - [`OciAdapterError::Bind`] if the listen socket fails to bind.
/// - [`OciAdapterError::Cas`] / [`OciAdapterError::Kv`] /
///   [`OciAdapterError::Audit`] on backend issues surfaced during
///   serve (very rare — these usually surface per-request).
pub async fn run_oci_adapter(config: OciAdapterConfig) -> Result<(), OciAdapterError> {
    config
        .sanity_check()
        .map_err(|e| OciAdapterError::Auth(e.to_string()))?;
    let bind = config.bind_addr;
    let state = AppState {
        config: Arc::new(config),
        clock_unix_ms: wallclock_unix_ms,
    };
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(OciAdapterError::Bind)?;
    axum::serve(listener, app)
        .await
        .map_err(OciAdapterError::Bind)?;
    Ok(())
}
