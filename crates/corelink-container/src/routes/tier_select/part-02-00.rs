use super::*;
use axum::http::HeaderValue;

const TEST_SHARED_KEY: &str = "shared-key-000000000000000000000";
const TEST_DEDICATED_KEY: &str = "dedicated-key-000000000000000000";

fn clear_auth_env() {
    std::env::remove_var(TIER_SELECT_AUTH_KEY_ENV);
    std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
}

#[test]
fn tier_select_auth_resolution_is_dedicated_first_and_32_char_fail_closed() {
    let _guard = crate::routes::admin::INTERNAL_AUTH_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_auth_env();

    // Dedicated beats shared: mutating the call back to the old raw
    // shared read makes this assertion fail.
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", TEST_SHARED_KEY);
    std::env::set_var(TIER_SELECT_AUTH_KEY_ENV, TEST_DEDICATED_KEY);
    assert_eq!(
        tier_select_auth_key_from_env().as_deref(),
        Some(TEST_DEDICATED_KEY)
    );

    // The migration fallback remains available when dedicated is absent.
    std::env::remove_var(TIER_SELECT_AUTH_KEY_ENV);
    assert_eq!(
        tier_select_auth_key_from_env().as_deref(),
        Some(TEST_SHARED_KEY)
    );

    // A 16-character key must not mount the money path, even if it is the
    // dedicated value; this kills a mutation restoring the former <16 gate.
    std::env::set_var(TIER_SELECT_AUTH_KEY_ENV, "1234567890123456");
    std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
    assert!(tier_select_auth_key_from_env().is_none());

    // A present but short dedicated value fails closed instead of falling
    // back to a valid shared key.
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", TEST_SHARED_KEY);
    assert!(tier_select_auth_key_from_env().is_none());
    clear_auth_env();
}

fn state() -> TierSelectRouteState {
    // These tests exercise ONLY the side-effect-free auth/tier gate
    // (`authorize_and_validate`), which reads ONLY `internal_auth_key`.
    // The collaborators are INERT fixtures (never called here); the
    // durable orchestration is covered separately by the in-memory
    // `MemStore`/`SpyCheckout`/`SpyAudit` tests below.
    TierSelectRouteState {
        internal_auth_key: Arc::from("super-secret-internal-key"),
        store: Arc::new(crate::routes::tier_select_store::D1HttpTierSelectStore::for_test()),
        checkout: Arc::new(crate::routes::tier_select_checkout::StripeCheckoutCreator::for_test()),
        audit: Arc::new(crate::routes::tier_select_audit::TierSelectAuditAdapter::for_test()),
        current_dpa_version: Arc::from("v3"),
    }
}

fn req(tier: &str) -> TierSelectRequest {
    TierSelectRequest {
        tier: tier.to_owned(),
        success_url: "https://humangr.com/corelink/en/upgraded?session_id={CHECKOUT_SESSION_ID}"
            .to_owned(),
        cancel_url: "https://humangr.com/corelink/en/pricing".to_owned(),
    }
}

fn headers(auth: Option<&str>, tenant: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some(a) = auth {
        h.insert(INTERNAL_AUTH_HEADER, HeaderValue::from_str(a).unwrap());
    }
    if let Some(t) = tenant {
        h.insert(TENANT_HEADER, HeaderValue::from_str(t).unwrap());
    }
    h
}

#[test]
fn rejects_missing_internal_auth() {
    let h = headers(None, Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::Unauthenticated);
}

#[test]
fn rejects_wrong_internal_auth() {
    let h = headers(Some("wrong-key"), Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::Unauthenticated);
}

// ── F2: length-oracle on the internal-auth secret ────────────────────────
// `subtle::ct_eq` short-circuits on unequal lengths; the previous direct
// `ct_eq` here leaked the secret length via timing. These mirror
// `internal_pat.rs`'s padded-ct tests: a wrong same-length, a SHORTER, a
// LONGER (correct-prefix), and a strict-prefix value must ALL be rejected
// on the SAME constant-time path — no branch distinguishes "wrong length"
// from "wrong content".
const F2_KEY: &str = "super-secret-internal-key";

#[test]
fn f2_rejects_wrong_same_length_internal_auth() {
    let wrong: String = "X".repeat(F2_KEY.len());
    assert_eq!(wrong.len(), F2_KEY.len());
    let h = headers(Some(&wrong), Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::Unauthenticated);
}

#[test]
fn f2_rejects_shorter_internal_auth() {
    let h = headers(Some("short"), Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::Unauthenticated);
}

#[test]
fn f2_rejects_longer_correct_prefix_internal_auth() {
    let longer = format!("{F2_KEY}-extra-trailing-bytes");
    assert!(longer.starts_with(F2_KEY));
    let h = headers(Some(&longer), Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::Unauthenticated);
}

#[test]
fn f2_rejects_strict_prefix_internal_auth() {
    let prefix = &F2_KEY[..F2_KEY.len() - 3];
    let h = headers(Some(prefix), Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::Unauthenticated);
}

#[test]
fn f2_accepts_exact_internal_auth() {
    // The correct secret still authenticates on the padded-ct path.
    let h = headers(Some(F2_KEY), Some("tenant-abc"));
    let (tenant, _tier) = authorize_and_validate(&state(), &h, &req("pro")).unwrap();
    assert_eq!(tenant, "tenant-abc");
}

#[test]
fn rejects_missing_verified_tenant_even_with_valid_auth() {
    // Fail-CLOSED: valid internal auth but NO verified tenant header
    // must NOT proceed (no `_unknown` default).
    let h = headers(Some("super-secret-internal-key"), None);
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::NoVerifiedTenant);
}

#[test]
fn rejects_empty_verified_tenant() {
    let h = headers(Some("super-secret-internal-key"), Some("   "));
    let e = authorize_and_validate(&state(), &h, &req("pro")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::NoVerifiedTenant);
}

#[test]
fn enterprise_routes_to_inquiry_form() {
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("enterprise")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::UseInquiryForm);
}

#[test]
fn rejects_unknown_tier() {
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let e = authorize_and_validate(&state(), &h, &req("platinum")).unwrap_err();
    assert_eq!(e, TierSelectHttpError::BadRequest);
}

#[test]
fn rejects_non_https_redirect_for_paid_tier() {
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut bad = req("pro");
    bad.success_url = "http://evil.example/upgraded".to_owned();
    let e = authorize_and_validate(&state(), &h, &bad).unwrap_err();
    assert_eq!(e, TierSelectHttpError::BadRequest);
}

// ── F6/F10: redirect URL host allowlist ─────────────────────────────────
// The scheme-only check was replaced with a strict host allowlist
// (ALLOWED_REDIRECT_HOSTS). These tests pin: (a) off-list https host
// rejected, (b) userinfo rejected, (c) explicit port rejected, (d)
// cancel_url off-list rejected, (e) both on-list accepted.

#[test]
fn f6_rejects_https_success_url_with_off_allowlist_host() {
    // An attacker-controlled https URL passes the old scheme check but
    // must be rejected by the new allowlist gate (F6/F10 fix).
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut bad = req("pro");
    bad.success_url = "https://attacker.example/steal?s={CHECKOUT_SESSION_ID}".to_owned();
    let e = authorize_and_validate(&state(), &h, &bad).unwrap_err();
    assert_eq!(e, TierSelectHttpError::BadRequest);
}

#[test]
fn f6_rejects_success_url_with_userinfo() {
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut bad = req("pro");
    bad.success_url = "https://user:pass@humangr.com/upgraded".to_owned();
    let e = authorize_and_validate(&state(), &h, &bad).unwrap_err();
    assert_eq!(e, TierSelectHttpError::BadRequest);
}

#[test]
fn f6_rejects_success_url_with_explicit_port() {
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut bad = req("pro");
    bad.success_url = "https://humangr.com:8443/upgraded".to_owned();
    let e = authorize_and_validate(&state(), &h, &bad).unwrap_err();
    assert_eq!(e, TierSelectHttpError::BadRequest);
}

#[test]
fn f6_rejects_off_allowlist_cancel_url() {
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut bad = req("pro");
    bad.cancel_url = "https://evil.example/cancel".to_owned();
    let e = authorize_and_validate(&state(), &h, &bad).unwrap_err();
    assert_eq!(e, TierSelectHttpError::BadRequest);
}

#[test]
fn f6_accepts_both_urls_on_allowlist() {
    // `humangr.com` is the sole canonical redirect host in
    // ALLOWED_REDIRECT_HOSTS — a valid combination must pass.
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut good = req("pro");
    good.success_url =
        "https://humangr.com/corelink/en/upgraded?session_id={CHECKOUT_SESSION_ID}".to_owned();
    good.cancel_url = "https://humangr.com/corelink/en/pricing".to_owned();
    let result = authorize_and_validate(&state(), &h, &good);
    assert!(result.is_ok());
}

#[test]
fn f6_redirect_check_not_applied_to_free_tier() {
    // The https/allowlist check is only applied to paid tiers (the free
    // path has no Checkout redirect).  An off-list URL for a free request
    // must NOT be rejected.
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut free = req("free");
    free.success_url = "http://localhost:3000/welcome".to_owned();
    free.cancel_url = "http://localhost:3000/cancel".to_owned();
    let (_t, tier) = authorize_and_validate(&state(), &h, &free).unwrap();
    assert_eq!(tier, RequestedTier::Free);
}

#[test]
fn accepts_valid_paid_request() {
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let (tenant, tier) = authorize_and_validate(&state(), &h, &req("pro")).unwrap();
    assert_eq!(tenant, "tenant-abc");
    assert_eq!(tier, RequestedTier::Pro);
    assert!(tier.is_paid());
}

#[test]
fn accepts_free_without_https_constraint() {
    // free does not hit Stripe → the https redirect constraint is not
    // applied (the free path has no Checkout redirect).
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let mut free = req("free");
    free.success_url = "http://localhost:3000/welcome".to_owned();
    let (_t, tier) = authorize_and_validate(&state(), &h, &free).unwrap();
    assert_eq!(tier, RequestedTier::Free);
    assert!(!tier.is_paid());
}

#[test]
fn tier_parse_is_case_insensitive_and_trims() {
    assert_eq!(
        RequestedTier::parse_self_serve("  PRO "),
        ParsedTier::Tier(RequestedTier::Pro)
    );
    assert_eq!(
        RequestedTier::parse_self_serve("Enterprise"),
        ParsedTier::Enterprise
    );
    assert_eq!(RequestedTier::parse_self_serve(""), ParsedTier::Invalid);
}

#[test]
fn runner_tiers_parse_and_are_paid_not_rejected() {
    // Each runner SKU parses to its variant (case-insensitive + trimmed),
    // is a paid tier (→ Stripe Checkout), and is NOT rejected as
    // Invalid/Enterprise (the allowlist previously 400'd unknown tiers).
    let cases = [
        ("runner_starter", RequestedTier::RunnerStarter),
        (" Runner_Pro ", RequestedTier::RunnerPro),
        ("RUNNER_TEAM", RequestedTier::RunnerTeam),
        ("runner_scale", RequestedTier::RunnerScale),
        ("runner_max", RequestedTier::RunnerMax),
    ];
    for (wire, expected) in cases {
        assert_eq!(
            RequestedTier::parse_self_serve(wire),
            ParsedTier::Tier(expected),
            "{wire} must parse to {expected:?}"
        );
        assert!(expected.is_paid(), "{expected:?} must be paid");
    }
}

#[test]
fn runner_pro_request_authorizes_and_is_paid() {
    // A full request carrying `runner_pro` passes auth/validation, is
    // recognized as the RunnerPro tier, and is NOT 400-rejected.
    let h = headers(Some("super-secret-internal-key"), Some("tenant-abc"));
    let (tenant, tier) = authorize_and_validate(&state(), &h, &req("runner_pro")).unwrap();
    assert_eq!(tenant, "tenant-abc");
    assert_eq!(tier, RequestedTier::RunnerPro);
    assert!(tier.is_paid());
}

// ── orchestration (durable heart) — adversarial coverage ───────────────
use std::collections::HashSet;
use std::sync::Mutex;
