use super::*;
use axum::body::Body;
use axum::http::{self, Request};
use tower::ServiceExt;

fn test_state() -> InternalPatRouteState {
    // 32-byte all-0x42 key — deterministic for tests.
    let key = PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap();
    InternalPatRouteState {
        internal_auth_key: Arc::from("test-internal-auth-key-32-bytes-x"),
        signing_key: Arc::new(key),
        signing_key_id: 1,
        inflight: Arc::new(MintInflightLimiter::new(DEFAULT_MAX_INFLIGHT_MINTS)),
        // Generous rate ceiling so the concurrency/auth tests are not
        // perturbed by the rate gate; the rate gate has dedicated tests.
        rate: Arc::new(MintRateLimiter::new(DEFAULT_MAX_MINTS_PER_WINDOW)),
    }
}

/// Variant of [`test_state`] with a custom in-flight ceiling (red-team #7).
fn test_state_with_inflight(max: u32) -> InternalPatRouteState {
    let key = PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap();
    InternalPatRouteState {
        internal_auth_key: Arc::from("test-internal-auth-key-32-bytes-x"),
        signing_key: Arc::new(key),
        signing_key_id: 1,
        inflight: Arc::new(MintInflightLimiter::new(max)),
        rate: Arc::new(MintRateLimiter::new(DEFAULT_MAX_MINTS_PER_WINDOW)),
    }
}

/// Variant of [`test_state`] with a custom per-window mint RATE cap
/// (cluster G). The in-flight ceiling stays generous so only the RATE
/// gate is exercised.
fn test_state_with_rate(max_per_window: u32) -> InternalPatRouteState {
    let key = PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap();
    InternalPatRouteState {
        internal_auth_key: Arc::from("test-internal-auth-key-32-bytes-x"),
        signing_key: Arc::new(key),
        signing_key_id: 1,
        inflight: Arc::new(MintInflightLimiter::new(DEFAULT_MAX_INFLIGHT_MINTS)),
        rate: Arc::new(MintRateLimiter::new(max_per_window)),
    }
}

fn make_mint_request(auth: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(http::Method::POST)
        .uri("/_internal/pat/mint")
        .header("content-type", "application/json")
        .header("x-corelink-internal-auth", auth)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

#[tokio::test]
async fn rejects_missing_auth_header() {
    let app = router(test_state());
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/_internal/pat/mint")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "tenant_id": Uuid::now_v7(),
                "principal_id": Uuid::now_v7(),
                "scopes": "admin",
                "ttl_seconds": 86400
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_wrong_auth_header() {
    let app = router(test_state());
    let req = make_mint_request(
        "wrong-secret",
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "admin",
            "ttl_seconds": 86400
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn mints_admin_pat_with_correct_auth() {
    let state = test_state();
    let app = router(state.clone());
    let req = make_mint_request(
        &state.internal_auth_key,
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "admin",
            "ttl_seconds": 31536000
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body: MintResponse = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body.token_plaintext.starts_with("corelink_pat_"));
    assert_eq!(body.token_id.len(), 16);
    assert!(body.expires_ms > 0);
    assert!(body.hash.starts_with("$argon2id$"));
}

#[tokio::test]
async fn mints_cas_rw_pat() {
    let state = test_state();
    let app = router(state.clone());
    let req = make_mint_request(
        &state.internal_auth_key,
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "cas:rw",
            "ttl_seconds": 86400
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_unknown_scope() {
    let state = test_state();
    let app = router(state.clone());
    let req = make_mint_request(
        &state.internal_auth_key,
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "bad_scope",
            "ttl_seconds": 86400
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── scope-label reconciliation (mint ⇄ persist CHECK ⇄ bitset) ────────────

/// The canonical `pat.scope` CHECK domain (D1 migration 0037). Every label
/// this route accepts MUST canonicalize into this set, or its persisted row
/// violates the CHECK and the token never authenticates.
const PERSIST_CHECK_DOMAIN: [&str; 3] = ["read-only", "read-write", "admin"];

/// The CHECK-valid `pat.scope` a given mint label persists as — mirrors
/// `worker/src/lib/session_exchange.ts::canonicalizePatScope` and
/// `scripts/admin/mint-dogfood-pat.sh`'s CANON map. `cas:rw` MUST collapse
/// to `read-write` (never the raw `cas:rw`, which fails the CHECK).
fn canonical_persist_label(label: &str) -> &'static str {
    match label {
        "admin" => "admin",
        "cas:rw" | "read-write" => "read-write",
        "read-only" => "read-only",
        other => panic!("label {other:?} has no canonical persist mapping"),
    }
}

#[test]
fn scope_labels_reconcile_bits_and_persist_check() {
    use corelink_pat::{SCOPE_ADMIN, SCOPE_CACHE_W};

    // (label, expects_read, expects_write, expects_admin)
    let cases = [
        ("read-only", true, false, false),
        ("read-write", true, true, false),
        ("cas:rw", true, true, false), // back-compat alias of read-write
        ("admin", true, true, true),
    ];

    for (label, want_read, want_write, want_admin) in cases {
        // 1. Mint route ACCEPTS the label (no 400).
        let bits = scope_label_to_bits(label)
            .unwrap_or_else(|| panic!("mint route must accept label {label:?}"));

        // 2. The resulting bitset is correct — read-only carries cache-READ
        //    but NOT write (and never an admin bit).
        assert_eq!(
            bits.has(SCOPE_CACHE_R),
            want_read,
            "{label}: cache-READ bit"
        );
        assert_eq!(
            bits.has(SCOPE_CACHE_W),
            want_write,
            "{label}: cache-WRITE bit"
        );
        assert_eq!(bits.has(SCOPE_ADMIN), want_admin, "{label}: admin bit");

        // 3. It canonicalizes into the persisted CHECK domain (a
        //    CHECK-valid row) — and `cas:rw` persists as `read-write`.
        let persist = canonical_persist_label(label);
        assert!(
            PERSIST_CHECK_DOMAIN.contains(&persist),
            "{label} → {persist:?} must be in the pat.scope CHECK domain"
        );
        assert_ne!(persist, "cas:rw", "cas:rw must never persist verbatim");
    }
}

#[test]
fn read_only_bits_are_read_not_write() {
    // A witness / read-only credential must NOT require or carry write.
    use corelink_pat::SCOPE_CACHE_W;
    let bits = scope_label_to_bits("read-only").expect("read-only is mintable");
    assert!(bits.has(SCOPE_CACHE_R), "read-only must grant cache READ");
    assert!(
        !bits.has(SCOPE_CACHE_W),
        "read-only must NOT grant cache WRITE"
    );
}

#[test]
fn cas_rw_alias_matches_read_write_bits() {
    assert_eq!(
        scope_label_to_bits("cas:rw").map(PatScopes::to_u64),
        scope_label_to_bits("read-write").map(PatScopes::to_u64),
        "cas:rw must be a pure back-compat alias of read-write"
    );
}

#[tokio::test]
async fn mints_read_only_pat() {
    // read-only was previously REJECTED by the mint route (400); it must now
    // mint end-to-end so a witness/read-only cred needs no admin.
    let state = test_state();
    let app = router(state.clone());
    let req = make_mint_request(
        &state.internal_auth_key,
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "read-only",
            "ttl_seconds": 86400
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn mints_read_write_pat() {
    let state = test_state();
    let app = router(state.clone());
    let req = make_mint_request(
        &state.internal_auth_key,
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "read-write",
            "ttl_seconds": 86400
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── #7: mint concurrency backstop (in-flight limiter) ─────────────────────

#[test]
fn inflight_limiter_admits_up_to_max_then_sheds() {
    let lim = Arc::new(MintInflightLimiter::new(2));
    let s1 = lim.try_acquire().expect("first slot");
    let s2 = lim.try_acquire().expect("second slot");
    // Ceiling reached → third acquire is shed.
    assert!(
        lim.try_acquire().is_none(),
        "over-ceiling acquire must shed"
    );
    // Releasing one slot (drop) frees capacity again.
    drop(s1);
    let s3 = lim.try_acquire().expect("slot freed after drop");
    drop(s2);
    drop(s3);
    // Fully drained → acquires succeed again.
    assert!(lim.try_acquire().is_some());
}

#[test]
fn inflight_limiter_clamps_zero_max_to_one() {
    // A mis-set ceiling of 0 must NOT wedge the surface shut entirely.
    let lim = Arc::new(MintInflightLimiter::new(0));
    let s = lim.try_acquire().expect("zero-max clamps to 1 → one slot");
    assert!(lim.try_acquire().is_none());
    drop(s);
}

#[tokio::test]
async fn mint_sheds_429_when_inflight_ceiling_reached() {
    // Red-team #7: with the ceiling held by an in-flight slot, an
    // authenticated mint is shed with 429 (NOT 200) — proving the loop
    // cannot force unbounded concurrent Argon2id work.
    let state = test_state_with_inflight(1);
    // Hold the single permit so the handler sees a full limiter.
    let _held = state.inflight.try_acquire().expect("hold the only slot");
    let app = router(state.clone());
    let req = make_mint_request(
        &state.internal_auth_key,
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "admin",
            "ttl_seconds": 86400
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn mint_releases_slot_after_completion() {
    // Red-team #7: the RAII slot must be released once the mint returns, so
    // a serial sequence of mints (ceiling 1) all succeed — the limiter
    // bounds CONCURRENCY, not lifetime throughput.
    let state = test_state_with_inflight(1);
    for _ in 0..3 {
        let app = router(state.clone());
        let req = make_mint_request(
            &state.internal_auth_key,
            serde_json::json!({
                "tenant_id": Uuid::now_v7(),
                "principal_id": Uuid::now_v7(),
                "scopes": "cas:rw",
                "ttl_seconds": 86400
            }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "each serial mint must reacquire the freed slot"
        );
    }
}

#[tokio::test]
async fn unauthenticated_flood_does_not_consume_mint_slot() {
    // Red-team #7 interaction with #3 ordering: an UNauthenticated caller is
    // shed at 401 BEFORE a mint permit is taken, so an unauth flood cannot
    // exhaust the in-flight budget and DoS legitimate mints.
    let state = test_state_with_inflight(1);
    let app = router(state.clone());
    // Wrong auth → 401, no permit consumed.
    let bad = make_mint_request(
        "wrong-secret",
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "admin",
            "ttl_seconds": 86400
        }),
    );
    assert_eq!(
        app.oneshot(bad).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    // A subsequent AUTHED mint still has its slot available → 200.
    let app2 = router(state.clone());
    let good = make_mint_request(
        &state.internal_auth_key,
        serde_json::json!({
            "tenant_id": Uuid::now_v7(),
            "principal_id": Uuid::now_v7(),
            "scopes": "admin",
            "ttl_seconds": 86400
        }),
    );
    assert_eq!(app2.oneshot(good).await.unwrap().status(), StatusCode::OK);
}

// ── cluster G: mint RATE limit (fixed window) ─────────────────────────────

#[test]
fn rate_limiter_admits_up_to_cap_then_sheds_within_window() {
    // A fixed clock (no window roll) → the limiter admits exactly
    // `max_per_window` mints then sheds the rest.
    fn frozen_clock() -> u64 {
        1_000_000
    }
    let lim = MintRateLimiter::with_clock(3, MINT_RATE_WINDOW, frozen_clock);
    assert!(lim.try_admit(), "1st admit");
    assert!(lim.try_admit(), "2nd admit");
    assert!(lim.try_admit(), "3rd admit (at cap)");
    assert!(!lim.try_admit(), "4th admit must be shed (over cap)");
    assert!(!lim.try_admit(), "still shed within the same window");
}

#[test]
fn rate_limiter_clamps_zero_cap_to_one() {
    // A mis-set cap of 0 must NOT wedge the surface fully shut.
    fn frozen_clock() -> u64 {
        5_000
    }
    let lim = MintRateLimiter::with_clock(0, MINT_RATE_WINDOW, frozen_clock);
    assert!(lim.try_admit(), "zero cap clamps to 1 → one mint admitted");
    assert!(!lim.try_admit(), "second is shed");
}
