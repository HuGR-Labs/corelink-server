//! libFuzzer harness — CTRL-CRED-001 "no secrets leaked in error paths".
//! WI-S15-006 §6.1 deliverable.
//!
//! Strategy:
//! 1. Feed random bytes into the config-key dispatch and TOML parser, capturing
//!    the `Display` impl of every `Err` value returned.
//! 2. Run the `count_pat_leaks` scanner over the captured error strings.
//! 3. Assert the count is 0 — every error message must scrub PAT material
//!    before user-visible rendering.
//!
//! Invariants:
//! - 0 panics across all inputs.
//! - 0 PAT-shaped substrings emitted by any error's `Display`.

#![no_main]

use corelink_cli::config::CorelinkConfig;
use corelink_cli::fuzz_api::{apply_config_key, count_pat_leaks, parse_config_toml};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut captured: String = String::new();

    // (1) TOML parse: capture any Display output.
    match parse_config_toml(data) {
        Ok(_cfg) => {}
        Err(e) => {
            use std::fmt::Write as _;
            let _ = write!(&mut captured, "{e}\n");
        }
    }

    // (2) Config-key apply with raw bytes as both key and value.
    if let Ok(s) = std::str::from_utf8(data) {
        let mut cfg = CorelinkConfig::default();
        // Use the full input as both key (path) and value to maximise
        // chances of routing user-controlled bytes into error messages.
        for key in ["auth.pat", s, "telemetry.enabled", "defaults.tenant_id"] {
            if let Err(e) = apply_config_key(&mut cfg, key, s) {
                use std::fmt::Write as _;
                let _ = write!(&mut captured, "{e}\n");
            }
        }
    }

    // (3) Scan captured output for PAT-shaped leaks.
    let leaks = count_pat_leaks(&captured);
    assert_eq!(
        leaks, 0,
        "CTRL-CRED-001 violation: error path leaked PAT-shaped substring(s); captured={captured:?}"
    );
});
