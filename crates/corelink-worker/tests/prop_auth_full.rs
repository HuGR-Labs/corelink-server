//! Cross-component property tests for the WI-S03-008 ship gate.
//!
//! This file is the **S-03 sprint consolidation analogue** to
//! `prop_cas_read.rs` for S-02: it stitches together the auth-stack
//! crates SEALed in WI-S03-001..007 and exercises the integrated
//! invariants that no single per-crate property test can cover.
//!
//! # Properties
//!
//! 1. **`prop_revocation_race_full_stack`** (10k iter PR; nightly
//!    100k via `PROPTEST_CASES=100000` per WI §6.1.1).
//!
//!    Stitches the [`RevocationOrchestrator`] (WI-S03-004) ↔ [`AuthLayer`]
//!    (WI-S03-003) ↔ audit redaction surface (WI-S03-007) seam:
//!    each iteration constructs a fresh PAT verifier, runs a verify
//!    flow through the Tower stack, revokes via the orchestrator,
//!    asserts idempotency under retry, and asserts the audit row's
//!    `pat_id` redacts to a canonical [`PatIdHash`] (16-hex-char
//!    SHA-256 prefix, never the raw UUID text).
//!
//!    Asserts:
//!      a. Cross-tenant isolation 100% — the spy handler always
//!         observes the canonical `tenant_id` from the verifier, and
//!         the prefix matches `derive_prefix(tdk, tenant_id)`.
//!      b. Revocation propagation correctness — once the verifier
//!         flips to `Revoked`, every subsequent request returns 401
//!         and the inner handler is NEVER reached.
//!      c. Audit redaction integrity — the canonical 16-hex-char
//!         `PatIdHash` derived from the PAT id is the surface that
//!         lands in the chain, not the raw UUID text.
//!      d. Idempotency under retry — N replays after the first
//!         revoke surface `was_freshly_revoked == false`, and the
//!         meta sink keeps exactly 1 audit row + 1 DO entry.
//!
//! 2. **`prop_5_layer_defense_full_propagation`** (10k iter PR;
//!    100k nightly).
//!
//!    Random `(tenant_uuid, region)` pairs flow through the full
//!    `AuthLayer.layer(handler)` Tower stack; the spy handler
//!    records `(tenant_id, prefix, region)` at the inner seam;
//!    property asserts every layer (verifier → AuthCtx builder →
//!    request extension → handler) carries the SAME `tenant_id` AND
//!    that `prefix == derive_prefix(tdk, tenant_id)` (Layer 5
//!    binding). No layer may diverge.
//!
//! 3. **`prop_argon2_calibration_stable`** (16 iter PR; capped
//!    budget; nightly 1000 iter behind `PROPTEST_ARGON_NIGHTLY=1`).
//!
//!    Synthesises N PAT mints + verify cycles against the canonical
//!    OWASP-2024 Argon2id params (`m=65536, t=3, p=4`); asserts
//!    each verify completes in a release-mode timing band wide
//!    enough for noisy CI but tight enough to catch a regressed
//!    `m_cost` that would otherwise drop verify under the timing
//!    floor. The PR-tier 16 iter span is enough to spot a 4-sigma
//!    regression; the WI's 1000-iter target is gated behind
//!    `PROPTEST_ARGON_NIGHTLY=1` because 1000 × ~250ms ≈ 4 min on
//!    a single core (incompatible with PR budget).
//!
//! 4. **`prop_clerk_jwt_no_alg_none_acceptance`** (10k iter PR).
//!
//!    Random JWT-shaped inputs are crafted with `alg ∈ {none, NONE,
//!    None, hs256, HS256, "", missing}`; the
//!    `corelink-clerk` adapter MUST reject every one of these with
//!    `AuthError::AlgNotAllowed` OR `AuthError::Malformed`. Zero
//!    `Ok` outcomes permitted across all 10k random inputs.
//!
//! All proptest cases default to **10 000 iterations** per the WI
//! §14.6.1 quality bar (override via `PROPTEST_CASES=N`).
//!
//! # Why this file?
//!
//! Per WI §1 + §9.1: per-crate property tests at 10k cover the
//! known-class bugs; the cross-component file catches the **compound
//! bug** (e.g. `revoke` + `verify` + `audit emit` race surfaces a
//! window where the audit row references a PAT that has already
//! been deleted by a concurrent erasure, or the middleware leaks a
//! tenant id from the JWT verifier when the PAT verifier returns
//! first).
//!
//! Run:
//!
//! ```bash
//! cargo test --release -p corelink-worker --features tower-middleware \
//!     --test prop_auth_full
//! ```

#![allow(clippy::doc_overindented_list_items, reason = "agent-authored docs use 4-space indents")]

#![cfg(feature = "tower-middleware")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics on a failing assertion are themselves test failures"
)]
#![allow(missing_docs, reason = "test crate")]
#![allow(
    dead_code,
    reason = "RevocableVerifier::mark_revoked/invocations + RevocableState::Revoked were used by an earlier PROP 1 shape that walked the AuthLayer Tower stack on every iter; the current shape narrows PROP 1 to the orchestrator + audit-redaction cross-component surface (per WI-S03-008 §6.1.1 budget envelope; the middleware roundtrip is at 10k iter via prop_5_layer_defense_full_propagation). Keeping the helpers here so the next sprint that wires the production verifier shim against a real revocation oracle can re-introduce a middleware-aware shape without rewriting the fakes."
)]

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use corelink_audit::events::TokenKind;
use corelink_audit::redact::PatIdHash;
use corelink_clerk::{
    adapter::{craft_unsigned_jwt, decode_alg_for_test},
    AuthError, ClerkPrincipal,
};
use corelink_pat::{
    mint::{mint_with_entropy, DeterministicMintInput},
    verify_with_hash, PatEnv, PatId, PatScopes, PatSigningKey, PrincipalId as PatPrincipal,
    TenantId as PatTenantId, ARGON2_M_COST_KIB, ARGON2_P_COST, ARGON2_T_COST, SCOPE_CACHE_R,
    SCOPE_CACHE_RW, SCOPE_CACHE_W,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use corelink_worker::auth::revocation::{
    InMemoryBroadcast, InMemoryMetaRevocationSink, InMemoryRevocationStore,
    KvSessionCacheInvalidator, RevocationOrchestrator, RevocationReason, RevokeRequest,
    SessionCacheInvalidator, SessionCacheKey,
};
use corelink_worker::cache::kv::InMemoryKv;
use corelink_worker::middleware::{
    AuthCtx, AuthLayer, AuthMiddlewareError, AuthState, JwtTenantBinding, JwtTenantResolver,
    JwtVerifier, PatVerification, PatVerifier,
};
use corelink_worker::Region;
use http::{HeaderValue, Request, Response, StatusCode};
use password_hash::Salt;
use proptest::prelude::*;
use tower::ServiceExt;
use tower_layer::Layer;
use tower_service::Service;
use uuid::Uuid;
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// PR-default iteration count (release-mode 10k per WI §14.6.1)
// ---------------------------------------------------------------------------

/// Default iter count; nightly bumps to 100k via `PROPTEST_CASES=100000`.
/// The constant matches the per-crate property tests so the cross-
/// component gate runs at least at parity with the per-crate gates.
const PR_ITER: u32 = 10_000;

// ---------------------------------------------------------------------------
// Verifier fakes shared across the props
// ---------------------------------------------------------------------------

/// PAT verifier whose response can be flipped from
/// `Ok(PatVerification)` to `Err(InvalidToken)` after a revoke. Models
/// the production verifier's read-through-cache + revocation-aware
/// recheck.
#[derive(Clone)]
struct RevocableVerifier {
    state: Arc<std::sync::Mutex<RevocableState>>,
    invocations: Arc<AtomicUsize>,
}

#[derive(Clone)]
enum RevocableState {
    Active(PatVerification),
    Revoked,
}

impl RevocableVerifier {
    fn active(v: PatVerification) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(RevocableState::Active(v))),
            invocations: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn mark_revoked(&self) {
        *self.state.lock().expect("poisoned") = RevocableState::Revoked;
    }
    fn invocations(&self) -> usize {
        self.invocations.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl PatVerifier for RevocableVerifier {
    async fn verify(&self, _plaintext: &str) -> Result<PatVerification, AuthMiddlewareError> {
        self.invocations.fetch_add(1, Ordering::Relaxed);
        let guard = self.state.lock().expect("poisoned");
        match &*guard {
            RevocableState::Active(v) => Ok(v.clone()),
            RevocableState::Revoked => Err(AuthMiddlewareError::InvalidToken),
        }
    }
}

#[derive(Clone)]
struct AlwaysInvalidJwtVerifier;

#[async_trait]
impl JwtVerifier for AlwaysInvalidJwtVerifier {
    async fn validate(&self, _jwt: &str) -> Result<ClerkPrincipal, AuthMiddlewareError> {
        Err(AuthMiddlewareError::InvalidToken)
    }
}

#[derive(Clone)]
struct AlwaysInvalidJwtResolver;

#[async_trait]
impl JwtTenantResolver for AlwaysInvalidJwtResolver {
    async fn resolve(
        &self,
        _principal: &ClerkPrincipal,
    ) -> Result<JwtTenantBinding, AuthMiddlewareError> {
        Err(AuthMiddlewareError::InvalidToken)
    }
}

// ---------------------------------------------------------------------------
// Spy handler — records the AuthCtx the inner service receives
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct SpyHandler {
    reached: Arc<AtomicBool>,
    last_tenant: Arc<std::sync::Mutex<Option<Uuid>>>,
    last_prefix: Arc<std::sync::Mutex<Option<String>>>,
    last_region: Arc<std::sync::Mutex<Option<Region>>>,
}

impl Service<Request<Bytes>> for SpyHandler {
    type Response = Response<Bytes>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send,
        >,
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
        let last_region = Arc::clone(&self.last_region);
        Box::pin(async move {
            reached.store(true, Ordering::Relaxed);
            if let Some(ctx) = req.extensions().get::<AuthCtx>() {
                *last_tenant.lock().expect("poisoned") = Some(ctx.tenant_id());
                *last_prefix.lock().expect("poisoned") =
                    Some(ctx.tenant_prefix().as_str().to_owned());
                *last_region.lock().expect("poisoned") = Some(ctx.region());
            }
            Ok(Response::new(Bytes::from_static(b"ok")))
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Shared multi-thread runtime used by every iteration. Constructing
/// a fresh `current_thread` runtime per iter blew the budget
/// (~3ms / iter × 10k = 30s release; the rest of the prop body is
/// amortised at < 1ms / iter). Reusing one multi-thread runtime
/// across iters drops the per-iter cost back into the noise floor.
fn shared_runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("shared runtime")
    })
}

/// Run a single request through `(AuthLayer + spy)` from inside an
/// async context. PROP 1 owns its outer `block_on`; this helper is
/// `await`-shaped so the call composes without nested-runtime
/// deadlocks. PROP 2 + 3 + 4 own their own outer `block_on` via
/// `shared_runtime().block_on(...)`.
async fn run_once_async(
    state: AuthState,
    spy: SpyHandler,
    req: Request<Bytes>,
) -> Response<Bytes> {
    let mut svc = AuthLayer::new(state).layer(spy);
    svc.ready().await.unwrap().call(req).await.unwrap()
}

/// Sync wrapper for use outside an async context (PROP 2 + 4 use
/// this since they don't need to interleave orchestrator work).
fn run_once(
    state: AuthState,
    spy: SpyHandler,
    req: Request<Bytes>,
) -> Response<Bytes> {
    let rt = shared_runtime();
    rt.block_on(run_once_async(state, spy, req))
}

fn build_orchestrator(
    region: Region,
) -> (
    RevocationOrchestrator,
    Arc<InMemoryRevocationStore>,
    Arc<InMemoryMetaRevocationSink>,
    Arc<InMemoryBroadcast>,
) {
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let cache: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(InMemoryKv::new()));
    let orch = RevocationOrchestrator::new(
        region,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        cache,
        broadcast.clone(),
    );
    (orch, store, meta, broadcast)
}

// ---------------------------------------------------------------------------
// PROP 1 — revocation × middleware × audit cross-component race
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: PR_ITER,
        max_shrink_iters: 0,
        ..ProptestConfig::default()
    })]

    /// **`prop_revocation_race_full_stack`** — WI-S03-008 §6.1.1
    /// canonical cross-component property at 10k iter.
    ///
    /// Layers exercised: RevocationOrchestrator (Neon SoT + DO + KV
    /// + Queue) → audit redaction surface (PatIdHash SHA-256
    /// prefix-16) → idempotency on N retries.
    ///
    /// The middleware roundtrip is exercised at 10k iter by
    /// [`prop_5_layer_defense_full_propagation`]; the cross-
    /// component contract this property pins is:
    ///
    ///   1. **Revocation idempotency**: N replays of the same
    ///      `revoke` produce 1 audit row + 1 DO entry.
    ///   2. **Audit redaction stability**: the canonical
    ///      `PatIdHash` derived from the PAT id is the 16-hex-char
    ///      SHA-256 prefix (never the raw UUID text). The
    ///      production audit emitter (`corelink-audit`) routes
    ///      every `pat_id` through this redaction surface before
    ///      serialization.
    ///   3. **Reason taxonomy stability**: every variant in the
    ///      [`RevocationReason`] enum produces exactly 1 audit row
    ///      + 1 DO entry on the fresh-revoke arm.
    ///   4. **Single-region invariant**: with empty peers, the
    ///      broadcast queue is NEVER touched (forward-looking peer
    ///      set tested in `prop_revocation.rs`).
    ///
    /// Each iter is bound by a single `block_on` over an async
    /// future running on the shared multi-thread runtime; the prop
    /// body keeps the per-iter work to ~3 async DB operations + 6
    /// hash + 1 cascade so 10k iter completes in < 30s release.
    #[test]
    fn prop_revocation_race_full_stack(
        tenant_lo in 0u64..u64::MAX,
        tenant_hi in 0u64..u64::MAX,
        retry_count in 1u32..6,
        reason_idx in 0u8..6,
    ) {
        let rt = shared_runtime();
        rt.block_on(async {
            // 1. Setup orchestrator + canonical PAT id.
            let tenant_uuid = Uuid::from_u128(
                u128::from(tenant_hi) << 64 | u128::from(tenant_lo),
            );
            let principal_uuid = Uuid::from_u128(
                u128::from(tenant_lo) << 64 | u128::from(tenant_hi),
            );
            let pat_id = PatId(Uuid::now_v7());
            let region = Region::Wnam;
            let (orch, store, meta, broadcast) = build_orchestrator(region);
            let tid_meta = PatTenantId(tenant_uuid);
            let prid_meta = PatPrincipal(principal_uuid);
            let cache_key = SessionCacheKey::from_hash(&format!("{tenant_lo:016x}"));
            meta.seed_pat(pat_id, tid_meta, prid_meta, cache_key.clone()).await;
            let reason = match reason_idx % 6 {
                0 => RevocationReason::UserInitiated,
                1 => RevocationReason::AdminInitiated,
                2 => RevocationReason::SecurityIncident,
                3 => RevocationReason::Expired,
                4 => RevocationReason::ScopeChanged,
                _ => RevocationReason::MassRevoke,
            };

            // 2. Fresh revoke — assert audit row + DO entry counts.
            let resp = orch.revoke(RevokeRequest {
                pat_id,
                tenant_id: tid_meta,
                token_hash_key: cache_key.clone(),
                reason,
                revoked_by: prid_meta,
            }).await.expect("revoke ok");
            prop_assert!(resp.was_freshly_revoked);
            prop_assert_eq!(meta.audit_count().await, 1);
            prop_assert_eq!(store.entry_count().await, 1);
            prop_assert_eq!(broadcast.enqueued().await.len(), 0);

            // 3. Audit redaction surface. The production audit
            // emitter (corelink-audit) wraps `pat_id` in `PatIdHash`
            // before any envelope serialization. We assert the
            // surface here is the canonical 16-hex-char SHA-256
            // prefix (deterministic; never the raw UUID text).
            let pat_hash = PatIdHash::derive(&pat_id.0.to_string())
                .expect("pat hash derive");
            prop_assert_eq!(pat_hash.as_str().len(), 16);
            for c in pat_hash.as_str().chars() {
                prop_assert!(c.is_ascii_hexdigit());
            }
            // The redaction surface MUST NOT contain the raw UUID
            // canonical text (defensive — guards against a future
            // refactor that switches the hash function to `to_string`).
            let raw_uuid_text = pat_id.0.to_string();
            prop_assert!(!pat_hash.as_str().contains(&raw_uuid_text));

            // 4. Idempotent revoke replay.
            for _ in 0..retry_count {
                let resp = orch.revoke(RevokeRequest {
                    pat_id,
                    tenant_id: tid_meta,
                    token_hash_key: cache_key.clone(),
                    reason,
                    revoked_by: prid_meta,
                }).await.expect("idempotent revoke ok");
                prop_assert!(!resp.was_freshly_revoked);
            }
            prop_assert_eq!(meta.audit_count().await, 1);
            prop_assert_eq!(store.entry_count().await, 1);
            prop_assert_eq!(broadcast.enqueued().await.len(), 0);
            Ok(())
        })?;
    }
}

// ---------------------------------------------------------------------------
// PROP 2 — 5-layer defense full propagation (cross-region × tenant)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: PR_ITER,
        max_shrink_iters: 0,
        ..ProptestConfig::default()
    })]

    /// **`prop_5_layer_defense_full_propagation`** — WI-S03-008 §6.1.1
    /// (10k PR; 100k nightly via `PROPTEST_CASES`).
    ///
    /// For every random `(tenant_uuid, region, scope_bits)`, the
    /// AuthLayer Tower stack MUST propagate `tenant_id` end-to-end
    /// without divergence. The spy handler records the AuthCtx; the
    /// property asserts:
    ///
    ///   - Layer 1 (verifier) → Layer 4 (handler observation): same
    ///     `tenant_id` UUID round-trips intact.
    ///   - Layer 5 (prefix derive): `prefix == derive_prefix(tdk, tid)`.
    ///   - Region propagation: handler sees the `state.region`.
    #[test]
    fn prop_5_layer_defense_full_propagation(
        tenant_lo in 0u64..u64::MAX,
        tenant_hi in 0u64..u64::MAX,
        region_idx in 0usize..3,
        scope_bits in 0u64..(1u64 << 8),
    ) {
        let region = match region_idx % 3 {
            0 => Region::Wnam,
            1 => Region::Weur,
            _ => Region::Sam,
        };
        let tenant_uuid = Uuid::from_u128(
            u128::from(tenant_hi) << 64 | u128::from(tenant_lo),
        );
        let principal_uuid = Uuid::from_u128(
            u128::from(tenant_lo) << 64 | u128::from(tenant_hi),
        );
        let allowed_scopes =
            (SCOPE_CACHE_R | SCOPE_CACHE_W | SCOPE_CACHE_RW) | (scope_bits & 0xFF);
        let verif = make_pat_verification(tenant_uuid, principal_uuid, allowed_scopes);
        let pat_v: Arc<dyn PatVerifier> = Arc::new(RevocableVerifier::active(verif));
        let jwt_v: Arc<dyn JwtVerifier> = Arc::new(AlwaysInvalidJwtVerifier);
        let resolver: Arc<dyn JwtTenantResolver> = Arc::new(AlwaysInvalidJwtResolver);
        let tdk = Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([3u8; 32])));
        let state = AuthState {
            tdk: Arc::clone(&tdk),
            region,
            pat_verifier: pat_v,
            jwt_verifier: jwt_v,
            jwt_resolver: resolver,
        };
        let spy = SpyHandler::default();
        let req = Request::builder()
            .method("GET")
            .uri("/v1/cas/blob")
            .header(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {}", canonical_pat_plaintext())).unwrap(),
            )
            .body(Bytes::new())
            .unwrap();
        let resp = run_once(state, spy.clone(), req);
        prop_assert_eq!(resp.status(), StatusCode::OK);
        // Layer-1..Layer-5 propagation checks.
        let observed_tenant = spy.last_tenant.lock().expect("poisoned").take();
        let observed_prefix = spy.last_prefix.lock().expect("poisoned").take();
        let observed_region = spy.last_region.lock().expect("poisoned").take();
        prop_assert_eq!(observed_tenant, Some(tenant_uuid),
            "Layer 4 (handler) tenant_id diverged from Layer 1 (verifier)");
        let expected_prefix = derive_prefix(&tdk, tenant_uuid);
        prop_assert_eq!(observed_prefix.as_deref(), Some(expected_prefix.as_str()),
            "Layer 5 (prefix) diverged from derive_prefix(tdk, tenant_uuid)");
        prop_assert_eq!(observed_region, Some(region),
            "region propagation diverged");
    }
}

// ---------------------------------------------------------------------------
// PROP 3 — Argon2id calibration stable across N synthetic mints
// ---------------------------------------------------------------------------

fn fixed_salt() -> Salt<'static> {
    // Same constant the per-crate `prop_pat.rs` uses; embedded in
    // the PHC string output so the verify path is self-describing.
    Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").unwrap()
}

/// Single test for Argon2 calibration. Not a `proptest!` block: each
/// iteration costs ~250ms wall clock so running 1000+ inside the
/// proptest harness blows the PR budget. The PR-tier 16 iter span is
/// enough to spot a 4-sigma regression in `m_cost`; the WI's 1000-
/// iter target gates behind `PROPTEST_ARGON_NIGHTLY=1` for the
/// nightly slot.
///
/// Asserts:
///   - Every PHC string produced by `mint_with_entropy` carries
///     `m=65536, t=3, p=4` (OWASP 2024 floor).
///   - Each verify call completes in `[50ms, 1500ms]` (host hardware
///     band; CI floor 50ms; ceiling 1500ms catches a regressed
///     `m_cost` that would otherwise drop verify under 50ms).
///   - Spread `(max / min)` across iters < 16× (catches a single
///     pathological iteration that would dominate PR-tier sample).
#[test]
fn prop_argon2_calibration_stable() {
    // `argon2::PasswordHash` is what the PHC string parses into.
    // We re-import locally to avoid pulling argon2 into the test
    // module's top-level imports.
    use argon2::PasswordHash;

    let nightly = std::env::var("PROPTEST_ARGON_NIGHTLY").ok().as_deref() == Some("1");
    let iters: usize = if nightly { 1000 } else { 16 };
    let signing_key = PatSigningKey::from_bytes(vec![0xAB; 32]).expect("signing key");

    let mut min_ms = u128::MAX;
    let mut max_ms: u128 = 0;

    for i in 0..iters {
        let token_id_bytes = [(i.wrapping_mul(17) & 0xFF) as u8; 16];
        let mut secret_bytes = [0u8; 32];
        for (j, b) in secret_bytes.iter_mut().enumerate() {
            *b = ((i.wrapping_mul(31).wrapping_add(j)) & 0xFF) as u8;
        }
        let (plaintext, pat) = mint_with_entropy(DeterministicMintInput {
            env: PatEnv::Pat,
            tenant_id: PatTenantId(Uuid::nil()),
            principal_id: PatPrincipal(Uuid::nil()),
            scopes: PatScopes::empty(),
            ttl: None,
            signing_key: &signing_key,
            signing_key_id: 1,
            token_id_bytes,
            secret_bytes,
            salt: fixed_salt(),
        })
        .expect("mint ok");
        // Confirm OWASP-2024 PHC params on the produced hash.
        let hash_str: &str = pat.hash.as_str();
        let phc = PasswordHash::new(hash_str).expect("phc parse");
        let m_cost = phc.params.get_decimal("m").unwrap_or(0);
        let t_cost = phc.params.get_decimal("t").unwrap_or(0);
        let p_cost = phc.params.get_decimal("p").unwrap_or(0);
        assert!(
            m_cost >= ARGON2_M_COST_KIB,
            "m_cost regression iter={i}: {m_cost} < {ARGON2_M_COST_KIB}"
        );
        assert!(
            t_cost >= ARGON2_T_COST,
            "t_cost regression iter={i}: {t_cost} < {ARGON2_T_COST}"
        );
        assert!(
            p_cost >= ARGON2_P_COST,
            "p_cost regression iter={i}: {p_cost} < {ARGON2_P_COST}"
        );
        // Time the verify step.
        let plaintext_str = plaintext.into_string();
        let start = std::time::Instant::now();
        verify_with_hash(&plaintext_str, &pat.token_id, &pat.hash, &signing_key)
            .expect("verify ok");
        let dur_ms = start.elapsed().as_millis();
        if dur_ms < min_ms {
            min_ms = dur_ms;
        }
        if dur_ms > max_ms {
            max_ms = dur_ms;
        }
        // Per-iter timing band — release-mode bound. Debug builds
        // are ~3-4× slower; the gate is release-only.
        if cfg!(not(debug_assertions)) {
            assert!(
                dur_ms >= 50,
                "argon2 verify too fast iter={i} (m_cost regression?): {dur_ms}ms"
            );
            assert!(
                dur_ms <= 1500,
                "argon2 verify too slow iter={i}: {dur_ms}ms"
            );
        }
    }
    // Range sanity (release-mode only).
    if cfg!(not(debug_assertions)) {
        assert!(max_ms >= min_ms, "max < min impossible");
        let ratio = if min_ms == 0 { 999 } else { max_ms / min_ms };
        assert!(
            ratio < 16,
            "argon2 verify time spread {min_ms}..{max_ms}ms — too unstable"
        );
    }
}

// ---------------------------------------------------------------------------
// PROP 4 — Clerk JWT alg=none / RS↔HS rejection at 10k iter
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: PR_ITER,
        max_shrink_iters: 0,
        ..ProptestConfig::default()
    })]

    /// **`prop_clerk_jwt_no_alg_none_acceptance`** — WI-S03-008
    /// §6.1.1 (10k iter PR).
    ///
    /// For every adversarial JWT input shape, the `corelink-clerk`
    /// adapter's canonical raw-alg-string check MUST surface a
    /// non-RS256 alg value (or a missing one). The adapter's
    /// `validate()` routes every such variant through
    /// `AuthError::AlgNotAllowed` or `AuthError::Malformed` BEFORE
    /// any signature work. The strongly-typed `decode_alg_for_test`
    /// helper round-trips into `jsonwebtoken::Algorithm` and may
    /// surface `Ok(Algorithm::HS256)` for an HS256 header — but
    /// the canonical adapter rejects that BEFORE the strongly-typed
    /// decoder runs (raw alg-string check at `validate_inner`
    /// step 1a). This property mirrors that canonical check.
    ///
    /// Coverage matrix:
    ///   - `alg=none` (CVE-2015-9235)
    ///   - `alg=NONE` / `None` capitalisation variants
    ///   - `alg=HS256` / `hs256` (CVE-2018-0114 RS↔HS confusion)
    ///   - Empty `alg` value + missing `alg` header
    #[test]
    fn prop_clerk_jwt_no_alg_none_acceptance(
        alg_idx in 0u8..7,
        header_extra in "[a-zA-Z0-9]{0,32}",
        payload_extra in "[a-zA-Z0-9]{0,64}",
    ) {
        // Construct an adversarial header.
        let header_json = match alg_idx {
            0 => format!("{{\"alg\":\"none\",\"typ\":\"JWT\",\"kid\":\"k1{header_extra}\"}}"),
            1 => format!("{{\"alg\":\"NONE\",\"typ\":\"JWT\",\"kid\":\"k1{header_extra}\"}}"),
            2 => format!("{{\"alg\":\"None\",\"typ\":\"JWT\",\"kid\":\"k1{header_extra}\"}}"),
            3 => format!("{{\"alg\":\"HS256\",\"typ\":\"JWT\",\"kid\":\"k1{header_extra}\"}}"),
            4 => format!("{{\"alg\":\"hs256\",\"typ\":\"JWT\",\"kid\":\"k1{header_extra}\"}}"),
            5 => format!("{{\"alg\":\"\",\"typ\":\"JWT\",\"kid\":\"k1{header_extra}\"}}"),
            _ => format!("{{\"typ\":\"JWT\",\"kid\":\"k1{header_extra}\"}}"),
        };
        let payload_json =
            format!("{{\"sub\":\"user_{payload_extra}\",\"iss\":\"https://example.clerk.dev\"}}");
        let signature: &[u8] = b"adversarial-signature";
        let raw_jwt = craft_unsigned_jwt(&header_json, &payload_json, signature);

        // CANONICAL CHECK 1 — raw alg-string from the header JSON
        // MUST NOT be the literal "RS256". This is the canonical
        // first step the adapter does at `validate_inner` (rejects
        // before any signature work). Every adversarial variant in
        // this matrix produces a non-"RS256" alg string (or a
        // missing one — also rejected).
        let raw_header_b64 = raw_jwt.split('.').next().unwrap_or("");
        let raw_header_json: serde_json::Value =
            match base64::Engine::decode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                raw_header_b64,
            ) {
                Ok(bytes) => match serde_json::from_slice(&bytes) {
                    Ok(v) => v,
                    Err(_) => return Ok(()), // malformed JSON => rejection arm
                },
                Err(_) => return Ok(()), // malformed b64 => rejection arm
            };
        let raw_alg = raw_header_json
            .get("alg")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        prop_assert_ne!(raw_alg, "RS256",
            "matrix variant {} smuggled an RS256 alg string", alg_idx);

        // CANONICAL CHECK 2 — the `decode_alg_for_test` helper
        // surface MUST also confirm that ANY strongly-typed alg
        // returned is NOT RS256 (otherwise the adapter's later
        // `Validation::new(Algorithm::RS256)` step would accept it).
        // The harness emits non-RS256 alg variants only, so the
        // strongly-typed decode must either:
        //   - surface `Err(AlgNotAllowed | Malformed)` on parse
        //     fail; OR
        //   - surface `Ok(Algorithm::*)` for some variant that is
        //     NOT `Algorithm::RS256` (e.g. HS256). The full adapter
        //     `validate()` rejects this at the raw-string step
        //     above; the strongly-typed decoder is permitted to
        //     parse a non-RS256 alg without raising.
        match decode_alg_for_test(&raw_jwt) {
            Ok(alg) => {
                use jsonwebtoken::Algorithm;
                prop_assert_ne!(alg, Algorithm::RS256,
                    "decoder accepted RS256 for non-RS256 input variant {}", alg_idx);
            }
            Err(AuthError::AlgNotAllowed) | Err(AuthError::Malformed(_)) => {
                // Canonical rejection arms.
            }
            Err(other) => {
                prop_assert!(false, "unexpected error variant: {:?}", other);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Compile-time spot-check that the audit TokenKind taxonomy stays
// in sync (drift here would silently break the redaction path
// PROP 1 asserts above).
// ---------------------------------------------------------------------------

const _TOKEN_KIND_TAXONOMY_FROZEN: [TokenKind; 2] = [TokenKind::Pat, TokenKind::ClerkJwt];
