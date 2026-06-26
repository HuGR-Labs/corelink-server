---
type: "ADR"
title: "ADR-0016 — FFI wrappers over a single Rust truth vs native HTTP per language"
description: "Decides the Python/Go/JS SDK wrappers link against the one canonical corelink-client-verify Rust crate via FFI, eliminating per-language drift in the client-verify integrity check."
source_files:
  - "specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s15", "ffi", "sdk", "client-verify", "security"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0016 — FFI wrappers over a single Rust truth vs native HTTP per language

CoreLink's client-verify invariant requires every CAS `get()` to BLAKE3-verify downloaded bytes against the claimed digest before returning them — and that check must behave identically in every SDK. This ADR decides the Python, Go, and JS wrappers all FFI into the single canonical `corelink-client-verify` Rust crate rather than each re-implementing HTTP + BLAKE3 natively. It matters because per-language re-implementation of a cryptographic check is a structural source of inadvertent verify-bypass, and FFI makes that class of bug impossible by construction. The native CAS surface it protects is [native-cas](/surfaces/native-cas.md).

# Context

S-15 adds SDK wrappers for Python, Go, and JS exposing `get`/`put`/`stat`, all of which must honor `CTRL-CAS-002` (client-verify default-on) (`specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:30-35`). Two strategies were evaluated: FFI into the one Rust crate (1 verify implementation, zero drift) versus native HTTP per language (3 BLAKE3 implementations that must stay in sync) (`specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:39-46`).

# Decision

Accept Option A — all three wrappers (PyO3, cgo, wasm-bindgen) link against `corelink-client-verify`, with the Rust type system enforcing default-on at compile time (`specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:51-58`). Native HTTP per language was rejected: drift between three independent BLAKE3 implementations (hash-encoding differences, an omitted constant-time compare, divergent edge cases, or an unpropagated security fix) creates an inadvertent bypass that violates the CRITICAL `INV-CAS-INTEGRITY` (`specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:64-78`).

# Consequences

- One canonical verify implementation means no drift is possible and security fixes propagate to all three languages on the next library release — a structural, not procedural, guarantee (`specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:142-147`).
- The cost is build-toolchain complexity (maturin + cgo + wasm-pack), cross-compilation sequencing, and WASM bundle size, all bounded/mitigated by CI caching, scripts, and a ≤1MB `wasm-opt` gate (`specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:152-160`).
- The Rust/C boundary crossing adds a memory-safety responsibility, mitigated by valgrind/`-race`/null-and-length checks (`specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:161-164`).

# Citations

1. `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:30-35` — client-verify (CTRL-CAS-002) requirement across SDKs.
2. `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:39-46` — the two strategies and their verify-implementation count.
3. `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:51-58` — the decision: Option A, FFI single Rust truth.
4. `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:64-78` — drift as a security issue and the rejected native-HTTP option.
5. `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:142-147` — the structural no-drift guarantee.
6. `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:152-164` — build/memory-safety trade-offs and mitigations.
