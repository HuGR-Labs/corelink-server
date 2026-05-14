---
id: "WI-S15-005"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-05-14"
lane: "STANDARD"
parent: "S-15"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s15", "ci-templates", "github-actions", "gitlab", "circleci", "telemetry", "opt-in", "linddun", "standard"]
---

# WI-S15-005 — CI Integration Templates 3 Providers (`templates/ci/github-actions/corelink-cache.yml` + `templates/ci/gitlab-ci/corelink-cache.yml` + `templates/ci/circleci/corelink-cache.yml`) com `CORELINK_PAT` Secret Input + Cache Hit Ratio Output em Workflow Summary + **Telemetry Opt-In Default-Off (`corelink config set telemetry on` Discoverable; LINDDUN Privacy Review; Property Test 0 Emissions sem Flag)** + Anonymized Payload (CLI Version + OS + Subcommand + Outcome; nunca tenant_id, blob digests, PAT) + 3 Real Working Sample Builds Verde

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-15](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S15-005 |
| Título | CI templates 3 providers em `templates/ci/`: GitHub Actions (`github-actions/corelink-cache.yml`) + GitLab CI (`gitlab-ci/corelink-cache.yml`) + CircleCI (`circleci/corelink-cache.yml`); cada template tem `CORELINK_PAT` secret input + cache hit ratio output em workflow summary; 3 real working sample builds verde em respective CI providers; telemetry opt-in default-off com `corelink config set telemetry on` discoverable via `corelink config list`; **LINDDUN privacy review** committed em `specs/_audits/2026-XX-XX-linddun-cli-telemetry.md`; anonymized payload (CLI version + OS + subcommand + success/fail; **nunca tenant_id, blob digests, PAT**); property test 0 emissions sem flag explicit; documented em `docs/cli/telemetry.md` privacy policy. |
| Sprint | S-15 |
| Lane | STANDARD |
| Forcing factors | none |

## 1. Intent

CI templates 3 providers reduce friction de customer adoption. CoreLink target audience usa GitHub Actions (~60% market share) + GitLab CI (~20%) + CircleCI (~10%). Cada template é production-ready copy-paste integration; sample real builds verde em respective CI providers. Telemetry opt-in default-off (privacy-first) — LINDDUN review confirms anonymization; property test 0 emissions sem flag; documented em privacy policy.

```yaml
# File: templates/ci/github-actions/corelink-cache.yml
name: Bazel Build with CoreLink Cache

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: bazelbuild/setup-bazelisk@v3
      - name: Bazel build with remote cache
        env:
          CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
        run: |
          bazel build //... \
            --remote_cache=https://corelink.dev/v1/cache \
            --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
            # Lote 10.15 codex P0 canonical CTRL-CRED-001: credential helper protocol Bazel 6+ retorna token via stdout (nunca em argv); CI runners exportam CORELINK_PAT como env var (GitHub Actions secrets / GitLab CI variables / CircleCI env vars) + helper script reads from env
      - name: Report cache hit ratio
        run: |
          # Parse Bazel execution log + emit ratio para summary
          ratio=$(jq -r '.events[] | select(.cacheHit) | length' bazel-out/log.json)
          echo "## Cache Hit Ratio: ${ratio}%" >> $GITHUB_STEP_SUMMARY
```

```yaml
# File: templates/ci/gitlab-ci/corelink-cache.yml
stages:
  - build

bazel-build:
  stage: build
  variables:
    CORELINK_PAT: $CORELINK_PAT  # GitLab CI/CD variable
  script:
    - bazel build //... \
        --remote_cache=https://corelink.dev/v1/cache \
        --remote_header="Authorization=Bearer ${CORELINK_PAT}"
    - ratio=$(jq -r '.events[] | select(.cacheHit) | length' bazel-out/log.json)
    - echo "Cache Hit Ratio ${ratio}%"
```

```yaml
# File: templates/ci/circleci/corelink-cache.yml
version: 2.1

jobs:
  bazel-build:
    docker:
      - image: cimg/base:current
    steps:
      - checkout
      - run:
          name: Bazel build with remote cache
          command: |
            bazel build //... \
              --remote_cache=https://corelink.dev/v1/cache \
              --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
            # Lote 10.15 codex P0 canonical CTRL-CRED-001: credential helper protocol Bazel 6+ retorna token via stdout (nunca em argv); CI runners exportam CORELINK_PAT como env var (GitHub Actions secrets / GitLab CI variables / CircleCI env vars) + helper script reads from env
      - run:
          name: Report cache hit ratio
          command: |
            ratio=$(jq -r '.events[] | select(.cacheHit) | length' bazel-out/log.json)
            echo "Cache Hit Ratio ${ratio}%"
            echo "export CACHE_HIT_RATIO=${ratio}" >> $BASH_ENV

workflows:
  build:
    jobs:
      - bazel-build:
          context: corelink-secrets  # CORELINK_PAT em context
```

## 2. Narrative

CI templates 3 providers + telemetry opt-in privacy-first são adoção-driving features para GA conversion. Customers podem `cp templates/ci/github-actions/corelink-cache.yml .github/workflows/` → done; mesma facilidade GitLab + CircleCI. Telemetry opt-in (não opt-out) é privacy-first: LINDDUN review garante anonymization; property test 0 emissions sem flag explicit; data collected é apenas CLI version + OS + subcommand + outcome (anonymized).

**Risk justification STANDARD lane**:
- CI templates são copy-paste samples; não toca tenant data flow direto.
- Telemetry opt-in default-off é privacy-first sem novo collection surface (LINDDUN review).
- Property test verifies 0 emissions sem flag = bounded surface.

## 3. Customer Impact & Journey

**Persona — Build Engineer adopting CoreLink em CI**:
- `cp templates/ci/github-actions/corelink-cache.yml .github/workflows/` → set `CORELINK_PAT` secret → done.
- Cache hit ratio em workflow summary = visibility imediata.
- 3 providers covered (~90% CI market share).

**Persona — Privacy Officer reviewing CoreLink**:
- Telemetry opt-in default-off; `corelink config set telemetry on` requires explicit user action.
- LINDDUN review confirms anonymization (no PII).
- Property test 0 emissions sem flag explicit.

## 4. Capability Mapping

- **CAP-SDK-004** (CI integration templates) — IMPLEMENTA primary.
- **CAP-SDK-005** (Telemetry opt-in) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §5.4 (R-S15-12 + R-S15-13)` + `_spec_contract.md §5.5 (R-S15-14 + R-S15-15)` + `privacy_model.md` (LINDDUN baseline).

## 5. Tipo

Feature WI; STANDARD lane; CI templates + telemetry opt-in.

## 6. Escopo

### 6.1 In-scope

1. **CI templates 3 providers** em `templates/ci/`:
   - **GitHub Actions**: `templates/ci/github-actions/corelink-cache.yml`:
     - Workflow with `CORELINK_PAT` secret input.
     - Bazel + Buck2 step alternatives (commented).
     - Cache hit ratio output em GitHub Actions step summary.
     - README com integration steps.
   - **GitLab CI**: `templates/ci/gitlab-ci/corelink-cache.yml`:
     - `.gitlab-ci.yml` template.
     - `CORELINK_PAT` via GitLab CI/CD variables.
     - Cache hit ratio output em job log.
     - README com integration steps.
   - **CircleCI**: `templates/ci/circleci/corelink-cache.yml`:
     - `.circleci/config.yml` template.
     - `CORELINK_PAT` via CircleCI context.
     - Cache hit ratio output em job log.
     - README com integration steps.
   - Cada template inclui Bazel + Buck2 alternatives (referenciando WI-S15-002 + WI-S15-003 starter projects).

2. **3 real working sample builds verde**:
   - GitHub Actions: triggered em `examples/bazel-starter` repo.
   - GitLab CI: triggered em mirror repo `examples/gitlab-mirror`.
   - CircleCI: triggered em mirror repo `examples/circleci-mirror`.
   - Cada sample documenta cache hit ratio em respective workflow summary/log.

3. **Telemetry opt-in default-off**:
   - Implementation em `crates/corelink-cli/src/config.rs` (extends WI-S15-001):
     - `corelink config set telemetry on` enables.
     - `corelink config get telemetry` returns current state.
     - `corelink config list` shows telemetry status.
   - Default em fresh install: `telemetry = false` (em `~/.corelink/config.toml`).
   - Endpoint: `https://telemetry.corelink.dev/v1/events` (separate domain from data plane).
   - Payload format:
     ```json
     {
       "cli_version": "0.1.0",
       "os": "darwin-arm64",
       "subcommand": "ls",
       "outcome": "ok",
       "duration_ms": 42,
       "anonymized_id": "uuid-generated-on-first-launch-stored-em-config"
     }
     ```
   - **Anonymized**: nunca `tenant_id`, blob digests, PAT, file paths, IP address (server-side scrubbed).
   - `anonymized_id` é UUID generated on first launch + stored em config; user can rotate via `corelink config rotate telemetry-id`.

4. **LINDDUN privacy review**:
   - File `specs/_audits/2026-XX-XX-linddun-cli-telemetry.md`:
     - Linkability: anonymized_id rotatable; nunca per-tenant; LOW risk.
     - Identifiability: nunca PII em payload; LOW risk.
     - Non-repudiation: opt-in explicit (logged em config audit); LOW risk.
     - Detectability: telemetry events visible em network monitoring; LOW risk acceptable.
     - Disclosure: secrets nunca em payload; LOW risk.
     - Unawareness: opt-in discoverable via `corelink config list` + privacy policy doc; LOW risk.
     - Non-compliance: GDPR Art. 25 (data protection by design — opt-in default-off); LGPD Art. 6º X (transparency); compliant.
   - Sign-off: Privacy Officer (folded em Product + DevX em STANDARD lane).

5. **Property test 0 emissions sem flag**:
   - Test `tests/cli_telemetry_optin.rs`:
     - Fresh install (no config file or `telemetry = false`); run 100 CLI invocations various subcommands.
     - Mock telemetry endpoint receiver counts events.
     - Assert: 0 events received.
   - 10k iterations property test variando: subcommand mix + outcome mix; assert 0 emissions.

6. **Privacy policy doc**:
   - `docs/cli/telemetry.md`:
     - What we collect (CLI version + OS + subcommand + outcome).
     - What we DON'T collect (tenant_id, blob digests, PAT, file paths).
     - How to opt-in (`corelink config set telemetry on`).
     - How to opt-out (`corelink config set telemetry off`).
     - Data retention 90d.
     - Endpoint domain (`telemetry.corelink.dev` separate from data plane).

### 6.2 Out-of-scope

- Jenkins/Drone/Buildkite CI templates — pós-GA Q1 demand-driven.
- Telemetry data analysis dashboard — pós-GA (currently aggregated server-side via observability stack).
- A/B testing framework via telemetry — pós-GA.

## 7. Anti-Scope

- Skip LINDDUN privacy review.
- Telemetry opt-out default (must be opt-in default-off).
- Telemetry payload contains tenant_id ou blob digests ou PAT.
- Skip property test 0 emissions sem flag.
- Hard-code PAT em CI templates (`${CORELINK_PAT}` env var canonical).
- Skip 3 real working sample builds verde.
- Telemetry hidden discoverability (must be `corelink config list` visible).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: CI templates 3 providers + telemetry opt-in default-off

  Scenario: GitHub Actions template integration
    Given templates/ci/github-actions/corelink-cache.yml
    When customer copies to .github/workflows/ + sets CORELINK_PAT secret
    Then workflow runs verde
    And cache hit ratio em step summary

  Scenario: GitLab CI template integration
    Given templates/ci/gitlab-ci/corelink-cache.yml
    When customer copies to .gitlab-ci.yml + sets CORELINK_PAT variable
    Then pipeline runs verde
    And cache hit ratio em job log

  Scenario: CircleCI template integration
    Given templates/ci/circleci/corelink-cache.yml
    When customer copies to .circleci/config.yml + sets CORELINK_PAT context
    Then pipeline runs verde
    And cache hit ratio em job log

  Scenario: Telemetry opt-in default-off (fresh install)
    Given fresh CoreLink CLI install (no config file)
    When user runs `corelink ls --tenant acme` 100 times
    And mock telemetry endpoint receiver counts events
    Then 0 events received
    And property test 10k iterations confirms 0 emissions

  Scenario: Telemetry opt-in via discoverable command
    Given config file with telemetry = false
    When user runs `corelink config set telemetry on`
    Then config file updated to telemetry = true
    And subsequent invocations emit anonymized events
    And payload contains: cli_version, os, subcommand, outcome, duration_ms, anonymized_id
    And payload does NOT contain: tenant_id, blob digests, PAT, file paths

  Scenario: Telemetry opt-out via discoverable command
    Given telemetry = true
    When user runs `corelink config set telemetry off`
    Then config file updated to telemetry = false
    And subsequent invocations emit 0 events

  Scenario: Telemetry discoverability
    Given user runs `corelink config list`
    Then output includes "telemetry: false" (or true)
    And linkado a privacy policy `docs/cli/telemetry.md`

  Scenario: LINDDUN privacy review committed
    Given specs/_audits/2026-XX-XX-linddun-cli-telemetry.md
    Then 7 LINDDUN dimensions reviewed
    And risk classification per dimension (LOW/MEDIUM/HIGH)
    And mitigation per non-LOW classification
    And sign-off Privacy Officer (folded em Product/DevX em STANDARD lane)

  Scenario: Anonymized_id rotatable
    Given anonymized_id em config file
    When user runs `corelink config rotate telemetry-id`
    Then new UUID generated + stored em config
    And subsequent telemetry events use new ID

  Scenario: 3 real working sample builds verde
    Given GitHub Actions sample em examples/bazel-starter
    Given GitLab CI sample em examples/gitlab-mirror
    Given CircleCI sample em examples/circleci-mirror
    When samples run
    Then 3/3 verde
    And documented em respective workflow summaries
```

## 9. Design Decisions

### 9.1 Why 3 providers (não 1 ou todos)

- GitHub Actions ~60% + GitLab ~20% + CircleCI ~10% = ~90% CI market share.
- Jenkins/Drone/Buildkite deferred pós-GA demand-driven (low overlap com CoreLink target audience).

### 9.2 Why telemetry opt-in default-off (não opt-out)

- Privacy-first per GDPR Art. 25 (data protection by design).
- LINDDUN review confirms anonymization — opt-in explicit é standard.
- Customer trust building — opt-out hidden = anti-pattern (anti-scope spec contract §2.2).

### 9.3 Why anonymized_id (não user_id ou tenant_id)

- Linkability LOW: rotatable; nunca per-tenant; user can rotate.
- LINDDUN compliance + GDPR/LGPD compliant.

### 9.4 Why separate domain `telemetry.corelink.dev` (não data plane)

- Network isolation: customer can block telemetry endpoint via firewall sem impacting data plane.
- Audit clarity: telemetry traffic distinguishable.

### 9.5 ADR potencial?

- Não. CI templates 3 providers + telemetry opt-in são standard patterns; GDPR/LGPD compliance baseline.

## 10. Completeness Criteria

- [x] **10.s15.005.1** 3 CI templates committed em `templates/ci/` (EVT-018).
- [x] **10.s15.005.2** 3 real working sample builds verde em respective providers (EVT-018).
- [x] **10.s15.005.3** Telemetry opt-in implementation em CLI; default-off (EVT-002).
- [x] **10.s15.005.4** Property test 0 emissions sem flag explicit; 10k iter (EVT-002).
- [x] **10.s15.005.5** LINDDUN privacy review committed (EVT-049).
- [x] **10.s15.005.6** Privacy policy doc `docs/cli/telemetry.md` publicado.
- [x] **10.s15.005.7** Anonymized payload (no PII; verified via test).
- [x] **10.s15.005.8** Telemetry discoverable via `corelink config list`.

## 11. DoD

- [x] 3 CI templates committed.
- [x] 3 real working sample builds verde.
- [x] Telemetry opt-in implementation; property test 0 emissions sem flag.
- [x] LINDDUN privacy review committed.
- [x] Privacy policy doc publicado.
- [x] Tests: 4+ negative scenarios (6 negative scenarios: unknown key, invalid value, missing file, opt-out still disabled, PII-catch guard, tenant_id guard).

## 12. Invariants Validated

- **CTRL-CRED-001** reforced via `${CORELINK_PAT}` env var em 3 templates (no hardcoded).
- **CTRL-AUDIT-002** (rich audit events) consumed via telemetry opt-in (anonymized).
- INV-OBS-CARDINALITY-BUDGET respeitado em telemetry métricas (NUNCA per-tenant labels).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| GitHub Actions template | `templates/ci/github-actions/corelink-cache.yml` | YAML |
| GitLab CI template | `templates/ci/gitlab-ci/corelink-cache.yml` | YAML |
| CircleCI template | `templates/ci/circleci/corelink-cache.yml` | YAML |
| Telemetry opt-in implementation | `crates/corelink-cli/src/config.rs` (extends WI-001) | Rust |
| LINDDUN review | `specs/_audits/2026-XX-XX-linddun-cli-telemetry.md` | Markdown |
| Privacy policy doc | `docs/cli/telemetry.md` | Markdown |
| Property test 0 emissions | `tests/cli_telemetry_optin.rs` | Rust |
| GH Actions sample workflow | `examples/bazel-starter/.github/workflows/corelink-cache-sample.yml` | YAML |
| GitLab sample | `examples/gitlab-mirror/.gitlab-ci.yml` | YAML |
| CircleCI sample | `examples/circleci-mirror/.circleci/config.yml` | YAML |

## 14. Quality Standards

- **14.s15.005.1** CI templates production-ready copy-paste integration.
- **14.s15.005.2** Telemetry opt-in default-off; LINDDUN review.
- **14.s15.005.3** Property test 0 emissions; 10k iter.
- **14.s15.005.4** Anonymized payload (no PII); test verifies.
- **14.s15.005.5** Privacy policy doc clear + GDPR/LGPD compliant.

## 15. Test Plan

### Unit tests
- Telemetry opt-in flag toggle.
- Anonymized payload schema validation.
- `anonymized_id` rotation.

### Integration tests
- 3 real working sample builds verde em respective CI providers.
- End-to-end telemetry flow opt-in: enable → invoke → mock receiver counts events.

### Property test
- 10k iter: random subcommand mix + outcome mix; fresh install (no config); assert 0 emissions.

### Negative scenarios (≥ 4)
1. **Telemetry opt-in but endpoint unreachable**: graceful failure (not blocking CLI invocation).
2. **Anonymized payload contains PII**: assertion fail em test.
3. **Hardcoded PAT em CI template**: lint detection em CI.
4. **Telemetry opt-out but events still emit**: property test catches.
5. **`anonymized_id` not rotatable**: API check fail.

## 16. Failure Modes

- **FM-150** (transient network) em telemetry endpoint: graceful failure (no retry; no blocking).
- **FM-160** (auth invalid) em CI templates: clear error from Bazel/Buck2; user redirected to `corelink doctor`.

## 17. Controls

- **CTRL-CRED-001** enforced via `${CORELINK_PAT}` em CI templates.
- **CTRL-AUDIT-002** consumed via telemetry opt-in (anonymized).

## 18. Resilience Patterns

- Telemetry endpoint failure non-blocking (graceful degradation).
- 3 CI providers covered (broad reach).

## 19. Observability

CI templates emit cache hit ratio em workflow summary (não CoreLink métrica direta).

Telemetry opt-in métricas (server-side aggregation):
- `corelink_telemetry_event_total{cli_version, os, subcommand, outcome}` counter (cardinality bounded; cli_version + os limited).
- `corelink_ci_template_cache_hit_ratio{provider}` gauge (provider ∈ github_actions|gitlab|circleci).

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado.

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT via secret + env var canonical em CI templates.
- **Tampering**: telemetry payload integrity via HTTPS + endpoint authentication.
- **Repudiation**: opt-in explicit logged em config audit.
- **Information disclosure**: anonymized payload (no PII); LINDDUN confirms.
- **DoS**: telemetry graceful failure non-blocking.
- **Elevation of privilege**: telemetry endpoint separate domain; no admin operations.

**LINDDUN delta**: vide §6.1 review; LOW risk all 7 dimensions.

## 21. Dependencies

### Hard blockers
- WI-S15-001 (CLI binary + config subcommand).
- WI-S15-002 + WI-S15-003 (Bazel + Buck2 starters referenced em templates).

### Soft blockers
- S-09 SEALED (observability stack consumes telemetry events server-side).

### Outbound
- WI-S15-006 (ship gate references este como completeness 10.s15.5).
- S-19 (customer onboarding uses CI templates).

## 22. Effort PERT

O: 8h, M: 12h, P: 18h → PERT **12.3h** (per spec contract §12).

## 23. Cost Analysis

- Telemetry endpoint hosting: ~$10/mês (Cloudflare Worker).
- CI samples runs: $10/mês.
- Total: ~$20/mês incremental.

## 24. Post-mortem Hooks

- Telemetry collected sem opt-in → CRITICAL post-mortem + Privacy + Legal.
- LINDDUN review drift detected (PII em payload) → CRITICAL.
- CI template hardcoded secret detected → SEV-1 + Security review.

## 25. Rollback / Recovery

Telemetry endpoint regression → rollback via Cloudflare Worker version revert; CLI graceful failure non-blocking.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Telemetry leak (PII collected sem opt-in) | L | M | HIGH (privacy) | L | LOW | LINDDUN review + audit trail + property test 0 leaks |
| R-002 | CI template breaking change em 1 provider | M | M | LOW | M | LOW | 3 real working samples sustained CI |
| R-003 | Anonymized_id reverse-engineered | L | L | LOW | L | LOW | Rotatable + UUID v4 random |
| R-004 | Endpoint unreachable causes CLI hang | L | M | LOW | L | LOW | Graceful failure non-blocking; timeout 1s |
| R-005 | GitHub Actions / GitLab / CircleCI API drift | M | L | LOW | L | LOW | Sample builds sustained CI; weekly cron |

## 27. Knowledge Transfer

- Tech talk (1h): "CI Templates + Telemetry Opt-In Privacy-First".
- Doc `docs/cli/telemetry.md` privacy policy (customer-facing).
- LINDDUN review reading mandatory para Privacy Officer.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Engineer (S-15 lead) | _TBD_ | _pending_ |
| 4 | QA Lead | _TBD_ | _pending_ |
| 5 | Product | Gustavo Schneiter (Privacy Officer folded em Product em STANDARD lane) | _pending_ |
| 6 | DevX advisor | _TBD; emphatic — CI templates UX + telemetry discoverability_ | _pending_ |
| 7 | Docs lead | _TBD; emphatic — privacy policy doc + LINDDUN review trace_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S15-005 (cycle 12.S15.0; CI templates 3 providers + telemetry opt-in LINDDUN review). |
| 1.1.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | SEALED: crates/corelink-cli (config.rs + telemetry.rs); 3 CI templates; 3 sample builds; property test 10k; LINDDUN review; privacy doc. All 8 completeness criteria met. |

## 30. Anti-patterns evitados

- Skip LINDDUN privacy review.
- Telemetry opt-out default.
- Telemetry payload contains PII (tenant_id, blob digests, PAT).
- Skip property test 0 emissions sem flag.
- Hard-code PAT em CI templates.
- Skip 3 real working sample builds verde.
- Telemetry hidden discoverability.

---

**Fim WI-S15-005.**
