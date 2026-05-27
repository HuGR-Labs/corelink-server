---
id: "WI-S18-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "LOW_RISK"
parent: "S-18"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "AUTH-MODEL"
  - "PRIVACY-MODEL"
tags: ["wi", "s18", "docs", "sdk-guides", "python", "go", "js", "cli", "client-verify", "low-risk"]
---

# WI-S18-003 — SDK Guides Per-language Production-ready 4 Deliverables (per Spec Contract §5.3 R-S18-5): Python `corelink-py` pyO3 (Install + First Cache Hit + Advanced BYOK + DSR; async/await asyncio Native) + Go `corelink-go` cgo (Install + First Cache Hit + Advanced BYOK + DSR; context-based API) + JS/TS `@corelink/client` WASM (Install + First Cache Hit + Advanced BYOK + DSR; Promise-based API + TypeScript .d.ts) + CLI `corelink` Reference Command per Command (7 Subcommands ls/get/put/stat/bench/doctor/version + `--output=json` Flag em Todos para Scripting per S-15 SEALED Reuse) + Client Verify Default-on per CTRL-CAS-002 Documented (R-S18-6) + Per-language Idiomatic API Patterns + Zero Customer Data em SDK Examples (Fixture Pipeline; CTRL-PRIV-001 Enforced)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** LOW_RISK
> **Parent:** [S-18](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S18-003 |
| Título | SDK guides Python pyO3 + Go cgo + JS/TS WASM + CLI per command reference. |
| Sprint | S-18 |
| Lane | LOW_RISK |
| Forcing factors | none (LOW_RISK; SDK guides consume FFI wrappers from S-15 SEALED reuse; client-verify default-on per CTRL-CAS-002 reflection; CLI 7 subcommands from S-15 SEALED reuse) |

## 1. Intent

Entrega **SDK guides per-language production-ready** 4 deliverables (per spec contract §5.3 R-S18-5): **Python** `corelink-py` pyO3 (PyPI; install + first cache hit + advanced BYOK + DSR; async/await asyncio native) + **Go** `corelink-go` cgo (pkg.go.dev; install + first cache hit + advanced BYOK + DSR; context-based API) + **JS/TS** `@corelink/client` WASM (npm; install + first cache hit + advanced BYOK + DSR; Promise-based API + TypeScript `.d.ts`) + **CLI** `corelink` reference command per command (7 subcommands ls/get/put/stat/bench/doctor/version + `--output=json` flag em todos para scripting per S-15 SEALED reuse) + client verify default-on per CTRL-CAS-002 documented (R-S18-6) + per-language idiomatic API patterns + zero customer data em SDK examples (fixture pipeline; CTRL-PRIV-001 enforced).

```python
# File: apps/docs/docs/sdk/python.mdx (excerpt)
import asyncio
from corelink import CoreLinkClient

# PAT format: corelink_<env>_<token_id>.<random_secret>.<hmac_sig> per S-03 decision (a)
async def main():
    async with CoreLinkClient(
        pat="corelink_dev_t_xxx.xxx.xxx",  # placeholder; obtain real PAT via https://app.corelink.humangr.com/tokens
        tenant_id="acme-corp",
    ) as client:
        # Cache write
        digest = await client.put(b"hello world")
        print(f"Stored: {digest}")

        # Cache read (client verify default-on per CTRL-CAS-002)
        data = await client.get(digest)  # auto-verify BLAKE3 post-download
        assert data == b"hello world"

asyncio.run(main())
```

## 2. Narrative

S-15 SEALED hard blocker entrega FFI wrappers (Python pyO3 + Go cgo + JS/TS WASM) reusing `corelink-client-verify` Rust crate (single source of truth per ADR-0016; client verify default-on per CTRL-CAS-002 verified em 3 languages via test). S-18 documenta the SDK guides per-language production-ready com idiomatic API patterns:
- **Python**: async/await asyncio native (CoreLink stack idiomatic Python).
- **Go**: context-based API (`func (c *Client) Get(ctx context.Context, digest string) ([]byte, error)`).
- **JS/TS**: Promise-based API + TypeScript `.d.ts` declaration.

CLI `corelink` reference command per command (7 subcommands ls/get/put/stat/bench/doctor/version + `--output=json` flag em todos para scripting per S-15 SEALED reuse) documenta cada subcommand com flags + examples + exit codes + JSON output schema.

Client verify default-on per CTRL-CAS-002 documented (R-S18-6) garante post-download BLAKE3 verify; opt-out requires explicit `verify=false` flag com warning logged (CTRL-CAS-002 reflection).

Zero customer data em SDK examples (fixture pipeline; CTRL-PRIV-001 enforced; PAT format placeholder `corelink_dev_t_xxx.xxx.xxx`; tenant_id placeholder `acme-corp`; never real customer data em examples).

**Risk justification LOW_RISK lane (zero forcing factors)**:
- SDK guides consume FFI wrappers from S-15 SEALED reuse (single source of truth `corelink-client-verify` Rust crate; ADR-0016).
- Client verify default-on per CTRL-CAS-002 reflection (S-15 SEALED enforced em 3 languages via test).
- CLI 7 subcommands from S-15 SEALED reuse (cross-OS signed binaries macOS/Linux/Windows).
- Não introduz tenant data flow path novo.
- Não há cripto-load-bearing controles novos.

## 3. Customer Impact & Journey

**Persona — External Developer / Build Engineer (prospect)**:
- SDK guide my-language idiomatic (Python async/await + Go context + JS Promise + CLI subcommands).
- Client verify default-on per CTRL-CAS-002 documented (post-download BLAKE3 verify; opt-out explicit `verify=false`).
- 7 CLI subcommands reference command per command (ls/get/put/stat/bench/doctor/version + `--output=json` flag scripting).
- Zero customer data em examples (PAT placeholder; tenant_id placeholder).
- Diferenciador competitivo vs NativeLink/BuildBuddy: 4 SDK guides production-ready (Python + Go + JS + CLI) com client-verify default-on (zero competitors).

## 4. Capability Mapping

- **CAP-DOCS-003** (SDK guides Python/Go/JS/CLI) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4` + `auth_model.md` (PAT auth flow para SDK examples).

## 5. Tipo

Content WI; LOW_RISK lane.

## 6. Escopo

### 6.1 In-scope

1. **Python SDK guide** em `apps/docs/docs/sdk/python.mdx`:
   - **Install**: `pip install corelink-py` (PyPI; S-15 SEALED reuse).
   - **First cache hit**: `CoreLinkClient(pat, tenant_id)` async/await; `await client.put(data)` + `await client.get(digest)`.
   - **Advanced BYOK**: BYOK setup via tenant config (S-14 SEALED reuse; admin plane).
   - **Advanced DSR**: Data Subject Request Erasure (S-16 SEALED reuse).
   - **Async/await asyncio native** (CoreLink stack idiomatic Python).
   - **Type stubs `.pyi`**: `CoreLinkClient`, `BlobDigest`, `TenantId` types.
   - **Examples sanitized**: PAT placeholder `corelink_dev_t_xxx.xxx.xxx`; tenant_id placeholder `acme-corp`; never real customer data.

2. **Go SDK guide** em `apps/docs/docs/sdk/go.mdx`:
   - **Install**: `go get github.com/humangr-labs/corelink-go` (pkg.go.dev; S-15 SEALED reuse).
   - **First cache hit**: `corelink.NewClient(pat, tenantID)` context-based; `client.Put(ctx, data)` + `client.Get(ctx, digest)`.
   - **Advanced BYOK**: BYOK setup análogo Python.
   - **Advanced DSR**: DSR Erasure análogo Python.
   - **Context-based API** (`func (c *Client) Get(ctx context.Context, digest string) ([]byte, error)`).
   - **Examples sanitized**: PAT placeholder + tenant_id placeholder.

3. **JS/TS SDK guide** em `apps/docs/docs/sdk/javascript.mdx`:
   - **Install**: `npm install @corelink/client` (npm; S-15 SEALED reuse).
   - **First cache hit**: `new CoreLinkClient({pat, tenantId})` Promise-based; `await client.put(data)` + `await client.get(digest)`.
   - **Advanced BYOK**: BYOK setup análogo Python.
   - **Advanced DSR**: DSR Erasure análogo Python.
   - **Promise-based API + TypeScript .d.ts** declaration.
   - **WASM bundle size benchmark**: ≤ 1MB tree-shake (S-15 SEALED reuse).
   - **Examples sanitized**: PAT placeholder + tenant_id placeholder.

4. **CLI reference** em `apps/docs/docs/sdk/cli.mdx`:
   - 7 subcommands per command reference (S-15 SEALED reuse):
     - `corelink ls --tenant <id> --prefix <p>`: list blobs em tenant.
     - `corelink get <digest> [-o file]`: download blob.
     - `corelink put <file> [--digest=<d>]`: upload blob.
     - `corelink stat <digest>`: inspect blob metadata.
     - `corelink bench [--write|--read|--full]`: benchmark cache.
     - `corelink doctor [--json]`: 8 checks actionable diagnostic (Network + Auth + Storage write + Storage read + BYOK + Region + Quota + Client verify; per-failure next-action linkado a `docs/error_taxonomy.md COR_*` code).
     - `corelink version`: precise (semver + git rev + SLSA attestation link).
   - `--output=json` flag em todos para scripting (S-15 SEALED canonical).
   - Per subcommand: usage + flags + examples + exit codes + JSON output schema.
   - PAT NUNCA em CLI args (env var `CORELINK_PAT` only; CTRL-CRED-001 reflection).

5. **Client verify default-on per CTRL-CAS-002 documented** (R-S18-6):
   - Post-download BLAKE3 verify default-on (S-15 SEALED enforced em 3 languages via test reusing `corelink-client-verify` Rust crate per ADR-0016).
   - Opt-out requires explicit `verify=false` flag com warning logged.
   - Documented per SDK guide (Python + Go + JS).

6. **Categorization Diátaxis discipline**:
   - SDK guides (Python + Go + JS + CLI) → **Reference** (Diátaxis information-oriented; per WI-S18-001 sidebar).
   - "How to integrate Bazel CI" + "How to configure BYOK" → **How-to** (task-oriented).
   - PR review checks Diátaxis taxonomy fit per Quality Standard 14.s18.2.

7. **Zero customer data em SDK examples** (CTRL-PRIV-001 reflection):
   - Fixture pipeline ensures no real PII (test fixtures only).
   - PAT format placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a)).
   - Tenant_id placeholder `acme-corp` (never real tenant ID).
   - CI lint check via grep for known PII patterns (reuse from WI-S18-002).

### 6.2 Out-of-scope (deferred)

- Compliance + security + pricing pages (WI-S18-004).
- WCAG 2.2 AA axe-core CI gate (WI-S18-005).
- Lighthouse ≥ 95 CI gate (WI-S18-005).
- Vale tone consistency lint CI (WI-S18-005).
- lychee broken-link CI (WI-S18-005).
- UX research session 5 dev sample (WI-S18-005).
- i18n native speaker review (WI-S18-005).
- Java/Kotlin/Swift/Ruby SDK guides (pós-GA Q1).

## 7. Anti-Scope

- Skip 4 SDK guides (Python + Go + JS + CLI canonical per spec contract §5.3 R-S18-5).
- Skip client verify default-on documentation (CTRL-CAS-002 reflection per R-S18-6).
- Real customer data em SDK examples (CTRL-PRIV-001 violation; fixture pipeline + CI lint check).
- PAT em CLI args em examples (CTRL-CRED-001 violation; env var only).
- Skip per-language idiomatic API patterns (Python async/await; Go context; JS Promise).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: SDK guides Python + Go + JS + CLI per command reference

  Scenario: Python SDK guide production-ready
    Given apps/docs/docs/sdk/python.mdx published
    When user follows install + first cache hit + advanced BYOK + DSR
    Then async/await asyncio native examples work
    And PAT placeholder corelink_dev_t_xxx.xxx.xxx (CTRL-CRED-001)
    And tenant_id placeholder acme-corp (CTRL-PRIV-001)

  Scenario: Go SDK guide production-ready
    Given apps/docs/docs/sdk/go.mdx published
    When user follows install + first cache hit + advanced BYOK + DSR
    Then context-based API examples work
    And PAT placeholder + tenant_id placeholder

  Scenario: JS/TS SDK guide production-ready
    Given apps/docs/docs/sdk/javascript.mdx published
    When user follows install + first cache hit + advanced BYOK + DSR
    Then Promise-based API + TypeScript .d.ts examples work
    And WASM bundle size ≤ 1MB tree-shake (S-15 SEALED reuse)

  Scenario: CLI reference command per command (7 subcommands)
    Given apps/docs/docs/sdk/cli.mdx published
    When user references corelink doctor [--json]
    Then 8 checks actionable diagnostic documented
    And per-failure next-action linkado a error_taxonomy COR_* code
    And --output=json flag canonical em todos subcommands

  Scenario: Client verify default-on per CTRL-CAS-002 documented (R-S18-6)
    Given any SDK guide (Python OR Go OR JS)
    When client.get(digest) called
    Then BLAKE3 verify post-download default-on
    And opt-out requires explicit verify=false flag com warning logged

  Scenario: Zero customer data em SDK examples (CTRL-PRIV-001)
    Given any SDK example em docs
    When PAT/tenant_id used
    Then placeholder format enforced (corelink_dev_t_xxx.xxx.xxx + acme-corp)
    And CI lint check via grep enforces (no real customer data)

  Scenario: PAT NUNCA em CLI args (CTRL-CRED-001)
    Given CLI reference example
    When PAT auth used
    Then env var CORELINK_PAT only (never --pat flag)

  Scenario: Diátaxis taxonomy discipline (SDK guides → Reference)
    Given new SDK guide page added em PR
    When PR submitted
    Then PR template enforces category Reference
    And reviewer Docs lead checks taxonomy fit
```

## 9. Design Decisions

### 9.1 Why 4 SDK guides (Python + Go + JS + CLI) e não mais

- Per spec contract §5.3 R-S18-5 canonical (Python + Go + JS + CLI).
- Coverage 80%+ build engineering ecosystem (Bazel/Buck2 = primary Go/Java/C++/Rust; SDKs = Python/Go/JS).
- CLI = power users + CI scripting use case.
- Adicional languages (Java/Kotlin/Swift/Ruby) pós-GA Q1.

### 9.2 Why per-language idiomatic API patterns

- Python async/await asyncio native (CoreLink stack idiomatic Python).
- Go context-based API (`ctx context.Context` canonical Go pattern).
- JS Promise-based API + TypeScript `.d.ts` declaration (canonical JS/TS pattern).
- Idiomatic = developer adoption baseline.

### 9.3 Why client verify default-on per CTRL-CAS-002 documented

- CTRL-CAS-002 reflection (S-15 SEALED enforced em 3 languages via test reusing `corelink-client-verify` Rust crate per ADR-0016).
- Opt-out requires explicit `verify=false` flag com warning logged (security-first default).
- Reviewer Security lead em WI-S18-004 (cross-functional gate compliance/security pages).

### 9.4 Why CLI doctor 8 checks (não 6)

- Lote 9.5c canonical 8 checks (Codex R3-14 fix); reuse S-15 SEALED.
- 8 checks: Network + Auth + Storage write + Storage read + BYOK + Region + Quota + Client verify.
- Per-failure next-action linkado a `docs/error_taxonomy.md COR_*` code (S-09 SEALED reuse).

### 9.5 Why fixture pipeline + CI lint check (CTRL-PRIV-001)

- Zero customer data real em SDK examples (privacy enforcement).
- Fixture pipeline ensures no real PII (test fixtures only).
- CI lint check via grep for known PII patterns (reuse from WI-S18-002).
- PAT format placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a); never real PAT).
- Tenant_id placeholder `acme-corp` (never real tenant ID).

### 9.6 ADR potencial?

- Não — SDK guides consume FFI wrappers from S-15 SEALED reuse (ADR-0016 canonical em S-15); CLI 7 subcommands from S-15 SEALED reuse; client verify default-on per CTRL-CAS-002 reflection (no novel decision; ADR não necessário em S-18 docs sprint).

## 10. Completeness Criteria

- [ ] **10.s18.003.1** Python SDK guide `apps/docs/docs/sdk/python.mdx` (install + first cache hit + advanced BYOK + DSR; async/await asyncio).
- [ ] **10.s18.003.2** Go SDK guide `apps/docs/docs/sdk/go.mdx` (install + first cache hit + advanced BYOK + DSR; context-based API).
- [ ] **10.s18.003.3** JS/TS SDK guide `apps/docs/docs/sdk/javascript.mdx` (install + first cache hit + advanced BYOK + DSR; Promise-based API + TypeScript .d.ts).
- [ ] **10.s18.003.4** CLI reference `apps/docs/docs/sdk/cli.mdx` (7 subcommands per command + --output=json + 8 checks doctor).
- [ ] **10.s18.003.5** Client verify default-on per CTRL-CAS-002 documented em 3 SDK guides (R-S18-6).
- [ ] **10.s18.003.6** Per-language idiomatic API patterns (Python async/await + Go context + JS Promise).
- [ ] **10.s18.003.7** Zero customer data em SDK examples (fixture pipeline + CI lint check; CTRL-PRIV-001).
- [ ] **10.s18.003.8** Diátaxis taxonomy discipline (SDK guides → Reference; PR review checks).

## 11. DoD

- [ ] 4 SDK guides published (Python + Go + JS + CLI).
- [ ] Client verify default-on documented em 3 SDK guides.
- [ ] Per-language idiomatic API patterns.
- [ ] Zero customer data em SDK examples (CI lint check).
- [ ] Diátaxis taxonomy discipline.
- [ ] Adversarial scenarios 3+ documented.

## 12. Invariants Validated

- **CTRL-CAS-002** (client verify default-on) — IMPLEMENTA reflection em 3 SDK guides (Python + Go + JS); opt-out requires explicit `verify=false` flag com warning logged.
- **CTRL-CRED-001** (no secrets em CLI output / docs) — IMPLEMENTA reflection em PAT format placeholder + PAT NUNCA em CLI args (env var only).
- **CTRL-PRIV-001** (zero PII em screenshots/examples) — IMPLEMENTA reflection em fixture pipeline + CI lint check via grep (reuse from WI-S18-002).
- **Não introduz INVs novas** (sprint consumer; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Python SDK guide | `apps/docs/docs/sdk/python.mdx` | MDX |
| Go SDK guide | `apps/docs/docs/sdk/go.mdx` | MDX |
| JS/TS SDK guide | `apps/docs/docs/sdk/javascript.mdx` | MDX |
| CLI reference | `apps/docs/docs/sdk/cli.mdx` | MDX |
| Fixture pipeline | `apps/docs/_fixtures/` (test fixtures only) | Markdown / Code samples |
| CI lint check (PII patterns + PAT format placeholder) | `.github/workflows/docs-pii-pat-lint.yml` | YAML |

## 14. Quality Standards

- **14.s18.003.1** Client verify default-on documented em 3 SDK guides (CTRL-CAS-002 reflection per R-S18-6).
- **14.s18.003.2** Per-language idiomatic API patterns (Python async/await + Go context + JS Promise).
- **14.s18.003.3** Examples sanitization: fixture pipeline ensures no real customer data; test fixtures only (per Quality Standard 14.s18.9).
- **14.s18.003.4** Diátaxis taxonomy discipline (SDK guides → Reference; PR review checks).
- **14.s18.003.5** Cost regression gate: zero adicional cost (SDK guides reuse FFI wrappers from S-15 SEALED).

## 15. Test Plan

### Unit tests
- Python SDK example async/await asyncio compiles (`python -c` smoke).
- Go SDK example context-based compiles (`go build` smoke).
- JS/TS SDK example Promise-based + TypeScript compiles (`tsc` smoke).
- CLI reference subcommand examples valid (smoke).

### Integration tests
- Python SDK guide first cache hit example works (PyPI install + run).
- Go SDK guide first cache hit example works (pkg.go.dev install + run).
- JS/TS SDK guide first cache hit example works (npm install + run).
- CLI doctor 8 checks output JSON schema valid.
- Fixture pipeline ensures no real PII em examples (CI lint check via grep; PR fails se real PAT/tenant_id detected).

### Adversarial scenarios (3+)
1. Real customer data em SDK example inadvertent (developer mistake) → CI lint check via grep fails (CTRL-PRIV-001 + CTRL-CRED-001 reflections).
2. PAT em CLI args em example (developer mistake) → CI lint check fails (CTRL-CRED-001; env var only canonical).
3. Client verify opt-out documented sem warning (developer mistake) → PR review reviewer Docs lead catches; reinforce CTRL-CAS-002 reflection.

## 16. Failure Modes

- **FM-DOC-PII-LEAK** (real customer data em examples inadvertent): CI lint check via grep enforces fixture pipeline (CTRL-PRIV-001 reflection mitigation).
- **FM-DOC-PAT-CLI-ARGS** (PAT em CLI args em example inadvertent): CI lint check via grep enforces env var only (CTRL-CRED-001 reflection mitigation).

## 17. Controls

- **CTRL-CAS-002** (client verify default-on) — IMPLEMENTA reflection em 3 SDK guides (Python + Go + JS); R-S18-6 canonical.
- **CTRL-CRED-001** (no secrets em CLI output / docs) — IMPLEMENTA reflection em PAT format placeholder + PAT NUNCA em CLI args.
- **CTRL-PRIV-001** (zero PII em screenshots/examples) — IMPLEMENTA reflection em fixture pipeline + CI lint check.

## 18. Resilience Patterns

- N/A (docs sprint consumer; non-cripto-load-bearing).

## 19. Observability

- CI lint check status (PR fails se PII or real PAT detected em examples).
- PR review status Diátaxis taxonomy fit (Docs lead reviewer).

Não introduz métricas Prometheus per-tenant em content WI (per observability_model §3.1; sprint consumer).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT format examples placeholder (never real PAT; CTRL-CRED-001 reflection).
- **Tampering**: docs source git-tracked + reviewer Docs lead.
- **Repudiation**: GitHub Actions CI gate log + git commit history.
- **Information disclosure**: PAT format placeholder + tenant_id placeholder enforced via CI lint check (CTRL-CRED-001 + CTRL-PRIV-001 reflections).
- **DoS**: nenhuma (docs content WI).
- **Elevation of privilege**: docs publish requires PR review + Docs lead sign-off + CI lint check pass.

**LINDDUN delta**:
- Linkability: nenhuma (docs content WI; no per-tenant identifiers em examples).
- Identifiability: PAT format placeholder + tenant_id placeholder (never real customer data em examples).
- Non-repudiation: GitHub Actions CI lint check log.
- Detectability: CI lint check via grep for known PII patterns + PAT format detection.
- Disclosure: CTRL-CRED-001 + CTRL-PRIV-001 reflections (no secrets ou PII em docs).

## 21. Dependencies

### Hard blockers
- **WI-S18-001 SEALED** (Docusaurus 3.x foundation + Diátaxis sidebar Reference category).
- **WI-S18-002 SEALED** (REAPI v2 reference auto-gen + 4-language code examples + PAT format placeholder + CI lint check).
- **S-15 SEALED** (FFI wrappers Python pyO3 + Go cgo + JS/TS WASM + CLI 7 subcommands cross-OS signed; client verify default-on per CTRL-CAS-002).

### Soft blockers
- **S-14 SEALED** (BYOK setup; advanced BYOK examples).
- **S-16 SEALED** (DSR Erasure; advanced DSR examples).

### Outbound
- WI-S18-004 (compliance + security + pricing pages cross-functional gate).
- WI-S18-005 (closing ship gate UX research 5 dev sample finds answer ≤ 30s).

## 22. Effort PERT

O: 12h, M: 18h, P: 28h → PERT **18.7h** (per spec contract §12; consolidated within WI-S18-002 PERT 23h baseline em PERT total 87h spec; 4 SDK guides production-ready).

## 23. Cost Analysis

**Direct cost**:
- Zero adicional cost (SDK guides reuse FFI wrappers from S-15 SEALED).
- GitHub Actions free tier (CI lint check): $0/mês.

**Total**: ~$0/mês adicional.

**Indirect cost**: 0 dev adoption friction (4 SDK guides production-ready) + dev experience baseline = priceless.

## 24. Post-mortem Hooks

- Real customer data em SDK example merged inadvertent → CRITICAL post-mortem (privacy; CTRL-PRIV-001 reinforce).
- PAT em CLI args em example merged inadvertent → CRITICAL post-mortem (security; CTRL-CRED-001 reinforce).
- Client verify opt-out documented sem warning → post-mortem + Docs lead review reinforce (CTRL-CAS-002 reflection).

## 25. Rollback / Recovery

Docs rollback: revert PR + redeploy CF Pages previous build. RTO ≤ 5min. CI lint check retroactive enforcement via PR re-run.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Real customer data em SDK example inadvertent | L | H (CI lint) | MEDIUM | L | LOW | Fixture pipeline + CI lint check via grep enforces (CTRL-PRIV-001 + CTRL-CRED-001 reflections) |
| R-002 | PAT em CLI args em example inadvertent | L | H (CI lint) | MEDIUM | L | LOW | CI lint check via grep enforces env var only (CTRL-CRED-001 reflection) |
| R-003 | Client verify opt-out documented sem warning (developer mistake) | M | M (PR review) | LOW | M | LOW | PR review reviewer Docs lead checks; CTRL-CAS-002 reflection per R-S18-6 |

## 27. Knowledge Transfer

- Tech talk (45min): "SDK guides discipline — Python async/await + Go context + JS Promise + CLI 7 subcommands".
- Doc `docs/internal/s18-sdk-guides.md` — SDK guide procedure.
- Onboarding test (3 questions): client verify default-on rationale (CTRL-CAS-002) + PAT format placeholder rationale (CTRL-CRED-001) + Diátaxis SDK guides → Reference categorization.

## 28. Sign-off (LOW_RISK 3 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Docs lead | _TBD; emphatic — 4 SDK guides + per-language idiomatic API + client verify default-on + zero customer data fixture pipeline_ | _pending_ | _pending_ |

> Cross-functional review gate (Finance/Legal/Privacy/Security) NÃO aplicável em WI-S18-003 (content WI; SDK guides Python + Go + JS + CLI; sensitive em WI-S18-004 compliance/security/pricing pages onde cross-functional gate canonical mandatory).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S18-003 (cycle 12.S18.0; LOW_RISK lane; SDK guides Python pyO3 + Go cgo + JS/TS WASM + CLI per command reference; client verify default-on per CTRL-CAS-002 documented; zero customer data fixture pipeline). |

## 30. Anti-patterns evitados

- Skip 4 SDK guides (Python + Go + JS + CLI canonical per spec contract §5.3 R-S18-5).
- Skip client verify default-on documentation (CTRL-CAS-002 reflection per R-S18-6).
- Real customer data em SDK examples (CTRL-PRIV-001 violation; fixture pipeline + CI lint check).
- PAT em CLI args em examples (CTRL-CRED-001 violation; env var only).
- Skip per-language idiomatic API patterns (Python async/await; Go context; JS Promise).

---

**Fim WI-S18-003.**
