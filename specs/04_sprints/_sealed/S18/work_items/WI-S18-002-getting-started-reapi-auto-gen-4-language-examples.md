---
id: "WI-S18-002"
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
tags: ["wi", "s18", "docs", "getting-started", "quickstart", "reapi", "protoc-gen-doc", "auto-gen", "code-examples", "low-risk"]
---

# WI-S18-002 — Getting Started 5-min Quickstart Bazel + Buck2 + Native CLI (per Spec Contract §5.1 R-S18-1; UX Research 5 Dev Sample Completes ≤ 5 min em WI-S18-005) + REAPI v2 Reference Auto-gen via `protoc-gen-doc` (de `.proto` Files; gRPC Services + Messages + REST Endpoints Worker Handlers; CI Gate **Zero Drift between `.proto` + Rendered Docs** per CTRL-DOC-AUTO-GEN canonical Quality Standard 14.s18.8 — Auto-gen Drift Prevention) + Manual Code Examples per Endpoint em 4 Languages Rust + Python + Go + JS (per Spec Contract §5.2 R-S18-4) + PAT Format Examples Placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 Decision (a) `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` — Never Real PAT em Examples; Never Confused Format)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** LOW_RISK
> **Parent:** [S-18](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S18-002 |
| Título | Getting started 5-min quickstart Bazel/Buck2/Native + REAPI v2 reference auto-gen via `protoc-gen-doc` + manual code examples 4 languages. |
| Sprint | S-18 |
| Lane | LOW_RISK |
| Forcing factors | none (LOW_RISK; auto-gen REAPI reference reuses single source of truth `.proto` files from S-04; CI gate zero drift; 4-language code examples manual maintenance scope) |

## 1. Intent

Entrega o **getting started 5-min quickstart** Bazel + Buck2 + Native CLI (per spec contract §5.1 R-S18-1; UX research 5 dev sample completes ≤ 5 min measured em WI-S18-005 closing ship gate; reuses `examples/bazel-starter/` + `examples/buck2-starter/` from S-15 hard blocker SEALED) + **REAPI v2 reference auto-gerada** de `.proto` files via `protoc-gen-doc` (gRPC services + messages; REST endpoints Worker handlers per spec contract §5.2 R-S18-3) + **CI gate zero drift** between `.proto` + rendered docs (CTRL-DOC-AUTO-GEN canonical per Quality Standard 14.s18.8 — auto-gen drift prevention; non-skippable per Waiver policy §19) + manual code examples per endpoint em 4 languages (Rust + Python + Go + JS per spec contract §5.2 R-S18-4) + PAT format examples placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a) PAT format `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` — never real PAT em examples; never confused format).

```yaml
# File: .github/workflows/docs-reapi-drift-check.yml
name: REAPI Auto-gen Drift Check (CTRL-DOC-AUTO-GEN)
on: [pull_request, push]
jobs:
  reapi-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install protoc-gen-doc
        run: go install github.com/pseudomuto/protoc-gen-doc/cmd/protoc-gen-doc@latest
      - name: Generate REAPI reference from .proto
        run: |
          protoc --doc_out=apps/docs/docs/reference/api/_generated \
                 --doc_opt=markdown,reapi.md \
                 -I=protos/ protos/reapi/v2/*.proto
      - name: Verify zero drift (CTRL-DOC-AUTO-GEN canonical)
        run: |
          git diff --exit-code apps/docs/docs/reference/api/_generated/reapi.md || \
            (echo "REAPI reference drift detected; .proto changed but rendered docs not regenerated. Run: protoc --doc_out=... and commit." && exit 1)
```

```markdown
<!-- File: apps/docs/docs/getting-started.mdx (placeholder) -->
# Getting Started in 5 Minutes

Choose your build system:

- [Bazel](#bazel) — `examples/bazel-starter/` (S-15 reuse)
- [Buck2](#buck2) — `examples/buck2-starter/` (S-15 reuse)
- [Native CLI](#native-cli) — `corelink` CLI

## Bazel
```bash
# 1. Set PAT (PAT format: corelink_<env>_<token_id>.<random_secret>.<hmac_sig> per S-03)
export CORELINK_PAT="corelink_dev_t_xxx.xxx.xxx"  # placeholder; obtain real PAT via https://app.corelink.humangr.com/tokens

# 2. Clone starter
git clone https://github.com/humangr-labs/corelink-server.git
cd corelink-server/examples/bazel-starter

# 3. Build with cache
bazel build //:hello

# 4. Confirm cache hit (second invocation)
bazel build //:hello  # → cache hit logged
```
```

## 2. Narrative

`protoc-gen-doc` é canonical reference em REAPI auto-gen (Linear API Docs auto-gen excellence parity; per spec contract §17 `protoc-gen-doc` <https://github.com/pseudomuto/protoc-gen-doc>). CI gate zero drift between `.proto` + rendered docs garante drift impossible by construction (vs manual maintenance drift risk inevitable). Quality Standard 14.s18.8 canonical: auto-gen drift prevention non-skippable per Waiver policy §19.

Manual code examples per endpoint em 4 languages (Rust + Python + Go + JS per spec contract §5.2 R-S18-4) cobrem use cases comuns: PAT auth flow + cache write + cache read + client verify default-on per CTRL-CAS-002 reflection (S-15 reuse). PAT format examples placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a) — never real PAT em examples; never confused format).

Getting started 5-min quickstart 3 build systems (Bazel + Buck2 + Native CLI per spec contract §5.1 R-S18-1) reuses `examples/bazel-starter/` + `examples/buck2-starter/` from S-15 hard blocker SEALED. UX research 5 dev sample completes ≤ 5 min measured em WI-S18-005 closing ship gate (per spec contract §6 EVT-018; 14.s18.2 Diátaxis discoverability test 5 dev sample finds answer ≤ 30s).

**Risk justification LOW_RISK lane (zero forcing factors)**:
- Auto-gen REAPI reference reuses single source of truth (`.proto` files from S-04); CI gate zero drift.
- 4-language code examples manual maintenance scope (Rust + Python + Go + JS); PR review checks examples consistency.
- PAT format examples placeholder (never real PAT em examples; CTRL-CRED-001 reflection).
- Não introduz tenant data flow path novo.
- Não há cripto-load-bearing controles novos.

## 3. Customer Impact & Journey

**Persona — External Developer / Build Engineer (prospect)**:
- Getting started 5-min quickstart 3 build systems (Bazel + Buck2 + Native CLI); UX research 5 dev sample completes ≤ 5 min.
- REAPI v2 reference fresh from `.proto` files (auto-gen drift impossible by construction).
- 4-language code examples (Rust + Python + Go + JS) per endpoint; copy-paste ready.
- PAT format examples placeholder (never real PAT; never confused format).
- Diferenciador competitivo vs NativeLink/BuildBuddy: auto-gen REAPI reference (Linear API Docs parity; vantagem competitive — competitors manual maintenance drift risk).

## 4. Capability Mapping

- **CAP-DOCS-001** (getting started 5-min quickstart) — IMPLEMENTA primary.
- **CAP-DOCS-002** (REAPI v2 reference auto-gen) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4` + `remote_cache_product_profile.md` (REAPI v2 API semantics) + `auth_model.md` (PAT format S-03 decision (a)).

## 5. Tipo

Content WI; LOW_RISK lane.

## 6. Escopo

### 6.1 In-scope

1. **Getting started 5-min quickstart** 3 build systems em `apps/docs/docs/getting-started.mdx`:
   - Bazel section: `examples/bazel-starter/` reuse (S-15 hard blocker SEALED); `bazel build //:hello` confirms cache hit em second invocation.
   - Buck2 section: `examples/buck2-starter/` reuse (S-15 hard blocker SEALED); análogo Bazel.
   - Native CLI section: `corelink` CLI 7 subcommands ls/get/put/stat/bench/doctor/version reference (per spec contract §5.1 R-S18-1).
   - PAT format examples placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a); never real PAT).
   - Categorized em **Tutorials** (Diátaxis learning-oriented; per WI-S18-001 sidebar).

2. **REAPI v2 reference auto-gen** via `protoc-gen-doc`:
   - Source: `.proto` files em `protos/reapi/v2/*.proto` (S-04 SEALED).
   - Tool: `protoc-gen-doc` <https://github.com/pseudomuto/protoc-gen-doc> (canonical per spec contract §17).
   - Output: `apps/docs/docs/reference/api/_generated/reapi.md` (auto-gen markdown).
   - Categorized em **Reference** (Diátaxis information-oriented; per WI-S18-001 sidebar).
   - Coverage: gRPC services + messages + REST endpoints Worker handlers (per spec contract §5.2 R-S18-3).

3. **CI gate zero drift** between `.proto` + rendered docs (CTRL-DOC-AUTO-GEN canonical):
   - GitHub Actions workflow `.github/workflows/docs-reapi-drift-check.yml` runs on PR + push.
   - `protoc --doc_out=...` regenerates reference markdown.
   - `git diff --exit-code` fails se drift detected.
   - Quality Standard 14.s18.8 canonical: auto-gen drift prevention non-skippable per Waiver policy §19.

4. **Manual code examples** per endpoint em 4 languages:
   - **Rust**: `corelink-rust` SDK (Cargo crate; future-proof scaffolded).
   - **Python**: `corelink-py` PyPI (S-15 SEALED reuse).
   - **Go**: `corelink-go` pkg.go.dev (S-15 SEALED reuse).
   - **JS/TS**: `@corelink/client` npm (S-15 SEALED reuse).
   - Examples cover: PAT auth flow + cache write + cache read + client verify default-on per CTRL-CAS-002 reflection (S-15 reuse).
   - Categorized em **Reference** (Diátaxis information-oriented).

5. **PAT format examples placeholder**:
   - Format `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` (S-03 decision (a)).
   - Placeholder: `corelink_dev_t_xxx.xxx.xxx`.
   - Never real PAT em examples (CTRL-CRED-001 reflection; CI lint check via grep enforces).
   - PAT obtained via https://app.corelink.humangr.com/tokens (S-13 admin plane reuse).

6. **Categorization Diátaxis discipline**:
   - Getting started → **Tutorials** (learning-oriented).
   - REAPI v2 reference + 4-language code examples → **Reference** (information-oriented).
   - PR review checks Diátaxis taxonomy fit per Quality Standard 14.s18.2.

### 6.2 Out-of-scope (deferred)

- SDK guides per-language production-ready (WI-S18-003).
- Compliance + security + pricing pages (WI-S18-004).
- WCAG 2.2 AA axe-core CI gate (WI-S18-005).
- Lighthouse ≥ 95 CI gate (WI-S18-005).
- Vale tone consistency lint CI (WI-S18-005).
- lychee broken-link CI (WI-S18-005).
- UX research session 5 dev sample (WI-S18-005).
- i18n native speaker review (WI-S18-005).

## 7. Anti-Scope

- Skip CI gate zero drift between `.proto` + rendered docs (CRITICAL gap; CTRL-DOC-AUTO-GEN non-skippable).
- Manual REAPI reference maintenance (drift risk inevitable; use `protoc-gen-doc` auto-gen).
- Real PAT em examples (CTRL-CRED-001 violation; CI lint check enforces placeholder).
- Skip 4-language code examples (Rust + Python + Go + JS canonical per spec contract §5.2 R-S18-4).
- Skip getting started 5-min quickstart 3 build systems (CAP-DOCS-001 primary deliverable).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Getting started 5-min quickstart + REAPI v2 reference auto-gen + 4-language code examples

  Scenario: Getting started 5-min quickstart 3 build systems
    Given apps/docs/docs/getting-started.mdx published
    When user follows quickstart Bazel section
    Then user clones examples/bazel-starter/
    And bazel build //:hello succeeds first invocation
    And second invocation confirms cache hit
    And UX research 5 dev sample completes ≤ 5 min (WI-S18-005)

  Scenario: REAPI v2 reference auto-gen via protoc-gen-doc
    Given protos/reapi/v2/*.proto files
    When protoc --doc_out=apps/docs/docs/reference/api/_generated runs
    Then reapi.md auto-gen markdown produced
    And gRPC services + messages + REST endpoints covered

  Scenario: CI gate zero drift between .proto + rendered docs (CTRL-DOC-AUTO-GEN)
    Given .proto file modified em PR
    When CI runs docs-reapi-drift-check.yml
    Then protoc regenerates reference markdown
    And git diff --exit-code fails se drift detected
    And PR fails until rendered docs regenerated + committed

  Scenario: Manual code examples per endpoint em 4 languages
    Given REAPI v2 endpoint X
    When apps/docs/docs/reference/api/X.mdx
    Then 4 code blocks present (Rust + Python + Go + JS)
    And examples cover PAT auth flow + cache write + cache read + client verify default-on

  Scenario: PAT format examples placeholder (S-03 decision (a))
    Given any code example em docs
    When PAT used
    Then placeholder format corelink_dev_t_xxx.xxx.xxx
    And CI lint check via grep enforces placeholder (never real PAT)

  Scenario: Diátaxis taxonomy discipline
    Given new docs page added (getting started OR reference)
    When PR submitted
    Then PR template enforces category (tutorial OR reference)
    And reviewer Docs lead checks taxonomy fit

  Scenario: REAPI auto-gen drift detected (waiver attempt)
    Given .proto modified + rendered docs not regenerated
    When developer attempts merge
    Then CI gate fails
    And waiver attempt rejected (non-skippable per Waiver policy §19)
```

## 9. Design Decisions

### 9.1 Why protoc-gen-doc (não OpenAPI Generator / Swagger / manual)

- `protoc-gen-doc` canonical reference em REAPI auto-gen (Linear API Docs parity; per spec contract §17).
- REAPI v2 protocol é gRPC-based (Bazel Remote Execution API); `protoc-gen-doc` native protoc plugin.
- OpenAPI Generator REST-first (REAPI is gRPC + REST hybrid; less idiomatic).
- Swagger Codegen overlapping (use `protoc-gen-doc` para protos).
- Manual maintenance drift risk inevitable (rejected).

### 9.2 Why CI gate zero drift (CTRL-DOC-AUTO-GEN canonical non-skippable)

- Quality Standard 14.s18.8 canonical: auto-gen drift prevention.
- Drift impossible by construction = no manual maintenance burden + no stale docs risk.
- Stripe + Linear parity em auto-gen rigor.
- Waiver policy §19 non-skippable.

### 9.3 Why 4 languages (Rust + Python + Go + JS) e não mais

- Per spec contract §5.2 R-S18-4 canonical (Rust + Python + Go + JS).
- Coverage 80%+ build engineering ecosystem (Bazel/Buck2 = primary Go/Java/C++/Rust; SDKs = Python/Go/JS).
- Adicional languages (Java/Kotlin/Swift/Ruby) pós-GA Q1.

### 9.4 Why PAT format placeholder canonical (não [REDACTED] / xxx)

- S-03 decision (a) PAT format `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`.
- Placeholder `corelink_dev_t_xxx.xxx.xxx` preserves format (educational; never confused).
- CI lint check via grep enforces placeholder (never real PAT em examples).
- CTRL-CRED-001 reflection (no secrets em CLI output / docs).

### 9.5 Why getting started Bazel + Buck2 + Native CLI (não outros build systems)

- Per spec contract §5.1 R-S18-1 canonical.
- Bazel + Buck2 = primary CoreLink target (REAPI v2 protocol).
- Native CLI = power users + CI scripting use case.
- Adicional build systems (Pants/Please/Pulumi build) pós-GA Q1.

### 9.6 ADR potencial?

- Não — `protoc-gen-doc` + CI gate zero drift são canonical references em S-18 spec contract §17 + §9.8 (Quality Standard 14.s18.8); CTRL-DOC-AUTO-GEN canonical introduced em S-18 (no novel decision; ADR não necessário; documented em sprint.md §12 Controls).

## 10. Completeness Criteria

- [ ] **10.s18.002.1** Getting started 5-min quickstart 3 build systems (Bazel + Buck2 + Native CLI) published em `apps/docs/docs/getting-started.mdx`.
- [ ] **10.s18.002.2** REAPI v2 reference auto-gen via `protoc-gen-doc` em `apps/docs/docs/reference/api/_generated/reapi.md`.
- [ ] **10.s18.002.3** CI gate zero drift between `.proto` + rendered docs (`.github/workflows/docs-reapi-drift-check.yml`; CTRL-DOC-AUTO-GEN canonical) (per Completeness Criteria 10.s18.6).
- [ ] **10.s18.002.4** Manual code examples per endpoint em 4 languages (Rust + Python + Go + JS).
- [ ] **10.s18.002.5** PAT format examples placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a); CI lint check enforces).
- [ ] **10.s18.002.6** Diátaxis taxonomy discipline (getting started → Tutorials; REAPI reference + code examples → Reference).
- [ ] **10.s18.002.7** UX research 5 dev sample completes ≤ 5 min (deferred WI-S18-005 closing ship gate).

## 11. DoD

- [ ] Getting started 5-min quickstart published Bazel + Buck2 + Native CLI.
- [ ] REAPI v2 reference auto-gen via `protoc-gen-doc` working.
- [ ] CI gate zero drift between `.proto` + rendered docs (CTRL-DOC-AUTO-GEN canonical).
- [ ] 4-language code examples per endpoint.
- [ ] PAT format examples placeholder + CI lint check.
- [ ] Diátaxis taxonomy discipline.
- [ ] Adversarial scenarios 3+ documented.

## 12. Invariants Validated

- **CTRL-DOC-AUTO-GEN** (auto-gen drift prevention CI gate) — INTRODUZ em S-18 cumulative; REAPI reference auto-gen via `protoc-gen-doc` + CI gate zero drift between `.proto` + rendered docs (Quality Standard 14.s18.8).
- **CTRL-CRED-001** (no secrets em CLI output / docs) — IMPLEMENTA reflection em PAT format placeholder (CI lint check via grep enforces).
- **Não introduz INVs novas** (sprint consumer; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Getting started quickstart | `apps/docs/docs/getting-started.mdx` | MDX |
| REAPI v2 reference auto-gen | `apps/docs/docs/reference/api/_generated/reapi.md` | Markdown (auto-gen) |
| CI gate zero drift workflow | `.github/workflows/docs-reapi-drift-check.yml` | YAML |
| 4-language code examples | `apps/docs/docs/reference/api/<endpoint>.mdx` × N | MDX |
| PAT format placeholder lint check | `.github/workflows/docs-pat-format-lint.yml` (grep canonical) | YAML |

## 14. Quality Standards

- **14.s18.002.1** Auto-gen drift prevention CI gate (CTRL-DOC-AUTO-GEN canonical per Quality Standard 14.s18.8; non-skippable per Waiver policy §19).
- **14.s18.002.2** PAT format examples placeholder enforced via CI lint check (CTRL-CRED-001 reflection).
- **14.s18.002.3** Diátaxis taxonomy discipline (getting started → Tutorials; reference → Reference; PR review checks).
- **14.s18.002.4** Cost regression gate: zero adicional cost (`protoc-gen-doc` OSS; GitHub Actions free tier).

## 15. Test Plan

### Unit tests
- `protoc-gen-doc` invocation succeeds (CI runs).
- Generated `reapi.md` non-empty + valid markdown.
- CI gate zero drift fails se `.proto` modified + rendered docs not regenerated.
- PAT format CI lint check fails se real PAT detected (grep `corelink_(dev|prod|stg)_t_[a-z0-9]+\.` excluding placeholder pattern).

### Integration tests
- Getting started 5-min quickstart Bazel section: clone + build + cache hit second invocation.
- Getting started 5-min quickstart Buck2 section: análogo.
- Getting started 5-min quickstart Native CLI section: `corelink put/get/stat` cycle.
- REAPI v2 reference auto-gen rendered em CF Pages preview.
- 4-language code examples render syntax-highlighted.

### Adversarial scenarios (3+)
1. `.proto` modified em PR; rendered docs not regenerated → CI gate fails (CTRL-DOC-AUTO-GEN).
2. Real PAT em examples inadvertent (developer mistake) → CI lint check fails (grep enforces placeholder).
3. 4-language code example out of sync with REAPI v2 contract (manual drift) → PR review reviewer Docs lead catches; quarterly review cadence reinforces.

## 16. Failure Modes

- **FM-DOC-AUTO-GEN-DRIFT** (REAPI reference drift): CI gate zero drift between `.proto` + rendered docs (CTRL-DOC-AUTO-GEN canonical mitigation).
- **FM-DOC-PAT-LEAK** (real PAT em examples inadvertent): CI lint check via grep enforces placeholder (CTRL-CRED-001 reflection mitigation).

## 17. Controls

- **CTRL-DOC-AUTO-GEN** (auto-gen drift prevention CI gate) — INTRODUZ primary em S-18 cumulative.
- **CTRL-CRED-001** (no secrets em CLI output / docs) — IMPLEMENTA reflection em PAT format placeholder.
- **CTRL-PRIV-001** (zero PII em screenshots/examples) — fixture pipeline reuse em WI-S18-004.

## 18. Resilience Patterns

- N/A (docs sprint consumer; non-cripto-load-bearing).

## 19. Observability

- CI gate zero drift status (PR fails se drift detected).
- PAT format lint CI status (PR fails se real PAT detected).

Não introduz métricas Prometheus per-tenant em content WI (per observability_model §3.1; sprint consumer).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT format examples placeholder (never real PAT).
- **Tampering**: docs source git-tracked + reviewer Docs lead; auto-gen REAPI reference fresh from `.proto` (drift impossible by construction).
- **Repudiation**: GitHub Actions CI gate log + git commit history.
- **Information disclosure**: PAT format placeholder enforced via CI lint check (CTRL-CRED-001).
- **DoS**: nenhuma (docs content WI).
- **Elevation of privilege**: docs publish requires PR review + Docs lead sign-off + CI gate zero drift pass.

**LINDDUN delta**:
- Linkability: nenhuma (docs content WI; no per-tenant identifiers em examples).
- Identifiability: PAT format placeholder (never real PAT em examples; never confused format).
- Non-repudiation: GitHub Actions CI gate log.
- Detectability: CI gate zero drift detection; PAT format lint detection.
- Disclosure: CTRL-CRED-001 reflection (no secrets em docs).

## 21. Dependencies

### Hard blockers
- **WI-S18-001 SEALED** (Docusaurus 3.x foundation + Diátaxis sidebar + i18n config).
- **S-04 SEALED** (`.proto` files em `protos/reapi/v2/*.proto` for `protoc-gen-doc` auto-gen).
- **S-15 SEALED** (`examples/bazel-starter/` + `examples/buck2-starter/` for getting started reuse).

### Soft blockers
- **S-03 SEALED** (PAT format `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` decision (a)).

### Outbound
- WI-S18-003 (SDK guides per-language consumes 4-language code examples + REAPI reference).
- WI-S18-004 (compliance + security + pricing pages cross-functional gate).
- WI-S18-005 (closing ship gate UX research 5 dev completes ≤ 5 min).

## 22. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; getting started + REAPI auto-gen + CI gate + 4-language examples).

## 23. Cost Analysis

**Direct cost**:
- `protoc-gen-doc` OSS: $0/mês.
- GitHub Actions free tier: $0/mês (CI gate runs).
- Algolia DocSearch free tier OSS (WI-S18-001 reuse): $0/mês.

**Total**: ~$0/mês adicional.

**Indirect cost**: 0 docs drift (CI gate enforced) + 0 manual maintenance burden + dev adoption baseline = priceless.

## 24. Post-mortem Hooks

- Auto-gen REAPI reference drift detected em prod → post-mortem + CI gate reinforce.
- Real PAT em examples merged inadvertent → CRITICAL post-mortem (security; rotate PAT + audit log).
- Getting started 5-min quickstart > 5 min UX research session → post-mortem + IA review.

## 25. Rollback / Recovery

Docs rollback: revert PR + redeploy CF Pages previous build. RTO ≤ 5min. CI gate zero drift retroactive enforcement via `protoc --doc_out=...` regenerate + commit.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | REAPI reference drift detected em prod | M | H (CI gate) | LOW | M | LOW | CI gate zero drift between `.proto` + rendered docs (CTRL-DOC-AUTO-GEN canonical; non-skippable per Waiver policy §19) |
| R-002 | Real PAT em examples inadvertent (developer mistake) | L | H (CI lint) | MEDIUM | L | LOW | CI lint check via grep enforces placeholder; never real PAT em examples (CTRL-CRED-001 reflection) |
| R-003 | 4-language code example out of sync (manual drift) | M | M (PR review) | LOW | M | LOW | PR review reviewer Docs lead checks; quarterly review cadence |

## 27. Knowledge Transfer

- Tech talk (45min): "REAPI auto-gen via protoc-gen-doc + CI gate zero drift + 4-language examples discipline".
- Doc `docs/internal/s18-reapi-auto-gen.md` — auto-gen procedure.
- Onboarding test (3 questions): CTRL-DOC-AUTO-GEN canonical + PAT format placeholder rationale + Diátaxis getting started/reference categorization.

## 28. Sign-off (LOW_RISK 3 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Docs lead | _TBD; emphatic — REAPI auto-gen drift prevention + 4-language examples + PAT format placeholder_ | _pending_ | _pending_ |

> Cross-functional review gate (Finance/Legal/Privacy/Security) NÃO aplicável em WI-S18-002 (content WI; getting started + REAPI reference + code examples; sensitive em WI-S18-004 compliance/security/pricing pages onde cross-functional gate canonical mandatory).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S18-002 (cycle 12.S18.0; LOW_RISK lane; getting started 5-min quickstart + REAPI v2 reference auto-gen via `protoc-gen-doc` + CI gate zero drift CTRL-DOC-AUTO-GEN + 4-language code examples + PAT format placeholder S-03 decision (a)). |

## 30. Anti-patterns evitados

- Manual REAPI reference maintenance (drift risk inevitable; use `protoc-gen-doc` auto-gen).
- Skip CI gate zero drift (CTRL-DOC-AUTO-GEN canonical non-skippable per Waiver policy §19).
- Real PAT em examples (CTRL-CRED-001 violation; CI lint check enforces placeholder).
- Skip 4-language code examples (Rust + Python + Go + JS canonical per spec contract §5.2 R-S18-4).
- Skip Diátaxis taxonomy discipline (PR review checks; reviewer Docs lead).

---

**Fim WI-S18-002.**
