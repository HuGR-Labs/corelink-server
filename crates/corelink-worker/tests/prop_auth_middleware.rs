//! Property tests (10k iter) for the WI-S03-003 Tower auth middleware.
//!
//! ## Invariants exercised
//!
//! Per WI §3 P0 + §10 plus the protocol §"Quality bar":
//!
//! 1. **`prop_missing_token_rejected`** — every request without an
//!    `Authorization` header (any method, any random URI, any
//!    arbitrary header soup) is rejected with the canonical
//!    [`COR_AUTH_HEADER_MISSING`] code. The PAT verifier MUST NOT be
//!    invoked (header check is the load-bearing gate; cost protection
//!    against header-stripping DDoS).
//!
//! 2. **`prop_malformed_token_rejected`** — random bytes after
//!    `Bearer ` that do not parse as PAT or JWT shape produce a 401
//!    with one of the canonical malformed/ambiguous error codes; the
//!    PAT verifier MAY be invoked (genuine PAT-prefixed garbage) but
//!    the cold-path constant-time pad is exercised regardless.
//!
//! 3. **`prop_expired_token_rejected`** — when the JWT verifier
//!    surfaces `Expired` for any random JWT-shaped input, the
//!    middleware translates to 401 `COR_AUTH_TOKEN_EXPIRED` and the
//!    handler is NEVER invoked (no AuthCtx leakage on expired tokens).
//!
//! 4. **`prop_cross_tenant_header_smuggle_rejected`** — for every
//!    pair `(verified_tenant, smuggled_tenant)` where
//!    `verified ≠ smuggled`, the middleware rejects with the
//!    `COR_AUTH_INVALID_TOKEN` code and the handler is NEVER invoked
//!    — even when the smuggled value is the canonical UUID form of a
//!    real tenant. The matching case
//!    (`verified == smuggled`) is allowed through; this dual-arm
//!    design prevents test false-positives where the rejection arm
//!    fires unconditionally.
//!
//! 5. **`prop_5_layer_consistency`** — for any verified
//!    `(tenant_id, region)`, the constructed `AuthCtx` satisfies
//!    `ctx.tenant_id() == verified.tenant_id` AND
//!    `ctx.tenant_prefix() == derive_prefix(tdk, ctx.tenant_id())` AND
//!    `ctx.tenant_ctx().tenant_id() == verified.tenant_id` AND
//!    `ctx.tenant_ctx().prefix() == ctx.tenant_prefix()` —
//!    INV-AUTH-5-LAYER-ORDERING.
//!
//! All proptest cases default to **10 000 iterations** per the WI
//! quality bar (override via `PROPTEST_CASES=N` env var).
//!
//! ## Why this file uses Tower service composition end-to-end
//!
//! Unit-level checks of the orchestrator function are nice but the
//! attacker exploits the composition (e.g. a layer ordering bug
//! that lets the smuggled header reach the handler before the
//! verifier completes). We exercise the full `AuthLayer.layer(inner)`
//! Tower stack so the property gate covers the canonical wiring.

#![cfg(feature = "tower-middleware")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use corelink_clerk::ClerkPrincipal;
use corelink_pat::{
    PatEnv, PatId, PatScopes, PrincipalId as PatPrincipal, TenantId as PatTenantId, SCOPE_CACHE_R,
    SCOPE_CACHE_RW, SCOPE_CACHE_W,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use corelink_worker::middleware::{
    AuthCtx, AuthLayer, AuthMiddlewareError, AuthState, JwtTenantBinding, JwtTenantResolver,
    JwtVerifier, PatVerification, PatVerifier,
};
use corelink_worker::Region;
use http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use proptest::prelude::*;
use tower::ServiceExt;
use tower_layer::Layer;
use tower_service::Service;
use uuid::Uuid;
use zeroize::Zeroizing;

const ITER: u32 = 10_000;

// ---------------------------------------------------------------------------
// Verifier fakes (script-driven; no DB)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct ScriptedPatVerifier {
    state: Arc<std::sync::Mutex<PatBehavior>>,
    invocations: Arc<AtomicUsize>,
}

#[derive(Clone, Default)]
enum PatBehavior {
    #[default]
    AlwaysInvalid,
    Constant(PatVerification),
}

impl ScriptedPatVerifier {
    fn always_invalid() -> Self {
        Self::default()
    }
    fn returning(v: PatVerification) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(PatBehavior::Constant(v))),
            invocations: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn invocations(&self) -> usize {
        self.invocations.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl PatVerifier for ScriptedPatVerifier {
    async fn verify(&self, _plaintext: &str) -> Result<PatVerification, AuthMiddlewareError> {
        self.invocations.fetch_add(1, Ordering::Relaxed);
        let guard = self.state.lock().expect("poisoned");
        match &*guard {
            PatBehavior::AlwaysInvalid => Err(AuthMiddlewareError::InvalidToken),
            PatBehavior::Constant(v) => Ok(v.clone()),
        }
    }
}

#[derive(Clone)]
struct ScriptedJwtVerifier {
    outcome: Arc<std::sync::Mutex<JwtScript>>,
}

#[derive(Clone)]
enum JwtScript {
    AlwaysInvalid,
    AlwaysExpired,
}

#[async_trait]
impl JwtVerifier for ScriptedJwtVerifier {
    async fn validate(&self, _jwt: &str) -> Result<ClerkPrincipal, AuthMiddlewareError> {
        let guard = self.outcome.lock().expect("poisoned");
        match &*guard {
            JwtScript::AlwaysInvalid => Err(AuthMiddlewareError::InvalidToken),
            JwtScript::AlwaysExpired => Err(AuthMiddlewareError::TokenExpired),
        }
    }
}

#[derive(Clone)]
struct ScriptedJwtResolver;

#[async_trait]
impl JwtTenantResolver for ScriptedJwtResolver {
    async fn resolve(
        &self,
        _principal: &ClerkPrincipal,
    ) -> Result<JwtTenantBinding, AuthMiddlewareError> {
        Err(AuthMiddlewareError::InvalidToken)
    }
}

// ---------------------------------------------------------------------------
// Spy handler — records whether a request reached the inner service
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct SpyHandler {
    reached: Arc<AtomicBool>,
    last_tenant: Arc<std::sync::Mutex<Option<Uuid>>>,
    last_prefix: Arc<std::sync::Mutex<Option<String>>>,
}

impl Service<Request<Bytes>> for SpyHandler {
    type Response = Response<Bytes>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Bytes>) -> Self::Future {
        let reached = Arc::clone(&self.reached);
        let last_tenant = Arc::clone(&self.last_tenant);
        let last_prefix = Arc::clone(&self.last_prefix);
        Box::pin(async move {
            reached.store(true, Ordering::Relaxed);
            if let Some(ctx) = req.extensions().get::<AuthCtx>() {
                *last_tenant.lock().expect("poisoned") = Some(ctx.tenant_id());
                *last_prefix.lock().expect("poisoned") =
                    Some(ctx.tenant_prefix().as_str().to_owned());
            }
            Ok(Response::new(Bytes::from_static(b"ok")))
        })
    }
}

// ---------------------------------------------------------------------------
// Tower scaffolding helpers
// ---------------------------------------------------------------------------

fn make_state(
    pat_verifier: Arc<dyn PatVerifier>,
    jwt_verifier: Arc<dyn JwtVerifier>,
    jwt_resolver: Arc<dyn JwtTenantResolver>,
    region: Region,
) -> (AuthState, Arc<TenantDerivationKey>) {
    let tdk = Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([7u8; 32])));
    (
        AuthState {
            tdk: Arc::clone(&tdk),
            region,
            pat_verifier,
            jwt_verifier,
            jwt_resolver,
        },
        tdk,
    )
}

fn make_pat_verification(tenant: Uuid, principal: Uuid, scopes: u64) -> PatVerification {
    PatVerification {
        env: PatEnv::Pat,
        pat_id: PatId(Uuid::nil()),
        tenant_id: PatTenantId(tenant),
        principal_id: PatPrincipal(principal),
        scopes: PatScopes::from_u64(scopes),
    }
}

fn canonical_pat_plaintext() -> String {
    let token_id = "0123456789ABCDEF";
    let secret = "A".repeat(43);
    let sig = "B".repeat(22);
    format!("corelink_pat_{token_id}.{secret}.{sig}")
}

/// Run a single request through `(AuthLayer + SpyHandler)` synchronously.
/// Uses a fresh per-iteration tokio runtime so proptest shrinking
/// behaves deterministically.
fn run_once(state: AuthState, spy: SpyHandler, req: Request<Bytes>) -> Response<Bytes> {
    let mut svc = AuthLayer::new(state).layer(spy);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async move { svc.ready().await.unwrap().call(req).await.unwrap() })
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn arb_method() -> impl Strategy<Value = http::Method> {
    prop_oneof![
        Just(http::Method::GET),
        Just(http::Method::POST),
        Just(http::Method::PUT),
        Just(http::Method::DELETE),
        Just(http::Method::PATCH),
        Just(http::Method::HEAD),
    ]
}

fn arb_uri_path() -> impl Strategy<Value = String> {
    "[a-z0-9/]{1,32}".prop_map(|s| {
        if s.starts_with('/') {
            s
        } else {
            format!("/{s}")
        }
    })
}

fn arb_extra_headers() -> impl Strategy<Value = Vec<(String, String)>> {
    prop::collection::vec(
        (
            "[a-z][a-z0-9-]{0,15}".prop_filter("not auth-related", |n: &String| {
                let n = n.to_ascii_lowercase();
                !n.starts_with("authorization")
                    && !n.starts_with("x-corelink-tenant")
                    && !n.starts_with("x-tenant")
                    && !n.eq("corelink-tenant")
                    && !n.eq("x-request-id")
            }),
            "[A-Za-z0-9_-]{0,32}",
        ),
        0..6,
    )
}

fn arb_random_bearer_token() -> impl Strategy<Value = String> {
    // Random ASCII printable, 1..120 bytes, NEVER starting with the
    // canonical PAT prefix (drives toward genuinely-malformed inputs).
    "[!-~]{1,120}".prop_filter("non-PAT-prefixed", |s: &String| {
        !s.starts_with("corelink_") && !s.contains(' ') && !s.contains(',')
    })
}

fn arb_jwt_shape() -> impl Strategy<Value = String> {
    // 3 dot-separated base64url segments (non-empty).
    (
        "[A-Za-z0-9_-]{1,32}",
        "[A-Za-z0-9_-]{1,32}",
        "[A-Za-z0-9_-]{1,32}",
    )
        .prop_map(|(a, b, c)| format!("{a}.{b}.{c}"))
}

fn arb_uuid() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(|bytes| {
        let mut b = bytes;
        // Set RFC 4122 variant + version-4 bits to keep the UUID
        // canonical-form-printable.
        b[6] = (b[6] & 0x0F) | 0x40;
        b[8] = (b[8] & 0x3F) | 0x80;
        Uuid::from_bytes(b)
    })
}

fn arb_region() -> impl Strategy<Value = Region> {
    prop_oneof![Just(Region::Wnam), Just(Region::Weur), Just(Region::Sam)]
}

fn arb_scope_mask() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(SCOPE_CACHE_R),
        Just(SCOPE_CACHE_W),
        Just(SCOPE_CACHE_RW),
    ]
}

fn build_request(
    method: http::Method,
    uri_path: String,
    bearer: Option<String>,
    smuggled: Option<(String, String)>,
    extra_headers: Vec<(String, String)>,
) -> Request<Bytes> {
    let mut builder = Request::builder().method(method).uri(uri_path);
    if let Some(b) = bearer {
        builder = builder.header(http::header::AUTHORIZATION, b);
    }
    if let Some((name, value)) = smuggled {
        if let (Ok(hn), Ok(hv)) = (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_str(&value),
        ) {
            builder = builder.header(hn, hv);
        }
    }
    for (n, v) in extra_headers {
        if let (Ok(hn), Ok(hv)) = (HeaderName::try_from(n.as_str()), HeaderValue::from_str(&v)) {
            builder = builder.header(hn, hv);
        }
    }
    builder.body(Bytes::new()).unwrap()
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(ITER))]

    /// (1) Every request lacking an `Authorization` header is rejected
    /// with `COR_AUTH_HEADER_MISSING`; verifier is NEVER invoked;
    /// handler is NEVER invoked.
    #[test]
    fn prop_missing_token_rejected(
        method in arb_method(),
        uri in arb_uri_path(),
        extras in arb_extra_headers(),
        smuggle_idx in 0usize..4,
        smuggle_value in "[A-Za-z0-9_-]{0,40}",
    ) {
        let pat_v = Arc::new(ScriptedPatVerifier::always_invalid());
        let jwt_v = Arc::new(ScriptedJwtVerifier {
            outcome: Arc::new(std::sync::Mutex::new(JwtScript::AlwaysInvalid)),
        });
        let resolver = Arc::new(ScriptedJwtResolver);
        let (state, _tdk) = make_state(pat_v.clone(), jwt_v, resolver, Region::Wnam);
        let spy = SpyHandler::default();

        // Optionally include an `X-Corelink-Tenant-Id` header — it
        // must be ignored when the Authorization header is missing.
        let smuggled_names = ["x-corelink-tenant-id", "x-tenant-id", "x-tenant", "corelink-tenant"];
        let smuggled = smuggled_names.get(smuggle_idx).map(|n| (n.to_string(), smuggle_value.clone()));

        let req = build_request(method, uri, None, smuggled, extras);
        let resp = run_once(state, spy.clone(), req);
        prop_assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        prop_assert_eq!(
            resp.headers().get("x-corelink-error-code").and_then(|h| h.to_str().ok()),
            Some("COR_AUTH_HEADER_MISSING"),
        );
        prop_assert!(!spy.reached.load(Ordering::Relaxed));
        prop_assert_eq!(pat_v.invocations(), 0);
    }

    /// (2) Random non-PAT, non-JWT bearer payloads are rejected with
    /// one of the canonical malformed/ambiguous codes; the handler is
    /// NEVER invoked.
    #[test]
    fn prop_malformed_token_rejected(
        token in arb_random_bearer_token(),
        extras in arb_extra_headers(),
    ) {
        let pat_v = Arc::new(ScriptedPatVerifier::always_invalid());
        let jwt_v = Arc::new(ScriptedJwtVerifier {
            outcome: Arc::new(std::sync::Mutex::new(JwtScript::AlwaysInvalid)),
        });
        let resolver = Arc::new(ScriptedJwtResolver);
        let (state, _tdk) = make_state(pat_v, jwt_v, resolver, Region::Wnam);
        let spy = SpyHandler::default();

        let req = build_request(
            http::Method::GET,
            "/v1/cas/abcd".into(),
            Some(format!("Bearer {token}")),
            None,
            extras,
        );
        let resp = run_once(state, spy.clone(), req);
        prop_assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let code = resp
            .headers()
            .get("x-corelink-error-code")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        // The acceptable codes are the malformed/ambiguous family
        // plus invalid-token (when the random shape happens to be
        // recognisable as a JWT and the verifier returns invalid).
        prop_assert!(
            matches!(
                code,
                "COR_AUTH_HEADER_MALFORMED"
                    | "COR_AUTH_HEADER_AMBIGUOUS"
                    | "COR_AUTH_INVALID_TOKEN",
            ),
            "unexpected error code: {code}",
        );
        prop_assert!(!spy.reached.load(Ordering::Relaxed));
    }

    /// (3) Any JWT-shaped input where the verifier returns Expired is
    /// rejected with `COR_AUTH_TOKEN_EXPIRED`.
    #[test]
    fn prop_expired_token_rejected(jwt in arb_jwt_shape(), extras in arb_extra_headers()) {
        let pat_v = Arc::new(ScriptedPatVerifier::always_invalid());
        let jwt_v = Arc::new(ScriptedJwtVerifier {
            outcome: Arc::new(std::sync::Mutex::new(JwtScript::AlwaysExpired)),
        });
        let resolver = Arc::new(ScriptedJwtResolver);
        let (state, _tdk) = make_state(pat_v, jwt_v, resolver, Region::Wnam);
        let spy = SpyHandler::default();

        let req = build_request(
            http::Method::GET,
            "/v1/cas/abcd".into(),
            Some(format!("Bearer {jwt}")),
            None,
            extras,
        );
        let resp = run_once(state, spy.clone(), req);
        prop_assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        prop_assert_eq!(
            resp.headers().get("x-corelink-error-code").and_then(|h| h.to_str().ok()),
            Some("COR_AUTH_TOKEN_EXPIRED"),
        );
        prop_assert!(!spy.reached.load(Ordering::Relaxed));
    }

    /// (4a) Cross-tenant header smuggling: when the smuggled value
    /// disagrees with the verified tenant, the middleware MUST reject
    /// with `COR_AUTH_INVALID_TOKEN` (folded variant) and the
    /// handler MUST NOT be invoked.
    #[test]
    fn prop_cross_tenant_header_smuggle_rejected(
        verified_tenant in arb_uuid(),
        smuggled_tenant in arb_uuid(),
        principal in arb_uuid(),
        scopes in arb_scope_mask(),
        smuggle_idx in 0usize..4,
        region in arb_region(),
        extras in arb_extra_headers(),
    ) {
        prop_assume!(verified_tenant != smuggled_tenant);
        let pat_v = Arc::new(ScriptedPatVerifier::returning(make_pat_verification(
            verified_tenant, principal, scopes,
        )));
        let jwt_v = Arc::new(ScriptedJwtVerifier {
            outcome: Arc::new(std::sync::Mutex::new(JwtScript::AlwaysInvalid)),
        });
        let resolver = Arc::new(ScriptedJwtResolver);
        let (state, _tdk) = make_state(pat_v.clone(), jwt_v, resolver, region);
        let spy = SpyHandler::default();
        let smuggled_names = ["x-corelink-tenant-id", "x-tenant-id", "x-tenant", "corelink-tenant"];
        let smuggle_name = smuggled_names.get(smuggle_idx).copied().unwrap_or("x-tenant-id");
        let req = build_request(
            http::Method::POST,
            "/v1/cas/foo".into(),
            Some(format!("Bearer {}", canonical_pat_plaintext())),
            Some((smuggle_name.to_string(), smuggled_tenant.to_string())),
            extras,
        );
        let resp = run_once(state, spy.clone(), req);
        prop_assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        prop_assert_eq!(
            resp.headers().get("x-corelink-error-code").and_then(|h| h.to_str().ok()),
            Some("COR_AUTH_INVALID_TOKEN"),
        );
        prop_assert!(!spy.reached.load(Ordering::Relaxed));
        prop_assert_eq!(pat_v.invocations(), 1);
    }

    /// (4b) Matching smuggled header passes through. This is the
    /// dual-arm property that prevents test false-positives where
    /// the rejection arm would fire unconditionally.
    #[test]
    fn prop_cross_tenant_header_matching_passes(
        verified_tenant in arb_uuid(),
        principal in arb_uuid(),
        scopes in arb_scope_mask(),
        region in arb_region(),
    ) {
        let pat_v = Arc::new(ScriptedPatVerifier::returning(make_pat_verification(
            verified_tenant, principal, scopes,
        )));
        let jwt_v = Arc::new(ScriptedJwtVerifier {
            outcome: Arc::new(std::sync::Mutex::new(JwtScript::AlwaysInvalid)),
        });
        let resolver = Arc::new(ScriptedJwtResolver);
        let (state, _tdk) = make_state(pat_v, jwt_v, resolver, region);
        let spy = SpyHandler::default();
        let req = build_request(
            http::Method::POST,
            "/v1/cas/foo".into(),
            Some(format!("Bearer {}", canonical_pat_plaintext())),
            Some(("x-corelink-tenant-id".to_string(), verified_tenant.to_string())),
            vec![],
        );
        let resp = run_once(state, spy.clone(), req);
        prop_assert_eq!(resp.status(), StatusCode::OK);
        prop_assert!(spy.reached.load(Ordering::Relaxed));
        prop_assert_eq!(*spy.last_tenant.lock().unwrap(), Some(verified_tenant));
    }

    /// (5) `AuthCtx` is internally consistent across all 5 layers:
    /// the tenant prefix is always `derive_prefix(tdk, tenant_id)`
    /// (both the auth-layer prefix AND the storage-layer projection)
    /// and the projected `TenantCtx` carries the same tenant id +
    /// region. INV-AUTH-5-LAYER-ORDERING.
    #[test]
    fn prop_5_layer_consistency(
        verified_tenant in arb_uuid(),
        principal in arb_uuid(),
        scopes in arb_scope_mask(),
        region in arb_region(),
    ) {
        let pat_v = Arc::new(ScriptedPatVerifier::returning(make_pat_verification(
            verified_tenant, principal, scopes,
        )));
        let jwt_v = Arc::new(ScriptedJwtVerifier {
            outcome: Arc::new(std::sync::Mutex::new(JwtScript::AlwaysInvalid)),
        });
        let resolver = Arc::new(ScriptedJwtResolver);
        let (state, tdk) = make_state(pat_v, jwt_v, resolver, region);

        // Custom inner service that captures the AuthCtx + builds
        // the TenantCtx projection, returning the captured fields
        // so the property body can assert.
        #[derive(Clone, Default)]
        struct Capture {
            tenant_id: Arc<std::sync::Mutex<Option<Uuid>>>,
            prefix_auth: Arc<std::sync::Mutex<Option<String>>>,
            tenant_id_storage: Arc<std::sync::Mutex<Option<Uuid>>>,
            prefix_storage: Arc<std::sync::Mutex<Option<String>>>,
            region: Arc<std::sync::Mutex<Option<Region>>>,
        }
        impl Service<Request<Bytes>> for Capture {
            type Response = Response<Bytes>;
            type Error = std::convert::Infallible;
            type Future = std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<Self::Response, Self::Error>,
                        > + Send,
                >,
            >;
            fn poll_ready(
                &mut self,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), Self::Error>> {
                std::task::Poll::Ready(Ok(()))
            }
            fn call(&mut self, req: Request<Bytes>) -> Self::Future {
                let tenant_id = Arc::clone(&self.tenant_id);
                let prefix_auth = Arc::clone(&self.prefix_auth);
                let tenant_id_storage = Arc::clone(&self.tenant_id_storage);
                let prefix_storage = Arc::clone(&self.prefix_storage);
                let region = Arc::clone(&self.region);
                Box::pin(async move {
                    if let Some(ctx) = req.extensions().get::<AuthCtx>() {
                        *tenant_id.lock().expect("p") = Some(ctx.tenant_id());
                        *prefix_auth.lock().expect("p") = Some(ctx.tenant_prefix().as_str().to_owned());
                        let tctx = ctx.tenant_ctx();
                        *tenant_id_storage.lock().expect("p") = Some(tctx.tenant_id());
                        *prefix_storage.lock().expect("p") = Some(tctx.prefix().as_str().to_owned());
                        *region.lock().expect("p") = Some(tctx.region());
                    }
                    Ok(Response::new(Bytes::new()))
                })
            }
        }
        let cap = Capture::default();
        let mut svc = AuthLayer::new(state).layer(cap.clone());
        let req = build_request(
            http::Method::GET,
            "/v1/cas/foo".into(),
            Some(format!("Bearer {}", canonical_pat_plaintext())),
            None,
            vec![],
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let resp = rt.block_on(async move { svc.ready().await.unwrap().call(req).await.unwrap() });
        prop_assert_eq!(resp.status(), StatusCode::OK);

        // Layer-1 / Layer-2 consistency: prefix derived from same TDK + tenant.
        let expected_prefix = derive_prefix(&tdk, verified_tenant).as_str().to_owned();
        let auth_prefix = cap.prefix_auth.lock().unwrap().clone().expect("auth prefix");
        let storage_prefix = cap.prefix_storage.lock().unwrap().clone().expect("storage prefix");
        prop_assert_eq!(auth_prefix.clone(), expected_prefix.clone());
        prop_assert_eq!(storage_prefix, expected_prefix);

        // tenant id propagation.
        prop_assert_eq!(*cap.tenant_id.lock().unwrap(), Some(verified_tenant));
        prop_assert_eq!(*cap.tenant_id_storage.lock().unwrap(), Some(verified_tenant));

        // region propagation.
        prop_assert_eq!(*cap.region.lock().unwrap(), Some(region));
    }
}

// Manual tests outside the proptest! macro so they share fixtures.

/// Sanity test that the property test scaffolding exercises the
/// canonical PAT plaintext shape (96 chars, `corelink_pat_*`).
#[test]
fn canonical_pat_plaintext_shape() {
    let pt = canonical_pat_plaintext();
    assert_eq!(pt.len(), 96);
    assert!(pt.starts_with("corelink_pat_"));
    let dots: Vec<_> = pt.match_indices('.').map(|(i, _)| i).collect();
    assert_eq!(dots.len(), 2);
}

/// Smoke test that the `Capture` recipe wires the full Tower stack.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_authlayer_full_stack() {
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001").unwrap();
    let principal = Uuid::parse_str("01938af0-abcd-7123-8456-000000000002").unwrap();
    let pat_v = Arc::new(ScriptedPatVerifier::returning(make_pat_verification(
        tenant,
        principal,
        SCOPE_CACHE_RW,
    )));
    let jwt_v = Arc::new(ScriptedJwtVerifier {
        outcome: Arc::new(std::sync::Mutex::new(JwtScript::AlwaysInvalid)),
    });
    let resolver = Arc::new(ScriptedJwtResolver);
    let (state, _tdk) = make_state(pat_v, jwt_v, resolver, Region::Wnam);
    let spy = SpyHandler::default();
    let mut svc = AuthLayer::new(state).layer(spy.clone());
    let req = Request::builder()
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {}", canonical_pat_plaintext()),
        )
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(*spy.last_tenant.lock().unwrap(), Some(tenant));
}
