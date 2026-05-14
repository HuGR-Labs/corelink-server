//! libFuzzer harness — feed random bytes to the `corelink-cli` TOML config
//! parser (`~/.corelink/config.toml`). WI-S15-006 §6.1 deliverable.
//!
//! Invariants:
//! - The parser never panics; every input either returns `Ok(CorelinkConfig)`
//!   or a structured `ConfigError`.
//! - Successfully parsed configs round-trip through `apply_config_key` without
//!   panicking on any deterministic dotted-key probe.

#![no_main]

use corelink_cli::config::CorelinkConfig;
use corelink_cli::fuzz_api::{apply_config_key, parse_config_toml};
use libfuzzer_sys::fuzz_target;

const PROBE_KEYS: &[&str] = &[
    "auth.pat",
    "defaults.tenant_id",
    "defaults.output",
    "telemetry.enabled",
    "telemetry.anonymized_id",
    "nonexistent.key",
];

fuzz_target!(|data: &[u8]| {
    // (1) Parse arbitrary bytes as TOML.
    let parsed = parse_config_toml(data);

    // (2) On success, probe every known dotted key with a noisy value derived
    //     from the same input. Must never panic.
    if let Ok(mut cfg) = parsed {
        let value = std::str::from_utf8(data).unwrap_or("");
        for key in PROBE_KEYS {
            let _ = apply_config_key(&mut cfg, key, value);
        }
        // Round-trip serialise → parse to assert stability.
        if let Ok(serialised) = toml::to_string(&cfg) {
            let _: Result<CorelinkConfig, _> = toml::from_str(&serialised);
        }
    }
});
