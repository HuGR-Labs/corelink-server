//! `/cargo/<tenant>/<key>` — sccache HTTP build-cache surface.
//!
//! Mounts the `corelink_adapter_host::cargo` adapter (the sccache HTTP
//! storage backend: `GET`/`PUT`/`HEAD /<key>`) into the container router.
//! This closes FINDING-sccache-adapter-gaps §Gap 1: the Worker already
//! forwards `/cargo/*` to the container, but the adapter was never mounted,
//! so every forwarded request 404'd.
//!
//! # Path shape
//!
//! The Worker forwards the FULL path `/cargo/<tenant>/<key>` (the
//! `<tenant>` segment is used only for DO routing). The adapter's own
//! router is `/:key` (a single segment) because the adapter derives the
//! tenant from the PAT, never from the path. We bridge the two shapes with
//! `nest_service("/cargo", …)`, which strips the static `/cargo` prefix and
//! hands `/<tenant>/<key>` to a thin gate layer that:
//!
//! 1. enforces the per-operation cache scope (GET/HEAD ⇒ read, PUT ⇒ write)
//!    from the Worker-set, server-trusted `x-corelink-scope` header; and
//! 2. rewrites the request path `/<tenant>/<key>` → `/<key>` so the
//!    adapter's `/:key` route matches.
//!
//! `nest_service` (not `nest`) is required: `nest` would flatten the
//! adapter's `/:key` into a 2-segment matcher (`/cargo/:key`) and reject
//! the 3-segment `/cargo/<tenant>/<key>` before the gate ever runs.
//!
//! # Tenant + scope trust model
//!
//! Tenant identity comes from the PAT, re-verified in the container against
//! the D1 `pat` store ([`crate::adapter_pat::PatVerifier`] — the ONE verifier
//! shared by cargo/brew/npm/pip, wrapped here via [`resolver_from_verifier`];
//! Option B — HMAC + Argon2id possession check). The path `<tenant>` is
//! NEVER trusted for storage. The scope gate here is the per-operation
//! layer (read vs write) on top of the resolver's "has cache capability"
//! check, mirroring the H1 scope spine used by cas/ac/turbo/bazel.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;

use async_trait::async_trait;
use corelink_adapter_host::cargo::config::DEFAULT_BODY_SIZE_LIMIT_BYTES;
use corelink_adapter_host::cargo::ports::{
    SharedTenantResolver, TenantResolveError, TenantResolver,
};
use corelink_adapter_host::cargo::{server, CargoAdapterConfig, CargoCasBridge};
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::scope::{requires_cache_read, requires_cache_write, SCOPE_HEADER};

/// Service principal recorded on adapter CAS operations. Identifies the
/// adapter-host service, NOT the end-user PAT (which the resolver verified).
const CARGO_SERVICE_PRINCIPAL: &str = "cargo-adapter-host";

/// Thin shell adapting the shared [`PatVerifier`] (Option B) to cargo's
/// `TenantResolver` port. Identical pattern to brew/npm/pip: the container
/// holds ONE `PatVerifier` and every adapter wraps it, so the
/// HMAC → D1 → Argon2id → fail-CLOSED-scope pipeline lives once in
/// [`crate::adapter_pat`].
#[derive(Debug)]
struct CargoPatResolver(Arc<PatVerifier>);

#[async_trait]
impl TenantResolver for CargoPatResolver {
    async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError> {
        self.0.verify(pat_plaintext).await.map_err(|e| match e {
            VerifyError::InvalidPat => TenantResolveError::InvalidPat,
            VerifyError::Backend(m) => TenantResolveError::Backend(m),
        })
    }
}

/// Wrap the shared [`PatVerifier`] as cargo's injectable [`SharedTenantResolver`].
/// The container build path calls this so cargo shares the one verifier with
/// brew/npm/pip rather than constructing a second D1-backed resolver.
#[must_use]
pub fn resolver_from_verifier(verifier: Arc<PatVerifier>) -> SharedTenantResolver {
    Arc::new(CargoPatResolver(verifier))
}

/// Build the `/cargo/*` sub-router from shared CAS handlers + a PAT→tenant
/// resolver.
///
/// The `resolver` is injected so production wires the shared [`PatVerifier`]
/// (via [`resolver_from_verifier`]) while tests pass a hermetic stub. The CAS
/// handlers are the SAME `Arc<dyn …>` trait objects the cas/bazel/turbo
/// surfaces use (no new R2 connection).
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    resolver: SharedTenantResolver,
) -> Router {
    let cas = Arc::new(CargoCasBridge::new(
        cas_read,
        cas_write,
        CARGO_SERVICE_PRINCIPAL,
    ));
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());
    // bind_addr is unused by `build_router` (only `run_cargo_adapter` binds);
    // pass an ephemeral placeholder.
    let config = CargoAdapterConfig::new(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        DEFAULT_BODY_SIZE_LIMIT_BYTES,
        cas,
        resolver,
        auditor,
    );

    let adapter = server::build_router(config).layer(middleware::from_fn(cargo_gate));
    Router::new().nest_service("/cargo", adapter)
}

/// Gate layer for the cargo surface: per-operation cache-scope enforcement.
///
/// Runs as a `layer` on the adapter router (which is a catch-all `/*path`, so
/// it matches the nested `/cargo/<tenant>/<key>` shape directly — no path
/// rewrite needed). The gate enforces the per-operation scope from the
/// Worker-set, server-trusted `x-corelink-scope` header, then forwards to the
/// adapter, which does PAT auth + key validation + CAS. An insufficient scope
/// → 403; everything else is the adapter's call (401/400/404/200).
///
/// Why scope lives here, not in the resolver: the adapter's `TenantResolver`
/// port carries no operation, so it can only check "has cache capability". The
/// HTTP method (read vs write) is known only at this layer, so per-operation
/// granularity (read-only PAT must not PUT) is enforced here — mirroring the
/// H1 scope spine used by cas/ac/turbo/bazel.
async fn cargo_gate(req: Request, next: Next) -> Response {
    let scope = req
        .headers()
        .get(SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let scope_ok = match *req.method() {
        Method::PUT => requires_cache_write(scope),
        Method::GET | Method::HEAD => requires_cache_read(scope),
        // Fail-CLOSED: the adapter only routes GET/HEAD/PUT (so anything else
        // would 405 downstream today), but the gate must not assume that —
        // an unmapped method is denied here so a future adapter route can
        // never ship without an explicit scope decision. No browser/CORS
        // clients exist on this surface, so OPTIONS is not legitimate traffic.
        _ => false,
    };
    if !scope_ok {
        return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
    }
    next.run(req).await
}
