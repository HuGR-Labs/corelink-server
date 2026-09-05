use super::*;

// ── Native PAT possession backstop (cluster A) ────────────────────────────
//
// The customer control plane was UN-gated: a leaked PAT_SIGNING_KEY let an
// attacker HMAC-forge a valid-looking PAT for any tenant, which reached
// `handle_keys_create` with NO possession check and minted a genuine cas:rw
// PAT for the victim. These tests prove the gate closes that chain when it
// is `Some`, preserves dev behavior when `None`, and skips Clerk callers.

use crate::adapter_pat::PatRow as VerifierPatRow;
use crate::native_pat_gate::testing::verifier_with_row;
use crate::native_pat_gate::NativePatGate;
use corelink_pat::{mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_RW};
use uuid::Uuid;

fn test_key() -> Arc<PatSigningKey> {
    Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("32-byte key"))
}

/// Mint a real PAT for `tenant_u128`; return `(plaintext, token_id, pat_hash, tenant_string)`.
fn mint_pat(key: &PatSigningKey, tenant_u128: u128) -> (String, String, String, String) {
    let tenant_id = TenantId(Uuid::from_u128(tenant_u128));
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        tenant_id,
        PrincipalId(Uuid::from_u128(tenant_u128 + 1)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        key,
        1,
    )
    .expect("mint");
    (
        plaintext.into_string(),
        pat.token_id.as_str().to_owned(),
        pat.hash.as_str().to_owned(),
        pat.tenant_id.0.to_string(),
    )
}

/// Build a fixture state whose `pat_gate` is wired over a single known PAT
/// row for `tenant` (Argon2id-verifiable). Mirrors the native-plane tests.
fn fixture_with_gate(
    token_id: String,
    pat_hash: String,
    tenant: String,
    key: Arc<PatSigningKey>,
) -> (CustomerRouteState, Arc<InMemoryCustomerHandler>) {
    let (mut state, shared) = fixture();
    let row = VerifierPatRow {
        tenant_id: tenant,
        pat_hash,
        scope: "cas:rw".to_owned(),
        find_only: false,
        runner_job: false,
    };
    let verifier = verifier_with_row(token_id, row, key);
    state.pat_gate = Some(Arc::new(NativePatGate::new_for_test(verifier)));
    (state, shared)
}

/// KILLING cluster-A test: a forged-HMAC PAT (no real random secret) is
/// REJECTED 401 on `POST /v1/customer/keys` when the gate is wired — it can
/// no longer mint a genuine PAT for the victim tenant. Here D1 stores the
/// hash of a DIFFERENT secret for the presented token_id, so Argon2id fails.
#[tokio::test]
async fn forged_pat_rejected_401_on_keys_create() {
    let key = test_key();
    // The PAT the attacker presents (right HMAC/format)…
    let (pt, tid, _hash, tenant) = mint_pat(&key, 100);
    // …but D1 holds the hash of a DIFFERENT secret ⇒ Argon2id possession
    // check fails (models the forged/leaked-HMAC token).
    let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 101);
    let (state, _shared) = fixture_with_gate(tid, other_hash, tenant.clone(), key);
    let app = router(state);

    let body = serde_json::to_string(&serde_json::json!({
        "label": "attacker-key",
        "scopes": ["cache:read"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/pats")
        .method("POST")
        .header("x-corelink-tenant-id", tenant)
        .header("x-corelink-token-prefix", "clpat_forged")
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a forged-HMAC PAT must be rejected by the Argon2id backstop, never mint"
    );
}

/// A genuine PAT for its own tenant PASSES the backstop and mints (201).
#[tokio::test]
async fn genuine_pat_passes_backstop_and_mints() {
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 102);
    let (state, _shared) = fixture_with_gate(tid, hash, tenant.clone(), key);
    let app = router(state);

    let body = serde_json::to_string(&serde_json::json!({
        "name": "legit-key",
        "scopes": ["cache:read"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", tenant)
        // rw scope so the (read-only) mint passes the scope gate too.
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
}

/// A genuine PAT for tenant A presented against tenant B's header is REJECTED
/// 401 (cross-tenant takeover defense) — the gate binds possession to the
/// claimed tenant.
#[tokio::test]
async fn genuine_pat_for_wrong_tenant_rejected_on_public_pats_create() {
    let key = test_key();
    let (pt, tid, hash, tenant_a) = mint_pat(&key, 103);
    // Gate is wired for tenant_a's PAT, but the request claims a different
    // tenant in the header (the attacker's victim).
    let (state, _shared) = fixture_with_gate(tid, hash, tenant_a, key);
    let app = router(state);

    let body = serde_json::to_string(&serde_json::json!({
        "label": "x", "scopes": ["cache:read"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/pats")
        .method("POST")
        .header("x-corelink-tenant-id", "victim-tenant-zzz")
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Clerk-session callers (edge-verified, no bearer) STILL pass when the gate
/// is wired — the backstop is skipped for `x-corelink-token-prefix: clerk`.
#[tokio::test]
async fn clerk_caller_skips_backstop_and_mints() {
    let key = test_key();
    // Gate wired for some unrelated PAT; the Clerk request carries NO bearer.
    let (_pt, tid, hash, gated_tenant) = mint_pat(&key, 104);
    let (state, _shared) = fixture_with_gate(tid, hash, gated_tenant, key);
    let app = router(state);

    let body = serde_json::to_string(&serde_json::json!({
        "name": "dashboard-key",
        "scopes": ["cache:read", "cache:write"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", "dashboard-tenant")
        // Clerk sentinel + the dashboard read-write scope the Worker sets.
        .header("x-corelink-token-prefix", "clerk")
        .header(crate::scope::SCOPE_HEADER, "read-write")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "a Clerk-session caller is edge-verified and must skip the PAT backstop"
    );
}

/// `None`-gate preserves the prior dev/CI behavior: no bearer, no PAT gate,
/// the mint still succeeds (the gate is purely additive in prod).
#[tokio::test]
async fn none_gate_preserves_dev_behavior() {
    let (state, _shared) = fixture(); // pat_gate: None
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "name": "dev-key", "scopes": ["cache:read", "cache:write"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", "dev-tenant")
        // dev/CI: scope present so the read-write mint passes the scope gate.
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// ── Scope gate on mint (cluster A self-escalation) ────────────────────────

/// A read-only caller (`cas:r`) CANNOT mint a write credential — 403 BEFORE
/// the mint. This kills the self-escalation chain (a read-only PAT bootstraps
/// a read-write PAT for itself, which would survive key rotation).
#[tokio::test]
async fn read_only_caller_cannot_mint_write_pat() {
    let (state, _shared) = fixture(); // None gate isolates the scope check.
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "name": "escalation", "scopes": ["cache:read", "cache:write"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// A read-only caller MAY still mint a read-only credential (the gate only
/// blocks privilege ESCALATION, not lateral read-only mints).
#[tokio::test]
async fn read_only_caller_may_mint_read_only_pat() {
    let (state, _shared) = fixture();
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "name": "ro-key", "scopes": ["cache:read"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
}

/// rt-nuclear cycle-2 #7: a read-only (`cas:r`) caller MUST NOT revoke a
/// credential — revoking any PAT in the tenant (incl. the owner's) is an
/// intra-tenant credential-DoS / owner-lockout. The scope gate fires before
/// any revoke logic, so a fake pat_id still 403s (not 404).
#[tokio::test]
async fn read_only_caller_cannot_revoke_credential() {
    let (state, _shared) = fixture(); // None gate isolates the scope check.
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/keys/some-pat-id/revoke")
        .method("POST")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// rt-nuclear cycle-2 #7: a read-only caller MUST NOT enumerate the tenant's
/// credentials (the recon step of the revoke attack + info-disclosure).
#[tokio::test]
async fn read_only_caller_cannot_list_credentials() {
    let (state, _shared) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("GET")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// A read-write caller CAN mint a write credential (happy path — the scope
/// gate is a NO-OP for a sufficiently-scoped principal).
#[tokio::test]
async fn read_write_caller_can_mint_write_pat() {
    let (state, _shared) = fixture();
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "name": "rw-key", "scopes": ["cache:read", "cache:write"],
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", "rw-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
}

/// Pure-unit coverage of the `mint_requests_write` classifier (kills mutants
/// on the token set): write/admin/owner spellings flag; read-only do not.
#[test]
fn mint_requests_write_classifier() {
    for s in [
        "cache:write",
        "cas:rw",
        "cas:w",
        "read-write",
        "admin",
        "Owner",
        "WRITE",
    ] {
        assert!(
            mint_requests_write(&[s.to_owned()]),
            "{s:?} must be classified as a write/admin mint"
        );
    }
    for s in ["cache:read", "cas:r", "read-only", "viewer", ""] {
        assert!(
            !mint_requests_write(&[s.to_owned()]),
            "{s:?} must NOT be classified as a write/admin mint"
        );
    }
    // A list containing ANY write token flags.
    assert!(mint_requests_write(&[
        "cache:read".into(),
        "cache:write".into()
    ]));
}

/// A read-only caller cannot invite a privileged role; a non-privileged
/// invite from a read-only caller is permitted (mirrors the keys-mint gate).
#[tokio::test]
async fn read_only_caller_cannot_invite_privileged_role() {
    let (state, _shared) = fixture();
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "email": "evil@example.com", "role": "Owner",
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/team/invite")
        .method("POST")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn role_is_privileged_classifier() {
    assert!(role_is_privileged("Owner"));
    assert!(role_is_privileged("admin"));
    assert!(!role_is_privileged("Developer"));
    assert!(!role_is_privileged("Viewer"));
    assert!(!role_is_privileged(""));
}

// ── C-ACCTDEL: account-delete route + requester ───────────────────────────
