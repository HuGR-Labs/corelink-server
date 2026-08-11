---
type: "Runbook"
title: "SDK reference (python/go/javascript)"
description: "The three first-party CoreLink SDKs (pure-Python httpx, cgo Go, pure-JS @noble/hashes JS/TS): a shared API shape (put/get/stat), client-side BLAKE3 verify default-on per CTRL-CAS-002 (Python in pure Python, Go via a Rust cgo bridge, JS via @noble/hashes), the canonical digest-mismatch error, and the explicit opt-out semantics."
source_files:
  - "docs/sdk/python.md"
  - "docs/sdk/go.md"
  - "docs/sdk/javascript.md"
source_blobs:
  - "docs/sdk/python.md@ec9f1468c8196bc7f53298b7b55cbb23219ede28"
  - "docs/sdk/go.md@a02a374b5a65de2c0b471dcfc7544c22f22ba79b"
checkpoint_sha: "d53bcc2dc642c656a37708e0f92c25bdf73e285e"
provenance: "AUTHORED"
tags: ["ops", "sdk", "client-verify", "blake3", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# SDK reference (python/go/javascript)

CoreLink defines three first-party client SDKs over the [native CAS surface](/surfaces/native-cas.md) —
Python (pure-Python `httpx`), Go (cgo cdylib), and JavaScript/TypeScript (pure-JS, `@noble/hashes`) — that
share one API shape (`put`/`get`/`stat`) and one load-bearing safety default: **client-side BLAKE3
verification on every `get`, on by default per control CTRL-CAS-002**. Go verifies through the Rust cgo
bridge (`corelink-client-verify`); Python does the BLAKE3 re-hash in pure Python and the JS/TS SDK uses the
pure-JS `@noble/hashes` BLAKE3 — the same digest, no shared binary in either the Python or the
browser/Worker target. The content-addressing guarantee is therefore enforced at the edge of the
customer's process, not just on the server. This runbook is the cross-language contract an integrator
reads before wiring a client.

> **Status vs shipped code (2026-08-09):** the **JS/TS SDK (`sdks/js/`) and the Python SDK
> (`sdks/python/`) are real, wired HTTP clients** — the JS client's `put`/`get`/`stat` + Action Cache
> methods hit the live `/v1/cas` and `/v1/ac` container routes over `fetch`, and the Python client is a
> plain `httpx` REST client whose `put`/`get`/`stat` map to `PUT`/`GET`/`HEAD /v1/cas/{tenant}/{digest}`
> (no PyO3, no FFI layer at all). The **Go SDK is still a network-client STUB**: `Put()` computes a real
> BLAKE3 digest via the Rust cgo bridge but does no network I/O, `Get()` never fetches the body (it
> returns empty bytes, so any non-empty-blob digest fails verify with `COR_CAS_DIGEST_MISMATCH`), and
> `Stat()` always reports `Exists: false` (`docs/sdk/go.md:6-12`). Treat the Go `put`/`get`/`stat`
> framing below as the target API shape, not a working round-trip today.

# Role
- The integration surface: the supported, first-party way a customer talks to CAS in three ecosystems.
- The client-side integrity guard: BLAKE3-verify-on-get, default-on, single-Rust-truth across languages.
- The opt-out governance: how each language makes disabling verify explicit and noisy, never accidental.

# How it works
1. The Python SDK is a plain `httpx` REST client (sync `CoreLinkClient` + async `AsyncCoreLinkClient`);
   `put(bytes)` returns a 64-char BLAKE3 hex digest and `get` re-hashes the downloaded bytes, raising
   `CoreLinkDigestMismatchError` on integrity failure (`docs/sdk/python.md:51-97`).
2. Verify defaults to `True` and is a **per-call** kwarg on `get()` — there is no client-wide
   `client_verify` flag — and opt-out is the explicit per-call `verify=False`, which skips the BLAKE3
   re-hash (`docs/sdk/python.md:60-61`, `docs/sdk/python.md:106-112`).
3. The Go SDK exposes `Put`/`Get`/`Stat` over a cgo cdylib with `ClientVerify` defaulting true per
   CTRL-CAS-002, but `Get()`/`Stat()` are stubs that never touch the network — `Get()` returns empty bytes
   (so a non-empty-blob digest fails verify with `COR_CAS_DIGEST_MISMATCH`) and `Stat()` always reports
   `Exists: false` (`docs/sdk/go.md:78-87`, `docs/sdk/go.md:100-119`).
4. Go opt-out requires BOTH `ClientVerify:false` AND `ClientVerifyExplicitFalse:true`, so a zero-value
   Config can never silently disable verification (`docs/sdk/go.md:129-142`).
5. The JS/TS SDK is a pure-JS Promise-API HTTP client (BLAKE3 via `@noble/hashes`); `clientVerify`
   defaults true and `get` throws `DigestMismatchError` (`COR_CAS_DIGEST_MISMATCH`) on mismatch
   (`docs/sdk/javascript.md:47-90`).
6. The Go SDK computes/verifies its digest through the Rust cgo bridge (`corelink-client-verify`); the
   Python SDK does the BLAKE3 re-hash in pure Python and the JS/TS SDK computes it with `@noble/hashes` —
   all produce the byte-identical BLAKE3 digest (`docs/sdk/python.md:134-140`, `docs/sdk/go.md:171-174`).

# Invariants
- Client-verify is ON by default in every SDK and cannot be disabled by accident: in Python it is a
  per-call `verify` kwarg on `get()` (default `True`; opt-out is the explicit per-call `verify=False`) and
  in Go the zero-value Config keeps it on (`docs/sdk/python.md:106-112`, `docs/sdk/go.md:129-142`).
- The Go zero-value Config does NOT opt out: the `ClientVerifyExplicitFalse` guard makes a quiet disable
  via an uninitialized struct impossible (`docs/sdk/go.md:85`).
- A `get` that fails the BLAKE3 check raises the canonical `COR_CAS_DIGEST_MISMATCH` error in every
  language (JS: `DigestMismatchError`), never returning corrupt bytes (`docs/sdk/javascript.md:147-160`).
- The JS/TS SDK is pure JavaScript (BLAKE3 via `@noble/hashes`) with no WASM or native build step — it
  runs anywhere the platform provides `fetch` (`docs/sdk/javascript.md:1-10`).

# Gotchas
- The Python SDK is a **pure-Python** package — `httpx` + `blake3` are its only runtime deps, there is no
  Rust extension module and no FFI boundary, so there is no valgrind/FFI-memory CI step; tests run under
  `pytest` (`docs/sdk/python.md:123-132`).
- The Go SDK is run under the race detector in CI; a data race in the cgo boundary is a gate failure
  (`docs/sdk/go.md:144-151`).
- The three SDKs are not architecturally symmetric: Python is a native-HTTP `httpx` client that does its
  BLAKE3 verify in pure Python (no FFI), Go shares a Rust cgo bridge for its verify path, and the JS/TS
  SDK verifies with pure-JS `@noble/hashes` — the FFI-vs-native trade-off is recorded in ADR-0016
  (`docs/sdk/python.md:134-140`, `docs/sdk/go.md:171-174`).

# Citations
1. `docs/sdk/python.md:51-97` — Python API (`put`/`get`/`stat`), 64-hex digest, `CoreLinkDigestMismatchError`.
2. `docs/sdk/python.md:106-112` — Python per-call verify opt-out (`verify=False` on `get()`).
3. `docs/sdk/python.md:123-132` — pure-Python package (`httpx` + `blake3`), no Rust extension / FFI.
4. `docs/sdk/python.md:134-140` — Python verifies BLAKE3 in pure Python (no FFI); ADR-0016 trade-off + not symmetric with Go's cgo bridge.
5. `docs/sdk/go.md:78-87` — Go API reference + `ClientVerify` default true (with `Get`/`Stat` stub notes at `docs/sdk/go.md:100-119`).
6. `docs/sdk/go.md:85` — `ClientVerifyExplicitFalse` zero-value guard.
7. `docs/sdk/go.md:129-142` — Go two-field explicit opt-out.
8. `docs/sdk/go.md:144-151` — Go race-detector CI.
9. `docs/sdk/javascript.md:47-90` — JS/TS API (`put`/`get`/`stat`), `clientVerify` default, `DigestMismatchError` throw.
10. `docs/sdk/javascript.md:147-160` — JS error reference (`DigestMismatchError` / `COR_CAS_DIGEST_MISMATCH`).
11. `docs/sdk/javascript.md:1-10` — pure-JS (`@noble/hashes`), no WASM/native build step.
