//! `corelink-adapter-oci` — OCI Distribution Spec v1.1 registry
//! adapter.
//!
//! CoreLink becomes a full OCI Distribution Spec v1.1 registry: any
//! standard OCI client (`docker`, `podman`, `buildah`, `containerd`,
//! `crane`, Kubernetes' image pull machinery, BuildKit cache,
//! Helm OCI artifacts) speaks to it as a vanilla registry. Manifests
//! (JSON) live in KV; blobs (layers + configs) live in CAS.
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
//! ## Charter constraints enforced at compile time
//!
//! - `#![forbid(unsafe_code)]` — entire crate.
//! - `#[non_exhaustive]` on every public type / enum.
//! - All credentials (PAT, bearer-token signing key) wrapped in
//!   [`corelink_core::SecretWrap`].
//! - Constant-time HMAC verify via `subtle::ConstantTimeEq`.
//! - Audit emit BEFORE every state mutation (blob put, manifest put,
//!   tag update) — see [`audit`].
//! - Declared-digest verification fail-CLOSED on every
//!   `PUT /blobs/uploads/<uuid>?digest=X` — see [`push::upload::put`].
//! - L2.10 file-size discipline: largest src/ file is well under
//!   500 LOC HARD CAP; sweet-spot ≤200.
//!
//! ## Hexagonal ports
//!
//! See [`ports`]. The spec names three external surfaces — `CasStore`,
//! `KvStore`, `TenantResolver`. None of these exist as canonical
//! workspace traits at the wave-34 baseline, so we declare adapter-
//! local equivalents ([`ports::BlobStore`], [`ports::ManifestKvStore`],
//! [`ports::TenantResolver`]) and ship in-memory test fakes for
//! integration testing. Production wiring will bridge these onto the
//! canonical surfaces once those land (per the consumer-migration
//! deferral pattern from Wave-33 Stage 2.A-v2).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod audit;
pub mod auth;
pub mod config;
pub mod digest;
pub mod error;
pub mod ports;
pub mod pull;
pub mod push;
pub mod server;
pub mod tags;

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
