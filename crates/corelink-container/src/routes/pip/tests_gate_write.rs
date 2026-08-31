//! The property: **a write is admitted only when the PAT's own D1 record
//! authorises it, and is billed to the tenant that record names.**
//!
//! This is F27 (two-layer write enforcement) and REV-S3 (authoritative
//! cost attribution), and the two are one property because they are decided
//! by one PAT verification. The gate's first layer is the server-trusted
//! `x-corelink-scope` header the Worker stamps. F27 exists because that
//! header is a claim made ABOUT the caller by another service: if the Worker
//! ever stamps a write scope onto a PAT whose D1 row does not grant one, the
//! header alone would hand out a write. So the gate asks the PAT itself, via
//! `resolve_with_capability` — one HMAC + Argon2id verification against D1,
//! not a redundant second check — and denies unless BOTH layers agree.
//!
//! Every test here therefore sends `x-corelink-scope: cas:rw`. That is not
//! incidental: a PUT without it is refused by the *scope* gate one branch
//! earlier, with its own 403, and would prove nothing about F27. The header
//! is held at "permissive" precisely so the only thing left deciding the
//! outcome is the PAT's D1 row.
//!
//! REV-S3 rides on the same verification. Because the write path has just
//! established the tenant cryptographically, that tenant — not the
//! `x-corelink-tenant-id` header — is the cost-attribution key. A forged
//! header can therefore over-charge only its own tenant on a READ, and
//! nothing at all on a write.
//!
//! The read-only PAT is built the way `adapter_pat`'s own scope-downgrade
//! test builds one: mint the token with `SCOPE_CACHE_RW` and set only the
//! D1 row's `scope` to `cas:r`. That mirrors the real downgrade — a token
//! already in a customer's hands, whose authorisation was narrowed in the
//! database — and it keeps the row the single thing under test.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use axum::http::StatusCode;
use corelink_pat::{mint, PatEnv, PatScopes, PrincipalId, TenantId as PatTenantId, SCOPE_CACHE_RW};
use tower::ServiceExt; // for `.oneshot`
use uuid::Uuid;

use crate::adapter_pat::PatRow;
use crate::tenant_quota::QuotaStore;

use super::tests_support::{
    put, router_with, router_with_quota_observable, test_key, EmptyLookup, FakeKv, FakeMap,
    OneTokenLookup, StubCas, SCOPE_RW,
};
use super::{CasReadHandler, CasWriteHandler, PatVerifier};

/// A wheel-shaped PUT path under a tenant segment the gate will strip.
const WRITE_URI: &str = "/pip/t/simple/requests/";

/// Mint a PAT and seed its D1 row with `row_scope`, returning the plaintext
/// token, the tenant uuid the row names, and a verifier that knows it.
///
/// `row_scope` is the only lever: the token is always minted `SCOPE_CACHE_RW`,
/// so a `cas:r` row is a token whose authorisation was narrowed in D1 after
/// it was issued — which is the case F27 is defending against.
fn pat_with_row_scope(row_scope: &str) -> (String, Uuid, Arc<PatVerifier>) {
    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0xBEEF);
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0xF00D)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        &key,
        1,
    )
    .unwrap();
    let lookup = OneTokenLookup {
        token_id: pat.token_id.as_str().to_owned(),
        row: PatRow {
            tenant_id: pat.tenant_id.0.to_string(),
            pat_hash: pat.hash.as_str().to_owned(),
            scope: row_scope.to_owned(),
            find_only: false,
            runner_job: false,
        },
    };
    let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));
    (plaintext.into_string(), tenant_uuid, verifier)
}

/// The gate-only router (no quota), wired to `verifier`.
fn app_with(verifier: Arc<PatVerifier>) -> axum::Router {
    let cas: Arc<StubCas> = Arc::new(StubCas::default());
    router_with(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(FakeKv::default()),
        verifier,
    )
}

#[tokio::test]
async fn put_without_bearer_token_is_401() {
    // The scope header alone must never be enough to write. With no bearer
    // there is no second layer to consult, so the gate refuses rather than
    // falling back on the header it was given.
    let (_pt, _tenant, verifier) = pat_with_row_scope(SCOPE_RW);
    let resp = app_with(verifier)
        .oneshot(put(WRITE_URI, None, Some(SCOPE_RW), None))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a PUT with a write scope header but no PAT must be 401, not admitted on the header"
    );
}

#[tokio::test]
async fn put_with_read_only_pat_is_403_despite_rw_scope_header() {
    // The F27 case in one request: the Worker-stamped header says the caller
    // may write, and the PAT's D1 row says it may not. The row wins. If this
    // ever returns 2xx, a scope-header mistake upstream has become a write.
    let (pt, _tenant, verifier) = pat_with_row_scope("cas:r");
    let resp = app_with(verifier)
        .oneshot(put(WRITE_URI, Some(&pt), Some(SCOPE_RW), None))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a PAT whose D1 row lacks write capability must be 403 even when the scope header grants it"
    );
}

#[tokio::test]
async fn put_with_unverifiable_pat_is_401_fail_closed() {
    // The resolver collapses every verification failure into an error, and
    // the gate treats any error as a denial. This pins the fail-CLOSED
    // direction: an unresolvable PAT is refused, never waved through on the
    // header that survived the first layer.
    let cas: Arc<StubCas> = Arc::new(StubCas::default());
    let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
    let app = router_with(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(FakeKv::default()),
        verifier,
    );
    let resp = app
        .oneshot(put(
            WRITE_URI,
            Some("corelink_pat_nobody_knows_this"),
            Some(SCOPE_RW),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "an unresolvable PAT on a write must fail closed with 401"
    );
}

#[tokio::test]
async fn put_with_write_capable_pat_clears_the_gate() {
    // Both layers agree, so the gate must hand the request on. pip exposes no
    // client write route, so what comes back is the ROUTER's answer to a PUT
    // it has no handler for — which is the observable difference between
    // "admitted" and "denied here". Asserting the specific downstream status
    // (rather than merely "not 401/403") keeps this from passing vacuously if
    // the request starts failing for some unrelated reason.
    let (pt, _tenant, verifier) = pat_with_row_scope(SCOPE_RW);
    let resp = app_with(verifier)
        .oneshot(put(WRITE_URI, Some(&pt), Some(SCOPE_RW), None))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "a write-capable PAT must clear the gate and reach the router, which has no PUT route"
    );
}

#[tokio::test]
async fn write_is_billed_to_the_pat_tenant_not_the_forged_header() {
    // REV-S3. The request carries a tenant header naming somebody else. The
    // charge must land on the tenant the PAT verification established, and
    // the named victim must have no row at all.
    //
    // Status cannot answer this — $1/op against a fresh tenant is admitted
    // under either label — so the assertion reads the quota store back.
    let (pt, pat_tenant, verifier) = pat_with_row_scope(SCOPE_RW);
    let victim = Uuid::from_u128(0xDEAD).to_string();
    assert_ne!(
        victim,
        pat_tenant.to_string(),
        "the forged header must name a DIFFERENT tenant or this proves nothing"
    );

    let cas: Arc<StubCas> = Arc::new(StubCas::default());
    let (app, store) = router_with_quota_observable(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(FakeKv::default()),
        verifier,
    );
    let _resp = app
        .oneshot(put(WRITE_URI, Some(&pt), Some(SCOPE_RW), Some(&victim)))
        .await
        .unwrap();

    let charged = store.get(&pat_tenant.to_string()).await.unwrap();
    assert!(
        charged.is_some(),
        "the write must be charged to the PAT-derived tenant {pat_tenant}"
    );
    let forged = store.get(&victim).await.unwrap();
    assert!(
        forged.is_none(),
        "a forged x-corelink-tenant-id must not attract the charge (found a row for {victim})"
    );
}

#[tokio::test]
async fn write_has_an_attribution_key_without_any_tenant_header() {
    // The differential that proves where the key comes from. The sibling
    // property file pins that a READ with no tenant header fails CLOSED with
    // 503, because a read runs no PAT verification in this gate and so has no
    // other source for the label. A WRITE with no tenant header must NOT
    // 503 — it already has the PAT-derived tenant. Same gate, same missing
    // header, opposite outcome, and the only difference is the F27 verify.
    let (pt, pat_tenant, verifier) = pat_with_row_scope(SCOPE_RW);
    let cas: Arc<StubCas> = Arc::new(StubCas::default());
    let (app, store) = router_with_quota_observable(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(FakeKv::default()),
        verifier,
    );
    let resp = app
        .oneshot(put(WRITE_URI, Some(&pt), Some(SCOPE_RW), None))
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a write carries its own attribution key, so it must not hit the no-tenant fail-closed arm"
    );
    assert!(
        store.get(&pat_tenant.to_string()).await.unwrap().is_some(),
        "and the key it carried must be the PAT-derived tenant {pat_tenant}"
    );
}
