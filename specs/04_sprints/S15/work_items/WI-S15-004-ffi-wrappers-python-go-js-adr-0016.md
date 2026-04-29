---
id: "WI-S15-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-15"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s15", "ffi", "pyo3", "cgo", "wasm", "client-verify", "adr-0016", "standard"]
---

# WI-S15-004 — SDK FFI Wrappers 3 Languages (Python via pyO3 → PyPI `corelink-py` async/await native; Go via cgo → pkg.go.dev `corelink-go` context-based; JS/TS via WASM → npm `@corelink/client` Promise-based) com **Client Verify Default-On per CTRL-CAS-002 reusing `corelink-client-verify` Rust Crate (S-02 — Single Source of Truth, Single Rust Truth, Zero Per-Language Drift)** + **ADR-0016 documenta Trade-off FFI vs Native HTTP per-Language (rejected — duplica client-verify logic across 3 languages = drift risk)** + Per-Language Idiomatic API + Memory Safety valgrind/MSAN per Language Harness + WASM Bundle Size ≤ 1MB Benchmark + Tests CI Matrix

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-15](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S15-004 |
| Título | FFI wrappers 3 languages (Python via pyO3 → PyPI `corelink-py` async/await native asyncio integration class `CoreLinkClient(pat, tenant_id)`; Go via cgo → pkg.go.dev `corelink-go` context-based API `func (c *Client) Get(ctx context.Context, digest string) ([]byte, error)`; JS/TS via WASM → npm `@corelink/client` Promise-based API com TypeScript declarations `.d.ts` + bundle size ≤ 1MB tree-shaken) reusing **`corelink-client-verify` Rust crate (S-02 single source of truth — client verify default-on per CTRL-CAS-002 enforced em 3 languages via test; opt-out requires explicit `verify=false` flag com warning logged)**; ADR-0016 documenta trade-off (FFI = single Rust truth sem drift; native HTTP per-language = duplica client-verify logic across 3 languages = drift risk; rejected); per-language idiomatic API; memory safety harness valgrind/MSAN (Python) + `go test -race` (Go) + Jest + asan flags (JS native bindings se applicable); WASM bundle size benchmark; CI matrix testing 3 languages × multiple runtime versions; samples em `examples/python|go|javascript/`. |
| Sprint | S-15 |
| Lane | STANDARD |
| Forcing factors | none (FFI reuses single Rust truth; client-verify default-on baseline CTRL-CAS-002 from S-02; not crypto-load-bearing novel surface) |

## 1. Intent

Biggest crypto-touching WI do S-15: FFI wrappers em 3 languages para reduce friction em customer adoption beyond CLI/Bazel/Buck2 — Python data scientists, Go backend engineers, JS/TS frontend devs todos consume CoreLink via idiomatic API per language. Critical decision: **single Rust truth via FFI** (reuse `corelink-client-verify` crate from S-02) **vs native HTTP per-language** (duplica client-verify logic). ADR-0016 rejects native HTTP — drift risk = client-verify implementations divergem entre Python/Go/JS = bypass possível inadvertidamente. FFI wraps single Rust truth = enforced em 3 languages via test.

```python
# Python pyO3 example (examples/python/quickstart.py)
import asyncio
from corelink import CoreLinkClient

async def main():
    client = CoreLinkClient(
        pat=os.environ["CORELINK_PAT"],
        tenant_id="acme-corp",
    )
    # client_verify default-on enforced via Rust crate; opt-out requires explicit verify=False
    blob_data = await client.get(digest="blake3:abc123...")
    digest = await client.put(data=b"hello", tenant_id="acme-corp")
    print(f"Uploaded: {digest}")

asyncio.run(main())
```

```go
// Go cgo example (examples/go/quickstart.go)
package main

import (
    "context"
    "os"
    "github.com/humangr-labs/corelink-go/v1"
)

func main() {
    client, err := corelink.NewClient(corelink.Config{
        PAT:      os.Getenv("CORELINK_PAT"),
        TenantID: "acme-corp",
        // ClientVerify default-on enforced via Rust crate; opt-out requires explicit
    })
    if err != nil { panic(err) }

    ctx := context.Background()
    data, err := client.Get(ctx, "blake3:abc123...")
    if err != nil { panic(err) }

    digest, err := client.Put(ctx, []byte("hello"))
    if err != nil { panic(err) }
    println("Uploaded:", digest)
}
```

```typescript
// JS/TS WASM example (examples/javascript/quickstart.ts)
import { CoreLinkClient } from "@corelink/client";

const client = new CoreLinkClient({
    pat: process.env.CORELINK_PAT!,
    tenantId: "acme-corp",
    // clientVerify default-on enforced via Rust crate em WASM
});

const blobData = await client.get("blake3:abc123...");
const digest = await client.put(new Uint8Array([0x68, 0x65, 0x6c, 0x6c, 0x6f]));
console.log("Uploaded:", digest);
```

## 2. Narrative

ADR-0016 trade-off é load-bearing decision em S-15 segurança model. Native HTTP per-language seria simpler initial implementation mas acumula drift inevitable: `corelink-client-verify` Rust crate em S-02 é single source of truth (BLAKE3 verify post-download enforcement); replicar logic em Python (`hashlib.blake3`) + Go (`zeebo/blake3`) + JS (`blake3-wasm`) = 3 implementations divergem ao longo de tempo = bypass possível inadvertidamente em refactor. FFI wraps single Rust truth via pyO3 + cgo + WASM-bindgen = enforced em 3 languages via test (single canonical implementation; same code path).

**Risk justification STANDARD lane**:
- Client-verify default-on baseline já em CTRL-CAS-002 (S-02 SEAL); este WI consume baseline.
- FFI bugs em wrappers risk LOW (per-language test harness valgrind/MSAN; CI matrix verifies).
- WASM bundle size ≤ 1MB benchmark provides bound.
- Não há cripto-load-bearing novel control (BYOK + Ed25519 = S-14 HIGH_RISK; este WI consume).

**Diferenciador SOTA**: zero competitors (NativeLink, BuildBuddy, Stripe, Heroku) entregam FFI wrappers em 3 languages com client-verify default-on enforcement.

## 3. Customer Impact & Journey

**Persona 1 — Python data scientist**:
- `pip install corelink-py` → `CoreLinkClient(pat, tenant_id)` async/await native.
- Type stubs `.pyi` para IDE intellisense.
- Sample em `examples/python/`.

**Persona 2 — Go backend engineer**:
- `go get github.com/humangr-labs/corelink-go/v1` → `client.Get(ctx, digest)` context-based.
- pkg.go.dev docs auto-generated.
- Sample em `examples/go/`.

**Persona 3 — JS/TS frontend dev**:
- `npm install @corelink/client` → `await client.get(digest)` Promise-based.
- TypeScript `.d.ts` declarations.
- Bundle size ≤ 1MB tree-shaken para web apps.
- Sample em `examples/javascript/`.

**Persona 4 — Security Engineer**:
- ADR-0016 documenta trade-off (FFI single Rust truth vs native HTTP drift risk).
- Client-verify default-on enforced em 3 languages via test (CTRL-CAS-002 reflection).
- Memory safety harness valgrind/MSAN per language.

## 4. Capability Mapping

- **CAP-SDK-003** (FFI wrappers client-verify) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §5.3 (R-S15-9..11)` + `security_model.md` (CTRL-CAS-002 client verify default-on baseline S-02).

## 5. Tipo

Feature WI; STANDARD lane; FFI wrappers crypto-touching surface.

## 6. Escopo

### 6.1 In-scope

1. **Python via pyO3** (`crates/corelink-py/`):
   - Crate workspace member com pyO3 macros.
   - Class `CoreLinkClient(pat: str, tenant_id: str, client_verify: bool = True)` (default-on enforced).
   - Methods: `async def get(digest: str) -> bytes`, `async def put(data: bytes) -> str`, `async def stat(digest: str) -> StatResult`.
   - Type stubs `.pyi` for IDE intellisense.
   - Build via `maturin build --release`; published para PyPI.
   - Tests: pytest + asyncio + memory safety harness (valgrind / MSAN).
   - Sample `examples/python/quickstart.py`.

2. **Go via cgo** (`crates/corelink-go/`):
   - Crate workspace member exposing `cdylib` + `staticlib`.
   - Wrapper Go module `corelink-go/` com cgo bindings:
     - `package corelink` + `func NewClient(cfg Config) (*Client, error)`.
     - Methods: `func (c *Client) Get(ctx context.Context, digest string) ([]byte, error)`, `Put`, `Stat`.
     - `Config { PAT string; TenantID string; ClientVerify bool }` (default-on enforced).
   - Build via `cargo build --release` + cgo wrapper compilation.
   - Tests: `go test -race ./...`; AddressSanitizer (asan) flags em CI.
   - Sample `examples/go/quickstart.go`.

3. **JS/TS via WASM** (`crates/corelink-wasm/`):
   - Crate workspace member com `wasm-bindgen` macros.
   - Class `CoreLinkClient(config: ClientConfig)`:
     - Methods: `get(digest: string): Promise<Uint8Array>`, `put(data: Uint8Array): Promise<string>`, `stat(digest: string): Promise<StatResult>`.
     - `ClientConfig { pat: string; tenantId: string; clientVerify?: boolean }` (default-on enforced).
   - Build via `wasm-pack build --target web --release`.
   - TypeScript declarations `.d.ts` auto-generated.
   - Bundle size ≤ 1MB tree-shaken (benchmark via `wasm-opt -O3`).
   - Tests: Jest + jsdom; cross-runtime tests (Node.js + browser via Playwright).
   - Sample `examples/javascript/quickstart.ts`.

4. **`corelink-client-verify` Rust crate reuse (S-02)**:
   - 3 wrappers (pyO3 + cgo + wasm-bindgen) reuse same crate via Cargo workspace dependency.
   - Single source of truth: BLAKE3 verify post-download em download paths.
   - Test verifica default-on em 3 languages:
     - Python: `assert client._inner_client_verify_enabled == True`.
     - Go: `assert client.IsClientVerifyEnabled() == true`.
     - JS: `expect(client._clientVerifyEnabled).toBe(true)`.
   - Opt-out requires explicit `client_verify=False` flag com warning logged via tracing crate.

5. **ADR-0016 documenta trade-off**:
   - File `specs/_decisions/ADR-0016-ffi-vs-native-http.md`:
     - Status: ACCEPTED.
     - Context: S-15 SDK FFI 3 languages; client-verify default-on enforcement em 3 languages.
     - Decision: FFI wraps single Rust truth `corelink-client-verify` crate.
     - Rejected alternative: native HTTP per-language (duplica client-verify logic = drift risk).
     - Consequences: build complexity (cgo + pyO3 + wasm-bindgen toolchains); single source of truth = enforced em 3 languages.
     - Reviewers: Engineer + Architect (Crypto SME folded em Architect specialization se applicable).

6. **Memory safety harness per language**:
   - Python: valgrind + MSAN via maturin's CI integration.
   - Go: `go test -race` + AddressSanitizer flags em CI.
   - JS WASM: Jest + jsdom; cross-runtime browser tests via Playwright.
   - CI matrix em GitHub Actions: 3 languages × multiple runtime versions (Python 3.10/3.11/3.12; Go 1.21/1.22; Node.js 18/20/22).

7. **WASM bundle size benchmark**:
   - `wasm-opt -O3` optimization.
   - Tree-shake unused dependencies.
   - Target ≤ 1MB; benchmark CI gate.
   - Documented em `docs/sdk/javascript.md` performance section.

8. **Per-language idiomatic API**:
   - Python: async/await native asyncio integration; type hints; PEP 8 compliant.
   - Go: context-based; error returns; package idioms (factory `NewClient`, exported types).
   - JS: Promise-based; TypeScript-first; ESM modules.

### 6.2 Out-of-scope

- Native HTTP client per-language (rejected per ADR-0016).
- Full SDK feature parity em FFI (admin ops via Python/Go/JS) — restrict to client-verify + cache ops; admin via REST API direct.
- Ruby / Java / Rust / .NET FFI wrappers — pós-GA Q1 (demand-driven).
- WASI Preview 2 target — pós-GA (current target = `--target web` for browser-first).

## 7. Anti-Scope

- Skip ADR-0016 (load-bearing decision).
- Skip client-verify default-on test em 3 languages.
- Skip memory safety harness per language.
- Skip WASM bundle size benchmark.
- Native HTTP per-language (rejected).
- Full SDK feature parity em FFI (anti-scope spec contract §2.2).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: FFI wrappers 3 languages com client-verify default-on

  Scenario: Python pyO3 publishes para PyPI
    Given crate corelink-py em workspace
    When `maturin build --release` runs
    Then wheel package produced
    And published para PyPI as corelink-py
    And `pip install corelink-py` works
    And `from corelink import CoreLinkClient` imports

  Scenario: Go cgo publishes para pkg.go.dev
    Given crate corelink-go em workspace + Go wrapper module
    When `go test ./...` runs
    Then tests pass
    And module published as github.com/humangr-labs/corelink-go/v1
    And pkg.go.dev docs auto-generated

  Scenario: JS/TS WASM publishes para npm
    Given crate corelink-wasm em workspace
    When `wasm-pack build --target web --release` runs
    Then npm package produced
    And published as @corelink/client
    And TypeScript .d.ts declarations included

  Scenario: Client-verify default-on enforced em 3 languages
    Given Python: client = CoreLinkClient(pat, tenant_id)
    Given Go: client, _ := corelink.NewClient(cfg)
    Given JS: const client = new CoreLinkClient(config)
    When test inspects internal state
    Then Python client._inner_client_verify_enabled == True
    And Go client.IsClientVerifyEnabled() == true
    And JS client._clientVerifyEnabled === true

  Scenario: Opt-out client-verify requires explicit flag + warning
    Given Python: client = CoreLinkClient(pat, tenant_id, client_verify=False)
    When client instantiated
    Then warning logged via tracing
    And client_verify_enabled == False
    And test confirms warning contains "DISABLE NOT RECOMMENDED"

  Scenario: ADR-0016 documenta trade-off
    Given specs/_decisions/ADR-0016-ffi-vs-native-http.md
    Then status: ACCEPTED
    And context describes 3-language client-verify enforcement
    And decision: FFI wraps single Rust truth
    And rejected alternative: native HTTP per-language drift risk
    And consequences documented

  Scenario: Memory safety valgrind clean (Python)
    Given Python pyO3 wrapper + sample test suite
    When `valgrind --leak-check=full pytest` runs
    Then 0 leaks detected
    And 0 invalid reads/writes

  Scenario: Memory safety go test -race clean (Go)
    Given Go cgo wrapper + sample test suite
    When `go test -race ./...` runs
    Then 0 race conditions detected
    And tests pass

  Scenario: WASM bundle size ≤ 1MB
    Given wasm-pack build --target web --release
    When wasm-opt -O3 applied + tree-shake
    Then bundle size ≤ 1MB
    And benchmark CI gate verifies

  Scenario: BLAKE3 verify same hash em 3 languages (single Rust truth)
    Given same blob bytes em Python + Go + JS
    When client.get(digest) called em 3 languages
    Then BLAKE3 verify produces same result (pass/fail)
    And uses single corelink-client-verify Rust crate

  Scenario: Cross-runtime CI matrix
    Given Python 3.10/3.11/3.12 + Go 1.21/1.22 + Node.js 18/20/22
    When CI matrix runs
    Then all combinations pass
    And no language-specific failures
```

## 9. Design Decisions

### 9.1 Why FFI (não native HTTP per-language) — ADR-0016

- **Single Rust truth**: `corelink-client-verify` crate (S-02) is single source of truth; FFI wraps = enforced em 3 languages via test.
- **Drift risk rejected**: native HTTP per-language = 3 BLAKE3 verify implementations divergem ao longo de tempo = bypass possível em refactor.
- **Build complexity acceptable**: cgo + pyO3 + wasm-bindgen toolchains well-known.

### 9.2 Why default-on (não opt-in)

- CTRL-CAS-002 baseline (S-02 SEAL); client-verify protege contra bit rot post-download.
- Opt-out requires explicit flag com warning logged (developer must intentional override).

### 9.3 Why per-language idiomatic API (não direct Rust API exposed)

- Python: async/await native; not Rust's Future polling exposed.
- Go: context-based; not Rust's borrow-checker concepts.
- JS: Promise-based; not Rust's match-on-Result.

### 9.4 Why pyO3 + cgo + wasm-bindgen (não single FFI tool)

- pyO3 gold-standard para Python (used em Polars, Cryptography lib).
- cgo standard para Go (Rust → cgo via cdylib).
- wasm-bindgen standard para JS/TS via WASM.

### 9.5 ADR-0016 mandatory

- Yes — load-bearing decision que afeta segurança model + 3-language drift risk.
- Reviewers: Engineer + Architect (Crypto SME folded em Architect specialization se applicable).

## 10. Completeness Criteria

- [ ] **10.s15.004.1** Python pyO3 wrapper published para PyPI as corelink-py (EVT-001).
- [ ] **10.s15.004.2** Go cgo wrapper published para pkg.go.dev as corelink-go (EVT-001).
- [ ] **10.s15.004.3** JS/TS WASM wrapper published para npm as @corelink/client (EVT-001).
- [ ] **10.s15.004.4** Client-verify default-on enforced em 3 languages via test (EVT-002).
- [ ] **10.s15.004.5** ADR-0016 documenta trade-off (FFI vs native HTTP).
- [ ] **10.s15.004.6** Memory safety harness valgrind/MSAN + go test -race + Jest clean.
- [ ] **10.s15.004.7** WASM bundle size ≤ 1MB benchmark CI gate.
- [ ] **10.s15.004.8** Per-language idiomatic API (async/await + context + Promise).
- [ ] **10.s15.004.9** CI matrix 3 languages × multiple runtimes verde.
- [ ] **10.s15.004.10** Samples `examples/python|go|javascript/` committed.

## 11. DoD

- [ ] 3 packages published (PyPI + pkg.go.dev + npm).
- [ ] Client-verify default-on test em 3 languages verde.
- [ ] ADR-0016 committed.
- [ ] Memory safety harness clean.
- [ ] WASM bundle size ≤ 1MB CI gate.
- [ ] Tests: 4+ negative scenarios.
- [ ] Cross-runtime CI matrix verde.
- [ ] Samples committed.

## 12. Invariants Validated

- **CTRL-CAS-002** (client verify default-on) — IMPLEMENTA reflection em FFI 3 languages via test (single Rust truth `corelink-client-verify` crate from S-02).
- **CTRL-CRED-001** reforced via FFI wrappers receive PAT via constructor argument; never logged.
- **INV-CAS-INTEGRITY** (CRITICAL — registry §3.X herdada) reforced via FFI client-verify default-on protege contra bit rot post-download.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Python pyO3 crate | `crates/corelink-py/` | Rust + Python |
| Go cgo crate | `crates/corelink-go/` | Rust |
| Go wrapper module | `corelink-go/` (separate Git tag) | Go |
| WASM crate | `crates/corelink-wasm/` | Rust |
| ADR-0016 | `specs/_decisions/ADR-0016-ffi-vs-native-http.md` | Markdown |
| Python sample | `examples/python/quickstart.py` | Python |
| Go sample | `examples/go/quickstart.go` | Go |
| JS/TS sample | `examples/javascript/quickstart.ts` | TypeScript |
| Python user guide | `docs/sdk/python.md` | Markdown |
| Go user guide | `docs/sdk/go.md` | Markdown |
| JS/TS user guide | `docs/sdk/javascript.md` | Markdown |
| Cross-runtime CI workflow | `.github/workflows/ffi-matrix-ci.yml` | YAML |

## 14. Quality Standards

- **14.s15.004.1** FFI safety: pyO3 + cgo + WASM-bindgen tested em CI; memory safety em valgrind/MSAN per language harness.
- **14.s15.004.2** Test coverage ≥ 85% per language wrapper.
- **14.s15.004.3** SAST: cargo-audit + cargo-deny clean; pip-audit clean; npm audit clean.
- **14.s15.004.4** WASM bundle size ≤ 1MB; benchmark CI gate.
- **14.s15.004.5** Cross-runtime CI matrix sustained 7d.
- **14.s15.004.6** Docs com ≥ 3 examples per language; copy-paste-runnable.

## 15. Test Plan

### Unit tests (≥ 85% coverage)
- Per-language: client-verify default-on assertion.
- Per-language: opt-out warning logged.
- Per-language: PAT validation.
- Per-language: API idiomatic style (async/await + context + Promise).

### Integration tests (E2E vs staging cluster)
- Python: round-trip put/get/stat com client-verify pass.
- Go: idem.
- JS WASM: idem (Node.js + browser via Playwright).

### Negative scenarios (≥ 4 per language)
1. **PAT invalid**: per-language clear auth error.
2. **Network unreachable**: retry com backoff (FM-150).
3. **Client-verify hash mismatch**: tampered blob → exception/error com clear message.
4. **Opt-out warning logged**: client_verify=False → warning emitted.
5. **WASM bundle size regression**: > 1MB → CI gate fails.
6. **Memory leak (Python)**: valgrind detect → CI fail.
7. **Race condition (Go)**: `go test -race` detect → CI fail.

## 16. Failure Modes

- **FM-150** (transient network): retry com exponential backoff per language.
- **FM-160** (auth invalid): clear error per language idiomatic style.

## 17. Controls

- **CTRL-CAS-002** (client verify default-on) IMPLEMENTA primary em 3 languages via FFI single Rust truth.
- **CTRL-CRED-001** reforced via FFI wrappers receive PAT via constructor; never logged.

## 18. Resilience Patterns

- Retry transient errors (FM-150) per language.
- WASM tree-shake + wasm-opt -O3 para bundle size bound.
- Single Rust truth = no per-language drift risk.

## 19. Observability

CLI/SDK telemetry opt-in default-off (WI-S15-005); quando ativo:
- `corelink_sdk_ffi_client_verify_total{language, outcome}` counter (language ∈ python|go|js; outcome ∈ ok|hash_mismatch).
- `corelink_sdk_ffi_invocation_total{language, op}` counter (op ∈ get|put|stat).
- Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT via constructor argument; never em FFI args bypass.
- **Tampering**: client-verify default-on em 3 languages via single Rust truth (CTRL-CAS-002).
- **Repudiation**: opt-out logged via tracing; LINDDUN review applicable.
- **Information disclosure**: PAT never logged; CTRL-CRED-001 enforced.
- **DoS**: retry com bounded backoff.
- **Elevation of privilege**: tenant scoped via PAT.

**LINDDUN delta**:
- Linkability: telemetry NÃO inclui tenant_id.
- Identifiability: nunca PII em FFI logs.
- Disclosure: secrets nunca em FFI output.

## 21. Dependencies

### Hard blockers
- WI-S15-001 (CLI binary patterns reused).
- S-02 SEALED (`corelink-client-verify` Rust crate single source of truth).

### Soft blockers
- WI-S15-005 (telemetry opt-in implementation).

### Outbound
- WI-S15-006 (ship gate references este como completeness 10.s15.4).
- S-19 (customer onboarding uses FFI samples).

## 22. Effort PERT

O: 18h, M: 28h, P: 44h → PERT **28.7h** (per spec contract §12; biggest crypto-touching WI; 3 wrappers + ADR + CI matrix).

## 23. Cost Analysis

- GitHub Actions matrix runs: ~$10/build × 5 builds/week = $200/mês.
- PyPI/npm/pkg.go.dev publishing: free.
- Total: ~$200/mês incremental.

## 24. Post-mortem Hooks

- FFI memory unsafety detected (valgrind/MSAN positive) → CRITICAL post-mortem.
- Client-verify drift detected entre 3 languages → CRITICAL + Security review.
- WASM bundle size > 2MB sustained → SEV-2 (DX regression).
- Native HTTP fork attempted (anti-scope) → SEV-1 + ADR-0016 review.

## 25. Rollback / Recovery

FFI wrapper bug detected → revert via PyPI/npm/pkg.go.dev version yank; SemVer patch release com fix; 24h SLA.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | FFI bugs em Python/Go/JS wrappers | M | M | LOW | L | LOW | Per-language test harness valgrind/MSAN; CI matrix |
| R-002 | JS WASM bundle size > 1MB | M | L | LOW | L | LOW | Tree-shake; benchmark; documented |
| R-003 | pyO3 / cgo / wasm-bindgen breaking changes | L | L | MEDIUM | L | LOW | Pin minor version; CHANGELOG monitoring |
| R-004 | Client-verify drift detection slow | L | L | HIGH | L | LOW | Property test compare hash em 3 languages periodically |
| R-005 | Memory leak em pyO3 wrapper | L | M | MEDIUM | L | LOW | valgrind CI gate |
| R-006 | Race condition em cgo wrapper | L | M | MEDIUM | L | LOW | go test -race CI gate |
| R-007 | npm install breaks em older Node.js | M | L | LOW | L | LOW | CI matrix Node 18/20/22 |
| R-008 | ADR-0016 reverted post-SEAL | L | L | HIGH | L | LOW | Document load-bearing rationale; review cycle |

## 27. Knowledge Transfer

- Tech talk (1.5h): "CoreLink FFI Architecture: Single Rust Truth via pyO3 + cgo + wasm-bindgen".
- Doc `docs/sdk/python.md` + `docs/sdk/go.md` + `docs/sdk/javascript.md`.
- ADR-0016 reading mandatory para new engineers.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Engineer (S-15 lead) | _TBD_ | _pending_ |
| 4 | QA Lead | _TBD; emphatic — memory safety + cross-runtime CI matrix_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | DevX advisor | _TBD; emphatic — per-language idiomatic API + WASM bundle size + DX comparison_ | _pending_ |
| 7 | Docs lead | _TBD; emphatic — `docs/sdk/python.md` + `docs/sdk/go.md` + `docs/sdk/javascript.md` + ADR-0016_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical. Crypto SME folds em Architect specialization se applicable em PR review (este WI é client-verify reflection, not novel cripto control).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S15-004 (cycle 12.S15.0; FFI wrappers 3 languages + ADR-0016 single Rust truth). |

## 30. Anti-patterns evitados

- Skip ADR-0016 (load-bearing decision).
- Skip client-verify default-on test em 3 languages.
- Skip memory safety harness per language.
- Skip WASM bundle size benchmark.
- Native HTTP per-language (rejected).
- Full SDK feature parity em FFI (anti-scope).
- Custom client-verify per language (drift risk).
- Hardcode PAT em FFI args.

---

**Fim WI-S15-004.**
