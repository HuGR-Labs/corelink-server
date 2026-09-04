// G4b: tenant-suspend gate tests for the OCI plane. These tests live in a
// sibling module so the production route file remains below the godfile
// threshold; `super::*` retains access to the parent test fixtures and the
// private route constructor without changing runtime visibility.

// This file is included as a child of `oci::tests` via `#[path]`.
use super::*;

// ── G4b: tenant-suspend gate on the OCI plane ──────────────────────────────
//
// Two enforcement points, one invariant: a tenant whose offboarding state ∈
// {suspended, erased} is DENIED push AND pull. (1) the `/token` mint refuses
// to issue a bearer; (2) the `/v2/*` legs 403 a suspended tenant even on a
// bearer minted BEFORE suspension (the TOKEN_TTL_SECS=300s residual window).

/// A [`crate::oci_suspend::SuspendResolver`] fake with a runtime-swappable
/// verdict + a forced-error switch (drives mid-session flip / fail-closed).
#[derive(Debug)]
struct FakeSuspend {
    suspended: std::sync::atomic::AtomicBool,
    error: std::sync::atomic::AtomicBool,
}
impl FakeSuspend {
    fn new(suspended: bool) -> Self {
        Self {
            suspended: std::sync::atomic::AtomicBool::new(suspended),
            error: std::sync::atomic::AtomicBool::new(false),
        }
    }
}
#[async_trait]
impl crate::oci_suspend::SuspendResolver for FakeSuspend {
    async fn suspended_state(&self, _tenant_id: &str) -> Result<bool, String> {
        if self.error.load(Ordering::SeqCst) {
            return Err("d1 down".to_owned());
        }
        Ok(self.suspended.load(Ordering::SeqCst))
    }
}

/// Build a verifier that accepts ONE freshly-minted `cas:rw` PAT for
/// `tenant_uuid`, returning `(verifier, plaintext_pat)`.
fn verifier_and_rw_pat(tenant_uuid: Uuid) -> (Arc<PatVerifier>, String) {
    let key = test_key();
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0x1234)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        &key,
        1,
    )
    .unwrap();
    let pt = plaintext.into_string();
    let lookup = OneTokenLookup {
        token_id: pat.token_id.as_str().to_owned(),
        row: PatRow {
            tenant_id: pat.tenant_id.0.to_string(),
            pat_hash: pat.hash.as_str().to_owned(),
            scope: "cas:rw".to_owned(),
            find_only: false,
            runner_job: false,
        },
    };
    (Arc::new(PatVerifier::new(Arc::new(lookup), key)), pt)
}

/// Build an OCI router with an optional suspend resolver wired into BOTH the
/// `/token` and `/v2` legs (all other gates off).
fn router_with_suspend(
    verifier: Arc<PatVerifier>,
    suspend: Option<Arc<dyn crate::oci_suspend::SuspendResolver>>,
) -> Router {
    let cas: Arc<StubCas> = Arc::new(StubCas::default());
    router(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(OciKvFake::default()),
        verifier,
        SecretWrap::new(OCI_KEY.to_owned()),
        None,
        None,
        None,
        suspend,
    )
}

/// Exchange `pt` at `/token` for a push,pull bearer; returns the bearer.
async fn mint_bearer(app: &Router, pt: &str) -> String {
    let resp = app
        .clone()
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:push,pull",
            Some(&basic(pt)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "active mint must succeed");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["token"].as_str().expect("token field").to_owned()
}

fn v2_get(bearer: &str) -> HttpRequest<Body> {
    req(
        Method::GET,
        "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
        Some(&format!("Bearer {bearer}")),
        None,
    )
}

#[tokio::test]
async fn g4b_suspended_tenant_token_leg_denies_no_bearer() {
    // (1) PRIMARY: a suspended tenant's `/token` exchange is refused → the
    // mint does NOT succeed and no bearer is issued (blocks push AND pull at
    // the door).
    let (verifier, pt) = verifier_and_rw_pat(Uuid::from_u128(0xB10C));
    let app = router_with_suspend(verifier, Some(Arc::new(FakeSuspend::new(true))));
    let resp = app
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:push,pull",
            Some(&basic(&pt)),
            None,
        ))
        .await
        .unwrap();
    assert!(
        !resp.status().is_success(),
        "a suspended tenant must NOT mint a bearer; got {}",
        resp.status()
    );
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    // No usable bearer in the body.
    let has_token = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|j| j.get("token").and_then(|t| t.as_str().map(str::to_owned)))
        .is_some();
    assert!(
        !has_token,
        "no bearer token must be minted for a suspended tenant"
    );
}

#[tokio::test]
async fn g4b_suspended_tenant_v2_leg_is_403() {
    // (2) RESIDUAL: a suspended tenant presenting a valid (elsewhere-minted)
    // bearer to `/v2/*` is 403'd before the op runs. Mint on an ACTIVE router,
    // replay on a SUSPENDED one (both share OCI_KEY, so the bearer verifies).
    let tenant = Uuid::from_u128(0xB11C);
    let (v_active, pt) = verifier_and_rw_pat(tenant);
    let active = router_with_suspend(v_active, None);
    let bearer = mint_bearer(&active, &pt).await;

    let (v_susp, _) = verifier_and_rw_pat(tenant);
    let suspended = router_with_suspend(v_susp, Some(Arc::new(FakeSuspend::new(true))));
    let resp = suspended.oneshot(v2_get(&bearer)).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a suspended tenant must be 403'd on the /v2 plane"
    );
}

#[tokio::test]
async fn g4b_active_tenant_passes_token_and_v2() {
    // (3) An ACTIVE tenant mints a bearer AND passes the /v2 gate (absent blob
    // ⇒ 404, i.e. the request reached the adapter — NOT a 403).
    let (verifier, pt) = verifier_and_rw_pat(Uuid::from_u128(0xAC71));
    let app = router_with_suspend(verifier, Some(Arc::new(FakeSuspend::new(false))));
    let bearer = mint_bearer(&app, &pt).await;
    let resp = app.clone().oneshot(v2_get(&bearer)).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "an active tenant's pull must pass the suspend gate (absent blob ⇒ 404)"
    );
}

#[tokio::test]
async fn g4b_mid_session_suspend_v2_leg_is_403_within_bearer_ttl() {
    // (4) A bearer minted while ACTIVE stays HMAC-valid for TOKEN_TTL_SECS
    // (300s). Suspending mid-session must 403 the SAME bearer on the next /v2
    // op WITHOUT any re-mint — the residual gate closes the leak window.
    let (verifier, pt) = verifier_and_rw_pat(Uuid::from_u128(0x50D5));
    let suspend = Arc::new(FakeSuspend::new(false));
    let app = router_with_suspend(
        verifier,
        Some(Arc::clone(&suspend) as Arc<dyn crate::oci_suspend::SuspendResolver>),
    );
    let bearer = mint_bearer(&app, &pt).await;
    // Active: the bearer pulls fine (404 absent blob).
    let resp = app.clone().oneshot(v2_get(&bearer)).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "active pull reaches adapter"
    );
    // Suspend MID-SESSION (no re-mint) → the same bearer is now 403'd.
    suspend.suspended.store(true, Ordering::SeqCst);
    let resp = app.oneshot(v2_get(&bearer)).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a mid-session suspend must 403 the /v2 plane within the bearer TTL"
    );
}

#[tokio::test]
async fn g4b_fail_closed_known_suspended_read_error_still_denies_v2() {
    // (5) FAIL-CLOSED: once a tenant is KNOWN suspended (cached), a later D1
    // read ERROR keeps it denied. Drive the real CachedSuspendResolver with a
    // fake clock: warm it suspended, then expire the TTL AND force the inner
    // to error — the /v2 gate must STILL 403.
    use crate::oci_suspend::{CachedSuspendResolver, SuspendResolver};
    use crate::wall_clock::{InMemoryFakeWallClock, WallClock};

    let tenant = Uuid::from_u128(0xDEAD);
    let tenant_text = TenantId::from_uuid(tenant).to_canonical_text();

    // Mint a valid bearer on an active router.
    let (v_active, pt) = verifier_and_rw_pat(tenant);
    let active = router_with_suspend(v_active, None);
    let bearer = mint_bearer(&active, &pt).await;

    // A switchable inner behind the production cache.
    let inner = Arc::new(FakeSuspend::new(true));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_781_481_600_000));
    let cached = Arc::new(CachedSuspendResolver::new(
        Arc::clone(&inner) as Arc<dyn SuspendResolver>,
        Arc::clone(&clock) as Arc<dyn WallClock>,
        crate::oci_suspend::DEFAULT_SUSPEND_CACHE_TTL_MS,
    ));
    // Warm the cache as SUSPENDED using the exact canonical text the /v2 gate
    // keys on.
    assert!(cached.suspended_state(&tenant_text).await.unwrap());
    // Now the inner errors AND the TTL has expired — the sticky verdict holds.
    inner.error.store(true, Ordering::SeqCst);
    clock.advance(std::time::Duration::from_millis(
        u64::try_from(crate::oci_suspend::DEFAULT_SUSPEND_CACHE_TTL_MS).unwrap() + 1,
    ));

    let (v_susp, _) = verifier_and_rw_pat(tenant);
    let suspended = router_with_suspend(
        v_susp,
        Some(Arc::clone(&cached) as Arc<dyn SuspendResolver>),
    );
    let resp = suspended.oneshot(v2_get(&bearer)).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "fail-closed: a known-suspended tenant stays 403'd through a D1 read error"
    );
}
