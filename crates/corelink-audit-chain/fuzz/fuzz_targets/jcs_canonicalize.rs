//! libFuzzer harness — RFC 8785 JCS canonicalization idempotence.
//!
//! Properties asserted:
//!
//! 1. **Idempotence.** `canonicalize(canonicalize(x)) == canonicalize(x)`
//!    for every JSON value `x` (the canonical form is a fixed point of
//!    the canonicalization function).
//! 2. **Determinism.** Two independent invocations produce byte-equal
//!    output. The CoreLink audit chain link hash depends on this
//!    invariant — drift = SOC 2 CC7.2 audit-chain-integrity violation.
//! 3. **Never panic on parsed JSON.** Any JSON value produced by
//!    `serde_json::from_slice` MUST canonicalize without panic.
//!
//! Input strategy: feed the bytes to `serde_json::from_slice` first; if
//! the bytes don't decode as JSON we skip the iteration (the parser-or-
//! crash invariant is already pinned by the `corelink-cli` fuzz suite
//! `json_deserialize` target). The fuzz signal here targets the
//! canonicalization stage specifically.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only meaningful JSON values reach the canonicalizer in production.
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return;
    };

    // ── (1) First canonicalization MUST succeed for any in-memory
    //        Value (non-finite floats are rejected by serde_jcs by
    //        contract; we accept that as Err, not as panic). ──────────
    let canon1 = match serde_jcs::to_vec(&value) {
        Ok(b) => b,
        Err(_) => return,
    };

    // ── (2) Re-parse the canonical bytes; canonicalize again. ────────
    let reparsed: serde_json::Value =
        serde_json::from_slice(&canon1).expect("JCS output MUST be valid JSON");
    let canon2 = serde_jcs::to_vec(&reparsed)
        .expect("canonicalize-then-parse-then-canonicalize MUST succeed");

    assert_eq!(
        canon1, canon2,
        "JCS canonicalization MUST be idempotent — chain-hash drift otherwise"
    );

    // ── (3) Determinism: a second invocation on the original Value
    //        MUST yield the same canonical bytes. ────────────────────
    let canon1b = serde_jcs::to_vec(&value)
        .expect("second JCS invocation must succeed on a Value that succeeded once");
    assert_eq!(
        canon1, canon1b,
        "JCS canonicalization MUST be deterministic across invocations on the same Value"
    );
});
