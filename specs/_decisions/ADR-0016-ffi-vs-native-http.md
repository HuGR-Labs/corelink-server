---
id: "ADR-0016"
type: "adr"
doc_status: "ACCEPTED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - "Engineer (S-15 lead)"
  - "Architect (Crypto SME)"
tags: ["adr", "s15", "ffi", "pyo3", "cgo", "wasm", "client-verify", "security", "drift-risk"]
supersedes: null
superseded_by: null
---

# ADR-0016 — FFI Wrappers via Single Rust Truth vs Native HTTP per Language

**Status:** ACCEPTED
**Sprint context:** S-15 (CLI + SDK Integration); WI-S15-004 (FFI wrappers 3 languages)
**Decision date:** 2026-05-14

---

## 1. Context

S-15 introduces SDK wrappers for three language ecosystems — Python, Go, and
JavaScript/TypeScript — to reduce customer adoption friction beyond CLI/Bazel/Buck2.

These wrappers must expose `get`, `put`, and `stat` operations against the
CoreLink CAS. A core security invariant is **CTRL-CAS-002** (client-verify
default-on): every `get()` call must BLAKE3-verify the downloaded bytes against
the claimed digest before returning them to the caller. S-02 (SEALED) delivered
`corelink-client-verify` as the single canonical Rust implementation of this check.

Two implementation strategies were evaluated for the per-language wrappers:

| | **Option A — FFI (single Rust truth)** | **Option B — Native HTTP thin client** |
|---|---|---|
| **Approach** | Python/Go/JS call into the `corelink-client-verify` Rust crate via PyO3 / cgo / wasm-bindgen. | Each language implements HTTP + BLAKE3 verify independently: Python (`hashlib.blake3`), Go (`zeebo/blake3`), JS (`blake3-wasm`). |
| **Verify implementation count** | 1 (Rust) | 3 (Python + Go + JS) |
| **Drift risk** | Zero: same code path, same constant-time compare | High: diverge over time |
| **Build complexity** | Higher: 3 toolchains (maturin, cgo, wasm-pack) | Lower: 3 HTTP libs |
| **Type safety** | Rust type system enforces invariants at compile time | Per-language, inconsistent |

---

## 2. Decision

**ACCEPTED: Option A — FFI wraps single Rust truth.**

All three language wrappers (Python via PyO3, Go via cgo, JS/TS via wasm-bindgen)
link against the `corelink-client-verify` Rust crate compiled as a native
extension / WASM module. The `ClientVerifier` type and its `VerifyConfig`
enforce default-on at the Rust type level; per-language wrappers expose
`client_verify=True/true` constructor flags that map to `VerifyConfig::new()`
(on) or `VerifyConfig::disabled()` (off + warning).

---

## 3. Rationale

### 3.1 Drift risk is a security issue, not just a maintenance cost

Native HTTP per-language means 3 BLAKE3 implementations that **must stay in
sync** with the Rust canonical version. Historically, per-language reimplementations
of cryptographic checks diverge:

- Hash encoding differences (raw bytes vs hex vs base64).
- Constant-time compare omitted in one language.
- Empty-input edge cases handled differently.
- BLAKE3 library version pinned in one language receives a security fix not
  propagated to others.

Any divergence creates an **inadvertent bypass**: a caller using the Python wrapper
may receive an integrity guarantee that the Go wrapper silently drops on a refactor.
This violates INV-CAS-INTEGRITY (CRITICAL registry entry).

### 3.2 Single Rust truth eliminates the class of drift bugs

FFI wrappers call **the same compiled code** via the same C-ABI symbols
(`corelink_verifier_new_default_on`, `corelink_verifier_verify`). A bug fix or
behavioral change in `corelink-client-verify` propagates to all three languages
on the next library release with zero per-language effort. This is a structural
guarantee, not a process guarantee.

### 3.3 Test surface is unified

CI validates client-verify default-on in all three languages against the
**same compiled binary**:

```
Python:  assert client._inner_client_verify_enabled == True
Go:      assert client.IsClientVerifyEnabled() == true
JS:      expect(client._clientVerifyEnabled).toBe(true)
```

A single Rust property test suite covers the verify logic; per-language tests
confirm the wrapper constructor plumbing. This is a smaller, more auditable
surface than 3 independent BLAKE3 implementations × 3 test suites.

### 3.4 Build complexity is acceptable and well-understood

The three toolchains (PyO3/maturin, cgo, wasm-pack) are industry-standard:

- **PyO3 + maturin**: used by Polars, cryptography, orjson — mature, CI-tested.
- **cgo**: standard Go FFI; no runtime overhead vs a pure Go alternative for
  the verify hot path.
- **wasm-bindgen + wasm-pack**: used by Cloudflare Workers Rust crates throughout
  this codebase.

WASM bundle size is bounded to ≤ 1MB via `wasm-opt -O3` + tree-shaking (CI gate).

---

## 4. Rejected Alternative — Option B: Native HTTP per Language

**Rejected.** The primary concern is **drift risk leading to inadvertent
client-verify bypass**.

Secondary concerns:

- 3 BLAKE3 library dependencies (Python / Go / JS) with independent security
  advisories and update cycles.
- 3 separate test suites for verify logic — combinatorial explosion of edge
  cases to maintain.
- No structural guarantee that a behavioral fix in the Rust canonical
  implementation reaches the Python or JS wrapper before it reaches production
  traffic.
- Admin complexity: 3 languages × version matrix × BLAKE3 spec updates.

The only benefit of Option B (lower initial build complexity) does not outweigh
the long-term security maintenance cost.

---

## 5. Consequences

### 5.1 Positive

- **Single source of truth**: `corelink-client-verify` (S-02 SEALED) is the
  one canonical BLAKE3 verify implementation for all three languages. No drift
  possible by construction.
- **CTRL-CAS-002 enforcement is structural**: the Rust type system + private
  constructor fields prevent silent opt-out in the FFI surface.
- **Security fixes propagate automatically** on library release.
- **Auditability**: one codebase to audit for the verify logic.

### 5.2 Negative / Trade-offs

- **Build toolchain complexity**: CI matrix must install maturin, Go + cgo,
  wasm-pack, and wasm-opt per build. Mitigated by caching toolchains in
  GitHub Actions.
- **Cross-compilation complexity**: WASM target requires
  `wasm32-unknown-unknown`; Go cgo requires the Rust library to be compiled
  for the host target before the Go module can be tested. CI scripts handle
  sequencing.
- **WASM bundle size**: wasm-bindgen adds overhead vs pure WASM. Bounded by
  `wasm-opt -O3` + tree-shaking to ≤ 1MB (CI gate).
- **Memory safety responsibility**: pyO3 and cgo wrappers cross the Rust/C
  boundary. Mitigated by: valgrind CI gate (Python), `go test -race` (Go),
  and the same null-check / length-check patterns from
  `corelink-client-verify/ffi.rs`.

### 5.3 Neutral

- Publishing to PyPI / pkg.go.dev / npm is independent of this choice and is
  handled by WI-S15-004 release pipeline.

---

## 6. Invariants Preserved

| Invariant | How ADR-0016 preserves it |
|---|---|
| **CTRL-CAS-002** (client verify default-on) | All 3 wrappers default to `VerifyConfig::new()` (enabled); opt-out requires explicit flag + emits warning. |
| **INV-CAS-INTEGRITY** (CRITICAL) | Single Rust truth — no per-language drift. |
| **CTRL-CRED-001** (no secrets in output) | PAT passed via constructor; never in FFI output. All wrappers follow `pat: str/string/PAT` constructor pattern. |

---

## 7. References

- `specs/04_sprints/S15/work_items/WI-S15-004-ffi-wrappers-python-go-js-adr-0016.md`
- `specs/04_sprints/S02/` — `corelink-client-verify` canonical implementation
- `crates/corelink-client-verify/src/ffi.rs` — C-ABI surface
- `corelink-go/corelink.go` — Go cgo wrapper
- `crates/corelink-py/src/lib.rs` — Python PyO3 wrapper
- `crates/corelink-wasm/src/lib.rs` — WASM wasm-bindgen wrapper

---

## 8. Reviewers

| Role | Name | Sign-off |
|---|---|---|
| Owner | Gustavo Schneiter | _pending_ |
| Engineer (S-15 lead) | _TBD_ | _pending_ |
| Architect (Crypto SME) | _TBD_ | _pending_ |

---

**Fim ADR-0016.**
