---
type: "Runbook"
title: "SDK reference (python/go/javascript)"
description: "The three first-party CoreLink SDKs (PyO3 Python, cgo Go, wasm-bindgen JS/TS): a shared API shape (put/get/stat), client-side BLAKE3 verify default-on per CTRL-CAS-002 via a single Rust truth, the canonical COR_CAS_DIGEST_MISMATCH error, and the explicit opt-out semantics."
source_files:
  - "crates/corelink-client-verify/src/error.rs"
  - "crates/corelink-client-verify/src/digest.rs"
  - "crates/corelink-client-verify/src/verifier.rs"
  - "docs/sdk/python.md"
  - "docs/sdk/go.md"
  - "docs/sdk/javascript.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "sdk", "client-verify", "blake3", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# SDK reference (python/go/javascript)

CoreLink ships three first-party client SDKs over the [native CAS surface](/surfaces/native-cas.md) —
Python (PyO3), Go (cgo cdylib), and JavaScript/TypeScript (wasm-bindgen WASM) — that share one API shape
(`put`/`get`/`stat`) and one load-bearing safety default: **client-side BLAKE3 verification on every
`get`, on by default per control CTRL-CAS-002**, implemented through the single Rust truth
(`corelink-client-verify`) so all three languages cannot disagree about what a valid digest is. The
content-addressing guarantee is therefore enforced at the edge of the customer's process, not just on the
server. This runbook is the cross-language contract an integrator reads before wiring a client.

# Role
- The integration surface: the supported, first-party way a customer talks to CAS in three ecosystems.
- The client-side integrity guard: BLAKE3-verify-on-get, default-on, single-Rust-truth across languages.
- The opt-out governance: how each language makes disabling verify explicit and noisy, never accidental.

# How it works
1. The Python SDK is a PyO3 wrapper with native asyncio; `put(bytes)` returns a 64-char BLAKE3 hex digest
   and `get` raises `COR_CAS_DIGEST_MISMATCH` on integrity failure (`docs/sdk/python.md:33-59`).
2. `client_verify=True` is the Python default and opt-out is explicit, logging a canonical
   "DISABLE NOT RECOMMENDED" warning (`docs/sdk/python.md:75-86`).
3. The Go SDK exposes the same `Put`/`Get`/`Stat` over a cgo cdylib, with `ClientVerify` defaulting true
   per CTRL-CAS-002 (`docs/sdk/go.md:61-72`).
4. Go opt-out requires BOTH `ClientVerify:false` AND `ClientVerifyExplicitFalse:true`, so a zero-value
   Config can never silently disable verification (`docs/sdk/go.md:106-118`).
5. The JS/TS SDK is a wasm-bindgen WASM wrapper with a Promise API; `clientVerify` defaults true and
   `get` throws `Error(COR_CAS_DIGEST_MISMATCH)` on mismatch (`docs/sdk/javascript.md:38-66`).
6. All three SDKs compute the digest through the single Rust truth (`corelink-client-verify`), so the
   verify result is byte-identical across languages (`docs/sdk/python.md:123-127`).

# Invariants
- Client-verify is ON by default in every SDK and disabling it MUST be explicit + warned — accidental
  opt-out is not reachable (`docs/sdk/python.md:75-86`).
- The Go zero-value Config does NOT opt out: the `ClientVerifyExplicitFalse` guard makes a quiet disable
  via an uninitialized struct impossible (`docs/sdk/go.md:69-70`).
- A `get` that fails the BLAKE3 check raises the canonical `COR_CAS_DIGEST_MISMATCH` error in every
  language, never returning corrupt bytes — the single Rust truth returns `Err(VerifyError::DigestMismatch)`
  (`crates/corelink-client-verify/src/verifier.rs:106-118`), whose stable code is `COR_CAS_DIGEST_MISMATCH`
  (`crates/corelink-client-verify/src/error.rs:29-39`, `crates/corelink-client-verify/src/error.rs:65`).
- The JS bundle is gated at ≤1MB (1,048,576 bytes) after `wasm-opt -O3` as a CI assertion
  (`docs/sdk/javascript.md:113-127`).

# Gotchas
- The Python wrapper is valgrind-checked for 0 leaks / 0 invalid reads on every CI build — an FFI memory
  regression fails CI, so do not treat the PyO3 boundary as untested
  (`docs/sdk/python.md:99-107`).
- The Go SDK is run under the race detector in CI; a data race in the cgo boundary is a gate failure
  (`docs/sdk/go.md:121-129`).
- The FFI-vs-native-HTTP choice per language is a deliberate trade-off recorded in ADR-0016 — the SDKs are
  FFI over a shared Rust core, not independent HTTP reimplementations (`docs/sdk/python.md:123-127`).

# Citations
1. `docs/sdk/python.md:33-59` — Python API (`put`/`get`/`stat`), 64-hex digest, mismatch error.
2. `docs/sdk/python.md:75-86` — Python explicit client-verify opt-out + canonical warning.
3. `docs/sdk/python.md:99-107` — valgrind 0-leak CI check.
4. `docs/sdk/python.md:123-127` — single Rust truth (`corelink-client-verify`) + ADR-0016 trade-off.
5. `docs/sdk/go.md:61-72` — Go API reference + `ClientVerify` default true.
6. `docs/sdk/go.md:69-70` — `ClientVerifyExplicitFalse` zero-value guard.
7. `docs/sdk/go.md:106-118` — Go two-field explicit opt-out.
8. `docs/sdk/go.md:121-129` — Go race-detector CI.
9. `docs/sdk/javascript.md:38-66` — JS/TS API + `clientVerify` default + mismatch throw.
10. `docs/sdk/javascript.md:106-113` — JS error reference (`COR_CAS_DIGEST_MISMATCH`) (spec).
10b. `crates/corelink-client-verify/src/verifier.rs:106-118` — `ClientVerifier::verify` returns `Err(VerifyError::DigestMismatch)` on a BLAKE3 mismatch (the enforcer).
10c. `crates/corelink-client-verify/src/error.rs:29-39`, `crates/corelink-client-verify/src/error.rs:65` — the `DigestMismatch` variant + its canonical `COR_CAS_DIGEST_MISMATCH` code.
10d. `crates/corelink-client-verify/src/digest.rs:19` — the canonical `COR_CAS_DIGEST_MISMATCH` constant (the single Rust truth, re-exported from `corelink-hash`).
11. `docs/sdk/javascript.md:113-127` — ≤1MB bundle-size CI gate.
