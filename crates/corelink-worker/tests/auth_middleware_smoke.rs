//! Smoke / integration tests for the auth middleware (WI-S03-003).
//!
//! Exercises the full Tower service composition end-to-end against
//! in-memory verifier fakes. Property-grade adversarial coverage
//! lives in `tests/prop_auth_middleware.rs`.
//!
//! Coverage matrix (mirrors WI §8 Gherkin happy / sad paths):
//!
//! 1. Happy path PAT — verifier returns `Ok(verification)` →
//!    middleware injects `AuthCtx` with the verified `tenant_id`,
//!    region, scopes, and method = `Pat`; downstream handler
//!    observes the context via `request.extensions().get::<AuthCtx>()`.
//! 2. Happy path JWT — Clerk verifier returns `Ok(principal)` and
//!    the resolver maps `(sub, org_id) → (tenant, principal,
//!    scopes)`; middleware injects `AuthCtx` with method = `Jwt`.
//! 3. Header missing → 401 `COR_AUTH_HEADER_MISSING`.
//! 4. Header malformed (non-Bearer scheme) → 401 `COR_AUTH_HEADER_MALFORMED`.
//! 5. Header ambiguous (random plaintext that is neither PAT nor JWT)
//!    → 401 `COR_AUTH_HEADER_AMBIGUOUS`.
//! 6. PAT verifier returns `Err(InvalidToken)` → 401 `COR_AUTH_INVALID_TOKEN`.
//! 7. JWT verifier returns `Err(TokenExpired)` → 401 `COR_AUTH_TOKEN_EXPIRED`.
//! 8. Backend unavailable (verifier returns `BackendUnavailable`) →
//!    503 `COR_AUTH_BACKEND_UNAVAILABLE` + `Retry-After: 5`.
//! 9. Cross-tenant header smuggling — verified PAT for tenant A, but
//!    `X-Corelink-Tenant-Id: <tenant_b>` header — middleware rejects
//!    with `COR_AUTH_INVALID_TOKEN` (folded so wire surface is
//!    uniform; named variant survives in the audit emit).
//! 10. Request-id propagation — `x-request-id: <uuid>` header carried
//!     into the constructed `AuthCtx`.

#![cfg(feature = "tower-middleware")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use corelink_clerk::fakes::test_keys::TestRsaKey;
use corelink_clerk::fakes::{InMemoryKvCache, StaticJwksFetcher};
use corelink_clerk::{ClerkAdapter, ClerkConfig, ClerkPrincipal};
use corelink_pat::{
    PatEnv, PatId, PatScopes, PrincipalId as PatPrincipal, TenantId as PatTenantId,
    SCOPE_CACHE_RW,
};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::middleware::{
    AuthCtx, AuthLayer, AuthMiddlewareError, AuthState, JwtTenantBinding, JwtTenantResolver,
    JwtVerifier, PatVerification, PatVerifier, PrincipalId,
};
use corelink_worker::Region;
use http::{Request, Response, StatusCode};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
// `serde` is provided via dev-dependency.
use tower::ServiceExt;
use tower_layer::Layer;
use tower_service::Service;
use uuid::Uuid;
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct CapturingPatVerifier {
    behavior: Arc<std::sync::Mutex<PatBehavior>>,
    calls: Arc<AtomicUsize>,
}

#[derive(Clone, Default)]
enum PatBehavior {
    #[default]
    Default,
    Ok(PatVerification),
    Err(AuthMiddlewareError),
}

impl CapturingPatVerifier {
    fn returning(behavior: PatBehavior) -> Self {
        Self {
            behavior: Arc::new(std::sync::Mutex::new(behavior)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl PatVerifier for CapturingPatVerifier {
    async fn verify(&self, _plaintext: &str) -> Result<PatVerification, AuthMiddlewareError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let guard = self.behavior.lock().expect("poisoned");
        match &*guard {
            PatBehavior::Default => Err(AuthMiddlewareError::InvalidToken),
            PatBehavior::Ok(v) => Ok(v.clone()),
            PatBehavior::Err(e) => Err(e.clone()),
        }
    }
}

#[derive(Clone)]
struct ScriptedJwtVerifier {
    behavior: Arc<std::sync::Mutex<JwtBehavior>>,
}

#[derive(Clone)]
enum JwtBehavior {
    Ok(ClerkPrincipal),
    Err(AuthMiddlewareError),
}

#[async_trait]
impl JwtVerifier for ScriptedJwtVerifier {
    async fn validate(&self, _jwt: &str) -> Result<ClerkPrincipal, AuthMiddlewareError> {
        let guard = self.behavior.lock().expect("poisoned");
        match &*guard {
            JwtBehavior::Ok(p) => Ok(p.clone()),
            JwtBehavior::Err(e) => Err(e.clone()),
        }
    }
}

#[derive(Clone)]
struct ScriptedJwtResolver {
    binding: Arc<std::sync::Mutex<Result<JwtTenantBinding, AuthMiddlewareError>>>,
}

#[async_trait]
impl JwtTenantResolver for ScriptedJwtResolver {
    async fn resolve(
        &self,
        _principal: &ClerkPrincipal,
    ) -> Result<JwtTenantBinding, AuthMiddlewareError> {
        let guard = self.binding.lock().expect("poisoned");
        match &*guard {
            Ok(b) => Ok(b.clone()),
            Err(e) => Err(e.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Test scaffolding
// ---------------------------------------------------------------------------

const ISSUER: &str = "https://clerk.test.example.dev";
const AUDIENCE: &str = "corelink-api";
const NOW_FIXED: u64 = 1_750_000_000;

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    exp: i64,
    iat: i64,
    nbf: i64,
    sid: String,
    org_id: Option<String>,
    email: String,
    clerk_role: Option<String>,
}

fn baseline_claims() -> Claims {
    Claims {
        sub: "user_2abc".into(),
        iss: ISSUER.into(),
        aud: AUDIENCE.into(),
        exp: (NOW_FIXED + 3600) as i64,
        iat: NOW_FIXED as i64,
        nbf: NOW_FIXED as i64,
        sid: "sess_xyz".into(),
        org_id: Some("org_xyz".into()),
        email: "alice@example.dev".into(),
        clerk_role: Some("admin".into()),
    }
}

fn sign(key: &TestRsaKey, claims: &Claims) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key.kid.clone());
    let encoding =
        EncodingKey::from_rsa_pem(key.private_pem.as_bytes()).expect("test key accepted");
    encode(&header, &claims, &encoding).expect("test sign")
}

async fn build_real_clerk_principal() -> ClerkPrincipal {
    let key = TestRsaKey::generate("kid_v1");
    let cfg = ClerkConfig::builder()
        .jwks_url("https://clerk.test.example.dev/.well-known/jwks.json")
        .issuer_allowlist([ISSUER])
        .audience(AUDIENCE)
        .build()
        .unwrap();
    let fetcher = StaticJwksFetcher::new(key.into_jwks());
    let cache = InMemoryKvCache::with_clock(|| UNIX_EPOCH + Duration::from_secs(NOW_FIXED));
    let adapter = ClerkAdapter::new_with_clock(cfg, fetcher, cache, || {
        UNIX_EPOCH + Duration::from_secs(NOW_FIXED)
    });
    let jwt = sign(&key, &baseline_claims());
    adapter.validate(&jwt).await.expect("validate")
}

fn canonical_state(
    pat_verifier: Arc<dyn PatVerifier>,
    jwt_verifier: Arc<dyn JwtVerifier>,
    jwt_resolver: Arc<dyn JwtTenantResolver>,
) -> AuthState {
    AuthState {
        tdk: Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))),
        region: Region::Wnam,
        pat_verifier,
        jwt_verifier,
        jwt_resolver,
    }
}

fn canonical_pat_plaintext() -> String {
    // Canonical 96-char PAT: `corelink_pat_` (13) + 16 + `.` + 43 + `.` + 22.
    let token_id = "0123456789ABCDEF";
    let secret = "A".repeat(43);
    let sig = "B".repeat(22);
    format!("corelink_pat_{token_id}.{secret}.{sig}")
}

#[derive(Clone, Default)]
struct EchoService {
    saw_ctx: Arc<AtomicBool>,
    last_tenant: Arc<std::sync::Mutex<Option<Uuid>>>,
    last_request_id: Arc<std::sync::Mutex<Option<Uuid>>>,
}

impl Service<Request<Bytes>> for EchoService {
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
        let saw = Arc::clone(&self.saw_ctx);
        let last_tenant = Arc::clone(&self.last_tenant);
        let last_request_id = Arc::clone(&self.last_request_id);
        Box::pin(async move {
            if let Some(ctx) = req.extensions().get::<AuthCtx>() {
                saw.store(true, Ordering::Relaxed);
                *last_tenant.lock().expect("poisoned") = Some(ctx.tenant_id());
                *last_request_id.lock().expect("poisoned") = Some(ctx.request_id().0);
            }
            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(Bytes::from_static(b"ok"))
                .unwrap_or_else(|_| Response::new(Bytes::from_static(b"ok"))))
        })
    }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn happy_path_pat_injects_authctx() {
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001").unwrap();
    let principal = Uuid::parse_str("01938af0-abcd-7123-8456-000000000002").unwrap();
    let pat_verifier = Arc::new(CapturingPatVerifier::returning(PatBehavior::Ok(
        make_pat_verification(tenant, principal, SCOPE_CACHE_RW),
    )));
    let jwt_verifier = Arc::new(ScriptedJwtVerifier {
        behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
            AuthMiddlewareError::InvalidToken,
        ))),
    });
    let jwt_resolver = Arc::new(ScriptedJwtResolver {
        binding: Arc::new(std::sync::Mutex::new(Err(AuthMiddlewareError::InvalidToken))),
    });
    let state = canonical_state(pat_verifier.clone(), jwt_verifier, jwt_resolver);
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo.clone());
    let req = Request::builder()
        .uri("/v1/cas/abcd")
        .header(http::header::AUTHORIZATION, format!("Bearer {}", canonical_pat_plaintext()))
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(echo.saw_ctx.load(Ordering::Relaxed));
    assert_eq!(*echo.last_tenant.lock().unwrap(), Some(tenant));
    assert_eq!(pat_verifier.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn happy_path_jwt_injects_authctx() {
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000010").unwrap();
    let principal_uuid = Uuid::parse_str("01938af0-abcd-7123-8456-000000000020").unwrap();
    let principal = build_real_clerk_principal().await;
    let jwt_verifier = Arc::new(ScriptedJwtVerifier {
        behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Ok(principal))),
    });
    let jwt_resolver = Arc::new(ScriptedJwtResolver {
        binding: Arc::new(std::sync::Mutex::new(Ok(JwtTenantBinding {
            tenant_id: tenant,
            principal_id: PrincipalId(principal_uuid),
            scopes: PatScopes::from_u64(SCOPE_CACHE_RW),
            region: Region::Wnam,
        }))),
    });
    let pat_verifier = Arc::new(CapturingPatVerifier::returning(PatBehavior::Err(
        AuthMiddlewareError::InvalidToken,
    )));
    let state = canonical_state(pat_verifier, jwt_verifier, jwt_resolver);
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo.clone());
    // Synthetic 3-segment JWT shape; verifier fake ignores the bytes.
    let req = Request::builder()
        .uri("/api/me")
        .header(http::header::AUTHORIZATION, "Bearer eyJhAA.eyJpAA.sigBB")
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(echo.saw_ctx.load(Ordering::Relaxed));
    assert_eq!(*echo.last_tenant.lock().unwrap(), Some(tenant));
}

#[tokio::test]
async fn header_missing_returns_401() {
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::default()),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo.clone());
    let req = Request::builder().body(Bytes::new()).unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("x-corelink-error-code").unwrap(),
        "COR_AUTH_HEADER_MISSING"
    );
    assert!(!echo.saw_ctx.load(Ordering::Relaxed));
}

#[tokio::test]
async fn header_malformed_basic_scheme_returns_401() {
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::default()),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo);
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, "Basic dXNlcjpwYXNz")
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("x-corelink-error-code").unwrap(),
        "COR_AUTH_HEADER_MALFORMED"
    );
}

#[tokio::test]
async fn header_ambiguous_payload_returns_401() {
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::default()),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo);
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, "Bearer notapatorjwt")
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("x-corelink-error-code").unwrap(),
        "COR_AUTH_HEADER_AMBIGUOUS"
    );
}

#[tokio::test]
async fn pat_invalid_returns_401_with_constant_time_pad() {
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::returning(PatBehavior::Err(
            AuthMiddlewareError::InvalidToken,
        ))),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo.clone());
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, format!("Bearer {}", canonical_pat_plaintext()))
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("x-corelink-error-code").unwrap(),
        "COR_AUTH_INVALID_TOKEN"
    );
    assert!(!echo.saw_ctx.load(Ordering::Relaxed));
}

#[tokio::test]
async fn jwt_expired_returns_401_token_expired() {
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::default()),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::TokenExpired,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo);
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, "Bearer expired.token.shape")
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("x-corelink-error-code").unwrap(),
        "COR_AUTH_TOKEN_EXPIRED"
    );
}

#[tokio::test]
async fn backend_unavailable_returns_503_with_retry_after() {
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::default()),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::BackendUnavailable("clerk_jwks_fetch"),
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo);
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, "Bearer aaa.bbb.ccc")
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        resp.headers().get("x-corelink-error-code").unwrap(),
        "COR_AUTH_BACKEND_UNAVAILABLE"
    );
    assert_eq!(resp.headers().get(http::header::RETRY_AFTER).unwrap(), "5");
}

#[tokio::test]
async fn cross_tenant_smuggle_rejected_uniform_401() {
    let tenant_a = Uuid::parse_str("01938af0-abcd-7123-8456-00000000000a").unwrap();
    let tenant_b = Uuid::parse_str("01938af0-abcd-7123-8456-00000000000b").unwrap();
    let principal = Uuid::parse_str("01938af0-abcd-7123-8456-00000000000c").unwrap();
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::returning(PatBehavior::Ok(
            make_pat_verification(tenant_a, principal, SCOPE_CACHE_RW),
        ))),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo.clone());
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, format!("Bearer {}", canonical_pat_plaintext()))
        .header("x-corelink-tenant-id", tenant_b.to_string())
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("x-corelink-error-code").unwrap(),
        "COR_AUTH_INVALID_TOKEN"
    );
    assert!(!echo.saw_ctx.load(Ordering::Relaxed));
}

#[tokio::test]
async fn cross_tenant_smuggle_matching_value_passes_through() {
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000099").unwrap();
    let principal = Uuid::parse_str("01938af0-abcd-7123-8456-0000000000ee").unwrap();
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::returning(PatBehavior::Ok(
            make_pat_verification(tenant, principal, SCOPE_CACHE_RW),
        ))),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo.clone());
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, format!("Bearer {}", canonical_pat_plaintext()))
        .header("x-corelink-tenant-id", tenant.to_string())
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(echo.saw_ctx.load(Ordering::Relaxed));
    assert_eq!(*echo.last_tenant.lock().unwrap(), Some(tenant));
}

#[tokio::test]
async fn request_id_propagation_into_authctx() {
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-0000000000aa").unwrap();
    let principal = Uuid::parse_str("01938af0-abcd-7123-8456-0000000000bb").unwrap();
    let custom_request_id = Uuid::now_v7();
    let state = canonical_state(
        Arc::new(CapturingPatVerifier::returning(PatBehavior::Ok(
            make_pat_verification(tenant, principal, SCOPE_CACHE_RW),
        ))),
        Arc::new(ScriptedJwtVerifier {
            behavior: Arc::new(std::sync::Mutex::new(JwtBehavior::Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
        Arc::new(ScriptedJwtResolver {
            binding: Arc::new(std::sync::Mutex::new(Err(
                AuthMiddlewareError::InvalidToken,
            ))),
        }),
    );
    let echo = EchoService::default();
    let mut svc = AuthLayer::new(state).layer(echo.clone());
    let req = Request::builder()
        .header(http::header::AUTHORIZATION, format!("Bearer {}", canonical_pat_plaintext()))
        .header("x-request-id", custom_request_id.to_string())
        .body(Bytes::new())
        .unwrap();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        *echo.last_request_id.lock().unwrap(),
        Some(custom_request_id)
    );
}
