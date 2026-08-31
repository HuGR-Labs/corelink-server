//! B-074 — the money path must carry the same internal-auth entropy floor as
//! every other internal surface, and must have a rotation path of its own.
//!
//! `/v1/onboarding/tier-select` (paid checkout) and `/v1/onboarding/dpa-accept`
//! (the legally binding consent receipt) used to read the shared
//! `CORELINK_INTERNAL_AUTH_KEY` raw, with a **16**-char floor, while every other
//! internal surface resolved through
//! [`corelink_server::routes::admin::resolve_internal_auth_key`] at a **32**-char
//! floor. Two consequences, both closed by this test:
//!
//! 1. **Entropy.** A 16–31-char secret was accepted precisely where money and
//!    consent pass. The `must_not_mount_*` cases pin the 32 floor.
//! 2. **Rotation.** Because the helper was never called, neither route could
//!    ever be moved to its own credential. The `dedicated_key_alone_mounts_*`
//!    cases pin that the dedicated env var is honoured on its own.
//!
//! ## Why this file is ONE `#[test]`
//!
//! `build_state_from_env` reads the process environment, and Rust runs the test
//! functions of a binary on a thread pool. Splitting these cases into separate
//! `#[test]` fns would let them race on the same globals and flake. One
//! sequential function is the honest way to test env-driven construction
//! without adding a dependency on a serialisation crate.
//!
//! ## The positive control is load-bearing, not decoration
//!
//! Every assertion here is `is_none()` — a *denial*. Denial assertions pass
//! vacuously if the harness never manages to satisfy the OTHER preconditions
//! (D1 config, Stripe config, DPA version, signing key): the route would return
//! `None` for a reason that has nothing to do with the auth key, and the test
//! would be green while measuring nothing. So each block asserts a
//! **`Some(...)` positive control first** with a properly sized key, proving the
//! harness can reach a MOUNTED state, and only then flips the key to a
//! sub-floor value and demands `None`.

use corelink_server::routes::{dpa_accept, tier_select};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;

/// A key at the shared 32-char floor (`INTERNAL_AUTH_KEY_MIN_LEN`), generously
/// sized like the `openssl rand -hex 32` the secrets-checklist prescribes.
const KEY_64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// 20 chars: comfortably past the OLD `< 16` gate, comfortably below the 32
/// floor. This is the exact band B-074 is about — the value that the money path
/// used to accept and every other internal surface already refused.
const KEY_20: &str = "01234567890123456789";

/// Satisfy every NON-auth precondition of both `build_state_from_env`s, so the
/// only variable left in the experiment is the internal-auth key.
///
/// All of these are pure env reads that construct clients without performing
/// I/O (`StorageEnv::from_env`, `D1HttpClient::new`,
/// `StripeRealClient::from_env`), so obviously-fake values are sufficient and
/// nothing here talks to Cloudflare or Stripe.
fn set_common_env(signing_key_pem: &str) {
    std::env::set_var("CORELINK_DPA_VERSION", "1.0.0");
    std::env::set_var("R2_S3_ENDPOINT", "https://example.invalid");
    std::env::set_var("R2_S3_ACCESS_KEY_ID", "test-access-key-id");
    std::env::set_var("R2_S3_SECRET_ACCESS_KEY", "test-secret-access-key");
    std::env::set_var("CLOUDFLARE_ACCOUNT_ID", "0123456789abcdef0123456789abcdef");
    std::env::set_var("CF_API_TOKEN", "test-cf-api-token");
    std::env::set_var("D1_DATABASE_ID", "00000000-0000-0000-0000-000000000000");
    std::env::set_var("STRIPE_AUTH_MODE", "direct");
    std::env::set_var("STRIPE_SECRET_KEY", "sk_test_0123456789");
    std::env::set_var("DPA_RECEIPT_SIGNING_KEY", signing_key_pem);
}

/// Clear every internal-auth env var so each case starts from a known state.
/// Without this the cases would inherit each other's keys and the later
/// assertions would measure the earlier ones.
fn clear_auth_env() {
    std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
    std::env::remove_var("CORELINK_TIER_SELECT_AUTH_KEY");
    std::env::remove_var("CORELINK_DPA_ACCEPT_AUTH_KEY");
}

fn test_signing_key_pem() -> String {
    let mut rng = rand::thread_rng();
    let key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
    key.to_pkcs8_pem(LineEnding::LF)
        .expect("pkcs8 pem")
        .to_string()
}

#[test]
fn money_path_enforces_the_32_char_floor_and_honours_a_dedicated_key() {
    let pem = test_signing_key_pem();
    set_common_env(&pem);

    // ── tier-select ─────────────────────────────────────────────────────────
    //
    // POSITIVE CONTROL. A properly sized SHARED key must MOUNT the route. If
    // this is `None`, the harness has not satisfied the non-auth preconditions
    // and every denial below would be vacuous — so this assertion is what makes
    // the rest of the test mean anything.
    clear_auth_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_64);
    assert!(
        tier_select::build_state_from_env().is_some(),
        "POSITIVE CONTROL FAILED: a 64-char shared key with all other env set \
         must mount /v1/onboarding/tier-select. Because it did not, the denial \
         assertions in this test would pass vacuously and prove nothing. Fix \
         the harness (some non-auth precondition is unmet) before trusting any \
         other case here."
    );

    // THE DEFECT (B-074). A 20-char shared key sits above the old `< 16` gate
    // and below the 32 floor. Before the fix this MOUNTED the paid-checkout
    // route; it must now be refused.
    clear_auth_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_20);
    assert!(
        tier_select::build_state_from_env().is_none(),
        "B-074: a 20-char CORELINK_INTERNAL_AUTH_KEY is below the 32-char \
         INTERNAL_AUTH_KEY_MIN_LEN floor and MUST NOT mount the paid-checkout \
         route (the old gate was `< 16` and let this through)"
    );

    // ROTATION. The dedicated key alone — no shared key present at all — must
    // mount the route. Before the fix the helper was never called, so the
    // dedicated var was dead config and this returned `None`.
    clear_auth_env();
    std::env::set_var("CORELINK_TIER_SELECT_AUTH_KEY", KEY_64);
    assert!(
        tier_select::build_state_from_env().is_some(),
        "B-074: CORELINK_TIER_SELECT_AUTH_KEY must mount tier-select on its own, \
         with no CORELINK_INTERNAL_AUTH_KEY set — that is the rotation path"
    );

    // A sub-floor DEDICATED key with no shared fallback must fail CLOSED rather
    // than silently widening to "no gate".
    clear_auth_env();
    std::env::set_var("CORELINK_TIER_SELECT_AUTH_KEY", KEY_20);
    assert!(
        tier_select::build_state_from_env().is_none(),
        "B-074: a sub-floor dedicated key with no shared fallback must fail CLOSED"
    );

    // ── dpa-accept ──────────────────────────────────────────────────────────
    //
    // Same three-part shape. The DPA receipt is the legally binding consent
    // record, so it carries the identical floor and rotation guarantees.
    clear_auth_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_64);
    assert!(
        dpa_accept::build_state_from_env().is_some(),
        "POSITIVE CONTROL FAILED: a 64-char shared key with all other env set \
         must mount /v1/onboarding/dpa-accept. Until this holds, the dpa-accept \
         denial assertions below are vacuous."
    );

    clear_auth_env();
    std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_20);
    assert!(
        dpa_accept::build_state_from_env().is_none(),
        "B-074: a 20-char CORELINK_INTERNAL_AUTH_KEY is below the 32-char floor \
         and MUST NOT mount the DPA consent-receipt route"
    );

    clear_auth_env();
    std::env::set_var("CORELINK_DPA_ACCEPT_AUTH_KEY", KEY_64);
    assert!(
        dpa_accept::build_state_from_env().is_some(),
        "B-074: CORELINK_DPA_ACCEPT_AUTH_KEY must mount dpa-accept on its own — \
         the rotation path"
    );

    clear_auth_env();
    std::env::set_var("CORELINK_DPA_ACCEPT_AUTH_KEY", KEY_20);
    assert!(
        dpa_accept::build_state_from_env().is_none(),
        "B-074: a sub-floor dedicated key with no shared fallback must fail CLOSED"
    );

    clear_auth_env();
}
