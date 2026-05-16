---
id: "WI-S15-002"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
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
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "AUTH-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s15", "bazel", "starter-project", "reapi", "ci-test", "dx", "standard"]
---

# WI-S15-002 — Bazel Integration Starter Project `examples/bazel-starter/` (Real `WORKSPACE` + `BUILD.bazel` + `.bazelrc` Reference com `--remote_cache=https://corelink.humangr.com/v1/cache --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh` — Credential Helper Protocol Bazel 6+ Pattern; PAT NUNCA em argv per CTRL-CRED-001 Lote 10.15 codex P0 fix) + README ≤ 5 min Setup + GitHub Actions CI Integration Test (clone → `bazel build //:hello` → Confirma Cache Hit em Subsequent Invocation via Bazel Log Inspection) + Benchmark Com/Sem Cache + `docs/integrations/bazel.md` User Guide + REAPI v2 Adherence

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-15](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S15-002 |
| Título | Real Bazel starter project em `examples/bazel-starter/` com `WORKSPACE` + `BUILD.bazel` + `.bazelrc` reference (`--remote_cache=https://corelink.humangr.com/v1/cache --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh` — credential helper Bazel 6+ pattern per <https://bazel.build/docs/credential-helper>; PAT NUNCA em argv per CTRL-CRED-001 Lote 10.15 codex P0 canonical fix; helper script reads `CORELINK_PAT` env var + emits Bazel JSON response) + `README.md` step-by-step setup ≤ 5 min + GitHub Actions CI integration test (clone → `bazel build //:hello` → segunda invocation confirma cache hit via Bazel `--noremote_upload_local_results` log inspection); benchmark sample build com / sem cache + report; fixture project não-trivial (hello-world inline + 1 transitive dep para validar cross-target dedup); `docs/integrations/bazel.md` user guide publicado; REAPI v2 specification adherence verified. |
| Sprint | S-15 |
| Lane | STANDARD |
| Forcing factors | none |

## 1. Intent

Bazel é o build system primary target para CoreLink (REAPI v2 client). Per BuildBuddy/NativeLink benchmark, starter project deve atingir cache hit ≤ 5 min from clone — esse é o **time-to-first-cache-hit** SLA do S-15 (DoD §6 + completeness 10.s15.6). Este WI entrega real Bazel starter project testado em CI continuous (vs competitors NativeLink/BuildBuddy que têm Bazel docs sem starter projects testados — diferenciador SOTA).

```bash
# Time-to-first-cache-hit ≤ 5 min target:
git clone https://github.com/corelink-dev/examples
cd examples/bazel-starter
export CORELINK_PAT=corelink_prod_...
bazel build //:hello                    # cold cache: ~2 min
bazel clean
bazel build //:hello                    # cache hit: ~10s (95% reduction)
```

```bazel
# File: examples/bazel-starter/WORKSPACE
workspace(name = "corelink_bazel_starter")

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# Minimal toolchain (rules_cc default + 1 transitive dep for dedup validation)
http_archive(
    name = "rules_cc",
    sha256 = "abc123...",
    urls = ["https://github.com/bazelbuild/rules_cc/releases/download/0.0.9/rules_cc-0.0.9.tar.gz"],
)
```

```bazel
# File: examples/bazel-starter/BUILD.bazel
cc_library(
    name = "greeter",
    srcs = ["greeter.cc"],
    hdrs = ["greeter.h"],
)

cc_binary(
    name = "hello",
    srcs = ["main.cc"],
    deps = [":greeter"],
)
```

```ini
# File: examples/bazel-starter/.bazelrc
build --remote_cache=https://corelink.humangr.com/v1/cache
build --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
# Lote 10.15 codex P0 canonical fix CTRL-CRED-001: credential helper protocol Bazel 6+ — helper reads CORELINK_PAT env var + emits JSON token; PAT nunca em argv (vs --remote_header= que shell-expand into argv = ps aux leak)
build --remote_timeout=30s
build --remote_upload_local_results=true
build --remote_download_minimal
build --experimental_remote_cache_compression=true
```

## 2. Narrative

CoreLink target audience é Bazel users em monorepos (Stripe, Pinterest, Snap, etc. all on Bazel). Per BuildBuddy benchmark, time-to-first-cache-hit ≤ 5 min é onde dev tools win/lose conversion. NativeLink/BuildBuddy têm Bazel docs sem starter projects testados em CI continuous — drift inevitable; CoreLink S-15 entrega real fixture project com CI integration test verde 7d sustained.

**Risk justification STANDARD lane**:
- Bazel REAPI v2 adherence well-known (zero novel protocol surface).
- `examples/bazel-starter` é fixture project; não toca tenant data flow.
- CI integration test verifies cache hit em CoreLink staging cluster — failure detected pre-merge.

## 3. Customer Impact & Journey

**Persona — Build Engineer adopting CoreLink**:
- Clone `examples/bazel-starter` → 5-min setup → first cache hit em CI.
- Copy `.bazelrc` reference para production project; copy `corelink-credential-helper.sh` script; helper reads `CORELINK_PAT` env var (NÃO shell-expand into argv per CTRL-CRED-001 Lote 10.15 codex P0).
- Diferenciador: starter project testado em CI continuous (NativeLink/BuildBuddy Bazel docs não testados).

## 4. Capability Mapping

- **CAP-SDK-001** (Bazel starter project + docs) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §5.2 (R-S15-6 + R-S15-8)` + REAPI v2 specification.

## 5. Tipo

Feature WI; STANDARD lane; integration starter project.

## 6. Escopo

### 6.1 In-scope

1. **`examples/bazel-starter/`** real project layout:
   - `WORKSPACE` (Bazel 7.x compatible com Bzlmod opt-out per default; documented opt-in).
   - `BUILD.bazel` (cc_library + cc_binary minimal hello-world + 1 transitive dep para validar cross-target dedup).
   - `greeter.cc` + `greeter.h` + `main.cc` (10-line C++ hello-world).
   - `.bazelrc` reference (vide §1; `--remote_cache` + `--remote_header` + flags otimizados).
   - `README.md` step-by-step ≤ 5 min setup (clone → export CORELINK_PAT → bazel build → verify cache hit).
   - `.gitignore` Bazel-specific (`bazel-*` symlinks).

2. **GitHub Actions CI integration test**:
   - Workflow `.github/workflows/bazel-starter-ci.yml`.
   - Steps:
     1. Checkout `examples/bazel-starter`.
     2. Setup Bazel 7.x via `bazelbuild/setup-bazelisk`.
     3. Set `CORELINK_PAT` from GitHub secret.
     4. First build: `bazel build //:hello` (cold cache; baseline duration captured).
     5. `bazel clean --expunge`.
     6. Second build: `bazel build //:hello` (warm cache; expected ≤ 30s for hello-world; cache hit confirmed via log parse).
     7. Verify cache hit via `bazel build //:hello --execution_log_json_file=/tmp/log.json` parse + assert ≥ 80% remote cache hits.
   - Cron: weekly + on-PR for `examples/bazel-starter/**` paths.
   - Failure = SEV-3 alert + blocking PR merge.

3. **Benchmark sample build com/sem cache**:
   - Script `examples/bazel-starter/scripts/benchmark.sh`:
     - 10 iterations cold + 10 iterations warm.
     - Output table: median + p95 latency + cache hit ratio.
   - Reported em `examples/bazel-starter/BENCHMARK.md` weekly automated update.

4. **`docs/integrations/bazel.md` user guide**:
   - Architecture overview (REAPI v2 client → CoreLink CAS).
   - `.bazelrc` flags reference detalhado.
   - Troubleshooting common issues (auth fail, network unreachable, cache miss debugging).
   - Performance tips (`--remote_download_minimal` + compression).
   - Migration guide from BuildBuddy/NativeLink.

5. **REAPI v2 adherence**:
   - Verify CoreLink CAS endpoints implementam `bazel-remote-execution-api` v2 contract.
   - Test against real Bazel client em CI (regression detection).

### 6.2 Out-of-scope

- Buck2 starter (WI-S15-003).
- Auto-generation tool (`corelink init bazel`) — pós-GA.
- Bazel migration tool from local cache to remote — pós-GA.
- Bzlmod opt-in default (WORKSPACE legacy é GA baseline; Bzlmod documented opt-in).

## 7. Anti-Scope

- Skip CI integration test (mandatory; differentiator vs competitors).
- README > 10 min setup (target ≤ 5 min).
- Hard-code PAT em `.bazelrc` example (canonical: credential helper protocol Bazel 6+ — helper reads `CORELINK_PAT` env var; PAT nunca em argv per CTRL-CRED-001).
- Skip benchmark com/sem cache (DX validation evidence).
- REAPI v2 fork (rejected per anti-scope §2.2 spec contract).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Bazel starter project + CI integration test

  Scenario: examples/bazel-starter clone-to-cache-hit ≤ 5 min
    Given fresh clone of examples/bazel-starter
    Given CORELINK_PAT env var set
    When user follows README step-by-step
    Then first bazel build completes
    And second bazel build (post-clean) completes em ≤ 30s (cache hit)
    And total elapsed setup time ≤ 5 min

  Scenario: GitHub Actions CI integration test verde
    Given workflow .github/workflows/bazel-starter-ci.yml
    When workflow runs (cron weekly + on-PR)
    Then bazel build //:hello succeeds
    And cache hit ≥ 80% verified via execution_log_json_file parse
    And artifact uploaded with build duration metrics

  Scenario: .bazelrc reference correto
    Given .bazelrc in examples/bazel-starter
    Then contains --remote_cache=https://corelink.humangr.com/v1/cache
    And contains --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
    And NOT contains hardcoded PAT value

  Scenario: Benchmark report generated
    Given examples/bazel-starter/scripts/benchmark.sh
    When 10 cold + 10 warm iterations run
    Then BENCHMARK.md updated com median + p95 + cache hit ratio
    And cache hit ratio ≥ 80%

  Scenario: REAPI v2 adherence verified
    Given Bazel 7.x client + CoreLink staging endpoint
    When `bazel build //:hello --remote_cache=https://staging.corelink.humangr.com/v1/cache`
    Then build succeeds
    And REAPI v2 protocol compliance (FindMissingBlobs + GetActionResult + UpdateActionResult endpoints)

  Scenario: Sustained CI green 7d
    Given weekly cron schedule
    When CI runs every day for 7 days
    Then 7/7 runs verde
    And no flakiness > 5%
```

## 9. Design Decisions

### 9.1 Why hello-world + 1 transitive dep (não trivial single-file)

- Validates cross-target dedup (greeter.o reused entre múltiplos targets).
- Realistic conversion benchmark vs NativeLink/BuildBuddy.

### 9.2 Why WORKSPACE (não Bzlmod default)

- Bazel 7.x WORKSPACE legacy ainda primary GA baseline (Bzlmod transition gradual).
- Bzlmod documented opt-in em `examples/bazel-starter/MODULE.bazel` future addition.

### 9.3 Why GitHub Actions CI (não GitLab/CircleCI primary)

- CI templates 3 providers em WI-S15-005; este WI fica em GitHub Actions canonical (CoreLink hosted).
- WI-S15-005 entrega templates equivalents para customer adoption.

### 9.4 ADR potencial?

- Não. REAPI v2 é well-known; Bazel CI pattern reused em industry.

## 10. Completeness Criteria

- [ ] **10.s15.002.1** `examples/bazel-starter/` real project committed (EVT-018).
- [ ] **10.s15.002.2** GitHub Actions CI integration test verde sustained 7d (EVT-018).
- [ ] **10.s15.002.3** README ≤ 5 min setup verified em dev workshop sample (EVT-018).
- [ ] **10.s15.002.4** Benchmark report com median + p95 + cache hit ratio (EVT-018).
- [ ] **10.s15.002.5** `docs/integrations/bazel.md` user guide publicado.
- [ ] **10.s15.002.6** REAPI v2 adherence verified.
- [ ] **10.s15.002.7** `.bazelrc` reference correct; PAT via env var (CTRL-CRED-001).

## 11. DoD

- [ ] `examples/bazel-starter/` committed; layout per §6.1.
- [ ] GitHub Actions CI verde 7d sustained.
- [ ] README ≤ 5 min setup.
- [ ] BENCHMARK.md updated weekly automated.
- [ ] `docs/integrations/bazel.md` publicado.
- [ ] Tests: 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-CRED-001** reforced via credential helper protocol Bazel 6+ (PAT nunca em argv); helper script reads `CORELINK_PAT` env var + emits JSON token. Lote 10.15 codex P0 canonical fix.
- **INV-CAS-INTEGRITY** (CRITICAL — registry §3.X herdada) reforced via Bazel REAPI client triggers BLAKE3 verify post-download.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Bazel starter project | `examples/bazel-starter/` | Bazel |
| CI integration test workflow | `.github/workflows/bazel-starter-ci.yml` | YAML |
| Benchmark script | `examples/bazel-starter/scripts/benchmark.sh` | Bash |
| Benchmark report | `examples/bazel-starter/BENCHMARK.md` | Markdown |
| User guide | `docs/integrations/bazel.md` | Markdown |
| README starter | `examples/bazel-starter/README.md` | Markdown |

## 14. Quality Standards

- **14.s15.002.1** Docs com ≥ 3 examples per section; copy-paste-runnable.
- **14.s15.002.2** CI integration test sustained 7d; flakiness ≤ 5%.
- **14.s15.002.3** REAPI v2 adherence verified em CI regression detection.
- **14.s15.002.4** Time-to-first-cache-hit ≤ 5 min measured + tracked.

## 15. Test Plan

### Unit tests (não-applicable — fixture project)

### Integration tests (E2E em CI)
- `bazel build //:hello` cold cache → succeeds.
- `bazel build //:hello` warm cache (post-clean) → cache hit ≥ 80%.
- `bazel build //:hello` com invalid PAT → clear error (FM-160).
- `bazel build //:hello` com network unreachable → retry com backoff (FM-150).

### Negative scenarios (≥ 4)
1. **PAT missing**: `unset CORELINK_PAT`; `bazel build` → clear auth error.
2. **PAT invalid**: malformed PAT → 401 + clear next-action.
3. **Cluster unreachable**: bad endpoint URL → retry com backoff + final timeout error.
4. **Quota exceeded**: tenant em hard limit → 429 + clear next-action.
5. **REAPI v2 protocol drift**: synthetic CoreLink endpoint returns invalid response → Bazel client logs error + CI detects regression.

## 16. Failure Modes

- **FM-150** (transient network): Bazel retry built-in; `--remote_timeout=30s` configured.
- **FM-160** (auth invalid): Bazel surfaces 401 from CoreLink; user redirected to `corelink doctor` (#2 Auth check).

## 17. Controls

- **CTRL-CRED-001** enforced via credential helper protocol (PAT nunca em argv; helper isolates token from process listing). Lote 10.15 codex P0 canonical.
- **CTRL-CAS-002** (client verify default-on) — Bazel REAPI client triggers; verified em CI test.

## 18. Resilience Patterns

- Bazel built-in retry com `--remote_timeout=30s` + `--remote_retries=3` (REAPI v2 standard).
- `--remote_download_minimal` + `--experimental_remote_cache_compression=true` para minimizar network usage.

## 19. Observability

CI integration test reports:
- Cache hit ratio per build (parsed from `--execution_log_json_file`).
- Build duration (cold vs warm).
- Cluster latency p50/p99.

Métricas sem cardinality issue (single workflow run; not per-tenant).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT via GitHub secret + env var canonical; never hardcoded em `.bazelrc`.
- **Tampering**: Bazel REAPI client triggers BLAKE3 verify post-download (CTRL-CAS-002).
- **Information disclosure**: README warns explicitly não commit PAT; `.gitignore` includes typical secrets paths.

## 21. Dependencies

### Hard blockers
- WI-S15-001 (CLI binary used em README step-by-step).
- S-01 + S-02 + S-03 + S-04 SEALED.

### Soft blockers
- WI-S15-005 (CI templates 3 providers; este WI é GitHub Actions canonical).

### Outbound
- WI-S15-006 (ship gate references este como completeness 10.s15.2).
- S-19 (customer onboarding uses starter project).

## 22. Effort PERT

O: 10h, M: 14h, P: 22h → PERT **14.7h** (per spec contract §12).

## 23. Cost Analysis

- GitHub Actions CI runs: ~$5/run × 7 runs/week = $35/mês.
- Bazel build cluster CPU: $5/mês.
- Total: ~$40/mês incremental.

## 24. Post-mortem Hooks

- CI integration test red sustained > 24h → SEV-2 (DX regression).
- Time-to-first-cache-hit > 10 min sustained → post-mortem (DX regression).
- REAPI v2 protocol drift detected → SEV-2 + Bazel community channel update.

## 25. Rollback / Recovery

CI flake → retry policy + threshold tuning; persistent fail → revert .bazelrc flag changes; root cause analysis.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Bazel REAPI quirks não cobertos | M | M | MEDIUM | M | LOW | Real CI test + community feedback channels |
| R-002 | CI flaky em GitHub Actions | M | M | LOW | M | LOW | Retry policy; threshold tuning |
| R-003 | Time-to-first-cache-hit > 5 min UX miss | M | M | MEDIUM | M | LOW | Dev workshop weekly + iterate starter project |
| R-004 | Bazel 8.x breaking changes | L | L | MEDIUM | L | LOW | Pin Bazel 7.x; CHANGELOG monitoring |
| R-005 | REAPI v2 spec drift | L | L | LOW | L | LOW | Bazel community channel monitoring |

## 27. Knowledge Transfer

- Tech talk (1h): "Bazel + CoreLink Integration Walkthrough".
- Doc `docs/integrations/bazel.md`.
- Customer-facing migration guide.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Engineer (S-15 lead) | _TBD_ | _pending_ |
| 4 | QA Lead | _TBD_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | DevX advisor | _TBD; emphatic — Bazel UX + REAPI adherence + time-to-first-cache-hit ≤ 5 min_ | _pending_ |
| 7 | Docs lead | _TBD; emphatic — `docs/integrations/bazel.md` user guide + README ≤ 5 min_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S15-002 (cycle 12.S15.0; Bazel starter + CI integration test). |
| 1.1.0 | 2026-05-14 | Gustavo (via Claude Opus 4.7) | SEAL: `examples/bazel-starter/` (WORKSPACE + .bazelrc + BUILD.bazel + credential helper) entregue; merge conflict markers resolvidos no sprint-close round-1 P0 remediation. work_status=DONE; doc_status=SEALED. |

## 30. Anti-patterns evitados

- Skip CI integration test.
- README > 10 min setup.
- Hard-code PAT em `.bazelrc`.
- Skip benchmark com/sem cache.
- REAPI v2 fork.
- Trivial fixture project (single-file no transitive dep — não valida dedup).

---

**Fim WI-S15-002.**
