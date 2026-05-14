//! libFuzzer harness — adversarial PAT shapes against
//! `corelink-cli::auth::validate_pat_shape` (CTRL-CRED-001).
//! WI-S15-006 §6.1 deliverable.
//!
//! The validator is intentionally strict: any input that does not match
//! `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` returns
//! `Err(CliError::PatMalformed)` without panicking.
//!
//! Invariants:
//! - 0 panics on any byte sequence (UTF-8 or otherwise).
//! - Idempotent: validating the same input twice yields the same result.
//! - Deterministic: no internal mutable state, no system calls.

#![no_main]

use corelink_cli::fuzz_api::validate_pat_shape;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        // Non-UTF8 inputs simulate env-var corruption; validate the empty
        // string fallback (which must error cleanly).
        let _ = validate_pat_shape("");
        return;
    };
    let r1 = validate_pat_shape(s).is_ok();
    let r2 = validate_pat_shape(s).is_ok();
    assert_eq!(r1, r2, "PAT shape validator must be deterministic");
});
