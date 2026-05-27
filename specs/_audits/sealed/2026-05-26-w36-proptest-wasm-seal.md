---
id: "AUDIT-2026-05-26-W36-PROPTEST-WASM-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-36", "proptest", "corelink-wasm", "density", "seal"]
references:
  - "specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/sealed/proptest-followup-tickets.md"
---

# Wave 36 — corelink-wasm proptest density SEAL

## §1. Scope

Closes WI-PROPTEST-FU-W33-002 partial (corelink-wasm portion) per
`specs/_audits/sealed/proptest-followup-tickets.md` and Wave 33/34 closure
follow-up #5 in `specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md`
§6.

The `corelink-wasm` crate had a pre-existing density gap with **zero**
property-based tests despite exposing the WASM CAS client critical path
(`put` / `get` / `stat`) to JS/TS consumers. Six (6) proptests added,
covering the named invariants below.

### Invariants covered

| ID | Invariant | Surface |
|----|-----------|---------|
| INV-WASM-PUT-HEX64 | `put_inner` always returns 64-char lowercase hex | Encoding determinism |
| INV-WASM-PUT-DETERMINISTIC | Same bytes → same digest, independent of `client_verify` flag | Hash purity |
| INV-WASM-PUT-ROUNDTRIP | `Digest::from_hex(put_inner(x)).to_hex() == put_inner(x)` | Hex codec round-trip |
| INV-WASM-GET-VERIFY-MATCH | `get_inner` Ok iff digest matches stub body, else `COR_CAS_DIGEST_MISMATCH` | Verify state machine |
| INV-WASM-GET-PANIC-FREE | `get_inner` never panics on arbitrary string input | FFI panic-free boundary |
| INV-WASM-STAT-ECHO | `stat_inner` echoes digest, size=0, exists=false on valid input | Stub contract |

## §2. Acceptance criteria

- [x] ≥3 new proptests added (delivered: **6**)
- [x] Each uses `proptest!` macro with `proptest_cases()` env-overridable helper
- [x] Each has rustdoc comment naming the invariant
- [x] No `#[ignore]` to bypass
- [x] No `matches!(..., Variant { .. })` anti-pattern (S-08 P1-1) — explicit field asserts used
- [x] `#![forbid(unsafe_code)]` / `#![deny(unsafe_code)]` preserved
- [x] No hard-coded `ProptestConfig::with_cases(N)` — uses `proptest_cases()` helper
- [x] `cargo build -p corelink-wasm` GREEN
- [x] `cargo clippy -p corelink-wasm --tests -- -D warnings` GREEN
- [x] `cargo test -p corelink-wasm` all PASS
- [x] `PROPTEST_CASES=256 cargo test -p corelink-wasm --release` all PASS

## §3. Output evidence

```
Proptest count: 0 → 6
Tests (lib unit, host target): 7 → 13
Build: GREEN
Clippy --tests -D warnings: GREEN
cargo test (default cases): 13 passed; 0 failed
PROPTEST_CASES=256 cargo test --release: 13 passed; 0 failed
```

Co-located in `crates/corelink-wasm/src/lib.rs` `#[cfg(test)] mod tests`
because:

1. Integration tests in `tests/*.rs` cannot call the public `JsValue`-based
   constructor on the host (`cargo test`) target — `serde_wasm_bindgen`
   deserialization paths assume wasm32.
2. The pure-Rust `*_inner` helpers (the production hash + verify code paths)
   are crate-private.
3. Matches the existing colocation pattern with `wasm_bindgen_test`
   tests already in that module.

`proptest = { workspace = true }` added to `[dev-dependencies]` in
`crates/corelink-wasm/Cargo.toml`.

## §4. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
