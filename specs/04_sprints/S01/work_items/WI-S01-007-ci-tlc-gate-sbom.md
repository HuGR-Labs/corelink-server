---
id: "WI-S01-007"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-01"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "COMPLIANCE-MATRIX"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s01", "ci", "tla", "tlc", "sbom", "supply-chain"]
---

# WI-S01-007 — CI Workflow: TLC Gate + SBOM CycloneDX 1.5+ + cargo-audit + fuzz nightly

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-007 |
| Título | CI workflow TLC gate + SBOM + cargo-audit + fuzz |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CI gates são security control enforcement; CTRL-FORMAL-001 + CTRL-SUPPLY-001..005) |

## 1. Intent

GitHub Actions workflow `.github/workflows/cas_foundation.yml` que enforce CI gates obrigatórios para CAS foundation:

```yaml
name: CAS Foundation CI
on: [pull_request, push]
jobs:
  - tla-check: TLC model checker on 4 specs (tenant_isolation, cas_integrity, gc_correctness, audit_immutability)
  - rust-build: cargo build --release --target wasm32-unknown-unknown
  - rust-test: cargo test --release (unit + integration + property tests 10k)
  - clippy: cargo clippy -D warnings
  - sast: semgrep SAST rules
  - cargo-audit: dependency CVE check
  - cargo-deny: license + advisories policy
  - fuzz-nightly: cargo-fuzz 1h (parsers + digest decode)
  - sbom: cyclonedx-cli generate SBOM CycloneDX 1.5+
  - cosign: sigstore sign release artifact
```

Cada job é **CI gate hard**: PR fail se any job red. SBOM publishedo como release asset.

## 2. Narrative (HIGH_RISK ≥ 300)

CI é **a última linha de defesa** entre code review e production. Falhas:

1. **TLC gate skipped**: PR introduz mudança em modelo distribuído (state machines em corelink-tenant-path, R2 path, refcount); TLA+ specs não re-validated; invariant violation slipped silently.
2. **Cargo-audit skipped**: dep CVE descoberto upstream; CoreLink ship vulnerable build; supply chain attack surface.
3. **SBOM ausente**: release sem SBOM = INV-SUPPLY-SBOM-PRESENT violation; auditor flag em SOC 2 review.
4. **Cosign sign skipped**: release não verificável; INV-SUPPLY-SIGNED-DEPLOY violation; CF deploy webhook reject (S-12 enforcement; soft pre-S-12).
5. **Fuzz coverage gap**: parser bugs slip (digest hex parse, REAPI proto deserialize); WASM panic em prod.

Mitigação:
- **TLC em CI**: `tlc -config tenant_isolation.cfg tenant_isolation.tla` em GitHub Actions; runtime ~5-15min; PR fail se model check falha.
- **cargo-audit nightly + per-PR**: detecta novel CVE em deps.
- **cargo-deny policy** (`deny.toml`): license allowlist + ban yanked + advisories deny.
- **cyclonedx-cli**: SBOM gen + validate NTIA minimum elements.
- **sigstore cosign**: keyless via OIDC GitHub Actions identity; Rekor inclusion proof.
- **cargo-fuzz**: 1h nightly per fuzz target; 100M iter target.

CI runtime budget:
- Per-PR: ~15-20 min (TLC 10min + Rust build 5min + tests 5min).
- Nightly: ~2h (fuzz 1h + extended property tests 5min + SBOM + sign).

**Risk justification HIGH_RISK:**
- **FF-HR-005**: CI gates são controle de segurança formal; bypass = security control failure.
- **Reversibility**: bug shipped without CI gate enforcement = downstream remediation costly.

## 3. Customer Impact & Journey

**JTBD (internal team):** "Como dev CoreLink, eu submeto PR e tenho confidence que CI catches: invariant violations (TLC), CVE em deps (cargo-audit), license violations (cargo-deny), SBOM missing, cosign signature missing, fuzz panics. Sem CI gate, esses problemas chegam em produção."

Indirect customer impact: trust + reliability + supply chain integrity.

## 4. Capability Mapping

Cross-cutting — supports all CAPs S-01 (CAP-CAS-001/002/003) via gate enforcement.
Trace: framework §33.5.4.1 HIGH_RISK matrix; security_model.md §6.4 CTRL-FORMAL-001 + §6.10 CTRL-SUPPLY-001..005.

## 5. Tipo

CI Infrastructure; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **GitHub Actions workflow** `.github/workflows/cas_foundation.yml`:
   - `tla-check` job: TLC model check 4 specs (vide §6.1.7 explicit bounds + JVM/timeout config).
   - `rust-build` job: cargo build --release --target wasm32-unknown-unknown (com sccache; vide §6.1.8 cache strategy).
   - `rust-test` job: unit + integration + property tests 10k.
   - `clippy-strict` job: `cargo clippy --all-targets -D warnings`.
   - `sast` job: semgrep + custom rules.
   - `cargo-audit` job: dependency CVE check (advisory-db nightly update).
   - `cargo-deny` job: license + sources + advisories policy enforcement.
   - `sbom` job: cyclonedx-cli generate + sbomqs NTIA validation (não apenas schema check).
   - `cosign-sign` job: sigstore keyless OIDC + Rekor inclusion (release branch only); fork-PR guard `if: github.event.pull_request.head.repo.full_name == github.repository`.

2. **Nightly workflow** `.github/workflows/nightly.yml`:
   - `fuzz` job: cargo-fuzz 1h per target **em paralelo** (matrix runner; 3 jobs concurrent).
   - `proptest-extended` job: 100k iter (vs 10k em PR).
   - `tlc-extended` job: TLC com larger bounds (5 tenants × 50 ops vs 3×10 em PR; vide §6.1.7 bounds table).

7. **TLC explicit state-space bounds** (P0 fix; resolve "gate é theatre" risco):

   Cada `.cfg` file commits explicit constants + timeout/coverage config:

   | Spec | PR bounds | Nightly bounds | TLC config |
   |---|---|---|---|
   | `tenant_isolation.cfg` (PR) | `MaxTenants = 3`, `MaxOps = 10`, `MaxDigests = 5` | `MaxTenants = 5`, `MaxOps = 50`, `MaxDigests = 20` | `-deadlock`, `-coverage 60`, `-workers 2`, `-checkpoint 0` (no checkpoint), `-fp 32` |
   | `cas_integrity.cfg` (PR) | `MaxBlobs = 5`, `MaxOps = 10` | `MaxBlobs = 20`, `MaxOps = 50` | idem |
   | `gc_correctness.cfg` (PR) | `MaxBlobs = 3`, `MaxRefs = 5`, `MaxOps = 8` | `MaxBlobs = 10`, `MaxRefs = 20`, `MaxOps = 30` | idem + `-difftrace` |
   | `audit_immutability.cfg` (PR) | `MaxEvents = 10`, `MaxChainLen = 20` | `MaxEvents = 50`, `MaxChainLen = 100` | idem |

   - **Hard timeout per spec** PR: 8 min (`timeout 480s java -jar tla2tools.jar ...`); nightly: 30 min (`timeout 1800s ...`).
   - **State-space size estimation** documented em `specs/tla/README.md`: explicit-state cardinality cap aprox 10^7 PR / 10^9 nightly em GitHub runners (16 GB RAM, 4 vCPU `ubuntu-latest`).
   - **TLC tools pinned**: `tla2tools.jar` URL `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar` + SHA-256 verified pre-exec; verification script `scripts/verify_tla_tools.sh`.
   - **Failure mode policy**: timeout = job FAIL (não PASS silent); coverage gauge 100% expected (vide TLC `-coverage` output); deadlock flag mandatory (catches stuck states).

8. **CI cache strategy** (resolve "≤ 20min PR" credibility):
   - `actions/cache` em `~/.cargo/registry`, `~/.cargo/git`, `target/` keyed por `Cargo.lock` SHA.
   - `sccache` com `actions-rs/sccache@<pinned>` para Rust incremental builds.
   - Expected runtime: cold cache 28-32 min; warm cache 12-16 min. Documentar em workflow comments.

9. **Action pinning policy** (SLSA L3 alignment):
   - All third-party actions pinned por SHA-256 (não tag).
   - Auto-update via Renovate config `.github/renovate.json`; PR review required.
   - Examples: `actions/checkout@8e5e7e5ab8b370d6c329ec480221332ada57f0ab` (não `@v4`).

10. **Fork-PR secret protection**:
    - Jobs com secrets (cosign, SBOM publish, deploy) gated por `if: github.event.pull_request.head.repo.full_name == github.repository`.
    - External fork PRs run public-only jobs (TLC, build, test, clippy); cosign + deploy skipped com explicit "skipped: external fork" log.

3. **deny.toml** policy file:
   - License allowlist: MIT, Apache-2.0, BSD-2/3-Clause, ISC, MPL-2.0, Unicode-DFS-2016.
   - Ban GPL/AGPL/SSPL/Commons-Clause.
   - 0 yanked deps.
   - Sources: only crates.io (no git deps non-pinned).
   - Advisories: deny RUSTSEC-* unless waived ADR.

4. **cyclonedx-cli config** integrado em release pipeline + sbomqs NTIA validator pós-generation.

5. **Cosign keyless setup** via GitHub Actions OIDC; Fulcio cert (10min lived) per build; Rekor inclusion proof attached. **Outage policy explícito**: Sigstore offline → release blocked + ops escalation runbook RB-FM-SIGSTORE-OUTAGE; sem "grace period" silencioso (cert curto não pode-se cachear; honestidade vs theatre).

6. **Fuzz harnesses** em `crates/corelink-*/fuzz/fuzz_targets/`:
   - `digest_parse.rs` — parse hex digest from string.
   - `reapi_deserialize.rs` — REAPI proto bytes deserialization.
   - `tenant_path_decode.rs` — HMAC16 (base64url, 16 chars) prefix decode (canonical per remote_cache_product_profile.md §7.1).

### 6.2 Out-of-scope (deferred)

- **SLSA L3 provenance** completo: WI-S12-001 (S-12 supply chain hardening sprint).
- **Dependency-Track ingestion**: WI-S12-005.
- **Reproducible builds 2-runner**: WI-S12-006 (S-12).
- **Cosign verify gate em CF deploy**: WI-S12-003 (S-12).
- **GitHub Action SLSA generator**: deferred S-12.

## 7. Anti-Scope

- ❌ Build em CI Worker production deploy (separate workflow).
- ❌ Custom fuzzer (cargo-fuzz com libFuzzer default).
- ❌ Long-running TLC > 30min (use Apalache symbolic for that, pós-GA).
- ❌ Skip CI on PR via `[skip ci]` (anti-pattern; gate is mandatory).
- ❌ Manual SBOM (always auto-gen).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: CI Foundation gates

  Scenario: PR with TLA+ violation blocked
    Given PR introduces refcount race em corelink-worker
    And gc_correctness.tla model check would fail
    When CI runs
    Then tla-check job fails
    And PR cannot merge
    And reviewer sees TLC error trace

  Scenario: PR with new CVE dep blocked
    Given PR adds dep with known RUSTSEC advisory
    When cargo-audit runs
    Then advisory triggered
    And PR fails

  Scenario: PR with GPL dep blocked
    Given PR adds dep com license GPL-3.0
    When cargo-deny runs
    Then license check fails
    And PR cannot merge

  Scenario: SBOM auto-generated em release
    Given push to release branch
    When sbom job runs
    Then CycloneDX 1.5+ JSON published as release asset
    And NTIA minimum elements check passes

  Scenario: Cosign sign + Rekor inclusion
    Given release artifact built
    When cosign-sign job runs
    Then signature attached em OCI registry
    And Rekor inclusion proof verifiable
    And inclusion log entry visible em rekor.sigstore.dev

  Scenario: Nightly fuzz 1h per target
    Given nightly cron triggers
    When fuzz job runs each target for 1h
    Then 0 panics observed
    And metrics emit duration + iterations + crashes_total

  Scenario: Property test extended 100k
    Given nightly runs proptest com cases=100000
    When all property tests execute
    Then 0 failures
    And runtime ≤ 5 min total

  Scenario: clippy strict
    Given PR with `unwrap()` em lib code
    When cargo clippy -D warnings
    Then warning treated as error
    And PR fails
```

## 9. Design Decisions

### 9.1 Why GitHub Actions (vs CircleCI, Jenkins)

- Free tier sufficient pre-GA.
- Native sigstore keyless OIDC.
- SLSA generator integration trivial.
- Cloudflare Workers deploy via wrangler é GitHub Actions friendly.

### 9.2 Why TLC em CI (vs Apalache symbolic)

- TLC explicit-state suficient para small bounds (3-5 tenants × 10 ops).
- Apalache symbolic é mais rápido for larger bounds mas adds tooling complexity.
- Decision: TLC at GA; Apalache pós-GA Q1 se gap detected.

### 9.3 Why cosign keyless OIDC

- No long-lived signing keys to manage.
- Fulcio short-lived cert (10min); attacker cannot forge offline.
- Rekor public transparency log → tampering detectable.
- Standard SLSA L3 setup.

### 9.4 Why per-PR + nightly split

- Per-PR: fast feedback (≤ 20min); essentials only (TLC + tests + clippy + audit).
- Nightly: extensive (fuzz 1h, 100k proptest, TLC larger bounds).
- Trade-off: minor CVE introduced em PR vs nightly catch (12-24h delay); aceitável.

### 9.5 ADR potencial?

ADR-0014 (SBOM CycloneDX 1.5+) já documentou. Adicional: ADR para TLC vs Apalache choice — defer pós-GA.

## 10. Completeness Criteria SOTA

- [ ] **10.7.1** TLC gate enforced: PR com invariant violation blocked (EVT-022).
- [ ] **10.7.2** SBOM CycloneDX 1.5+ NTIA-compliant em cada release (EVT-010).
- [ ] **10.7.3** Cosign keyless sign + Rekor inclusion em release (EVT-011).
- [ ] **10.7.4** cargo-audit + cargo-deny policy enforced em PR (EVT-012).
- [ ] **10.7.5** Fuzz nightly 1h × 3 targets sustained 7d zero panics (EVT-008).
- [ ] **10.7.6** Property tests 100k nightly green (EVT-002).
- [ ] **10.7.7** Cost regression gate: CI runtime ≤ 20min PR / ≤ 2h nightly (§14.10).

## 11. DoD

- [ ] Workflows committed em `.github/workflows/`.
- [ ] deny.toml policy committed.
- [ ] Fuzz harnesses committed em `fuzz/fuzz_targets/`.
- [ ] All gates pass em PR de validação.
- [ ] SBOM artifact em release.
- [ ] Cosign signature + Rekor proof em release.
- [ ] Code review.
- [ ] PRR + Security lead sign-off.

## 12. Invariants Enforced

- INV-SUPPLY-SBOM-PRESENT (HIGH).
- INV-SUPPLY-SIGNED-DEPLOY (HIGH; pre-S-12 deploy verify).
- INV-SUPPLY-NO-YANKED (HIGH).
- INV-SUPPLY-LICENSE-ALLOWLIST (HIGH).

Validations enforced (não invariants per se):
- INV-TENANT-ISOLATION (CRITICAL) — TLC.
- INV-CAS-INTEGRITY (CRITICAL) — TLC.
- INV-GC-001/004 (CRITICAL) — TLC.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| PR workflow | `.github/workflows/cas_foundation.yml` | YAML |
| Nightly workflow | `.github/workflows/nightly.yml` | YAML |
| deny.toml policy | `deny.toml` | TOML |
| Fuzz harness — digest | `crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs` | Rust |
| Fuzz harness — REAPI | `crates/corelink-worker/fuzz/fuzz_targets/reapi_deserialize.rs` | Rust |
| Fuzz harness — path | `crates/corelink-tenant-path/fuzz/fuzz_targets/tenant_path_decode.rs` | Rust |

## 14. Quality Standards SOTA

- **14.7.1** Workflow YAMLs reviewed for security (no `pull_request_target` em untrusted).
- **14.7.2** Documentação inline + README badge.
- **14.7.3** Coverage: workflow itself covered by smoke tests (validation PR).
- **14.7.4** CI runtime ≤ 20min PR.
- **14.7.5** SAST: semgrep rules + clippy strict.
- **14.7.6** Métricas: GitHub Actions native + CI duration to Grafana (S-09 forward).
- **14.7.7** Runbook: nenhum runtime; CI failure debug é dev workflow.
- **14.7.8** Breaking workflow changes via PR + ADR.
- **14.7.9** Cache strategies para Rust deps + TLC tools.
- **14.7.10** Cost regression gate: CI minutes budget tracked.

## 15. Chaos Experiments

1. **Inject fake invariant violation em test branch**: verify TLC catches.
2. **Inject fake CVE dep**: verify cargo-audit catches.
3. **Inject GPL dep**: verify cargo-deny catches.

## 16. PRR

PRR + Security lead + AppSec.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | PR workflow scaffold + tla-check job | 2h |
| ST-002 | rust-build + rust-test jobs | 1.5h |
| ST-003 | clippy-strict + sast jobs | 1h |
| ST-004 | cargo-audit + cargo-deny jobs | 2h |
| ST-005 | deny.toml policy authoring | 1.5h |
| ST-006 | sbom job (cyclonedx-cli) | 2h |
| ST-007 | cosign-sign job (keyless OIDC + Rekor) | 3h |
| ST-008 | Nightly workflow scaffold | 1h |
| ST-009 | 3 fuzz harnesses | 4h |
| ST-010 | proptest-extended nightly | 1h |
| ST-011 | tlc-extended nightly | 1.5h |
| ST-012 | Validation PR (intentional invariant violation) | 1.5h |
| ST-013 | Documentation + README | 1h |
| ST-014 | PRR + Security walkthrough | 2h |

**Total**: ~25h Optimistic; PERT ~30h.

## 18. Dependencies

- WI-S01-001..006 SEALED (todos os crates produced; CI testa todos).
- TLA+ specs já existentes (Lote 5.13 / 7.1).
- GitHub repo configured + Actions enabled.

### Outbound

- WI-S12-001 (SLSA L3) extends este workflow para full SLSA L3.
- WI-S12-003 (cosign deploy verify) consume artifacts deste workflow.

## 19. Effort PERT

O: 22h, M: 25h, P: 45h → PERT 28.5h.

## 20. Time-boxing

32h hard limit; escalation ST-007 (cosign) > 5h → AppSec.

## 21. Observability

GitHub Actions emit metrics native; forward to Grafana (S-09 forward).

## 22. Cost Analysis

GitHub Actions free tier ≤ 2000 min/mês public repo; CoreLink consumption ~500 min/mês PR + ~600 min/mês nightly = ~1100 min/mês; well within free tier. Cosign/Sigstore/Rekor public free.

## 23. API Contract

CI workflows internal; semver não-applicable. SBOM format CycloneDX 1.5+ semver-locked.

## 24. Post-mortem Hooks

- CI gate bypassed via misconfiguration → CRITICAL post-mortem.
- Cosign signing failure em release → SEV-1 + immediate fix.
- TLC red sustained > 4h em main branch → CRITICAL (rollback).
- CVE descoberto via cargo-audit em prod release → SEV-2 + remediation.

## 25. Rollback / Recovery

CI workflow rollback via git revert; previous workflow version active immediately next PR/push.

## 26. Security & Privacy

STRIDE: tampering em workflow detectable via git history + signed commits.
LINDDUN: build artifacts não contêm PII.

## 27. Knowledge Transfer

Doc `docs/internal/ci-gates.md` — explica each gate's rationale + how to debug failures.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | TLC runtime > 30min em large bounds | M | M | LOW (CI delay) | M | LOW | Per-PR small bounds; nightly extended |
| R-002 | Cosign Fulcio/Sigstore outage blocks release | L | L | MEDIUM | L | LOW | Outage = release blocked (Fulcio certs 10min lived; nada para cachear). Runbook RB-FM-SIGSTORE-OUTAGE: escalation + comm; release queue até restore. Sem "grace period" silencioso. |
| R-003 | False positive cargo-audit (unrelated CVE) | M | L | LOW | L | LOW | RUSTSEC-* waiver via ADR if applicable |
| R-004 | GitHub Actions free tier exceeded | L | L | LOW | L | LOW | Pay tier ~$5/month if needed |
| R-005 | Workflow YAML injection | L | M | HIGH | L | LOW | No `pull_request_target` em untrusted; review |

## 29. Review Checkpoints

1. Design (D+0): Security + AppSec.
2. Code (D+3): peer + Security.
3. Validation PR (D+5): demonstrate gate behavior.

## 30. Sign-off (HIGH_RISK 11 canonical)

11 roles incl. Security + AppSec emphatic.

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.1.

## 32. Anti-patterns evitados

- ❌ `pull_request_target` em untrusted (privilege escalation).
- ❌ Skip CI via commit message (gate é mandatory).
- ❌ Custom fuzzer roll-own.
- ❌ Long-lived signing keys (cosign keyless instead).
- ❌ Manual SBOM gen (drift risk).
- ❌ TLC só em manual run (CI gate is automatic).

---

**Fim WI-S01-007.** **Sprint S-01 fully specified.** Próximo Lote 10.2: S-02 finish (5 WIs).
