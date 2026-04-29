---
id: "WI-S12-007"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
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
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s12", "supply-chain", "rb-fm-156", "rb-fm-157", "security-walkthrough", "prr", "ship-gate", "high-risk"]
---

# WI-S12-007 — RB-FM-156 (dep maintainer malicioso) + RB-FM-157 (typosquatting) Dry-Runs + Security Walkthrough + PRR S-12 Ship Gate (11 Sign-offs Canonical)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-12](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S12-007 |
| Título | S-12 ship gate — RB-FM-156 (dep maintainer malicioso → SolarWinds-style scenario) dry-run em staging com Security Lead + SRE; RB-FM-157 (typosquatting) dry-run; Security walkthrough sessão 2h adversarial review; PRR doc S-12 com 11 sign-offs canonical (Owner + Final Approver + Architect com Crypto SME specialization mandatory + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance + Privacy + AppSec); evidence pack: SLSA L3 attestation 100% releases últimos 30d + SBOM CycloneDX 1.5+ + Cosign verify chaos test green + cargo-audit zero HIGH/CRITICAL + cargo-deny verde + Dependency-Track CVE alerts ≤ 15 min sustained 30d + reproducible build 2-runner diff ≤ 5% + ADR-0015 ratificado |
| Sprint | S-12 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controles supply chain — bypass = blast radius global; ship gate é última defesa pré-prod) |

## 1. Intent

WI-S12-001..006 implementam features. **Este WI prova que o sistema supply chain completo funciona sob adversarial scrutiny + production-equivalent load + compliance scrutiny**. É o ship gate do S-12; **bloqueia merge para main de qualquer S-13+ até SEALED**.

Deliverables 4-fold:

1. **RB-FM-156 dry-run** (dep maintainer malicioso → SolarWinds-style):
   - Runbook: `specs/05_runbooks/RB-FM-156.md`.
   - Scenario: legitimate dep (e.g., `serde@1.0.X`) maintainer credentials stolen; malicious update published em crates.io; Dependabot opens auto-merge PR; CI gates engage (cargo-audit detects RUSTSEC ≤ 24h; Dependency-Track flags CRITICAL CVSS; auto-merge blocked).
   - Validations:
     - cargo-audit detects RUSTSEC advisory dentro 24h pós-publish.
     - Dependency-Track webhook fires alert SEV-2 ≤ 15 min.
     - Slack #supply-chain-cve-alerts notification arrived.
     - PagerDuty SEV-2 incident triggered.
     - Auto-merge PR blocked via CI fail.
     - Investigation steps em runbook executable.
   - Output: `specs/_audits/2026-XX-XX-rb-fm-156-dry-run.md` com timeline + drift findings + runbook updates committed.
   - Automated harness: `scripts/rb_fm_156_dry_run.rs` (Rust binary; reuse pattern de S-02 RB-FM-253).

2. **RB-FM-157 dry-run** (typosquatting):
   - Runbook: `specs/05_runbooks/RB-FM-157.md`.
   - Scenario: attacker publishes `corelink-fake` (typo de `corelink-server`) em crates.io com legitimate-looking metadata; manual `cargo add corelink-fake` simulated em PR.
   - Validations:
     - Lockfile diff PR comment Action emits warning.
     - cargo-deny `unknown-registry` blocks (only crates.io allowed; mas typosquat IS em crates.io — diff catches via review).
     - PR review process catches: CODEOWNERS + manual review + cargo-crev opcional review.
     - SBOM ingestion DT detects new component; CVE matching N/A (typosquat ≠ CVE) but anomaly em DT review queue.
   - Output: `specs/_audits/2026-XX-XX-rb-fm-157-dry-run.md` + runbook updates.

3. **Security walkthrough** (1 sessão 2h adversarial review):
   - Pentester team: Security Lead + AppSec + 1 external advisor opcional.
   - Scope: full S-12 supply chain surface — SLSA L3 + SBOM + Cosign + cargo-audit/deny + Dependabot + DT + reproducible builds.
   - Tools: manual review + provenance forge attempt + Cosign verify bypass attempt + DT injection + Dependabot replay.
   - Output: `specs/_audits/2026-XX-XX-security-walkthrough-s12.md` com findings classified P0/P1/P2; remediation plan; sign-off.
   - Cycle: every 6 months for major sprints; ad-hoc for HIGH_RISK changes.

4. **PRR doc S-12** em `specs/04_sprints/S12/PRR-S12.md`:
   - 11 sign-offs canonical documented (per framework §33.5.4.3 + ADR-0034).
   - Evidence pack:
     - SLSA L3 attestation 100% releases últimos 30d (verifiable via `rekor-cli search`).
     - SBOM CycloneDX 1.5+ + NTIA + RFC 3161 TSA + DT ingestion verde.
     - Cosign verify chaos test "deploy unsigned" + "deploy Rekor missing" green.
     - cargo-audit zero HIGH/CRITICAL findings sustained 30d.
     - cargo-deny verde policy em CI; license allowlist enforced; 0 yanked deps.
     - Dependabot weekly grouped PRs functioning; auto-merge minor patches working.
     - Dependency-Track CVE alerts ≤ 15 min p99 sustained 30d staging.
     - Reproducible build 2-runner diff ≤ 5% bytes em release builds; ADR-0015 ratificado.
     - 3 novas INVs (INV-SUPPLY-PROVENANCE-IN-REKOR, INV-SUPPLY-NO-YANKED, INV-SUPPLY-LICENSE-ALLOWLIST) ratificadas em registry.
     - RB-FM-156 + RB-FM-157 dry-runs executados; runbook updates committed.
     - Security walkthrough sessão executada; P0=0; P1 100% remediated.
   - Promotion gate decision: `APPROVED` | `CONDITIONALLY_APPROVED` (com waivers) | `REJECTED`.
   - SLOs validated: SLO-SUPPLY-CVE-DETECTION ≤ 15 min p99; SLO-SUPPLY-DEPLOY-VERIFY-LATENCY ≤ 5s p99 sustained 30d staging.
   - Final gate releases all S-12 WIs to SEALED state.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

S-12 é o **maior salto de superfície de segurança supply chain** em CoreLink: introduz controles cripto-load-bearing (SLSA L3 + Cosign + Rekor + reproducible builds) substituindo baseline ad-hoc. Cada um dos 6 WIs anteriores tem PRR mini-checklist; **WI-S12-007 é o gate cumulativo**: valida que o sistema completo composto funciona sob:

1. **Adversarial scenarios materializing FM-156 + FM-157** (RB dry-runs): SolarWinds-class scenarios validated em staging; runbook drift identified pre-production; on-call confidence built.

2. **Security walkthrough** (real human + tools 2h focused session): automated tests catch known patterns; walkthrough catches novel scenarios. Provenance forge attempts, Cosign bypass attempts, DT injection, Dependabot replay validated.

3. **Compliance scrutiny**: SOC 2 CC6.7 (signed deploy) + CC7.1 (vulnerability detection) + CC8.1 (system change management) + EO 14028 (SBOM mandatory) + NIST SP 800-218 (SSDF) + OWASP ASVS V14 + V11.1. PRR doc + walkthrough report doubles as compliance evidence.

4. **Operational readiness**: RB-FM-156 + RB-FM-157 dry-runs validate runbook accuracy + alert paths + dashboard panels; drift identified pre-production. Without dry-runs, production incident = panic + drift.

**Bugs catastróficos que este WI deve catch**:

- **Compound bug**: WI-S12-001 alone OK + WI-S12-003 alone OK; **integrated em deploy chain**, edge case (e.g., Rekor lookup latency + Fulcio rotation race) triggers verifier false reject.
- **Audit chain integrity break em concurrent emit**: WI-S12-003 audit emit em outbox; mass deploy alerts from WI-S12-005 stress chain hash deterministic computation.
- **DSR pseudonymization gap em supply chain logs**: SBOM ingestion logs contain dep maintainer email (PII); DSR worker (S-11) DELETE user not aware of supply chain logs; raw PII leak post-erasure.
- **Cross-WI integration drift**: WI-S12-002 SBOM PURL ecosystem mismatch breaks WI-S12-005 DT CVE matching.

**Atacante adversarial scenarios validated em walkthrough**:

- **Provenance forge via fork**: pentester stages release em fork; verify customer-side CLI rejects via builder_id mismatch.
- **Cosign signature bypass via stolen CF API token**: pentester captures token; tries direct `wrangler deploy`; verify CF IAM scoping blocks.
- **DT instance compromise**: pentester gains read-only access to DT; tries inject fake alerts; verify HMAC + admin auth + reconciliation catches.
- **Dependabot replay**: pentester replays Dependabot PR on closed/reverted branch; verify GitHub deduplication.
- **SBOM tampering pós-publish**: pentester swaps SBOM em mirror; verify SLSA provenance attestation chain detect via material hash mismatch.

**Operational adversarial scenarios** (RB-FM-156 + RB-FM-157 dry-runs):

- **RB-FM-156**: legitimate dep maintainer compromise scenario; expected behavior validated em staging.
- **RB-FM-157**: typosquatting scenario; expected detection validated.
- Drift assessment: runbook commands accurate? Dashboard panel visible? Alert fires correctly? On-call escalation works?

**Risk justification HIGH_RISK**:

- **FF-HR-005**: cumulative WI; supply chain correctness sustained ou breach inevitable.
- **Reversibility**: ship gate failure detected pre-deploy = correctable; post-deploy = catastrophic (customer trust permanently lost).

11 sign-offs canonical **mandatory** (sprint contract S-12 §14; Crypto SME folds into Architect role per framework §33.5.4.3 + ADR-0034); cripto-touching WIs receive specialization review within Architect sign-off.

## 3. Customer Impact & Journey

**Persona 1 — Customer adopting CoreLink (post-S-12 GA)**:
- Documentation: "Supply chain posture: SLSA L3 + SBOM CycloneDX 1.5+ + Cosign keyless OIDC + Rekor inclusion + cargo-audit/deny + Dependabot + Dependency-Track + reproducible builds 2-runner".
- PRR doc é evidence-grade artifact: customers can request via NDA.
- Security walkthrough report (sanitized) shareable em sales conversations.

**Persona 2 — Compliance auditor (SOC 2 Type II + ISO 27001)**:
- Audit query: S-12 WIs SEALED state; PRR doc 11 sign-offs canonical documented.
- Walkthrough report: known-good auditor signal.
- SOC 2 CC6.7 + CC7.1 + CC8.1 satisfied via evidence pack.

**Persona 3 — Internal SRE on-call**:
- RB-FM-156 + RB-FM-157 dry-runs committed → on-call confidence.
- Dashboard panels validated → alerts wire correctly.
- Mock incident post-mortem em training.

**SLA addendum**:
- S-12 ship gate enforces SLOs: SLO-SUPPLY-CVE-DETECTION ≤ 15 min p99; SLO-SUPPLY-DEPLOY-VERIFY-LATENCY ≤ 5s p99 sustained.
- Security walkthrough cycle: every 6 months for major sprints; ad-hoc for HIGH_RISK changes.
- DR test (DT) cycle: quarterly.
- Quarterly Legal review (license allowlist).

## 4. Capability Mapping

- All CAP-SUPPLY-* (validates whole supply chain domain).
- **CAP-COMPLIANCE-001** (SOC 2 evidence package) — IMPLEMENTA partial (PRR doc + walkthrough report).
- Trace: `_spec_contract.md §6 (Definition of Done)` + `framework §33.5.4.3 (HIGH_RISK 11 sign-offs canonical)` + `slo_catalog.md SLO-SUPPLY-CVE-DETECTION + SLO-SUPPLY-DEPLOY-VERIFY-LATENCY`.

## 5. Tipo

Sprint ship gate; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **RB-FM-156 dry-run em staging**:
   - **Runbook**: `specs/05_runbooks/RB-FM-156.md` (escrita aqui se ainda não existe).
   - **Scenario**: simulated dep maintainer compromise — synthetic RUSTSEC advisory injection + Dependabot PR auto-open.
   - **Validations**:
     - cargo-audit (WI-S12-004) detects within 24h.
     - Dependency-Track (WI-S12-005) fires alert SEV-2 ≤ 15 min.
     - Slack + Email + PagerDuty channels alert delivered.
     - Auto-merge PR blocked via CI fail (cargo-audit + cargo-deny gate).
     - Runbook commands executable; output captured.
     - On-call paged via PagerDuty test channel; ack within 5min.
     - Dashboard panel `DASH-SUPPLY > Supply Chain CVE Alerts` visualizes.
   - **Output**: `specs/_audits/2026-XX-XX-rb-fm-156-dry-run.md` com timeline + drift findings + runbook updates committed.
   - **Automated harness**: `scripts/rb_fm_156_dry_run.rs` (Rust binary, não bash).

2. **RB-FM-157 dry-run em staging**:
   - **Runbook**: `specs/05_runbooks/RB-FM-157.md`.
   - **Scenario**: typosquat published em crates.io (synthetic; dev env crate publish); manual `cargo add corelink-fake` simulated em PR.
   - **Validations**:
     - Lockfile diff PR comment Action emits warning.
     - PR review process catches: CODEOWNERS + manual review.
     - SBOM ingestion DT detects new component; anomaly em DT review queue.
     - cargo-deny não-blocks (typosquat IS em crates.io); manual review é defesa.
   - **Output**: `specs/_audits/2026-XX-XX-rb-fm-157-dry-run.md` + runbook updates.
   - **Automated harness**: `scripts/rb_fm_157_dry_run.rs`.

3. **Security walkthrough sessão 2h adversarial review**:
   - **Pentester team**: Security Lead + AppSec + 1 external advisor opcional (rotated por sprint).
   - **Scope** (S-12 surface):
     - SLSA L3 attestation: provenance forge attempts via fork; Rekor inclusion proof tampering; in-toto schema drift.
     - SBOM: tampering pós-publish; NTIA placeholder injection; PURL confusion; TSA replay.
     - Cosign + CF deploy verify: unsigned deploy attempt; Rekor missing; identity confusion; replay; CF API IAM bypass.
     - cargo-audit + cargo-deny + Dependabot: GPL leak; yanked auto-merge; typosquat; vendored patch sem ADR.
     - Dependency-Track: HMAC bypass; alert flood; DT compromise; PURL confusion.
     - Reproducible builds: compromised builder; non-determinism regression; rustc upgrade.
   - **Tools**: manual review + scripted attempts + Burp Suite (if web exposed); custom forge harnesses.
   - **Output**: `specs/_audits/2026-XX-XX-security-walkthrough-s12.md` com:
     - Executive summary.
     - Findings classified P0 (blocker) / P1 (must-fix-sprint) / P2 (next sprint).
     - Remediation plan + ETA.
     - Sign-off: Security Lead + AppSec.
   - **Cycle**: every 6 months for major sprints; ad-hoc for HIGH_RISK changes em S-13+.

4. **PRR doc S-12** em `specs/04_sprints/S12/PRR-S12.md`:
   - **Mandatory 11 sign-offs canonical table** (vide §30).
   - **Evidence pack**:
     - SLSA L3 attestation: `rekor-cli search` outputs for 100% releases últimos 30d.
     - SBOM CycloneDX: NTIA validation reports; DT ingestion logs; TSR tokens samples.
     - Cosign verify chaos test reports.
     - cargo-audit findings logs (zero HIGH/CRITICAL).
     - cargo-deny policy verification logs.
     - Dependabot PR auto-merge ratio + breakdown.
     - DT alert delivery latency p99 logs sustained 30d.
     - Reproducible build diff bytes per release; ADR-0015 ratification.
     - 3 INVs registry entries.
     - RB-FM-156 + RB-FM-157 dry-run reports.
     - Security walkthrough report.
   - **Promotion gate decision**: `APPROVED | CONDITIONALLY_APPROVED | REJECTED`.
   - **CONDITIONALLY_APPROVED waivers**: list of accepted residual risks com justification + ADR + review cadence.
   - **SLOs validated**: SLO-SUPPLY-CVE-DETECTION ≤ 15 min p99; SLO-SUPPLY-DEPLOY-VERIFY-LATENCY ≤ 5s p99 sustained 30d.

5. **Adversarial test summary aggregation report**:
   - Aggregate report linking todos os adversarial tests em S-12 WIs:
     - WI-S12-001: 5 CVE-class regressions (forge fork, Rekor tampered, Fulcio expired, schema drift, alg=none).
     - WI-S12-002: 5 scenarios (tampered, NTIA placeholder, DT exhausted, TSA replay, PURL confusion).
     - WI-S12-003: 5 CVE-class (unsigned, Rekor missing, identity confusion, replay, audit fail-CLOSED).
     - WI-S12-004: 5 scenarios (GPL leak, yanked auto-merge, typosquat, unmaintained, vendor patch sem ADR).
     - WI-S12-005: 5 scenarios (HMAC bypass, alert flood, DT outage, Slack outage, PagerDuty outage).
     - WI-S12-006: 5 scenarios (compromised builder, regression, build.rs lint, CPU heterogeneity, rustc upgrade).
   - **Output**: `specs/_audits/2026-XX-XX-adversarial-summary-s12.md` com 30+ scenarios documented; 100% mitigated.

6. **OWASP ASVS V14 + V11.1 + SSDF + EO 14028 checklist**:
   - V14 (Configuration): build pipeline + CI gates 100% pass.
   - V11.1 (Business Logic): supply chain controls 100% pass.
   - SSDF PS.1 (cripto integrity): satisfied.
   - SSDF PW.4 (third-party software): satisfied.
   - EO 14028 SBOM mandatory: satisfied.
   - Output: `specs/04_sprints/S12/asvs-v14-v11.1-ssdf-eo14028-checklist.md`.

7. **Métricas validation**:
   - All métricas listed em individual WIs (~12 metrics across S-12) emitting em staging com expected ranges; dashboard DASH-SUPPLY validated.

8. **All 6 prior S-12 WIs SEALED state precondition**.

### 6.2 Out-of-scope (deferred)

- **External (third-party) supply chain audit**: defer S-20 GA hardening (engagement com Trail of Bits / Chainguard / NCC Group).
- **Continuous fuzzing platform** (OSS-Fuzz integration): pós-GA Q1.
- **Bug bounty program**: pós-GA Q1.
- **Customer-facing supply chain dashboard**: pós-GA enterprise.
- **SLSA Level 4** (hermetic verifier + two-party review): pós-GA Q3.
- **DT v4.12+ upgrade**: pós-S-12 via ADR.

## 7. Anti-Scope

- ❌ Skip RB-FM-156 dry-run (mandatory ship gate).
- ❌ Skip RB-FM-157 dry-run (mandatory ship gate).
- ❌ Skip security walkthrough cycle (mandatory 6-month for HIGH_RISK).
- ❌ APPROVED PRR sem todos 11 sign-offs canonical (compliance requirement).
- ❌ Production deploy sem RB-FM-156 + RB-FM-157 dry-runs (operational readiness).
- ❌ Bash scripts > 30 lines (Rust binary preferred).
- ❌ Single pentester (rotation mandatory; bias mitigation).
- ❌ Walkthrough scope creep (time-boxed 2h focused).
- ❌ Skip OWASP ASVS + SSDF + EO 14028 checklist.
- ❌ Skip métricas validation (~12 metrics em DASH-SUPPLY).
- ❌ Waivers acumulando sem expiry.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: S-12 ship gate — RB-FM-156 + RB-FM-157 + security walkthrough + PRR

  Scenario: RB-FM-156 dry-run staged
    Given simulated dep maintainer compromise scenario em staging
    When dry-run executes
    Then cargo-audit detects RUSTSEC ≤ 24h
    And Dependency-Track alert SEV-2 fires ≤ 15 min
    And Slack + Email + PagerDuty channels alert delivered
    And Dependabot auto-merge PR blocked via CI fail
    And on-call paged via PagerDuty test channel
    And runbook commands executable; output captured
    And drift findings documented em report
    And runbook updates committed em PR

  Scenario: RB-FM-157 dry-run staged
    Given simulated typosquat publish em dev crates.io clone
    When dry-run executes (manual cargo add)
    Then lockfile diff PR comment Action emits warning
    And CODEOWNERS + manual review catches
    And SBOM ingestion DT detects new component
    And anomaly em DT review queue
    And drift findings documented + runbook updates committed

  Scenario: Security walkthrough complete
    Given 2h sessão Security Lead + AppSec + external advisor (opcional)
    Given full S-12 surface scope
    When walkthrough runs
    Then findings classified P0/P1/P2
    And specs/_audits/security-walkthrough-s12.md committed
    And Security Lead + AppSec sign-off

  Scenario: P0 findings BLOCK ship gate
    Given walkthrough output has P0 findings
    When PRR review proceeds
    Then promotion gate = REJECTED until P0 remediated
    And remediation iteration before SEALED state

  Scenario: P1 findings remediated em sprint window
    Given walkthrough output has P1 findings (3 items)
    When sprint window allows
    Then 100% P1 remediated em fix branch
    And re-tested em smaller walkthrough re-run
    And PRR proceeds APPROVED

  Scenario: PRR S-12 11 sign-offs canonical documented
    Given PRR-S12.md created
    When 11 reviewers sign
    Then sign-off table populated com names + dates + status=approved
    And promotion gate = APPROVED

  Scenario: SLOs validated em PRR
    Given SLO-SUPPLY-CVE-DETECTION ≤ 15 min p99
    Given SLO-SUPPLY-DEPLOY-VERIFY-LATENCY ≤ 5s p99
    When 30d staging measurement
    Then sustained ≤ 15 min p99 / ≤ 5s p99
    And SLO error budget within tolerance

  Scenario: All 12+ S-12 métricas emitting em staging
    Given all WI-S12-001..006 deployed em staging
    When workload simulator runs
    Then all expected métricas appear em dashboard DASH-SUPPLY
    And ranges em expected baselines
    And alerts wire correctly

  Scenario: Adversarial test summary aggregated
    Given individual adversarial tests em WI-S12-001..006
    When summary report aggregated
    Then 30+ adversarial scenarios documented
    And 100% mitigation rate sustained
    And report committed em specs/_audits/adversarial-summary-s12.md

  Scenario: Evidence pack complete
    Given PRR-S12.md drafted
    When evidence pack assembled
    Then SLSA L3 attestation samples (Rekor URLs) listed
    And SBOM samples (CycloneDX 1.5+) listed
    And Cosign chaos test reports linked
    And cargo-audit + cargo-deny logs linked
    And DT alert delivery logs linked
    And reproducible build diff bytes logs linked
    And 3 INVs registry entries verified
    And RB dry-run reports linked
    And walkthrough report linked

  Scenario: OWASP ASVS V14 + V11.1 + SSDF + EO 14028 checklist
    Given checklist scoped to S-12 surface
    When self-checklist executed
    Then 100% items pass
    And report committed em specs/04_sprints/S12/asvs-v14-v11.1-ssdf-eo14028-checklist.md
```

## 9. Design Decisions

### 9.1 Why RB-FM-156 + RB-FM-157 dry-runs (não só docs)

- Runbook drift accumulates without enforced validation.
- Dry-run validates: alert paths, dashboard panels, on-call escalation, runbook commands.
- S-02 lessons (RB-FM-253): manual runbook drift problematic; automated dry-run essential.

### 9.2 Why security walkthrough internal (não external em S-12)

- External pentest (Trail of Bits, Chainguard, NCC Group) cycle 12+ weeks lead time + ~$50-100k cost.
- S-12 internal walkthrough:
  - Faster (2h focused session).
  - Cheaper.
  - Covers known-class issues.
- External defer S-20 GA hardening (full third-party validation pre-public launch).

### 9.3 Why 11 sign-offs canonical (per framework §33.5.4.3 + ADR-0034)

- Sprint contract S-12 §14 = **11 sign-offs canonical** = Owner + Final Approver + Architect + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance + Privacy + AppSec advisor.
- Crypto SME (cripto-load-bearing review for WI-001 SLSA + WI-003 Cosign + WI-005 HMAC) folds into **Architect role** as specialization (precedent: S-01..S-11 SEAL ceremonies; ADR-0034 solo-tier waiver).
- Compliance + Privacy continue mandatory dado SOC 2 + EO 14028 posture.
- Adversarial reviewer folds into AppSec; peer reviewers folded into Engineer + Architect.

### 9.4 Why automated dry-runs em Rust (não manual bash)

- S-02 lessons: manual runbook drift accumulates over time without enforced validation.
- Automated dry-run:
  - Quarterly cadence reusable.
  - Drift detection bound (script changes vs runbook text).
  - Rust binary type-safe (vs bash error-prone).

### 9.5 Why CONDITIONALLY_APPROVED gate option

- Sometimes walkthrough finds P1 mas remediation requires next-sprint work.
- PRR proceeds com waivers documented:
  - Specific risk acknowledged.
  - Mitigating controls compensating.
  - ADR documents waiver scope + expiry.
  - Review cadence: 1 sprint.
- Rejection of all waivers = sprint blocks unnecessarily.

### 9.6 Why 6-month walkthrough cycle (não annual)

- Supply chain surface mutations frequent (S-12 → S-13 → ...).
- Annual cycle = 6 sprints unaudited surface. 6-month = balance cost vs surface drift.

### 9.7 Why mandatory 11 sign-offs (não fewer)

- HIGH_RISK lane SLA per framework.
- Each role brings distinct lens:
  - Owner / Final Approver: ultimate accountability.
  - Architect (Crypto SME): cripto-load-bearing review.
  - Security Lead: threat model + adversarial.
  - SRE Lead: operational readiness + chaos.
  - Engineer: implementation correctness.
  - QA Lead: test strategy.
  - Product: customer impact.
  - Compliance: SOC 2 + EO 14028.
  - Privacy: LGPD + GDPR.
  - AppSec: dep + workflow security.

### 9.8 ADR potencial?

- Não. Patterns reused (sprint ship gate canonical pattern em S-01..S-11). No novel architecture decision.

## 10. Completeness Criteria SOTA

- [ ] **10.s12.007.1** RB-FM-156 dry-run report committed; runbook updates committed; SLO validated (alert ≤ 15 min) (EVT-017).
- [ ] **10.s12.007.2** RB-FM-157 dry-run report committed; runbook updates committed (EVT-017).
- [ ] **10.s12.007.3** Security walkthrough report committed; P0=0; P1 remediated 100% (EVT-040).
- [ ] **10.s12.007.4** PRR-S12.md 11 sign-offs canonical documented (EVT-031).
- [ ] **10.s12.007.5** SLO-SUPPLY-CVE-DETECTION ≤ 15 min p99 sustained 30d staging (EVT-021).
- [ ] **10.s12.007.6** SLO-SUPPLY-DEPLOY-VERIFY-LATENCY ≤ 5s p99 sustained 30d staging (EVT-021).
- [ ] **10.s12.007.7** All 12+ S-12 métricas emitting + dashboards DASH-SUPPLY validated (EVT-013).
- [ ] **10.s12.007.8** Adversarial test summary aggregated em report (EVT-040).
- [ ] **10.s12.007.9** OWASP ASVS V14 + V11.1 + SSDF PS.1/PW.4 + EO 14028 100% checklist pass (EVT-002).
- [ ] **10.s12.007.10** All 6 WIs (WI-S12-001..006) SEALED state precondition.
- [ ] **10.s12.007.11** 3 novas INVs (INV-SUPPLY-PROVENANCE-IN-REKOR, INV-SUPPLY-NO-YANKED, INV-SUPPLY-LICENSE-ALLOWLIST) ratificadas em registry; CI gates ativos.
- [ ] **10.s12.007.12** Cost regression gate: full S-12 supply chain CI ≤ 8 min adicional + infra ≤ $50/mês.

## 11. DoD

- [ ] RB-FM-156 dry-run committed (report + automation script + runbook updates).
- [ ] RB-FM-157 dry-run committed.
- [ ] Security walkthrough sessão executed + report committed.
- [ ] PRR-S12.md committed com 11 sign-offs canonical.
- [ ] Adversarial test summary report committed.
- [ ] OWASP ASVS + SSDF + EO 14028 checklist em sprint folder.
- [ ] All métricas emitting em staging (DASH-SUPPLY validated).
- [ ] WIs S-12-001..006 SEALED state.
- [ ] Sprint S-12 closed; release notes committed.
- [ ] Quarterly walkthrough + DR cadence documented.

## 12. Invariants Validated

- **INV-SUPPLY-SIGNED-DEPLOY** (HIGH): chaos test "deploy unsigned" green (WI-S12-003).
- **INV-SUPPLY-SBOM-PRESENT** (HIGH): release sem SBOM blocked validated (WI-S12-002).
- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH — NEW): Rekor inclusion enforced (WI-S12-001 + WI-S12-003).
- **INV-SUPPLY-NO-YANKED** (HIGH — NEW): cargo-deny CI gate validated (WI-S12-004).
- **INV-SUPPLY-LICENSE-ALLOWLIST** (HIGH — NEW): cargo-deny enforces (WI-S12-004).
- All S-12 controls cumulatively validated.

TLA+ alignment: não-aplicável (build-time + deploy-time controles, não runtime state machine).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| RB-FM-156 runbook | `specs/05_runbooks/RB-FM-156.md` | Markdown |
| RB-FM-156 dry-run automation | `scripts/rb_fm_156_dry_run.rs` | Rust binary |
| RB-FM-156 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-156-dry-run.md` | Markdown |
| RB-FM-157 runbook | `specs/05_runbooks/RB-FM-157.md` | Markdown |
| RB-FM-157 dry-run automation | `scripts/rb_fm_157_dry_run.rs` | Rust binary |
| RB-FM-157 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-157-dry-run.md` | Markdown |
| Security walkthrough report | `specs/_audits/2026-XX-XX-security-walkthrough-s12.md` | Markdown |
| PRR doc S-12 | `specs/04_sprints/S12/PRR-S12.md` | Markdown |
| Adversarial test summary | `specs/_audits/2026-XX-XX-adversarial-summary-s12.md` | Markdown |
| OWASP ASVS + SSDF + EO 14028 checklist | `specs/04_sprints/S12/asvs-v14-v11.1-ssdf-eo14028-checklist.md` | Markdown |
| Release notes S-12 | `specs/04_sprints/S12/RELEASE_NOTES.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s12.007.1** RB-FM-156 + RB-FM-157 dry-runs automated em Rust (não bash > 30 lines).
- **14.s12.007.2** Security walkthrough report standard format (executive summary + findings + remediation + sign-off).
- **14.s12.007.3** Test coverage WIs S-12-001..006 ≥ 90% aggregated.
- **14.s12.007.4** PRR doc 11 sign-offs canonical documented; non-fictional gates.
- **14.s12.007.5** SAST: cargo-audit + cargo-deny clean across S-12 crates.
- **14.s12.007.6** SLO-SUPPLY-CVE-DETECTION ≤ 15 min p99 sustained 30d.
- **14.s12.007.7** SLO-SUPPLY-DEPLOY-VERIFY-LATENCY ≤ 5s p99 sustained 30d.
- **14.s12.007.8** Runbook RB-FM-156 + RB-FM-157 dry-runs automated; reusable quarterly.
- **14.s12.007.9** Adversarial summary aggregates 30+ scenarios.
- **14.s12.007.10** Memory bounded em dry-run scripts.
- **14.s12.007.11** Cost regression gate: full S-12 supply chain CI ≤ 8 min adicional; infra ≤ $50/mês.
- **14.s12.007.12** OWASP ASVS V14 + V11.1 100% pass.

## 15. Chaos Experiments

1. **RB-FM-156 dry-run validation**: full operational dry-run; identify drift; commit updates.

2. **RB-FM-157 dry-run validation**: full operational dry-run typosquat scenario.

3. **Security walkthrough realistic adversary simulation**: 2h focused session.

4. **Compound bug discovery**: integrated test stress (SLSA + Cosign + SBOM + DT + Dependabot + reproducible) — assert no novel emergent failures.

5. **Cross-WI integration drift**: synthetic patch breaks WI-S12-002 SBOM PURL → verify WI-S12-005 DT CVE matching catches via integration test.

6. **SLO violation injection**: synthetic CVE alert delay > 15 min sustained; verify alert fires + on-call paged + remediation runs.

7. **Cost regression**: synthetic 10× workload; verify cost gates per-op limits hold.

8. **Production parity validation em staging**: full S-12 deployed em staging com real CF + Neon + ghcr.io; load test + chaos.

9. **Walkthrough finding remediation cycle**: synthetic P1 finding; verify remediation flow → re-test → SEAL.

10. **DR test validation (DT instance)**: simulate Postgres corruption; restore via PITR; verify ≤ 1h recovery (cycle quarterly).

## 16. PRR (este WI emite o PRR doc)

PRR HIGH_RISK 11 sign-offs canonical **mandatory**:

- [ ] All Gherkin green.
- [ ] RB-FM-156 + RB-FM-157 dry-runs committed.
- [ ] Security walkthrough P0=0; P1 100% remediated.
- [ ] SLOs sustained 30d.
- [ ] OWASP ASVS V14 + V11.1 + SSDF + EO 14028 100%.
- [ ] All 6 WIs SEALED.
- [ ] 3 novas INVs ratificadas.
- [ ] 11 sign-offs canonical documented.
- [ ] Cost regression gate green.
- [ ] All 12+ métricas validated em DASH-SUPPLY.

Promotion gate decisions:
- **APPROVED**: all criteria met; sprint SEALED; merge unblocked.
- **CONDITIONALLY_APPROVED**: criteria met; specific waivers documented com expiry + ADR.
- **REJECTED**: criteria not met; remediation cycle.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | RB-FM-156 runbook escrita | 2h |
| ST-002 | RB-FM-156 dry-run automation script (Rust binary) | 3h |
| ST-003 | RB-FM-156 dry-run execution + report | 2h |
| ST-004 | RB-FM-157 runbook escrita | 1.5h |
| ST-005 | RB-FM-157 dry-run automation script | 2h |
| ST-006 | RB-FM-157 dry-run execution + report | 1.5h |
| ST-007 | Security walkthrough scoping | 1h |
| ST-008 | Security walkthrough execution (2h sessão) | 2h |
| ST-009 | Security walkthrough report write-up + remediation tracking | 4h |
| ST-010 | PRR-S12.md drafting + evidence pack assembly | 5h |
| ST-011 | Adversarial test summary aggregation | 3h |
| ST-012 | OWASP ASVS V14 + V11.1 + SSDF + EO 14028 self-checklist | 4h |
| ST-013 | Métricas + dashboards DASH-SUPPLY validation | 2h |
| ST-014 | 11 sign-off coordination | 4h |
| ST-015 | Sprint S-12 release notes + retrospective | 2h |
| ST-016 | Sign-off coordination + waivers (if CONDITIONALLY_APPROVED) | 2h |

**Total Optimistic**: ~41h. **PERT** (O=8h, M=12h, P=18h per spec contract): **12.3h** (concentrated; pentester parallel work). Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- WI-S12-001..006 all SEALED.
- Staging environment operational.
- Security Lead + AppSec availability for walkthrough.
- PRR reviewers available (11 roles canonical).

### Soft blockers

- S-09 audit chain processor (consumer; este WI tests emit; full chain validation S-09).

### Outbound

- Sprint S-13 (admin plane; S-12 must SEAL antes S-13 starts).

## 19. Effort PERT

O: 8h, M: 12h, P: 18h → PERT **12.3h** (per spec contract §12; owner critical path).

## 20. Time-boxing

**16h hard limit owner**. Walkthrough **2h dedicated**. Se exceder: split em sub-WI (RB dry-runs vs walkthrough + PRR).

## 21. Observability

Dashboard em PRR:
- RB dry-run pass/fail status.
- Walkthrough findings burndown (P0/P1/P2 chart).
- SLO sustained ratio.
- 11 sign-off canonical status.
- All métricas validation status.
- 3 INVs CI gates active status.

## 22. Cost Analysis

**Direct cost**:
- Walkthrough engagement: internal (Security Lead + AppSec time) + external advisor opcional ~$2k/engagement × 2/yr = $4k/yr.
- RB dry-run automation: ~$10/mês CI compute = $120/yr.
- DR test quarterly: ~$50 per test × 4 = $200/yr.
- **Total ship gate**: ~$4.3k/yr.

**Indirect cost**:
- 0 production supply chain incidents prevented = priceless.

## 23. API Contract

Não-aplicável (este WI é gate; não introduz API).

## 24. Post-mortem Hooks

- PRR APPROVED mas production incident em primeira semana → CRITICAL post-mortem + 5-Why.
- Walkthrough cycle missed (>6 months) → SEV-2 + compliance gap.
- RB-FM-156 ou RB-FM-157 dry-run drift sustained > 30 days → SEV-2 (operational readiness gap).
- DR test failed → CRITICAL + Security incident.
- SLO-SUPPLY-CVE-DETECTION violated > 1h sustained → SEV-2 + post-mortem.

## 25. Rollback / Recovery

PRR REJECTED → sprint reverts to DRAFT; remediation cycle. RTO ≤ 1 sprint.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: walkthrough validates origin/identity checks across all 6 WIs.
- **Tampering**: SLSA + reproducible builds + Cosign deploy gate validated.
- **Repudiation**: PRR sign-off table + walkthrough report + dry-run report = forensic-grade trail.
- **Information disclosure**: SBOM expõe deps (intentional; standard practice OSS); no PII em supply chain artifacts.
- **DoS**: RB-FM-156 + RB-FM-157 dry-runs validate alert paths + on-call escalation.
- **Elevation of privilege**: deploy gate + IAM scoping + Cosign keyless OIDC validated.

**LINDDUN delta**:
- **Linkability**: supply chain artifacts publicly known (Rekor + ghcr.io); intentional.
- **Identifiability**: builder identity = service principal; maintainer email em SBOM (PII).
- **Non-repudiation**: cripto property intentional.
- **Detectability**: walkthrough verifies anomaly emit + alerts wire.
- **Disclosure**: vendored patches require ADR.
- **Unawareness**: Trust Center customer-facing supply chain documentation.
- **Non-compliance**: SOC 2 CC6.7 + CC7.1 + CC8.1 + EO 14028 + NIST SSDF + LGPD Art. 38 + GDPR Art. 32 + OWASP ASVS V14 + V11.1 satisfied.

## 27. Knowledge Transfer

- **Tech talk** (2h): "S-12 Supply Chain System Whole-Stack Review + Walkthrough Findings".
- **Doc** `docs/internal/s12-walkthrough-summary.md` — sanitized findings (customer-shareable post-NDA).
- **Doc** `docs/internal/s12-runbook-validation.md` — RB-FM-156 + RB-FM-157 dry-run pattern reusable.
- **PRR-S12 release party** post-SEAL com Architect + Security + AppSec + Compliance + on-call.
- **Onboarding test** (10 questions): SLSA L3, Cosign keyless, Rekor inclusion, NTIA, cargo-deny, DT alerts, reproducible builds, etc.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | RB dry-run flaky em CI | M | H | LOW | M | LOW | Retry policy + threshold tuning |
| R-002 | Walkthrough finds P0 em final week (sprint slip) | L | M | HIGH | M | LOW | Walkthrough scoping early; remediation buffer |
| R-003 | PRR sign-off staffing gap (Tier-1 reviewers unavailable) | M | M | HIGH | M | LOW | 2-week notice; alternate reviewers documented |
| R-004 | RB-FM-156 dry-run impacts production (test isolation) | L | L | HIGH | L | LOW | Run em staging only; production-isolated infra |
| R-005 | RB-FM-157 typosquat publish em real crates.io (unintended) | L | L | LOW | L | LOW | Use dev crates.io clone OR `cargo-publish-dry-run` |
| R-006 | Walkthrough scope creep | M | L | LOW | L | LOW | Time-boxed 2h; explicit out-of-scope list |
| R-007 | OWASP ASVS checklist incomplete | L | M | MEDIUM | L | LOW | Checklist template; 2-eng review |
| R-008 | External pentester quality variance | M | M | MEDIUM | M | LOW | Rotation policy; reference checks |
| R-009 | Customer expectation drift (post-PRR breach) | L | L | CRITICAL | L | LOW | Continuous monitoring + 6-month walkthrough cycle |
| R-010 | CONDITIONALLY_APPROVED waivers accumulate (tech debt) | M | M | MEDIUM | M | LOW | Waiver expiry mandatory; quarterly review |
| R-011 | Cost regression em walkthrough cycle (>$5k/yr) | L | L | LOW | L | LOW | Internal + 1 external advisor opcional; budget gate |
| R-012 | DR test failure quarterly | L | L | HIGH | L | LOW | Postgres PITR + alert SEV-2 + remediation runbook |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Security Lead review walkthrough scope + RB design.
2. **Walkthrough scoping (D+1)**: Security Lead + AppSec align scope + tools.
3. **RB dry-run review (D+3)**: SRE + Security validate RB scripts + reports.
4. **Walkthrough mid-check (D+5)**: progress report; adjust scope if needed.
5. **Walkthrough final report (D+7)**: pentester + Security Lead sign-off.
6. **PRR draft (D+10)**: Owner drafts; circulates pra Tier-1 reviewers.
7. **PRR final (D+13)**: 11 sign-offs canonical collected; gate decision; SEAL.

## 30. Sign-off (HIGH_RISK 11 canonical — sprint ship gate)

Este WI emite o PRR; sign-off do PRR-S12.md doc é o sign-off final S-12 sprint.

**Staffing reality (per ADR-0034 solo-tier)**:

Pré-PRR mandatory check: confirmed canonical reviewers vs pending. Sprint S-12 pode-se SEAL apenas com **11 sign-offs canonical** completos. Tier-1 staffing gap = sprint cannot SEAL until staffed OR explicit waiver com expiry + ADR.

| Status atual (2026-04-29) | Roles |
|---|---|
| **Confirmed (3)** | Owner (Gustavo Schneiter); Final Approver (Gustavo Schneiter); Product (Gustavo Schneiter — solo founder dual-hat) |
| **Pending Tier-1 hire/contract (8 specialized canonical roles)** | Architect (com Crypto SME specialization mandatory para WI-001 SLSA + WI-003 Cosign + WI-005 HMAC), Security Lead, SRE Lead, Engineer (S-12 lead), QA Lead, Compliance Officer, Privacy Officer, AppSec advisor |
| **Total pending** | 8 of 11 canonical (per framework §33.5.4.3 + ADR-0034; Crypto SME folds into Architect specialization; peer reviewers folded into Engineer + Architect) |

**Escalation plan se PRR sem todos 11 canonical staffed**:
1. **Option A — solo-tier waiver**: Owner + Final Approver assume múltiplos dual-hats com explicit ADR (`ADR-0034-solo-tier-prr-waiver.md`). Documenta accepted residual risk + post-staffing review cadence (every 2 sprints até all 11 canonical staffed). **Apenas válido para Tier solo/team launch**; enterprise tier requires full staffing.
2. **Option B — defer SEAL**: spec final permanece DRAFT até staffing closes; implementation paused.
3. **Option C — external advisor pool**: contract per-engagement Tier-1 reviewers (Compliance, Privacy, Crypto SME, AppSec) via consulting marketplaces — typically 12-week lead; budget $30-100k para full S-12 PRR.

**Recommended path (current state)**: Option A com ADR-0034 + Option C parallel staffing track for S-13 onwards.

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — sprint coherence + canonical sources alignment_ (com Crypto SME specialization mandatory: cripto-touching WIs cross-validation (SLSA + Cosign + HMAC) + adversarial review (mandatory pair-program)) | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — walkthrough report + supply chain controls validation_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — RB dry-runs + DR test + operational readiness_ | _pending_ | _pending_ |
| 6 | Engineer (S-12 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — SOC 2 CC6.7/CC7.1/CC8.1 + EO 14028 + NIST SSDF ship gate evidence_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; LGPD Art. 38 supply chain logs PII review_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — adversarial review + walkthrough + Dependabot policy_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (cycle 1 codex SEAL alignment per framework §33.5.4.3 + ADR-0034 solo-tier waiver). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S12-007 (cycle 11.S12.0); SOTA full ship gate (RB-FM-156 + RB-FM-157 dry-runs + Security walkthrough + PRR doc 11 sign-offs canonical + adversarial summary 30+ scenarios + OWASP ASVS V14 + V11.1 + SSDF + EO 14028 + 12-row risk + 10 chaos). |

## 32. Anti-patterns evitados

- ❌ Skip RB-FM-156 dry-run (mandatory ship gate).
- ❌ Skip RB-FM-157 dry-run (mandatory ship gate).
- ❌ Skip security walkthrough cycle (mandatory 6-month).
- ❌ Approve PRR sem todos 11 sign-offs canonical.
- ❌ Production deploy sem RB dry-runs.
- ❌ Bash automation > 30 lines (Rust binary).
- ❌ Single pentester sem rotation.
- ❌ Waivers acumulando sem expiry.
- ❌ Walkthrough scope creep (time-boxed).
- ❌ Skip OWASP ASVS V14 + V11.1 + SSDF + EO 14028 checklist.
- ❌ External walkthrough only (interno + externo cycle).
- ❌ Skip métricas validation em DASH-SUPPLY.
- ❌ Skip 3 novas INVs ratificação.

---

**Fim WI-S12-007.** **S-12 sprint full WI spec completo (7/7 WIs SOTA HIGH_RISK).** Próximo lote: 12.S13.0 (S-13 — Admin Plane SLO + dual-approval destructive ops).
