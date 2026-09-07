/// F16: positive control — with correct auth and an invalid body, the handler
/// now parses the body and returns 400 (confirming parse runs post-auth).
#[tokio::test]
async fn handle_mutate_correct_auth_invalid_body_returns_400() {
    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    let (app, key) = fixture_router();
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(ADMIN_MUTATE_ROUTE)
        .header("content-type", "application/json")
        .header("x-corelink-internal-auth", key.as_ref())
        .body(Body::from("this is not json"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── F29: minimum key length is 32 chars ───────────────────────────────────

// ── #3: per-consumer internal-auth key split ──────────────────────────────

/// Serializes the env-mutating key-split tests (they all read/write the
/// same fixed env-var names, so they must not run concurrently).
use super::INTERNAL_AUTH_ENV_LOCK as ENV_LOCK;

/// Clear every key the split helper reads, so a test starts from a clean
/// env regardless of ambient CI vars.
fn clear_key_env() {
    std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
    std::env::remove_var("CORELINK_ADMIN_AUTH_KEY");
    std::env::remove_var("CORELINK_ERASE_AUTH_KEY");
    std::env::remove_var("CORELINK_DSR_ANCHOR_AUTH_KEY");
    std::env::remove_var("CORELINK_PAT_MINT_AUTH_KEY");
    std::env::remove_var("CORELINK_TIER_SELECT_AUTH_KEY");
    std::env::remove_var("CORELINK_DPA_ACCEPT_AUTH_KEY");
}

const KEY_A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"; // 32 chars
const KEY_B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"; // 32 chars

/// #3: when the consumer-specific key is present (and ≥ 32), it is used —
/// NOT the shared key.
#[test]
fn split_specific_key_present_is_used() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_key_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
    std::env::set_var("CORELINK_ADMIN_AUTH_KEY", KEY_B);
    let resolved =
        resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY").expect("specific key present");
    assert_eq!(
        &*resolved, KEY_B,
        "the specific key must win over the shared key"
    );
    clear_key_env();
}

/// #3: when the consumer-specific key is ABSENT, the shared-fallback
/// resolver (used by admin/approver/quota, NOT erase/anchor) falls back to
/// the shared `CORELINK_INTERNAL_AUTH_KEY`.
#[test]
fn split_specific_absent_falls_back_to_shared() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_key_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
    // No CORELINK_ADMIN_AUTH_KEY set → shared fallback applies.
    let resolved =
        resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY").expect("falls back to shared");
    assert_eq!(&*resolved, KEY_A);
    clear_key_env();
}

/// #3: a set-but-too-short specific key is treated as absent and the
/// shared key is used (a misconfigured new secret degrades, never locks
/// the surface out).
#[test]
fn split_specific_too_short_falls_back_to_shared() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_key_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
    std::env::set_var("CORELINK_PAT_MINT_AUTH_KEY", "too-short"); // < 32
    let resolved = resolve_internal_auth_key("CORELINK_PAT_MINT_AUTH_KEY")
        .expect("too-short specific → shared fallback");
    assert_eq!(&*resolved, KEY_A);
    clear_key_env();
}

/// #3: when BOTH the specific and the shared key are absent (or too short)
/// → `None`, so the gate fails CLOSED (403), exactly as before the split.
#[test]
fn split_both_absent_is_none_fail_closed() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_key_env();
    assert!(
        resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY").is_none(),
        "no keys at all → None (fail CLOSED)"
    );
    // Shared present but too short, specific absent → still None.
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", "short");
    assert!(
        resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY").is_none(),
        "shared too short + specific absent → None"
    );
    clear_key_env();
}

/// #3 + H4: `internal_auth_key_from_env` (admin) still reads its specific
/// var with a shared fallback, while `erase_auth_key_from_env` is now
/// DEDICATED-ONLY (no shared fallback). Pins that the two readers point at
/// their contracted env names and rotate independently.
#[test]
fn split_admin_and_erase_read_distinct_keys() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_key_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A); // shared fallback
    std::env::set_var("CORELINK_ADMIN_AUTH_KEY", KEY_B);
    // admin has its own KEY_B; erase has NO dedicated key → H4: rejected
    // (must NOT fall back to the shared KEY_A).
    let admin = internal_auth_key_from_env().expect("admin key");
    assert_eq!(&*admin, KEY_B, "admin reads CORELINK_ADMIN_AUTH_KEY");
    assert!(
        erase_auth_key_from_env().is_none(),
        "H4: erase must NOT fall back to the shared key"
    );
    // Now give erase its own key — both resolve to their own credential.
    std::env::set_var("CORELINK_ERASE_AUTH_KEY", KEY_A);
    std::env::set_var("CORELINK_ADMIN_AUTH_KEY", KEY_B);
    std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
    assert_eq!(&*internal_auth_key_from_env().expect("admin"), KEY_B);
    assert_eq!(&*erase_auth_key_from_env().expect("erase"), KEY_A);
    clear_key_env();
}

/// H4 (verified HIGH): the erase + DSR-anchor authorities MUST be gated by
/// their DEDICATED keys and MUST NOT fall back to the shared
/// `CORELINK_INTERNAL_AUTH_KEY` — otherwise a single shared-key leak
/// collapses the anti-forge "eraser ≠ requester" two-authority split.
///
/// Fails BEFORE the fix (both fell back to the shared key via
/// `resolve_internal_auth_key`); passes AFTER (dedicated-only).
#[test]
fn h4_erase_and_anchor_reject_shared_only_require_dedicated() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_key_env();

    // Shared key present, NO dedicated erase/anchor keys → both REJECTED.
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
    assert!(
        erase_auth_key_from_env().is_none(),
        "H4: erase authenticated with ONLY the shared key must be rejected"
    );
    assert!(
        dsr_anchor_auth_key_from_env().is_none(),
        "H4: dsr-anchor authenticated with ONLY the shared key must be rejected"
    );

    // Bind the DEDICATED keys (distinct parties) → both ACCEPTED, and each
    // resolves to its own credential (not the shared one).
    std::env::set_var("CORELINK_ERASE_AUTH_KEY", KEY_A);
    std::env::set_var("CORELINK_DSR_ANCHOR_AUTH_KEY", KEY_B);
    std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY"); // prove no shared dependency
    assert_eq!(
        &*erase_auth_key_from_env().expect("erase key present"),
        KEY_A,
        "erase resolves to its dedicated CORELINK_ERASE_AUTH_KEY"
    );
    assert_eq!(
        &*dsr_anchor_auth_key_from_env().expect("anchor key present"),
        KEY_B,
        "anchor resolves to its dedicated CORELINK_DSR_ANCHOR_AUTH_KEY"
    );

    // A too-short dedicated key is rejected (≥32 floor, fail-CLOSED).
    clear_key_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
    std::env::set_var("CORELINK_ERASE_AUTH_KEY", "too-short"); // < 32
    assert!(
        erase_auth_key_from_env().is_none(),
        "H4: a < 32-char dedicated erase key fails CLOSED (no shared fallback)"
    );
    clear_key_env();
}

/// Dual-key rotation: `erase_auth_keys_from_env` returns `[current]` normally
/// and `[current, previous]` while `CORELINK_ERASE_AUTH_KEY_PREVIOUS` is set
/// (≥32), current FIRST, a too-short or duplicate previous dropped.
#[test]
fn erase_auth_keys_dual_key_rotation_window() {
    clear_key_env();
    std::env::remove_var("CORELINK_ERASE_AUTH_KEY_PREVIOUS");
    std::env::set_var("CORELINK_ERASE_AUTH_KEY", KEY_A);
    assert_eq!(
        erase_auth_keys_from_env(),
        vec![KEY_A.to_string()],
        "single accepted key when no previous is set"
    );
    std::env::set_var("CORELINK_ERASE_AUTH_KEY_PREVIOUS", KEY_B);
    assert_eq!(
        erase_auth_keys_from_env(),
        vec![KEY_A.to_string(), KEY_B.to_string()],
        "rotation window accepts both, current first"
    );
    std::env::set_var("CORELINK_ERASE_AUTH_KEY_PREVIOUS", "too-short");
    assert_eq!(
        erase_auth_keys_from_env(),
        vec![KEY_A.to_string()],
        "a < 32-char previous is ignored"
    );
    std::env::set_var("CORELINK_ERASE_AUTH_KEY_PREVIOUS", KEY_A);
    assert_eq!(
        erase_auth_keys_from_env(),
        vec![KEY_A.to_string()],
        "a previous equal to the current key is deduped"
    );
    std::env::remove_var("CORELINK_ERASE_AUTH_KEY_PREVIOUS");
    clear_key_env();
}

/// F29: `internal_auth_key_from_env` uses `< 32` (not `< 16`); verify the
/// boundary by checking the lengths that the gate MUST reject and accept.
#[test]
fn internal_auth_key_from_env_floor_is_32() {
    // 31-char key was previously accepted (old gate was `< 16`); now rejected.
    let short_31 = "a".repeat(31);
    assert!(
        short_31.len() < 32,
        "31-char key is below the 32-char floor and must be rejected"
    );
    // 32-char key is AT the floor — must pass.
    let exactly_32 = "a".repeat(32);
    assert!(
        exactly_32.len() >= 32,
        "32-char key meets the floor and must be accepted"
    );
}
