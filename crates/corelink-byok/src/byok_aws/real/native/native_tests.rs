use super::*;
use std::sync::Mutex;

/// Serializes every test in this module that touches env vars.
static ENV_LOCK: Mutex<()> = Mutex::new(());

const P: &str = "CORELINK_BYOK_KMS_TEST_PRIMARY";
const F: &str = "CORELINK_BYOK_KMS_TEST_FALLBACK";

fn clear_test_vars() {
    std::env::remove_var(P);
    std::env::remove_var(F);
}

#[test]
fn read_env_fallback_prefers_non_empty_primary() {
    let _g = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_test_vars();
    std::env::set_var(P, "primary-val");
    std::env::set_var(F, "fallback-val");
    // Kills None / Some("") / Some("xyzzy") constant mutants.
    assert_eq!(read_env_fallback(P, F), Some("primary-val".to_string()));
    clear_test_vars();
}

#[test]
fn read_env_fallback_uses_fallback_when_primary_unset() {
    let _g = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_test_vars();
    std::env::set_var(F, "fallback-val");
    assert_eq!(read_env_fallback(P, F), Some("fallback-val".to_string()));
    clear_test_vars();
}

#[test]
fn read_env_fallback_skips_empty_primary() {
    // Kills the `!`-deletion in `if !v.is_empty()`: an EMPTY primary
    // must be skipped so the (non-empty) fallback is returned.
    let _g = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_test_vars();
    std::env::set_var(P, "");
    std::env::set_var(F, "fallback-val");
    assert_eq!(read_env_fallback(P, F), Some("fallback-val".to_string()));
    clear_test_vars();
}

#[test]
fn read_env_fallback_none_when_both_unset() {
    let _g = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_test_vars();
    assert_eq!(read_env_fallback(P, F), None);
}

#[test]
fn with_fips_errors_when_credentials_unset() {
    // [C4] guard: with NO credentials in env, construction must fail
    // CLOSED (Err) at the cred-read step — no AWS/network call. Kills
    // an `Ok(Default::default())` / unconditional-Ok mutant.
    // Driven with `block_on` (not `#[tokio::test]`) so no `.await`
    // happens while the env lock is held (`with_fips` is synchronous).
    let _g = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for v in [
        "CORELINK_BYOK_KMS_ACCESS_KEY_ID",
        "AWS_ACCESS_KEY_ID",
        "CORELINK_BYOK_KMS_SECRET_ACCESS_KEY",
        "AWS_SECRET_ACCESS_KEY",
        "CORELINK_BYOK_KMS_SESSION_TOKEN",
        "AWS_SESSION_TOKEN",
    ] {
        std::env::remove_var(v);
    }
    let r = futures::executor::block_on(AwsKmsRealProvider::with_fips("us-east-1", false));
    assert!(
        matches!(r, Err(BYOKError::Provider(_))),
        "with_fips must fail closed when credentials are unset, got {r:?}"
    );
}
