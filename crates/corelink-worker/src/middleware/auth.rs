//! Tower middleware that authenticates an inbound HTTP/gRPC request,
//! constructs an immutable [`AuthCtx`], and injects it into the
//! request extensions for downstream handlers (WI-S03-003).
//!
//! ## Layer ordering (canonical, WI-S03-003 §1)
//!
//! The middleware is structured as a **single Tower layer** that
//! orchestrates the full pipeline in a fixed sequence — there is no
//! way to reorder steps because they are inlined in the service
//! `call` body:
//!
//! 1. **Header extraction** — RFC 6750 `Authorization: Bearer <token>`.
//!    Multi-value rejection is mandatory; missing header is
//!    rejected with [`AuthMiddlewareError::HeaderMissing`].
//! 2. **Cross-tenant smuggling pre-check** — any client-supplied
//!    `X-Corelink-Tenant-Id` / `X-Tenant-Id` header is captured for
//!    post-verify reconciliation; a mismatch with the verified
//!    tenant binding surfaces as
//!    [`AuthMiddlewareError::CrossTenantHeaderRejected`].
//! 3. **Method routing** — token prefix `corelink_` → PAT path;
//!    JWT shape (3 dot-separated base64url segments) → JWT path;
//!    ambiguous → [`AuthMiddlewareError::HeaderAmbiguous`].
//! 4. **Verify** — delegates to [`PatVerifier`] OR [`JwtVerifier`];
//!    failures fold into [`AuthMiddlewareError::InvalidToken`] /
//!    [`AuthMiddlewareError::TokenExpired`].
//! 5. **Constant-time pad on cold path** — every PAT verify failure
//!    path (parse fail, sig mismatch, token_id absent, Argon2id
//!    mismatch) MUST invoke
//!    [`corelink_pat::dummy_verify_for_constant_time`] so the wire
//!    response latency envelope is identical to the warm path. This
//!    is INV-AUTH-CONSTANT-TIME-COLD-PAD.
//! 6. **Tenant smuggling reconciliation** — captured header from
//!    step 2 is compared against the verified `tenant_id` via
//!    [`subtle::ConstantTimeEq`]. Disagreement → reject as
//!    [`AuthMiddlewareError::CrossTenantHeaderRejected`].
//! 7. **AuthCtx construction** — [`AuthCtxBuilder::build`] derives
//!    the tenant prefix from `(TDK, tenant_id)`; resulting context
//!    is immutable.
//! 8. **Inject into request extensions** —
//!    `req.extensions_mut().insert(auth_ctx)`. Downstream handlers
//!    consume via `req.extensions().get::<AuthCtx>()`.
//! 9. **Forward to inner service** — the wrapped Tower service.
//!
//! ## Why constant-time pad on PAT cold path
//!
//! Without padding, the PAT cold path returns in ~5µs (parse fail or
//! token_id-not-in-DB) while the PAT warm path returns in ~250ms
//! (Argon2id verify). An attacker observes the latency envelope and
//! distinguishes "token doesn't exist" from "token exists but was
//! revoked / wrong secret" — that is an existence oracle that
//! enumerates valid `token_id`s in the address space. The mandatory
//! [`corelink_pat::dummy_verify_for_constant_time`] call at every
//! PAT failure site closes the oracle. The JWT path does **not**
//! need padding: JWT verification cost is ~5ms regardless of whether
//! the token is valid (RS256 signature check + claim validation are
//! both constant-time on the cryptographic surface).
//!
//! ## Anti-patterns (do NOT)
//!
//! - **Do not** allow the PAT cold path to skip the dummy verifier
//!   — even when the operator "knows" the parse failed early. The
//!   [`AuthLayer`] hardcodes the call.
//! - **Do not** trust an `X-Corelink-Tenant-Id` header to override
//!   the verified tenant binding. The header is reconciliation
//!   evidence only; the authoritative tenant id always comes from
//!   the verifier.
//! - **Do not** add a "test bypass" path that injects an [`AuthCtx`]
//!   without invoking a verifier. The
//!   [`super::auth_ctx::__test_helpers`] module is gated behind
//!   `#[doc(hidden)]` and `__` prefix; its sole consumer is the
//!   in-crate property test suite.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::sync::Arc;

use async_trait::async_trait;
use http::{header, Request, Response, StatusCode};
use subtle::ConstantTimeEq;
use tower_layer::Layer;
use tower_service::Service;

use corelink_clerk::{ClerkPrincipal, ClerkSessionId, ClerkUserId};
use corelink_pat::{
    dummy_verify_for_constant_time, parse_env, PatEnv, PatId, PatScopes,
    PrincipalId as PatPrincipal, TenantId as PatTenantId,
};
use corelink_tenant_path::TenantDerivationKey;

use crate::middleware::auth_ctx::{AuthCtx, AuthCtxBuilder, AuthMethod, PrincipalId, RequestId};
use crate::middleware::auth_error::AuthMiddlewareError;
use crate::region::Region;

/// Canonical RFC 6750 Bearer scheme prefix (case-insensitive per
/// RFC 7235 §2.1; we lowercase the input before compare).
const BEARER_PREFIX_LOWER: &[u8] = b"bearer ";

/// Canonical PAT plaintext prefix (`corelink_`). Matches
/// [`corelink_pat::PAT_PREFIX_LEN`].
const PAT_LITERAL_PREFIX: &[u8] = b"corelink_";

/// Canonical headers the middleware inspects for cross-tenant
/// smuggling attempts. Any of these — when present and disagreeing
/// with the verified tenant binding — triggers
/// [`AuthMiddlewareError::CrossTenantHeaderRejected`].
const CROSS_TENANT_HEADER_NAMES: &[&str] = &[
    "x-corelink-tenant-id",
    "x-tenant-id",
    "x-tenant",
    "corelink-tenant",
];

/// Canonical request-id header name.
const REQUEST_ID_HEADER: &str = "x-request-id";

/// Outcome of a PAT verifier call. Carries the canonical principal /
/// tenant / scope fields the auth middleware uses to construct the
/// [`AuthCtx`]. The verifier MUST validate that the PAT row is not
/// expired / revoked at the authoritative DB layer; expiry surfaces
/// as `Err(AuthMiddlewareError::InvalidToken)` so the wire response
/// matches the canonical taxonomy.
#[derive(Clone, Debug)]
pub struct PatVerification {
    /// Canonical PAT environment from the validated plaintext.
    pub env: PatEnv,
    /// PAT row primary key (Neon `pat.id`).
    pub pat_id: PatId,
    /// Authenticated tenant binding.
    pub tenant_id: PatTenantId,
    /// Authenticated principal binding.
    pub principal_id: PatPrincipal,
    /// Granted scope bitset.
    pub scopes: PatScopes,
}

/// Async verifier for PAT plaintexts.
///
/// The implementation owns the hot/cold path discipline:
/// - Parse plaintext via [`corelink_pat::parse_plaintext`].
/// - HMAC fast-fail via [`corelink_pat::verify_hmac_only`].
/// - Lookup row by `token_id` (DB-shape under the implementor's
///   control; canonical SoT is Neon `pat`).
/// - Argon2id verify via [`corelink_pat::verify_with_hash`].
/// - **Cold path** (parse fail OR sig mismatch OR token_id-not-in-DB)
///   MUST invoke [`corelink_pat::dummy_verify_for_constant_time`]
///   before returning. The middleware ALSO invokes the dummy
///   verifier in its own error fold as a defense-in-depth pad — the
///   constant-time property holds even if a future verifier impl
///   forgets the pad.
#[async_trait]
pub trait PatVerifier: Send + Sync + 'static {
    /// Verify a PAT plaintext. The input is the raw bytes after the
    /// `Bearer ` header prefix; the verifier owns parsing.
    ///
    /// # Errors
    ///
    /// - [`AuthMiddlewareError::InvalidToken`] for parse fail, sig
    ///   mismatch, Argon2id mismatch, OR row not found — all folded
    ///   uniformly so the wire surface cannot discriminate.
    /// - [`AuthMiddlewareError::BackendUnavailable`] when the DB
    ///   round-trip fails outright (network / timeout / 5xx).
    async fn verify(&self, plaintext: &str) -> Result<PatVerification, AuthMiddlewareError>;
}

/// Async verifier for Clerk JWT bearer tokens.
#[async_trait]
pub trait JwtVerifier: Send + Sync + 'static {
    /// Validate a JWT and return the canonical principal.
    ///
    /// # Errors
    ///
    /// - [`AuthMiddlewareError::InvalidToken`] for sig fail, malformed
    ///   token, issuer / audience mismatch, KID-not-in-JWKS.
    /// - [`AuthMiddlewareError::TokenExpired`] for `exp`-past tokens.
    /// - [`AuthMiddlewareError::BackendUnavailable`] when the JWKS
    ///   endpoint fetch fails.
    async fn validate(&self, jwt: &str) -> Result<ClerkPrincipal, AuthMiddlewareError>;
}

/// Resolves a Clerk principal (`sub` + optional `org_id`) into the
/// canonical CoreLink `(tenant_id, principal_uuid, scopes, region)`
/// tuple. The implementation typically queries the Neon `tenant`
/// table by `org_id` (or by `user_id` for individual-tier
/// principals) and looks up the granted scopes.
///
/// Returning `Err(TenantNotProvisioned)` surfaces as 412 to the
/// client — caller is expected to retry after onboarding lands.
#[async_trait]
pub trait JwtTenantResolver: Send + Sync + 'static {
    /// Resolve a Clerk principal into the canonical tenant binding.
    async fn resolve(
        &self,
        principal: &ClerkPrincipal,
    ) -> Result<JwtTenantBinding, AuthMiddlewareError>;
}

/// Canonical resolved tenant binding for a JWT-authenticated request.
#[derive(Clone, Debug)]
pub struct JwtTenantBinding {
    /// CoreLink tenant UUID (Neon `tenant.id`).
    pub tenant_id: uuid::Uuid,
    /// CoreLink principal UUID (Neon `principal.id` derived from
    /// Clerk `sub`).
    pub principal_id: PrincipalId,
    /// Granted scopes for this Clerk role.
    pub scopes: PatScopes,
    /// Residency region.
    pub region: Region,
}

/// Auth middleware state. Cheap to clone (every collaborator is
/// behind an `Arc`).
#[derive(Clone)]
pub struct AuthState {
    /// Per-region tenant derivation key (HMAC seed).
    pub tdk: Arc<TenantDerivationKey>,
    /// Region this auth middleware instance serves.
    pub region: Region,
    /// PAT verifier (delegate to a concrete implementor in the
    /// host-server crate).
    pub pat_verifier: Arc<dyn PatVerifier>,
    /// JWT verifier (typically a [`corelink_clerk::ClerkAdapter`]
    /// wrapped behind a thin adapter that maps [`corelink_clerk::AuthError`]
    /// → [`AuthMiddlewareError`]).
    pub jwt_verifier: Arc<dyn JwtVerifier>,
    /// JWT tenant resolver (Clerk `sub` / `org_id` → CoreLink tenant).
    pub jwt_resolver: Arc<dyn JwtTenantResolver>,
}

impl std::fmt::Debug for AuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState")
            .field("region", &self.region)
            .field("tdk", &"<redacted>")
            .field("pat_verifier", &"<dyn>")
            .field("jwt_verifier", &"<dyn>")
            .field("jwt_resolver", &"<dyn>")
            .finish()
    }
}

/// Tower [`Layer`] that wraps an inner service with authentication.
#[derive(Clone, Debug)]
pub struct AuthLayer {
    state: AuthState,
}

impl AuthLayer {
    /// Construct a new [`AuthLayer`] from canonical state.
    #[must_use]
    pub fn new(state: AuthState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// Tower service produced by [`AuthLayer`].
#[derive(Clone, Debug)]
pub struct AuthService<S> {
    inner: S,
    state: AuthState,
}

impl<S> AuthService<S> {
    /// Borrow the auth state. Useful for tests asserting the state
    /// was wired correctly.
    #[must_use]
    pub fn state(&self) -> &AuthState {
        &self.state
    }
}

/// Detected canonical auth method shape from the bearer payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedAuth<'a> {
    /// Looks like a CoreLink PAT (`corelink_<env>_*`).
    Pat(&'a str),
    /// Looks like a JWT (3 dot-separated segments, all base64url).
    Jwt(&'a str),
}

/// Detect whether the bearer payload is a PAT or a JWT.
///
/// Returns [`AuthMiddlewareError::HeaderAmbiguous`] when neither
/// shape matches.
pub fn detect_auth_method(payload: &str) -> Result<DetectedAuth<'_>, AuthMiddlewareError> {
    let bytes = payload.as_bytes();
    if bytes.is_empty() {
        return Err(AuthMiddlewareError::HeaderAmbiguous);
    }
    if bytes.len() >= PAT_LITERAL_PREFIX.len() {
        let prefix = bytes.get(..PAT_LITERAL_PREFIX.len()).unwrap_or(&[]);
        if bool::from(prefix.ct_eq(PAT_LITERAL_PREFIX)) {
            return Ok(DetectedAuth::Pat(payload));
        }
    }
    // JWT shape: exactly 3 dot-separated segments, every segment
    // non-empty, every segment base64url-permissible bytes.
    if looks_like_jwt(payload) {
        return Ok(DetectedAuth::Jwt(payload));
    }
    Err(AuthMiddlewareError::HeaderAmbiguous)
}

/// Cheap structural shape check for a JWT — exactly 3 dot-separated
/// non-empty base64url-permissible segments. Does **not** decode
/// or verify; that is the [`JwtVerifier`]'s job.
fn looks_like_jwt(s: &str) -> bool {
    let mut segs = 0usize;
    let mut empty = false;
    for seg in s.split('.') {
        segs += 1;
        if seg.is_empty() {
            empty = true;
        }
        for b in seg.bytes() {
            if !is_base64url_byte(b) {
                return false;
            }
        }
    }
    segs == 3 && !empty
}

#[inline]
const fn is_base64url_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'=')
}

/// Extract the bearer payload from an [`http::HeaderMap`].
///
/// # Errors
///
/// - [`AuthMiddlewareError::HeaderMissing`] when the
///   `Authorization` header is absent.
/// - [`AuthMiddlewareError::HeaderMultiValue`] when more than one
///   `Authorization` header value is present.
/// - [`AuthMiddlewareError::HeaderMalformed`] when the value does
///   not parse as `Bearer <non-empty-token>` (case-insensitive on
///   the scheme).
pub fn extract_bearer(headers: &http::HeaderMap) -> Result<&str, AuthMiddlewareError> {
    let mut iter = headers.get_all(header::AUTHORIZATION).iter();
    let first = iter.next().ok_or(AuthMiddlewareError::HeaderMissing)?;
    if iter.next().is_some() {
        return Err(AuthMiddlewareError::HeaderMultiValue);
    }
    let raw = first
        .to_str()
        .map_err(|_| AuthMiddlewareError::HeaderMalformed)?;
    parse_bearer_value(raw)
}

/// Parse a single `Authorization` header value (case-insensitive
/// scheme; trims a single space between scheme and token).
pub fn parse_bearer_value(raw: &str) -> Result<&str, AuthMiddlewareError> {
    let bytes = raw.as_bytes();
    if bytes.len() <= BEARER_PREFIX_LOWER.len() {
        return Err(AuthMiddlewareError::HeaderMalformed);
    }
    let scheme = bytes
        .get(..BEARER_PREFIX_LOWER.len())
        .ok_or(AuthMiddlewareError::HeaderMalformed)?;
    let mut lowered = [0u8; 7];
    for (i, b) in scheme.iter().enumerate() {
        if let Some(slot) = lowered.get_mut(i) {
            *slot = b.to_ascii_lowercase();
        }
    }
    if !bool::from(lowered.ct_eq(BEARER_PREFIX_LOWER)) {
        return Err(AuthMiddlewareError::HeaderMalformed);
    }
    let payload = raw
        .get(BEARER_PREFIX_LOWER.len()..)
        .ok_or(AuthMiddlewareError::HeaderMalformed)?
        .trim_end();
    if payload.is_empty() || payload.contains(' ') || payload.contains(',') {
        // Trailing comma → multi-value smuggling attempt. Whitespace
        // inside payload → mangled token.
        return Err(AuthMiddlewareError::HeaderMalformed);
    }
    Ok(payload)
}

/// Capture any client-supplied tenant override header, if present.
/// Returns the FIRST recognised header's value; later canonical
/// names are walked but never override an earlier hit.
fn capture_tenant_override(headers: &http::HeaderMap) -> Option<String> {
    for name in CROSS_TENANT_HEADER_NAMES {
        if let Some(value) = headers.get(*name) {
            if let Ok(s) = value.to_str() {
                return Some(s.trim().to_owned());
            }
        }
    }
    None
}

/// Constant-time check that the smuggled tenant override (if any)
/// matches the verified tenant binding. Always emits a deterministic
/// `Choice` value; callers MUST then collapse via `bool::from`.
fn smuggled_tenant_matches(verified: uuid::Uuid, smuggled: &str) -> bool {
    // Render the verified UUID in canonical hyphenated form (the
    // wire-stable surface our APIs document); compare via
    // ConstantTimeEq.
    let canonical = verified.hyphenated().to_string();
    let canonical_bytes = canonical.as_bytes();
    let smuggled_bytes = smuggled.as_bytes();
    // Zero-pad to the longer of the two so ConstantTimeEq has equal
    // lengths; differing input lengths short-circuit to `Choice(0)`
    // in `subtle::ConstantTimeEq` for slices, which is fine for our
    // accept/reject boolean.
    canonical_bytes.ct_eq(smuggled_bytes).into()
}

/// Convert a [`corelink_clerk::AuthError`] into the canonical
/// [`AuthMiddlewareError`] surface.
pub fn map_clerk_error(err: corelink_clerk::AuthError) -> AuthMiddlewareError {
    match err {
        corelink_clerk::AuthError::Expired | corelink_clerk::AuthError::NotYetValid => {
            AuthMiddlewareError::TokenExpired
        }
        corelink_clerk::AuthError::JwksFetchFailed(_) => {
            AuthMiddlewareError::BackendUnavailable("clerk_jwks_fetch")
        }
        corelink_clerk::AuthError::SignatureInvalid
        | corelink_clerk::AuthError::IssuerMismatch { .. }
        | corelink_clerk::AuthError::AudienceMismatch
        | corelink_clerk::AuthError::KidNotInJwks
        | corelink_clerk::AuthError::Malformed(_)
        | corelink_clerk::AuthError::AlgNotAllowed => AuthMiddlewareError::InvalidToken,
    }
}

/// Convert a [`corelink_pat::PatError`] into the canonical
/// [`AuthMiddlewareError`] surface.
pub fn map_pat_error(err: corelink_pat::PatError) -> AuthMiddlewareError {
    match err {
        corelink_pat::PatError::EntropyUnavailable => {
            AuthMiddlewareError::BackendUnavailable("pat_entropy")
        }
        corelink_pat::PatError::HashError(_) | corelink_pat::PatError::SigningKeyTooShort => {
            AuthMiddlewareError::BackendUnavailable("pat_internal")
        }
        corelink_pat::PatError::Malformed | corelink_pat::PatError::InvalidPat => {
            AuthMiddlewareError::InvalidToken
        }
    }
}

/// The boxed future type for the auth middleware. We intentionally
/// box the future because the orchestration body has a non-trivial
/// state machine (PAT vs JWT branch + DB lookup + inner.call) that
/// is cleanest in async sugar.
type AuthFuture<S, ReqBody, ResBody> = Pin<
    Box<
        dyn Future<Output = Result<Response<ResBody>, <S as Service<Request<ReqBody>>>::Error>>
            + Send,
    >,
>;

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for AuthService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ReqBody: Send + Sync + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = AuthFuture<S, ReqBody, ResBody>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        let state = self.state.clone();
        // Canonical Tower clone-and-replace pattern: the `&mut self`
        // we hold lives only until the future is constructed, so we
        // clone the inner service into the future and keep the
        // original ready for the next call.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let outcome = orchestrate_auth(&state, &req).await;
            match outcome {
                Ok(auth_ctx) => {
                    req.extensions_mut().insert(auth_ctx);
                    inner.call(req).await
                }
                Err(err) => Ok(error_response::<ResBody>(&err)),
            }
        })
    }
}

/// Build the canonical wire response for an [`AuthMiddlewareError`].
fn error_response<ResBody: Default>(err: &AuthMiddlewareError) -> Response<ResBody> {
    let status = StatusCode::from_u16(err.http_status()).unwrap_or(StatusCode::UNAUTHORIZED);
    let mut resp = Response::builder()
        .status(status)
        .header("x-corelink-error-code", err.error_code())
        .body(ResBody::default())
        .unwrap_or_else(|_| Response::new(ResBody::default()));
    if matches!(err, AuthMiddlewareError::BackendUnavailable(_)) {
        if let Ok(value) = http::HeaderValue::from_str("5") {
            resp.headers_mut().insert(header::RETRY_AFTER, value);
        }
    }
    if let AuthMiddlewareError::ScopeInsufficient { required, .. } = err {
        if let Ok(value) = http::HeaderValue::from_str(required) {
            resp.headers_mut()
                .insert("x-corelink-scope-required", value);
        }
    }
    resp
}

/// Core orchestration body.
async fn orchestrate_auth<ReqBody>(
    state: &AuthState,
    req: &Request<ReqBody>,
) -> Result<AuthCtx, AuthMiddlewareError> {
    let payload = extract_bearer(req.headers())?;
    let detected = detect_auth_method(payload)?;
    let smuggled_tenant = capture_tenant_override(req.headers());

    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .map(RequestId);

    let (verified_tenant, builder) = match detected {
        DetectedAuth::Pat(plaintext) => {
            // Defense-in-depth env parse — the verifier is also
            // expected to validate, but a malformed-prefix that
            // somehow slipped past `detect_auth_method` would skip
            // the cold-pad without this guard.
            let _ = parse_env(plaintext).map_err(|e| {
                let _ = dummy_verify_for_constant_time(plaintext);
                map_pat_error(e)
            })?;
            let verification = match state.pat_verifier.verify(plaintext).await {
                Ok(v) => v,
                Err(err) => {
                    // Cold-path constant-time pad. Discard result;
                    // we always re-surface the original error.
                    let _ = dummy_verify_for_constant_time(plaintext);
                    return Err(err);
                }
            };
            let principal = PrincipalId(verification.principal_id.0);
            let auth_method = AuthMethod::Pat {
                env: verification.env,
                pat_id: verification.pat_id,
            };
            (
                verification.tenant_id.0,
                AuthCtxBuilder::new(
                    principal,
                    verification.tenant_id.0,
                    state.region,
                    verification.scopes,
                    auth_method,
                    Arc::clone(&state.tdk),
                ),
            )
        }
        DetectedAuth::Jwt(jwt) => {
            let principal = state.jwt_verifier.validate(jwt).await?;
            let binding = state.jwt_resolver.resolve(&principal).await?;
            // DEBT-013 OPT-07: each of these three clones is an
            // `Arc<str>` refcount bump (~5 ns), not a `String::clone`
            // heap allocation (~50-100 ns) — see
            // `corelink-clerk::principal` newtype docs.
            let auth_method = AuthMethod::Jwt {
                clerk_session_id: ClerkSessionId_clone(&principal.session_id),
                user_id: ClerkUserId_clone(&principal.user_id),
                org_id: principal.org_id.clone(),
            };
            (
                binding.tenant_id,
                AuthCtxBuilder::new(
                    binding.principal_id,
                    binding.tenant_id,
                    binding.region,
                    binding.scopes,
                    auth_method,
                    Arc::clone(&state.tdk),
                ),
            )
        }
    };

    if let Some(smuggled) = smuggled_tenant {
        if !smuggled_tenant_matches(verified_tenant, &smuggled) {
            return Err(AuthMiddlewareError::CrossTenantHeaderRejected);
        }
    }

    let mut builder = builder;
    if let Some(rid) = request_id {
        builder = builder.with_request_id(rid);
    }
    Ok(builder.build())
}

// `ClerkSessionId` / `ClerkUserId` constructors are `pub(crate)` in
// the `corelink-clerk` crate so we cannot re-instantiate them from
// outside; the `Clone` impl is the canonical projection. Wrap in
// helper fns so the call site reads cleanly.
//
// **DEBT-013 OPT-07 (2026-05-15)** — the underlying newtype inner
// storage is now `Arc<str>` (see `corelink-clerk::principal`), so
// these helpers compile down to a refcount bump (~5 ns) instead of
// a fresh `String` heap allocation (~50-100 ns). On the JWT auth
// hot path this is called once per request for `clerk_session_id`
// and once for `user_id` — the win is ~100-200 ns/req saved off
// `orchestrate_auth` plus reduced allocator pressure.
#[allow(
    non_snake_case,
    reason = "pseudo-import alias for legibility at call site"
)]
fn ClerkSessionId_clone(id: &ClerkSessionId) -> ClerkSessionId {
    id.clone()
}

#[allow(
    non_snake_case,
    reason = "pseudo-import alias for legibility at call site"
)]
fn ClerkUserId_clone(id: &ClerkUserId) -> ClerkUserId {
    id.clone()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn detect_auth_pat_prefix() {
        let pat = "corelink_pat_ABCDEFGH12345678.\
                   AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.\
                   BBBBBBBBBBBBBBBBBBBBBB";
        assert!(matches!(detect_auth_method(pat), Ok(DetectedAuth::Pat(_))));
    }

    #[test]
    fn detect_auth_jwt_three_segments() {
        let jwt = "abc.def.ghi";
        assert!(matches!(detect_auth_method(jwt), Ok(DetectedAuth::Jwt(_))));
    }

    #[test]
    fn detect_auth_ambiguous_short_string() {
        assert!(matches!(
            detect_auth_method("notajwt"),
            Err(AuthMiddlewareError::HeaderAmbiguous)
        ));
    }

    #[test]
    fn detect_auth_ambiguous_two_segments() {
        assert!(matches!(
            detect_auth_method("aa.bb"),
            Err(AuthMiddlewareError::HeaderAmbiguous)
        ));
    }

    #[test]
    fn detect_auth_ambiguous_empty_segment() {
        assert!(matches!(
            detect_auth_method("aa..bb"),
            Err(AuthMiddlewareError::HeaderAmbiguous)
        ));
    }

    #[test]
    fn parse_bearer_lowercase_scheme() {
        let payload = parse_bearer_value("bearer abc.def.ghi").expect("ok");
        assert_eq!(payload, "abc.def.ghi");
    }

    #[test]
    fn parse_bearer_mixed_case_scheme() {
        let payload = parse_bearer_value("BeArEr abc.def.ghi").expect("ok");
        assert_eq!(payload, "abc.def.ghi");
    }

    #[test]
    fn parse_bearer_rejects_no_token() {
        assert!(matches!(
            parse_bearer_value("Bearer "),
            Err(AuthMiddlewareError::HeaderMalformed)
        ));
    }

    #[test]
    fn parse_bearer_rejects_other_scheme() {
        assert!(matches!(
            parse_bearer_value("Basic dXNlcjpwYXNz"),
            Err(AuthMiddlewareError::HeaderMalformed)
        ));
    }

    #[test]
    fn parse_bearer_rejects_comma_smuggle() {
        assert!(matches!(
            parse_bearer_value("Bearer abc,def"),
            Err(AuthMiddlewareError::HeaderMalformed)
        ));
    }

    #[test]
    fn smuggled_match_canonical_uuid() {
        let id = uuid::Uuid::parse_str("01938af0-abcd-7123-8456-000000000001").expect("uuid");
        assert!(smuggled_tenant_matches(
            id,
            "01938af0-abcd-7123-8456-000000000001"
        ));
        assert!(!smuggled_tenant_matches(
            id,
            "01938af0-abcd-7123-8456-000000000002"
        ));
        assert!(!smuggled_tenant_matches(id, ""));
    }

    #[test]
    fn map_clerk_error_table() {
        use corelink_clerk::AuthError;
        assert_eq!(
            map_clerk_error(AuthError::Expired),
            AuthMiddlewareError::TokenExpired,
        );
        assert_eq!(
            map_clerk_error(AuthError::SignatureInvalid),
            AuthMiddlewareError::InvalidToken,
        );
        assert!(matches!(
            map_clerk_error(AuthError::JwksFetchFailed("net".into())),
            AuthMiddlewareError::BackendUnavailable(_)
        ));
    }

    #[test]
    fn map_pat_error_table() {
        use corelink_pat::PatError;
        assert_eq!(
            map_pat_error(PatError::Malformed),
            AuthMiddlewareError::InvalidToken,
        );
        assert_eq!(
            map_pat_error(PatError::InvalidPat),
            AuthMiddlewareError::InvalidToken,
        );
        assert!(matches!(
            map_pat_error(PatError::EntropyUnavailable),
            AuthMiddlewareError::BackendUnavailable(_)
        ));
    }
}
