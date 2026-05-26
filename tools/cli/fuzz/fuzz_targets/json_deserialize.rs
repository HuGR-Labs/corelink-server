//! libFuzzer harness — random JSON fed to the `--output=json` deserialisation
//! paths the CLI exposes to scripts. WI-S15-006 §6.1 deliverable.
//!
//! The CLI emits JSON for `ls / get / put / stat / bench / doctor / version`
//! responses; consumers parse these via serde. We exercise the symmetric
//! direction: feed adversarial JSON into the CorelinkConfig deserialiser
//! (`telemetry.enabled`, `defaults.*`, `auth.*`), which is the same type
//! family used by `corelink config list --output=json`.
//!
//! Invariants:
//! - Never panic on any byte sequence.
//! - Successful parses round-trip via serde without panicking.

#![no_main]

use corelink_cli::config::CorelinkConfig;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    // (1) Try to parse as a Config object.
    if let Ok(cfg) = serde_json::from_str::<CorelinkConfig>(s) {
        // Round-trip.
        let _ = serde_json::to_string(&cfg);
    }
    // (2) Try as opaque Value (this is what scripts using `--output=json` do).
    let _ = serde_json::from_str::<serde_json::Value>(s);
});
