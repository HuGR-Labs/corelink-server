---
id: "WI-S12-004"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-05-13"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-12"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s12", "supply-chain", "cargo-audit", "cargo-deny", "dependabot", "rustsec", "license-allowlist", "yanked", "high-risk"]
---

# WI-S12-004 — `cargo-audit` PR + Daily Cron + `cargo-deny` Policy (License Allowlist + Yanked + Sources + Advisories) + Dependabot Weekly Grouped + Auto-merge Minor

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-12](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S12-004 |
| Título | `cargo-audit` PR check + daily cron com SEV-2/SEV-3 alerts; `cargo-deny` em `deny.toml` enforces license allowlist (MIT/Apache-2.0/BSD-2-Clause/BSD-3-Clause/ISC/MPL-2.0/Unicode-DFS-2016) + GPL-*/AGPL-*/SSPL-*/Commons-Clause banned + zero yanked deps + sources crates.io only (git deps requerem explicit commit hash) + RUSTSEC advisories denied unless waived ADR; Dependabot weekly grouped PRs (security/non-security separate); auto-merge minor patches passing CI; INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST enforced |
| Sprint | S-12 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controles supply chain — bypass = blast radius global) |

## 1. Intent

Implementar `.github/workflows/cargo-audit.yml` + `deny.toml` + `.github/dependabot.yml` que (1) executa `cargo-audit` em **PR check** + **daily cron** com alert SEV-2 (CRITICAL CVEs) e SEV-3 (HIGH CVEs); (2) `cargo-deny` policy enforcement em `deny.toml`: license allowlist explicit (7 OSI-approved licenses), banned licenses (GPL/AGPL/SSPL/Commons-Clause copyleft viral), zero yanked deps, sources restricted (crates.io only; git deps requerem explicit commit hash), RUSTSEC advisories denied unless waived via ADR; (3) Dependabot weekly grouped PRs separados por security/non-security; auto-merge minor patches passing CI. **Enforces INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST** (ambos novos S-12).

```toml
# File: deny.toml (canonical)

[graph]
all-features = true
no-default-features = false
exclude = ["dev-dependencies"]

[advisories]
db-path = "~/.cargo/advisory-db"
db-urls = ["https://github.com/rustsec/advisory-db"]
vulnerability = "deny"           # RUSTSEC-* deny unless waived
unmaintained = "warn"            # warn em unmaintained; review trimestral
yanked = "deny"                  # INV-SUPPLY-NO-YANKED enforce
notice = "warn"
ignore = []                      # waivered RUSTSEC-* listed here com ADR ref

[licenses]
unlicensed = "deny"              # deps sem license = block
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "MPL-2.0",
    "Unicode-DFS-2016",
    "Unicode-3.0",                # newer Unicode license forwarded
    "CC0-1.0",                   # public domain dedication; OSI-approved
]
deny = [
    "GPL-1.0",
    "GPL-2.0",
    "GPL-2.0+",
    "GPL-3.0",
    "GPL-3.0+",
    "AGPL-1.0",
    "AGPL-3.0",
    "SSPL-1.0",
    "Commons-Clause",
    "BUSL-1.1",                  # business source non-OSI
]
copyleft = "deny"                # any copyleft license auto-deny
default = "deny"                 # unknown license = deny
exceptions = []                  # waived per-crate com ADR ref

[bans]
multiple-versions = "warn"       # warn em duplicate deps; trimestral cleanup
deny = []                        # specific crate denylist (e.g., known-malicious)
skip-tree = []                   # skip transitive checks for specific crate

[sources]
unknown-registry = "deny"        # only crates.io
unknown-git = "deny"             # git deps must be allow-listed
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = []                   # explicit git URL allowlist (com commit hash em Cargo.toml)
```

```yaml
# File: .github/dependabot.yml (canonical)

version: 2
updates:
  - package-ecosystem: cargo
    directory: "/"
    schedule:
      interval: weekly
      day: monday
      time: "08:00"
      timezone: America/Sao_Paulo
    open-pull-requests-limit: 10
    groups:
      security-updates:
        applies-to: security-updates
        update-types: ["patch", "minor", "major"]
      non-security-minor-patch:
        applies-to: version-updates
        update-types: ["patch", "minor"]
      non-security-major:
        applies-to: version-updates
        update-types: ["major"]
    labels: ["dependencies", "rust"]
    commit-message:
      prefix: "deps"
      include: scope
    reviewers:
      - "humangr-labs/security"
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Dependency hygiene é **continuous control surface**: ecosystem Rust tinha 600+ RUSTSEC advisories em 2024 (triplicou vs 2022); event-stream 2018 + ua-parser-js 2021 + XZ Utils 2024 demonstram que **automated detection + license enforcement + auto-merge mínimo** é mínimo defensável. CoreLink HIGH_RISK substrate exige **3 controles compostos**:

1. **`cargo-audit`**: detecta RUSTSEC advisories em `Cargo.lock` (PR + daily cron); SEV-2/SEV-3 alert escalation.
2. **`cargo-deny`**: policy enforcement (license allowlist, banned, yanked, sources, advisories) em `deny.toml`.
3. **Dependabot**: automated dep updates weekly grouped; auto-merge minor patches passing CI (operational hygiene).

**Bugs catastróficos possíveis** (todos endereçados):

1. **Yanked dep introduzida via Dependabot auto-merge**: yanked = author retired (segurança ou bug); bypass detection via auto-merge minor. Mitigação: cargo-deny `yanked = "deny"` em CI gate; auto-merge requer CI green (cargo-deny passa antes merge); INV-SUPPLY-NO-YANKED enforced.

2. **License audit miss (GPL leak)**: transitive dep com GPL-3.0 introduzida via update; copyleft viral expõe CoreLink legal risk. Mitigação: cargo-deny `allow` whitelist explicit; `copyleft = "deny"` auto-block; quarterly Legal review.

3. **Typosquatting (FM-157)**: attacker publishes `corelink-server-utils` (typo) em crates.io; manual `cargo add` introduces. Mitigação: cargo-deny `unknown-registry = "deny"` (only crates.io official); `cargo-crev` review opcional pós-GA; lockfile diff em PR review (mandatory comment).

4. **Dep maintainer malicioso (FM-156)**: legitimate dep (e.g., serde) maintainer compromised; malicious update published; cliente updates inadvertently. Mitigação: cargo-audit detection ≤ 24h pós-RUSTSEC publish; Dependency-Track CVE alert (WI-S12-005); auto-merge minor patch policy = small blast radius (não auto-merge major).

5. **Auto-merge regression**: auto-merge minor patch contains breaking change (semver violation); CI passes mas integration tests fail downstream. Mitigação: auto-merge requer **all** CI checks green (não só cargo-deny); revert protocol em failure ≤ 24h; canary deploy stage.

6. **RUSTSEC advisory delay**: CVE published em NVD mas RUSTSEC mirror lag > 24h; CoreLink behind window. Mitigação: Dependency-Track (WI-S12-005) usa NVD + OSV + GHSA múltiplas sources; redundant detection.

7. **`[patch.crates-io]` patches sem ADR**: developer applies vendor patch; cargo-deny passa (license OK); Security review ausente. Mitigação: ADR-XXXX-vendor-patch-policy.md mandatory ADR + Security review per patch.

8. **Unmaintained dep silent risk**: dep marked `unmaintained` (RUSTSEC-2024-XXXX); cargo-deny `warn` mode; risco bypass. Mitigação: quarterly review unmaintained deps; replacement plan em ADR.

**Atacante adversarial scenarios**:

- **SolarWinds-style maintainer compromise** (FM-156): legitimate dep maintainer credentials stolen; malicious update published; cliente auto-merges minor. Mitigação: cargo-audit detect within hours; Dependency-Track real-time alert; auto-merge minor policy bounded blast radius (não major); revert ≤ 24h.

- **Typosquat published mid-sprint** (FM-157): attacker publishes `corelink-fake` crate; developer manual addition. Mitigação: lockfile diff PR review (mandatory comment); cargo-deny sources allowlist; pair-review on new dep additions.

- **License laundering**: dep marked MIT em Cargo.toml mas actual code is GPL fragments (legal risk). Mitigação: quarterly Legal review (manual sample audit) + cargo-deny SPDX expression validation.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: dep CVE bypass = full pipeline trust violation; ecosystem Rust frequência de attacks crescente.
- **Reversibility**: malicious dep em production = customer data exfil possible; rollback non-trivial (cache invalidation + forensics).

11 sign-offs canonical incl. Compliance Officer (license allowlist + Legal review cadence) + AppSec (RUSTSEC monitoring + Dependabot auto-merge policy) + Security Lead (FM-156 + FM-157 mitigation review).

## 3. Customer Impact & Journey

**Persona 1 — Auditor SOC 2 / ISO 27001**:
- Audit query: "Show 100% releases with cargo-audit zero findings HIGH/CRITICAL + cargo-deny green policy".
- Evidence: CI workflow runs (`cargo-audit.yml`) + `deny.toml` config + Dependabot PR history.
- Compliance Matrix: SOC 2 CC7.1 (vulnerability detection).

**Persona 2 — SecOps lead em incident**:
- Slack alert SEV-2: "CVE CRITICAL detected em `serde@1.0.X` em main branch".
- On-call escalation: investigate em 30 min; remediation PR ≤ 24h (auto-merge fix se passes).
- Dashboard DASH-SUPPLY: cargo-audit findings trend 30d + cargo-deny violations trend.

**Persona 3 — Engineer adding new dep**:
- `cargo add tokio` → automatic CI check on PR.
- cargo-deny validates license + source + lockfile diff.
- Dependabot weekly review (Monday 8am BRT) batches updates.

**SLA addendum**:
- cargo-audit detection latency: ≤ 24h pós-RUSTSEC publish (daily cron).
- SEV-2 alert (CRITICAL CVE): on-call paged ≤ 30s.
- SEV-3 alert (HIGH CVE): Slack notification ≤ 5 min.
- License audit cycle: quarterly Legal review.
- Dependabot weekly cadence; auto-merge minor patches CI green ≤ 1h.

## 4. Capability Mapping

- **CAP-SUPPLY-004** (cargo-audit + cargo-deny + Dependabot) — IMPLEMENTA primary.
- **CAP-SUPPLY-007** (Vendored deps audit + lockfile pinning) — IMPLEMENTA partial.
- Trace: `_spec_contract.md §4 + §5.4 + §5.7` + `security_model.md §11.4 (CTRL-SUPPLY-004 + CTRL-SUPPLY-006 (license allowlist) + CTRL-SUPPLY-007 (yanked-dep block; codex SEAL cycle 1 alignment com canonical security_model.md §6.4))` + `failure_modes.md (FM-156 + FM-157)`.

## 5. Tipo

CI gate + dep hygiene; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`.github/workflows/cargo-audit.yml`** workflow novo:
   - **PR trigger**: every PR; runs `cargo audit --deny warnings`.
   - **Daily cron**: `0 6 * * *` UTC; runs `cargo audit`; alert SEV-2 em CRITICAL findings, SEV-3 em HIGH.
   - **Manual trigger**: `workflow_dispatch`.
   - Job 1: setup Rust toolchain pinned via `rust-toolchain.toml`.
   - Job 2: `cargo install cargo-audit --version 0.20.x` (pinned).
   - Job 3: `cargo audit --deny warnings` (PR mode) ou `cargo audit --json` (cron mode).
   - Job 4 (cron only): parse JSON findings; classify HIGH/CRITICAL; emit Slack webhook + PagerDuty ack.
   - Permissions: `contents: read`.
2. **`deny.toml`** policy file (canonical em §1):
   - `[advisories]`: vulnerability=deny, yanked=deny, unmaintained=warn.
   - `[licenses]`: 7 OSI-approved licenses allow + GPL-*/AGPL-*/SSPL-*/Commons-Clause/BUSL banned; copyleft=deny.
   - `[bans]`: multiple-versions=warn (cleanup quarterly); explicit denylist for known-malicious crates.
   - `[sources]`: unknown-registry=deny (crates.io only); unknown-git=deny (allowlist only).
3. **`.github/workflows/cargo-deny.yml`** workflow:
   - PR trigger: `cargo deny check --hide-inclusion-graph`.
   - Daily cron: same.
   - Job: install cargo-deny pinned + run check.
4. **`.github/dependabot.yml`** config (canonical em §1):
   - Weekly grouped PRs: security-updates + non-security-minor-patch + non-security-major.
   - Schedule: Monday 8am BRT.
   - Reviewers: `humangr-labs/security` team.
5. **`.github/workflows/dependabot-auto-merge.yml`** auto-merge workflow:
   - Trigger: Dependabot PR opened.
   - Conditions: PR labeled `dependencies` + update-type `minor` OR `patch` + all CI checks green (cargo-audit + cargo-deny + cargo-test + clippy + clippy `-D warnings`).
   - Action: `gh pr merge --auto --squash` (auto-merge enabled).
   - Major updates: NEVER auto-merge; manual review required.
6. **Lockfile diff PR review**:
   - GitHub Action `EndBug/lockfile-diff@v1` (or equiv) emits comment em PR se `Cargo.lock` mudou.
   - Mandatory comment includes: added/removed/upgraded deps + license check + cargo-deny output.
7. **`docs/internal/dep-policy.md`** documentation:
   - License allowlist rationale (per license OSI status + business compatibility).
   - Banned licenses rationale (copyleft viral risk).
   - Yanked deps zero tolerance rationale.
   - Vendored patches `[patch.crates-io]` ADR requirement.
   - Dependabot auto-merge policy.
   - RUSTSEC triage process (≤ 24h triage, ≤ 7d HIGH fix, ≤ 30d MEDIUM).
8. **Métricas underscored Prometheus**:
   - `corelink_supply_cargo_audit_findings_total{severity}` (severity ∈ critical|high|medium|low|info).
   - `corelink_supply_cargo_audit_runs_total{outcome}` (outcome ∈ pass|fail).
   - `corelink_supply_cargo_deny_violations_total{rule}` (rule ∈ license|yanked|sources|advisory|copyleft|unknown_registry).
   - `corelink_supply_dependabot_prs_total{outcome,update_type}` (outcome ∈ auto_merged|manual|failed; update_type ∈ patch|minor|major|security).
   - `corelink_supply_dep_count_gauge` (total deps em Cargo.lock).
   - `corelink_supply_yanked_deps_count_gauge` (target = 0).
9. **Observability** — trace span `cargo_audit.run` + `cargo_deny.check` + `dependabot.auto_merge` com attributes:
   - `cargo_audit.findings_count` (u32).
   - `cargo_deny.violations_count` (u32).
   - `dependabot.update_type` (enum).
   - `result` (enum).
10. **Property tests** (10k iter PR + 100k iter nightly):
    - `prop_cargo_deny_license_allowlist_enforced`: 10k synthetic Cargo.toml com license combinations; assert allowlist enforcement.
    - `prop_cargo_deny_yanked_blocked`: 10k yanked dep injections; assert 100% rejection.
    - `prop_cargo_deny_unknown_source_blocked`: 10k unknown registry/git URLs; assert 100% rejection.
    - `prop_cargo_audit_critical_alerts`: 10k synthetic CRITICAL CVEs; assert SEV-2 alert path.
11. **Adversarial regression tests**:
    - GPL-3.0 dep introduced → cargo-deny blocks.
    - Yanked dep introduced via Dependabot auto-merge → cargo-deny blocks (CI gate).
    - Typosquat `corelink-fake` em crates.io → manual review catches via lockfile diff.
    - Unmaintained dep → cargo-deny warn + quarterly review trigger.
    - `[patch.crates-io]` sem ADR → pre-merge check fails.
12. **Quarterly Legal review checklist**:
    - Sample 5% deps; verify license SPDX expression matches actual code.
    - Update `deny.toml` allowlist com new licenses if business-compatible.
    - Document decisions em ADR-XXXX-license-review-quarterly.md.
13. **Integration test E2E**:
    - Real Dependabot PR em staging fork; verify auto-merge minor patch flow.
    - Mock CRITICAL CVE injection; verify SEV-2 alert path.

### 6.2 Out-of-scope (deferred)

- **SLSA L3 + Rekor**: WI-S12-001.
- **SBOM CycloneDX**: WI-S12-002.
- **Cosign + CF deploy verify**: WI-S12-003.
- **Dependency-Track self-host**: WI-S12-005.
- **Reproducible builds**: WI-S12-006.
- **`cargo-crev` review** (community trust web): pós-GA Q3 (mature ecosystem needed).
- **Bug bounty program**: pós-GA Q1.
- **Custom advisory database** (CoreLink-specific findings): pós-GA enterprise.
- **License auto-classifier ML model**: pós-GA (manual quarterly review suffices).

## 7. Anti-Scope

- ❌ Skip cargo-audit em PR ("nightly only") — both required.
- ❌ License allowlist mais permissivo (manter 7 OSI-approved + future via ADR).
- ❌ Yanked dep "warn but allow" (deny mandatory; INV-SUPPLY-NO-YANKED enforces).
- ❌ Sources allow git URLs sem commit hash explicit.
- ❌ Auto-merge major version updates (manual review required).
- ❌ Auto-merge security CRITICAL sem CI green (CI green mandatory).
- ❌ `[patch.crates-io]` sem ADR + Security review.
- ❌ Skip Dependabot ("manual updates suffices") — anti-pattern; volume 50+ deps.
- ❌ Lockfile diff PR review optional (mandatory comment via Action).
- ❌ Quarterly Legal review skipped (mandatory cycle).
- ❌ RUSTSEC waiver sem ADR + 90d sunset (formal waiver process required).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: cargo-audit + cargo-deny + Dependabot

  Background:
    Given .github/workflows/cargo-audit.yml configured
    And deny.toml configured per canonical schema
    And .github/dependabot.yml configured

  Scenario: PR triggers cargo-audit + cargo-deny check
    Given a PR opened with Cargo.lock changes
    When cargo-audit + cargo-deny workflows run
    Then cargo-audit returns 0 findings or HIGH/CRITICAL classification
    And cargo-deny returns 0 violations or specific rule violations
    And PR check status reflects results
    And metric corelink_supply_cargo_audit_runs_total{outcome="pass"} incremented

  Scenario: Daily cron detects new CVE CRITICAL
    Given a CRITICAL CVE published em RUSTSEC overnight
    When daily cron runs at 06:00 UTC
    Then cargo-audit detects finding
    And alert SEV-2 fires via Slack webhook + PagerDuty
    And on-call paged ≤ 30s
    And metric corelink_supply_cargo_audit_findings_total{severity="critical"} incremented

  Scenario: GPL-3.0 dep blocked by cargo-deny
    Given a dep with license = "GPL-3.0" introduced em Cargo.toml
    When cargo-deny check runs
    Then violation reported: license deny
    And exit code != 0
    And PR check fails
    And metric corelink_supply_cargo_deny_violations_total{rule="license"} incremented

  Scenario: Yanked dep blocked
    Given a transitive dep upgraded to yanked version
    When cargo-deny check runs
    Then violation: yanked dep
    And PR check fails
    And metric corelink_supply_cargo_deny_violations_total{rule="yanked"} incremented

  Scenario: Unknown git source blocked
    Given Cargo.toml has dep from git URL não-allowlisted
    When cargo-deny check runs
    Then violation: unknown-git
    And PR check fails

  Scenario: Dependabot weekly grouped PRs
    Given Monday 08:00 BRT
    When Dependabot runs
    Then PRs grouped: security-updates / non-security-minor-patch / non-security-major
    And max 10 open PRs at any time

  Scenario: Auto-merge minor patch passing CI
    Given Dependabot PR opened with update_type=minor
    Given all CI checks green (cargo-audit + cargo-deny + cargo-test + clippy)
    When auto-merge workflow runs
    Then PR merged via squash
    And metric corelink_supply_dependabot_prs_total{outcome="auto_merged",update_type="minor"} incremented

  Scenario: Auto-merge major update blocked
    Given Dependabot PR opened with update_type=major
    When auto-merge workflow runs
    Then PR NOT auto-merged
    And manual review required

  Scenario: Lockfile diff comment on PR
    Given a PR with Cargo.lock changes
    When PR opened/synced
    Then GitHub Action emits comment: added/removed/upgraded deps
    And license check summary
    And cargo-deny output

  Scenario: Property test 100k iter green
    Given prop_cargo_deny_license_allowlist_enforced 100k iter
    When test runs nightly
    Then 0 false-accepts (GPL passing)
    And 0 false-rejects (MIT/Apache-2.0 failing)

  Scenario: SLA latency
    Given CRITICAL CVE published em RUSTSEC
    When daily cron runs em ≤ 24h
    Then detection latency ≤ 24h
    And alert SEV-2 ≤ 30s after detection
```

## 9. Design Decisions

### 9.1 Why cargo-audit + cargo-deny (não single tool)

- **cargo-audit**: focused on RUSTSEC advisories (CVE-class).
- **cargo-deny**: broader policy enforcement (license + yanked + sources + bans).
- Complementary: audit detects CVEs; deny enforces policy.
- Industry standard: usado em Mozilla Servo, Fuchsia, Bevy, Tauri.

### 9.2 Why license allowlist explicit (não denylist)

- **Allowlist**: known-good; new license requires ADR review (default-deny).
- **Denylist**: known-bad; new license bypasses unless explicitly added.
- Safer for compliance: GPL-1.0+ or new copyleft = unintended exposure.

### 9.3 Why 7 OSI-approved licenses (não more permissive)

- MIT/Apache-2.0/BSD-2-Clause/BSD-3-Clause/ISC: permissive; widely compatible.
- MPL-2.0: weak copyleft (file-level); accepted em SaaS.
- Unicode-DFS-2016/Unicode-3.0: data files (icu_data); essential.
- Adicional licenses ratificadas via ADR-XXXX (cycle 9.4 future case).

### 9.4 Why GPL/AGPL/SSPL/Commons-Clause banned

- GPL/AGPL: viral copyleft; SaaS distribution risk.
- SSPL: MongoDB-style; anti-cloud.
- Commons-Clause: contradicts OSI; restricts commercial use.
- BUSL: not OSI-approved; commercial restrictions.

### 9.5 Why yanked = deny (não warn)

- INV-SUPPLY-NO-YANKED enforces zero tolerance.
- Yanked = author retired (segurança ou bug); usar = risco.
- Workaround if needed: explicit waiver via ADR + 90d sunset.

### 9.6 Why sources crates.io only

- Typosquat mitigation: official registry only.
- Git deps require explicit allowlist + commit hash em Cargo.toml (não tag, não branch).
- Industry pattern: Rust-lang nightly disallows git deps em crates.io packages.

### 9.7 Why Dependabot weekly (não daily)

- Daily: noise (50+ PRs/sem); reviewer fatigue.
- Weekly Monday 8am BRT: batch review fits sprint cadence.
- Security-updates exception: triggered immediately on advisory.

### 9.8 Why auto-merge minor (não auto-merge security)

- Auto-merge security CRITICAL: tempting mas semver violation possible (security fix may break API).
- Manual review required for security despite urgency.
- Auto-merge minor: bounded blast radius; CI green = high confidence.

### 9.9 Why quarterly Legal review

- License SPDX expression vs actual code drift detected only manually.
- Quarterly cadence balanced (cost vs risk).
- ADR-XXXX-license-review-quarterly.md documents process.

### 9.10 Why cargo-audit + cargo-deny pinned versions

- Reproducible build property: pinned tooling = same output.
- ADR upgrade cadence: bump via ADR with regression testing.

### 9.11 ADR potencial?

- Sim — **ADR-XXXX**: "Dep policy: cargo-audit + cargo-deny + Dependabot canonical config". Pattern reusable.
- Sim — **ADR-XXXX**: "License allowlist 7 OSI-approved + banned copyleft". Legal evidence pack.
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s12.004.1** Property test 10k iter (PR) + 100k iter (nightly) sobre fuzz Cargo.toml + Cargo.lock inputs → 0 false-accepts (EVT-002).
- [ ] **10.s12.004.2** Adversarial test: 5 scenarios (GPL leak, yanked auto-merge, typosquat, unmaintained, vendor patch sem ADR) — 100% mitigated (EVT-040).
- [ ] **10.s12.004.3** E2E test contra staging fork: Dependabot PR → auto-merge flow + CRITICAL CVE injection → alert (EVT-018).
- [ ] **10.s12.004.4** **`cargo-audit`** zero findings HIGH/CRITICAL em `Cargo.lock` no momento do release; daily cron green sustained 30d staging (EVT-002).
- [ ] **10.s12.004.5** **`cargo-deny`** policy verde em CI; license allowlist enforced; 0 yanked deps; 0 banned licenses (EVT-002).
- [ ] **10.s12.004.6** **Dependabot** ativo + weekly grouped PRs functioning; auto-merge minor patches working sem regression em CI (EVT-002).
- [ ] **10.s12.004.7** SAST clean em CoreLink workspace (cargo-audit + cargo-deny + clippy `-D warnings`) (EVT-002).
- [ ] **10.s12.004.8** Cost regression gate: cargo-audit + cargo-deny CI ≤ 1 min adicional; Dependabot infra free (Lote 9.4 §14.10).
- [ ] **10.s12.004.9** OWASP ASVS V14 + SSDF PW.4 (third-party software) 100% checklist (EVT-002).
- [ ] **10.s12.004.10** INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST enforced (EVT-022).
- [ ] **10.s12.004.11** Quarterly Legal review process documented + ADR-XXXX ratificado.
- [ ] **10.s12.004.12** Lockfile diff PR comment Action functioning em 100% Cargo.lock-changing PRs últimos 30d.

## 11. DoD

- [ ] `.github/workflows/cargo-audit.yml` + `cargo-deny.yml` + `dependabot-auto-merge.yml` committed.
- [ ] `deny.toml` committed em canonical schema.
- [ ] `.github/dependabot.yml` committed.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em PR + 100k green em nightly.
- [ ] E2E test contra staging green.
- [ ] Métricas emitidas (6 listadas §6.1.8).
- [ ] Trace spans 3 listadas.
- [ ] `docs/internal/dep-policy.md` published.
- [ ] ADR-XXXX (Dep policy) + ADR-XXXX (License allowlist) escritos.
- [ ] Code review (Compliance + AppSec + Security).
- [ ] PRR mini-sign-off.
- [ ] Cost regression gate green.
- [ ] Quarterly Legal review process documented.

## 12. Invariants Validated

### Mantidas

- nenhuma específica deste WI (INVs supply chain herdadas validadas em outros WIs S-12).

### Novas (introduzidas por S-12)

- **INV-SUPPLY-NO-YANKED** (HIGH — NEW): este WI implementa primary; cargo-deny `yanked = "deny"` enforces; CI gate.
- **INV-SUPPLY-LICENSE-ALLOWLIST** (HIGH — NEW): este WI implementa primary; cargo-deny `licenses.allow` 7 OSI + `licenses.deny` GPL/AGPL/SSPL enforces; CI gate.

TLA+ alignment: não-aplicável (build-time policy enforcement).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| cargo-audit workflow | `.github/workflows/cargo-audit.yml` | YAML |
| cargo-deny workflow | `.github/workflows/cargo-deny.yml` | YAML |
| Dependabot auto-merge workflow | `.github/workflows/dependabot-auto-merge.yml` | YAML |
| deny.toml policy | `deny.toml` | TOML |
| Dependabot config | `.github/dependabot.yml` | YAML |
| Lockfile diff Action setup | (ref Action existing) | YAML |
| Dep policy doc | `docs/internal/dep-policy.md` | Markdown |
| ADR-XXXX (Dep policy) | `specs/03_architecture/adrs/ADR-XXXX-dep-policy-cargo-audit-deny-dependabot.md` | Markdown |
| ADR-XXXX (License allowlist) | `specs/03_architecture/adrs/ADR-XXXX-license-allowlist-7-osi.md` | Markdown |
| ADR-XXXX (License review quarterly) | `specs/03_architecture/adrs/ADR-XXXX-license-review-quarterly.md` | Markdown |
| Property tests | `tests/prop_cargo_deny.rs` | Rust |
| Adversarial tests | `tests/adversarial_dep_policy.rs` | Rust |
| E2E integration | `tests/e2e_dependabot.rs` | Rust |

## 14. Quality Standards SOTA

- **14.s12.004.1** cargo-audit + cargo-deny pinned versions (audit 0.20.x; deny 0.16.x); bump via ADR.
- **14.s12.004.2** rustdoc 100% em test files.
- **14.s12.004.3** Property test coverage 100% deny.toml rules.
- **14.s12.004.4** Latência: cargo-audit + cargo-deny CI ≤ 1 min adicional p99.
- **14.s12.004.5** SAST clean em CoreLink workspace.
- **14.s12.004.6** Métricas RED.
- **14.s12.004.7** Runbook RB-FM-156 + RB-FM-157 referenciado WI-S12-007.
- **14.s12.004.8** Breaking changes em deny.toml schema = bump major + migration note.
- **14.s12.004.9** Memory: bounded em CI (cargo-audit ~50 MB).
- **14.s12.004.10** Cost regression gate.
- **14.s12.004.11** RUSTSEC triage SLA: ≤ 24h triage, ≤ 7d HIGH fix, ≤ 30d MEDIUM.
- **14.s12.004.12** Quarterly Legal review documented ADR.

## 15. Chaos Experiments

1. **GPL-3.0 dep injection**: red team adds dep with license=GPL-3.0; cargo-deny blocks PR; verify CI fail. **Validates INV-SUPPLY-LICENSE-ALLOWLIST**.

2. **Yanked dep transitive injection**: red team upgrades a transitive dep to yanked version; cargo-deny blocks. **Validates INV-SUPPLY-NO-YANKED**.

3. **Typosquat manual addition**: red team adds `corelink-fake` (typo); lockfile diff comment catches; manual review rejects.

4. **CRITICAL CVE injection**: synthetic RUSTSEC advisory; daily cron detects; SEV-2 alert fires; on-call paged.

5. **Auto-merge regression**: Dependabot minor patch with breaking change; CI fails (integration test); auto-merge blocked.

6. **Dependabot DoS**: simulate 50 PRs em 1h (mass advisory); rate limit caps em 10 open PRs.

7. **`[patch.crates-io]` sem ADR**: developer applies vendor patch sem ADR; pre-merge check fails.

8. **Unmaintained dep accumulation**: synthetic 5 unmaintained deps em Cargo.lock; quarterly review trigger fires.

9. **License audit drift simulation**: sample dep com SPDX=MIT mas actual code GPL fragments; quarterly Legal review catches.

10. **RUSTSEC mirror lag**: simulate RUSTSEC advisory delayed 48h; verify Dependency-Track (WI-S12-005) NVD/OSV sources catch first.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-12 ship gate é WI-S12-007; este WI passa por mini-PRR):

- [ ] All Gherkin green.
- [ ] Property + adversarial tests green.
- [ ] E2E staging green.
- [ ] cargo-audit zero HIGH/CRITICAL findings.
- [ ] cargo-deny zero violations.
- [ ] Dependabot weekly cadence functioning.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards em DASH-SUPPLY.
- [ ] ADR-XXXX (Dep policy) + ADR-XXXX (License allowlist) published.
- [ ] Compliance review (license allowlist + Legal cycle).
- [ ] AppSec review (RUSTSEC monitoring + Dependabot auto-merge policy).
- [ ] Security review (FM-156 + FM-157 mitigation).
- [ ] OWASP ASVS V14 + SSDF PW.4 100% pass.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | `.github/workflows/cargo-audit.yml` PR + cron | 1.5h |
| ST-002 | `.github/workflows/cargo-deny.yml` | 0.5h |
| ST-003 | `deny.toml` policy escrita + tests | 2h |
| ST-004 | `.github/dependabot.yml` config | 1h |
| ST-005 | `.github/workflows/dependabot-auto-merge.yml` | 1.5h |
| ST-006 | Lockfile diff PR comment Action setup | 1h |
| ST-007 | Métricas emit (6 metrics) + trace spans | 1.5h |
| ST-008 | Property tests 10k iter (4 props) | 2.5h |
| ST-009 | Adversarial regression tests (5 scenarios) | 2h |
| ST-010 | E2E integration test (Dependabot + CVE injection) | 2h |
| ST-011 | `docs/internal/dep-policy.md` | 1h |
| ST-012 | ADR-XXXX (Dep policy) redação | 1h |
| ST-013 | ADR-XXXX (License allowlist) redação | 1h |
| ST-014 | ADR-XXXX (License review quarterly) redação | 0.5h |
| ST-015 | Compliance + AppSec + Security review feedback | 2h |
| ST-016 | PRR mini-sign-off | 0.5h |

**Total Optimistic**: ~21.5h. **PERT** (O=8h, M=12h, P=20h per spec contract): **12.7h**.

## 18. Dependencies

### Hard blockers

- Cargo workspace structure baseline (S-01 SEALED).

### Soft blockers

- WI-S12-005 (Dependency-Track) — DT integration ideal mas não bloqueante; staging stub OK durante S-12.

### Outbound

- WI-S12-005 (Dependency-Track) — DT consume cargo-audit findings.
- WI-S12-007 (PRR ship gate).

## 19. Effort PERT

O: 8h, M: 12h, P: 20h → PERT **12.7h** (per spec contract §12).

## 20. Time-boxing

**16h hard limit**. If exceeded → escalation: split em sub-WI (cargo-audit + cargo-deny vs Dependabot).

## 21. Observability

6 métricas + 3 trace spans listadas. Logs structured JSON.

Dashboard widget DASH-SUPPLY:
- cargo-audit findings trend 30d (per severity).
- cargo-deny violations trend 30d (per rule).
- Dependabot PR auto-merge ratio.
- Yanked deps count gauge (target = 0).
- Total deps count gauge.

## 22. Cost Analysis

- cargo-audit + cargo-deny CI: ~1 min × $0.008/min × 50 PRs/mês = $0.40/mês.
- Daily cron: ~1 min × 30 = $0.24/mês.
- Dependabot infra: free (GitHub-hosted).
- **Total custo direto S-12 WI-004**: ~$0.64/mês = $8/yr. Negligível.

## 23. API Contract

Configuration files (deny.toml + dependabot.yml) são policy declarative; semver stable post v1.0 (deny.toml schema upgrade via ADR).

Erro mapping:
- cargo-audit findings: stdout JSON parsed (cargo-audit native output).
- cargo-deny violations: stdout (cargo-deny native output).
- Dependabot PR status: GitHub API.

## 24. Post-mortem Hooks

- CRITICAL CVE em production sustained > 7d → SEV-2 + post-mortem (root cause RUSTSEC delay or triage gap).
- Yanked dep introduced via auto-merge regression → SEV-3 + post-mortem + cargo-deny rule strengthen.
- License audit miss (GPL leak) → CRITICAL + Legal + remediation timeline ≤ 30 dias.
- Vendored patch sem ADR mergeado → SEV-3 + retroactive ADR + Security review.
- Auto-merge minor breaking change → SEV-3 + revert ≤ 24h + post-mortem.
- Dependabot PR DoS sustained > 4h → SEV-3 + ops post-mortem.

## 25. Rollback / Recovery

- Workflow rollback: revert PR.
- deny.toml rollback: revert PR (causes CI re-run on main).
- Dependabot auto-merge regression: manual revert via `git revert <sha>`; CI re-run.
- RTO: ≤ 30 min.
- RPO: 0 (config files versioned em git).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: cargo-deny sources allowlist previne typosquat.
- **Tampering**: cargo-audit detects RUSTSEC advisories ≤ 24h.
- **Repudiation**: CI run history em GitHub Actions = forensic trail.
- **Info disclosure**: dep list em Cargo.lock public OSS substrate.
- **DoS**: Dependabot rate limit 10 open PRs.
- **Elevation of privilege**: auto-merge requires CI green (não bypass).

**LINDDUN delta**:
- **Linkability**: dep list publicly known.
- **Identifiability**: maintainer identity em Cargo.toml (intentional).
- **Non-repudiation**: CI log trail.
- **Detectability**: dep CVEs publicly known.
- **Disclosure**: vendored patches require ADR + Security review.
- **Unawareness**: customer-facing supply chain documentation.
- **Non-compliance**: SOC 2 CC7.1 + NIST SSDF PW.4 satisfied.

## 27. Knowledge Transfer

- `docs/internal/dep-policy.md` overview.
- ADR-XXXX (Dep policy) + ADR-XXXX (License allowlist).
- Workshop interno (1h) Compliance + AppSec.
- Quarterly Legal review process documented.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | RUSTSEC mirror lag > 24h | M | H | MEDIUM | M | LOW | Dependency-Track NVD/OSV redundant sources (WI-S12-005) |
| R-002 | License audit miss (GPL leak) | M | M | HIGH (legal) | M | LOW | cargo-deny exhaustive allowlist + quarterly Legal review |
| R-003 | Dep maintainer compromise (FM-156) | L | H | CRITICAL | M | LOW | cargo-audit + DT + auto-merge minor bounded blast radius + revert ≤ 24h |
| R-004 | Typosquat (FM-157) | L | H | HIGH | M | LOW | cargo-deny sources crates.io only + lockfile diff PR comment |
| R-005 | Auto-merge minor breaking change | M | M | MEDIUM | M | LOW | CI green required + integration test + revert ≤ 24h |
| R-006 | Dependabot DoS (50+ PRs/h) | L | L | LOW | L | LOW | Rate limit 10 open PRs |
| R-007 | Yanked dep auto-merge bypass | L | M | HIGH | L | LOW | INV-SUPPLY-NO-YANKED + cargo-deny CI gate |
| R-008 | Vendored patch sem ADR | M | M | MEDIUM | M | LOW | Pre-merge check + Security review mandatory |
| R-009 | cargo-audit/deny tooling regression em version bump | L | H | HIGH | M | LOW | Pin versions + ADR antes bump |
| R-010 | Quarterly Legal review missed | L | M | MEDIUM | L | LOW | Calendar reminder + ADR cadence |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect review policy + ADRs outline.
2. **Compliance (D+1)**: Compliance Officer review license allowlist + Legal cycle.
3. **AppSec (D+1)**: AppSec review RUSTSEC monitoring + Dependabot auto-merge.
4. **Security (D+1)**: Security review FM-156 + FM-157 mitigation.
5. **Code (D+2)**: peer review (folded Engineer + Architect).
6. **PRR mini (D+2)**: Architect sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — FM-156 + FM-157 mitigation review_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-12 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — license allowlist + Legal review cadence_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — RUSTSEC monitoring + Dependabot auto-merge policy_ | _pending_ | _pending_ |

> Crypto SME (não cripto-load-bearing aqui) folds into Architect. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S12-004 (cycle 11.S12.0). |
| 1.1.0 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | SEALED: all artifacts committed (cargo-audit.yml + cargo-deny.yml + dependabot-auto-merge.yml + lockfile-diff.yml + deny.toml + dependabot.yml + corelink-supply-chain-policy crate with 3 test files + docs/internal/dep-policy.md + ADR-S12-045/046/047). |

## 32. Anti-patterns evitados

- ❌ Skip cargo-audit em PR ("nightly only").
- ❌ License allowlist mais permissivo sem ADR.
- ❌ Yanked dep "warn but allow".
- ❌ Sources allow git URLs sem commit hash.
- ❌ Auto-merge major version updates.
- ❌ Auto-merge security CRITICAL sem CI green.
- ❌ `[patch.crates-io]` sem ADR + Security review.
- ❌ Skip Dependabot.
- ❌ Lockfile diff PR review optional.
- ❌ Quarterly Legal review skipped.
- ❌ RUSTSEC waiver sem ADR + 90d sunset.
- ❌ cargo-audit + cargo-deny tooling unpinned.

---

**Fim WI-S12-004.** Próximo: WI-S12-005 (Dependency-Track self-host + CVE alerts webhook).
