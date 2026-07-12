//! Axum router + `run_cargo_adapter` entrypoint.
//!
//! The router exposes three routes mirroring the sccache HTTP storage
//! backend wire (from <https://github.com/mozilla/sccache/blob/main/docs/HTTP.md>):
//!
//! - `GET  /<key>` — read artifact: 200 + bytes on hit; 404 on miss.
//! - `PUT  /<key>` — write artifact: 200 on success.
//! - `HEAD /<key>` — existence check: 200 on hit; 404 on miss.
//!
//! Every request flows through:
//!
//! 1. `extract_bearer` → PAT plaintext (HTTP 401 on mismatch);
//! 2. `resolve_tenant` → tenant id (HTTP 401);
//! 3. Route handler → CAS read / write via adapter-local `CasStore` port.
//!
//! ## Status code mapping
//!
//! | `CargoAdapterError` variant | HTTP | Why |
//! |---|---|---|
//! | `Auth`           | 401 | bad / missing PAT |
//! | `Cas`            | 502 | CAS dependency failure |
//! | `Audit`          | 503 | audit chokepoint failed (fail-CLOSED) |
//! | `BodyOversized`  | 413 | PUT body > `body_size_limit_bytes` |
//! | `Bind`           | n/a | bind failures from `run_cargo_adapter` directly |

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};

use crate::cargo::audit::AuditOrchestrator;
use crate::cargo::auth::{extract_bearer, resolve_tenant};
use crate::cargo::config::CargoAdapterConfig;
use crate::cargo::error::CargoAdapterError;
use crate::cargo::ports::{CasError, SharedCasStore, SharedTenantResolver};
use crate::cargo::translate::key_from_path;

/// Shared router state. `Clone`-cheap (every field is `Arc`-shaped).
#[derive(Clone)]
#[non_exhaustive]
pub struct CargoRouterState {
    cas: SharedCasStore,
    tenant_resolver: SharedTenantResolver,
    auditor: Arc<AuditOrchestrator>,
    body_size_limit_bytes: u64,
}

impl std::fmt::Debug for CargoRouterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CargoRouterState")
            .field("cas", &"Arc<dyn CasStore>")
            .field("tenant_resolver", &"Arc<dyn TenantResolver>")
            .field("body_size_limit_bytes", &self.body_size_limit_bytes)
            .finish()
    }
}

/// Tenant id already resolved by an UPSTREAM gate layer (the container's
/// `cargo_gate`) from a SINGLE PAT verification.
///
/// When the adapter is mounted behind the container's two-layer write gate
/// (F27), the gate runs the full HMAC + Argon2id PAT verify
/// (`resolve_with_capability`) on every PUT to derive the tenant and the
/// `can_write` bit. Re-running `resolve_tenant` in [`handle_put`] would run
/// Argon2id a *second* time on the same request (REV-S3) — pure latency on the
/// CAS hot path. The gate therefore inserts this typed extension carrying the
/// authoritative, crypto-verified tenant id, and `handle_put` consumes it
/// instead of re-resolving.
///
/// ## Trust model
///
/// This is a request *extension* — server-internal typed storage, NOT derived
/// from any client-supplied header. An HTTP client cannot inject a Rust-typed
/// extension over the wire; the only producer is the gate middleware, and only
/// AFTER a successful PAT verification. In standalone mode (adapter run without
/// the gate, e.g. `run_cargo_adapter` or hermetic tests) the extension is
/// absent and `handle_put` falls back to a full `resolve_tenant` — so the
/// adapter remains self-sufficient and fail-CLOSED on auth.
#[derive(Debug, Clone)]
pub struct GateResolvedTenant(pub String);

/// Hash a value to a short, stable hex correlation handle for logging
/// (INV-NO-PII-IN-LOGS). First 8 bytes of SHA-256, hex-encoded —
/// consistent with the `hash_for_log` convention in `internal_pat.rs`
/// and `durable_object.ts`. Never log the raw tenant UUID; log this
/// handle instead.
#[must_use]
fn hash_for_log(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut out = String::with_capacity(16);
    for b in digest.iter().take(8) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Build the axum [`Router`] without binding a listener — useful for
/// in-process tests (drive it directly via `tower::ServiceExt`).
pub fn build_router(config: CargoAdapterConfig) -> Router {
    let auditor = Arc::new(AuditOrchestrator::new(config.auditor.clone()));
    let state = CargoRouterState {
        cas: Arc::<dyn crate::cargo::ports::CasStore>::clone(&config.cas),
        tenant_resolver: Arc::<dyn crate::cargo::ports::TenantResolver>::clone(
            &config.tenant_resolver,
        ),
        auditor,
        body_size_limit_bytes: config.body_size_limit_bytes,
    };
    // axum 0.7: HEAD is served automatically by the GET handler when
    // not explicitly registered. We register an explicit HEAD handler
    // to avoid serving body bytes on HEAD requests.
    //
    // The route is a catch-all (`/*path`), not `/:key`, so the adapter can be
    // mounted UNCHANGED behind a container prefix (`/cargo/<tenant>/<key>`):
    // `handle_*` resolve the actual key via `key_from_path`, which already
    // takes the LAST path segment. In standalone sccache mode the client hits
    // `/<key>` (a single segment), which the catch-all matches identically.
    Router::new()
        .route(
            "/{*path}",
            get(handle_get).put(handle_put).head(handle_head),
        )
        .with_state(state)
}

/// Bind the configured [`SocketAddr`] and serve forever.
///
/// # Errors
///
/// Returns [`CargoAdapterError::Bind`] if the listener cannot bind, or
/// propagates any axum / hyper serve failure as `CargoAdapterError::Bind`.
pub async fn run_cargo_adapter(config: CargoAdapterConfig) -> Result<(), CargoAdapterError> {
    let bind_addr: SocketAddr = config.bind_addr;
    let router = build_router(config);
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(CargoAdapterError::Bind)?;
    tracing::info!(?bind_addr, "corelink-adapter-cargo listening");
    axum::serve(listener, router)
        .await
        .map_err(CargoAdapterError::Bind)?;
    Ok(())
}

/// `GET /<key>` — read artifact; 200 + bytes on hit, 404 on miss.
async fn handle_get(
    State(state): State<CargoRouterState>,
    Path(raw_key): Path<String>,
    headers: HeaderMap,
) -> Response {
    let key = match key_from_path(&raw_key) {
        Some(k) => k,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let pat = match extract_bearer(&headers) {
        Ok(pat) => pat,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    let tenant_id = match resolve_tenant(&state.tenant_resolver, &pat).await {
        Ok(t) => t,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    match state.cas.get(&tenant_id, &key).await {
        Ok(Some(bytes)) => {
            tracing::debug!(tenant_hash = %hash_for_log(&tenant_id), key = %key, "cargo CAS hit");
            let mut response = Response::new(Body::from(bytes));
            *response.status_mut() = StatusCode::OK;
            response
        }
        Ok(None) => {
            tracing::debug!(tenant_hash = %hash_for_log(&tenant_id), key = %key, "cargo CAS miss");
            StatusCode::NOT_FOUND.into_response()
        }
        Err(CasError::Backend(msg)) => CargoAdapterError::Cas(msg).into_response(),
    }
}

/// `HEAD /<key>` — existence check; 200 on hit, 404 on miss.
///
/// Uses the metadata-only [`crate::cargo::ports::CasStore::exists`] probe — NOT
/// `cas.get` — so a sccache HEAD does not pull (and discard) the full blob body,
/// which would double R2 GET egress on every pre-write existence check (COGS).
async fn handle_head(
    State(state): State<CargoRouterState>,
    Path(raw_key): Path<String>,
    headers: HeaderMap,
) -> Response {
    let key = match key_from_path(&raw_key) {
        Some(k) => k,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let pat = match extract_bearer(&headers) {
        Ok(pat) => pat,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    let tenant_id = match resolve_tenant(&state.tenant_resolver, &pat).await {
        Ok(t) => t,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    match state.cas.exists(&tenant_id, &key).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(CasError::Backend(msg)) => CargoAdapterError::Cas(msg).into_response(),
    }
}

/// `PUT /<key>` — write artifact; 200 on success.
async fn handle_put(
    State(state): State<CargoRouterState>,
    Path(raw_key): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let key = match key_from_path(&raw_key) {
        Some(k) => k,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    // REV-S3: if an upstream gate (the container's `cargo_gate`) already ran the
    // full HMAC + Argon2id PAT verification for this PUT, it threaded the
    // authoritative tenant id in via the [`GateResolvedTenant`] extension. Reuse
    // it instead of re-running Argon2id (a redundant second verify, ~40-200ms on
    // the CAS hot path). The extension is server-internal typed storage — never
    // client-supplied — so this is not a trust downgrade. When absent (standalone
    // adapter / hermetic tests) we fall back to a full `resolve_tenant`, which
    // remains fail-CLOSED on auth.
    let tenant_id = if let Some(GateResolvedTenant(t)) =
        request.extensions().get::<GateResolvedTenant>().cloned()
    {
        t
    } else {
        let pat = match extract_bearer(&headers) {
            Ok(pat) => pat,
            Err(err) => {
                state.auditor.emit_auth_failed(&err.to_string());
                return err.into_response();
            }
        };

        match resolve_tenant(&state.tenant_resolver, &pat).await {
            Ok(t) => t,
            Err(err) => {
                state.auditor.emit_auth_failed(&err.to_string());
                return err.into_response();
            }
        }
    };

    // Collect body with size enforcement.
    let bytes = match collect_body(request, state.body_size_limit_bytes).await {
        Ok(b) => b,
        Err(err) => return err.into_response(),
    };

    // Audit emit BEFORE CAS put (fail-CLOSED contract).
    if let Err(err) = state
        .auditor
        .emit_cache_write(&tenant_id, &key, bytes.len() as u64)
    {
        return err.into_response();
    }

    match state.cas.put(&tenant_id, &key, bytes).await {
        Ok(()) => {
            tracing::debug!(tenant_hash = %hash_for_log(&tenant_id), key = %key, "cargo CAS put");
            StatusCode::OK.into_response()
        }
        Err(CasError::Backend(msg)) => CargoAdapterError::Cas(msg).into_response(),
    }
}

/// Collect the request body up to `limit` bytes.
///
/// Returns [`CargoAdapterError::BodyOversized`] if the streamed total
/// exceeds `limit`.
async fn collect_body(request: Request, limit: u64) -> Result<Vec<u8>, CargoAdapterError> {
    // axum 0.7 / http-body-util: collect the full body into Bytes.
    // We enforce the size limit here rather than using axum's
    // `DefaultBodyLimit` so we can return a structured error.
    let body = request.into_body();
    let collected = body
        .collect()
        .await
        .map_err(|e| CargoAdapterError::Cas(format!("body read: {e}")))?;
    let bytes = collected.to_bytes();
    let len = bytes.len() as u64;
    if len > limit {
        return Err(CargoAdapterError::BodyOversized(len));
    }
    Ok(bytes.to_vec())
}

impl IntoResponse for CargoAdapterError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Auth(_) => StatusCode::UNAUTHORIZED,
            Self::Cas(_) => StatusCode::BAD_GATEWAY,
            Self::Audit(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::BodyOversized(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Bind(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // Cluster E (A27/A29): scrub internal backend detail (raw D1/CF/R2
        // error, possibly SQL; R2 storage topology; derived per-tenant prefix)
        // from the client body. Mint a correlation id, log the REAL detail
        // server-side only, and return an opaque, ref-tagged message.
        let request_id = uuid::Uuid::new_v4().simple().to_string();
        if self.leaks_internal_detail() {
            tracing::error!(
                request_id = %request_id,
                // Display carries the full internal detail — server-side only.
                detail = %self,
                "cargo: backend error (scrubbed from client response; see ref)"
            );
        }
        let body = self.client_message(&request_id);
        let mut response = Response::new(Body::from(body));
        *response.status_mut() = status;
        response
    }
}

#[doc(hidden)]
pub fn _unused_marker(_: &SharedCasStore) {}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    /// F4 regression: `hash_for_log` emits a short hex handle, not the
    /// raw UUID, and is stable (same input → same output, different
    /// input → different output).
    #[test]
    fn hash_for_log_is_short_hex_not_raw_uuid() {
        let raw = "550e8400-e29b-41d4-a716-446655440000";
        let handle = hash_for_log(raw);
        // 16 hex chars (8 bytes).
        assert_eq!(handle.len(), 16, "handle must be 16 hex chars");
        assert!(
            handle.bytes().all(|b| b.is_ascii_hexdigit()),
            "handle must be hex"
        );
        // Must NOT be the raw UUID.
        assert_ne!(handle, raw, "handle must not be the raw tenant id");
        // Stability: same input always produces the same handle.
        assert_eq!(handle, hash_for_log(raw), "hash_for_log must be stable");
        // Different UUIDs produce different handles (collision-free for
        // any realistic tenant space).
        let other = hash_for_log("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert_ne!(
            handle, other,
            "distinct UUIDs must produce distinct handles"
        );
    }

    /// F4 regression: verify the first 8 bytes of SHA-256 match the
    /// expected value for a known input.
    #[test]
    fn hash_for_log_matches_sha256_first_8_bytes() {
        use sha2::{Digest as _, Sha256};
        let input = "550e8400-e29b-41d4-a716-446655440000";
        let full = Sha256::digest(input.as_bytes());
        let expected: String = full.iter().take(8).map(|b| format!("{b:02x}")).collect();
        assert_eq!(hash_for_log(input), expected);
    }

    // --- REV-S3: PUT reuses the gate-resolved tenant, never re-verifying ---

    use std::sync::Mutex as StdMutex;

    use crate::cargo::ports::{CasError, CasStore, TenantResolveError, TenantResolver};
    use axum::body::Body;
    use http::Request as HttpRequest;
    use tower::ServiceExt as _; // for `oneshot`

    /// CAS that records the tenant id of every `put` so the test can assert
    /// which tenant the write was attributed to.
    #[derive(Debug, Default)]
    struct RecordingCas {
        puts: StdMutex<Vec<(String, String)>>,
    }

    #[async_trait::async_trait]
    impl CasStore for RecordingCas {
        async fn get(
            &self,
            _tenant_id: &str,
            _digest_hex: &str,
        ) -> Result<Option<Vec<u8>>, CasError> {
            Ok(None)
        }

        async fn put(
            &self,
            tenant_id: &str,
            digest_hex: &str,
            _bytes: Vec<u8>,
        ) -> Result<(), CasError> {
            self.puts
                .lock()
                .unwrap()
                .push((tenant_id.to_owned(), digest_hex.to_owned()));
            Ok(())
        }
    }

    /// Resolver that PANICS if invoked — proves the PUT fast-path skips the
    /// (Argon2id-backed) PAT verification when the gate already resolved.
    #[derive(Debug, Default)]
    struct PanicResolver;

    #[async_trait::async_trait]
    impl TenantResolver for PanicResolver {
        async fn resolve(&self, _pat: &str) -> Result<String, TenantResolveError> {
            panic!("resolver must NOT run when GateResolvedTenant is present (REV-S3)");
        }
    }

    fn test_config(cas: Arc<dyn CasStore>) -> CargoAdapterConfig {
        use std::net::{Ipv4Addr, SocketAddr};
        CargoAdapterConfig::new(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            crate::cargo::config::DEFAULT_BODY_SIZE_LIMIT_BYTES,
            cas,
            Arc::new(PanicResolver),
            Arc::new(corelink_audit::ports::InMemoryAuditEmitter::new()),
        )
    }

    /// REV-S3 regression: when an upstream gate inserts [`GateResolvedTenant`],
    /// `handle_put` reuses it and does NOT call the resolver a second time. The
    /// `PanicResolver` would abort the test if `resolve` ran; the write is
    /// attributed to the gate-resolved tenant.
    #[tokio::test]
    async fn put_reuses_gate_resolved_tenant_without_reverifying() {
        let cas = Arc::new(RecordingCas::default());
        let router = build_router(test_config(cas.clone()));

        let key = "a".repeat(64);
        let req = HttpRequest::builder()
            .method(http::Method::PUT)
            .uri(format!("/{key}"))
            // NOTE: no Authorization header at all — the extension is the only
            // tenant source, proving the resolver is never consulted.
            .extension(GateResolvedTenant("tenant-from-gate".to_owned()))
            .body(Body::from(vec![1u8, 2, 3]))
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let puts = cas.puts.lock().unwrap();
        assert_eq!(puts.len(), 1, "exactly one CAS put");
        assert_eq!(
            puts[0].0, "tenant-from-gate",
            "write attributed to the gate-resolved tenant"
        );
        assert_eq!(puts[0].1, key, "write keyed by the path digest");
    }

    /// REV-S3 regression (fallback): with NO gate extension and NO bearer, the
    /// standalone adapter fails CLOSED on auth (401) rather than writing — the
    /// fallback `resolve_tenant` path still runs. (Here the missing
    /// Authorization header is rejected by `extract_bearer` BEFORE the resolver,
    /// so `PanicResolver` is never reached.)
    #[tokio::test]
    async fn put_without_gate_extension_or_bearer_is_unauthorized() {
        let cas = Arc::new(RecordingCas::default());
        let router = build_router(test_config(cas.clone()));

        let key = "a".repeat(64);
        let req = HttpRequest::builder()
            .method(http::Method::PUT)
            .uri(format!("/{key}"))
            .body(Body::from(vec![1u8, 2, 3]))
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            cas.puts.lock().unwrap().len(),
            0,
            "no write on auth failure"
        );
    }

    // --- M7 (COGS): sccache HEAD must use the metadata-only `exists` probe,
    // never pull (and discard) the full blob body via `get` ---

    /// CAS that records whether `get` (body fetch) or `exists` (metadata) was
    /// called. `present` is the set of keys that "exist". `get` returns a large
    /// payload so a HEAD that mistakenly fetched would be obvious egress.
    #[derive(Debug, Default)]
    struct HeadProbeCas {
        present: StdMutex<std::collections::HashSet<String>>,
        get_calls: StdMutex<usize>,
        exists_calls: StdMutex<usize>,
    }

    #[async_trait::async_trait]
    impl CasStore for HeadProbeCas {
        async fn get(
            &self,
            _tenant_id: &str,
            digest_hex: &str,
        ) -> Result<Option<Vec<u8>>, CasError> {
            *self.get_calls.lock().unwrap() += 1;
            if self.present.lock().unwrap().contains(digest_hex) {
                Ok(Some(vec![0u8; 1_000_000])) // 1 MiB — body fetch would be costly
            } else {
                Ok(None)
            }
        }

        async fn put(
            &self,
            _tenant_id: &str,
            _digest_hex: &str,
            _bytes: Vec<u8>,
        ) -> Result<(), CasError> {
            Ok(())
        }

        async fn exists(&self, _tenant_id: &str, digest_hex: &str) -> Result<bool, CasError> {
            *self.exists_calls.lock().unwrap() += 1;
            Ok(self.present.lock().unwrap().contains(digest_hex))
        }
    }

    /// Resolver that resolves any PAT to a fixed tenant (so the HEAD path
    /// reaches the CAS probe). Distinct from `PanicResolver`, which aborts.
    #[derive(Debug, Default)]
    struct FixedResolver;

    #[async_trait::async_trait]
    impl TenantResolver for FixedResolver {
        async fn resolve(&self, _pat: &str) -> Result<String, TenantResolveError> {
            Ok("tenant-fixed".to_owned())
        }
    }

    fn head_config(cas: Arc<dyn CasStore>) -> CargoAdapterConfig {
        use std::net::{Ipv4Addr, SocketAddr};
        CargoAdapterConfig::new(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            crate::cargo::config::DEFAULT_BODY_SIZE_LIMIT_BYTES,
            cas,
            Arc::new(FixedResolver),
            Arc::new(corelink_audit::ports::InMemoryAuditEmitter::new()),
        )
    }

    #[tokio::test]
    async fn head_hit_uses_exists_and_never_fetches_body() {
        let cas = Arc::new(HeadProbeCas::default());
        let key = "a".repeat(64);
        cas.present.lock().unwrap().insert(key.clone());
        let router = build_router(head_config(cas.clone()));

        let req = HttpRequest::builder()
            .method(http::Method::HEAD)
            .uri(format!("/{key}"))
            .header(http::header::AUTHORIZATION, "Bearer corelink_pat_test")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            *cas.exists_calls.lock().unwrap(),
            1,
            "HEAD must probe via exists()"
        );
        assert_eq!(
            *cas.get_calls.lock().unwrap(),
            0,
            "HEAD must NOT fetch the body via get() (COGS: doubled R2 egress)"
        );
    }

    #[tokio::test]
    async fn head_miss_returns_404_without_fetching_body() {
        let cas = Arc::new(HeadProbeCas::default());
        let router = build_router(head_config(cas.clone()));

        let key = "b".repeat(64);
        let req = HttpRequest::builder()
            .method(http::Method::HEAD)
            .uri(format!("/{key}"))
            .header(http::header::AUTHORIZATION, "Bearer corelink_pat_test")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            *cas.get_calls.lock().unwrap(),
            0,
            "miss path also must not fetch the body"
        );
    }
}
