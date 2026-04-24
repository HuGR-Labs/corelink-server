---
id: "ADR-0016"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "sdk", "ffi", "client-verify", "stub"]
---

# ADR-0016 — FFI Wrappers (pyO3 + cgo + WASM) vs Native HTTP Thin Clients

## Context

S-15 entrega SDK em 3 languages (Python + Go + JS/TS). Two approaches:

1. **FFI wrappers** sobre `corelink-client-verify` Rust crate (single source of truth).
2. **Native thin HTTP clients** per language (independently implemented).

## Decision

Adotamos **FFI wrappers** (pyO3 + cgo + WASM-bindgen) sobre a única source of truth `corelink-client-verify` (S-02).

## Rationale

**Por que FFI wrappers:**

1. **Client verify default-on** (CTRL-CAS-002): hash check post-download deve ser identical em 3 languages. Native implementations criam **drift risk** — divergent implementations = inconsistent verify behavior = INV-CAS-INTEGRITY violation.
2. **Maintainability**: 1 Rust crate < 3 native clients de manter; bug fixes propagated automaticamente.
3. **Performance**: BLAKE3 hash computation Rust-native is 5-10× faster que pure Python (no SIMD) ou pure JS (no WASM hand-tune).
4. **Type safety**: pyO3 + cgo + WASM-bindgen são battle-tested; provide ergonomic per-language idioms.

**Counterarguments:**

- Build complexity: 3 toolchains pré-compiled per platform (acceptable; ci handles).
- Distribution: native binaries embedded em wheels/modules/wasm bundles (acceptable; Stripe + sigstore + others do this).
- Debugging: stack traces cross-FFI menos legíveis (mitigated via comprehensive logging).

## Consequences

**Positive:**
- Single Rust source of truth → INV-CAS-INTEGRITY consistente.
- Faster hash compute em high-volume scenarios.
- Reduced maintenance burden.

**Negative:**
- More complex build pipeline (per-platform binary wheels).
- Slightly larger SDK download (vs pure native).

## Alternatives considered

- **Native HTTP thin clients per language** (independently implemented): rejected — drift risk.
- **WASM-only across 3 languages** (no native binary): rejected — Python WASM ecosystem still nascent at GA timeline.

## References

- pyO3 <https://pyo3.rs/>.
- cgo Rust <https://doc.rust-lang.org/nomicon/ffi.html>.
- wasm-bindgen <https://rustwasm.github.io/wasm-bindgen/>.
- `specs/04_sprints/S15/_spec_contract.md`.
