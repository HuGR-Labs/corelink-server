//! libFuzzer harness — random CLI argv combinations against the `corelink-cli`
//! config-key dispatch surface. WI-S15-006 §6.1 deliverable.
//!
//! Rather than re-instantiate the binary's clap parser (which lives in
//! `main.rs`, a private crate root), we exercise the most user-controlled
//! pure-function surface that the CLI invokes per-call:
//!
//! 1. `apply_config_key(cfg, key, value)` — every `corelink config set K V`
//!    funnels through this dispatch. Random `(key, value)` pairs must never
//!    panic; only return `Ok` or `Err(ConfigError::*)`.
//! 2. `validate_pat_shape(arg)` — argv elements that happen to start with
//!    `corelink_` get probed for shape; must never panic.
//!
//! Invariants asserted:
//! - No panic / abort across any (key, value) tuple.
//! - PAT validator returns deterministic Ok/Err on identical input
//!   (stateless).

#![no_main]

use corelink_cli::config::CorelinkConfig;
use corelink_cli::fuzz_api::{apply_config_key, validate_pat_shape};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Split data into (key, value, extra) chunks deterministically.
    if data.len() < 2 {
        return;
    }
    let key_len = usize::from(data[0]).min(data.len() - 1);
    let key_bytes = &data[1..1 + key_len];
    let value_bytes = &data[1 + key_len..];

    let Ok(key) = std::str::from_utf8(key_bytes) else {
        return;
    };
    let Ok(value) = std::str::from_utf8(value_bytes) else {
        return;
    };

    // (1) Config key apply dispatch.
    let mut cfg = CorelinkConfig::default();
    let _ = apply_config_key(&mut cfg, key, value);

    // (2) PAT shape validator — feed value through, must never panic.
    let _ = validate_pat_shape(value);

    // (3) Determinism re-check.
    let r1 = validate_pat_shape(value).is_ok();
    let r2 = validate_pat_shape(value).is_ok();
    assert_eq!(r1, r2, "PAT shape validator must be stateless");
});
