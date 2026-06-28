---
type: "Runbook"
title: "SDK reference (python/go/javascript)"
description: "The three first-party CoreLink SDKs (PyO3 Python, cgo Go, wasm-bindgen JS/TS): a shared API shape (put/get/stat), client-side BLAKE3 verify default-on per CTRL-CAS-002 via a single Rust truth, the canonical COR_CAS_DIGEST_MISMATCH error, and the explicit opt-out semantics."
source_files:
  - "docs/sdk/python.md"
  - "docs/sdk/go.md"
  - "docs/sdk/javascript.md"
checkpoint_sha: "0aad76e1d132cd98d35c814a5bb23008c226d08e"
provenance: "AUTHORED"
tags: ["ops", "sdk", "client-verify", "blake3", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# SDK reference (python/go/javascript)

CoreLink defines three first-party client SDKs over the [native CAS surface](/surfaces/native-cas.md) —
Python (PyO3), Go (cgo cdylib), and JavaScript/TypeScript (wasm-bindgen WASM) — that share one API shape
(`put`/`get`/`stat`) and one load-bearing safety default: **client-side BLAKE3 verification on every
`get`, on by default per control CTRL-CAS-002**, implemented through the single Rust truth
(`corelink-client-verify`) so all three languages cannot disagree about what a valid digest is. The
content-addressing guarantee is therefore enforced at the edge of the customer's process, not just on the
server. This runbook is the cross-language contract an integrator reads before wiring a client.

> **Status vs shipped code (2026-06-28):** what SHIPS today in these SDK crates is the client-side
> BLAKE3 compute + verify layer — NOT a live CAS network surface. The `put`/`get`/`stat` network layer
> is a STUB: `get` simulates a cache miss and returns empty bytes (it only succeeds for the empty-blob
> digest), `put` computes the local BLAKE3 digest but uploads nothing, and `stat` returns a placeholder
> (`exists: false`) — see the Python crate (tools/sdks/python/src/lib.rs, the `get`/`put`/`stat` bodies
> around lines 106-114, 146 and 155-164, each documented as a stub awaiting the production HTTP/gRPC
> wiring). Treat the `put`/`get`/`stat` "live CAS surface" framing below as the intended FFI contract;
> only the client-side BLAKE3/verify truth is actually exercised end-to-end today.

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
  language, never returning corrupt bytes (`docs/sdk/javascript.md:106-113`).
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
10. `docs/sdk/javascript.md:106-113` — JS error reference (`COR_CAS_DIGEST_MISMATCH`).
11. `docs/sdk/javascript.md:113-127` — ≤1MB bundle-size CI gate.
