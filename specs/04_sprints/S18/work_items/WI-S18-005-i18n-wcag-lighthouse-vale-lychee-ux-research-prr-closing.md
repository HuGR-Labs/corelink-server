---
id: "WI-S18-005"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-05-14"
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
  - "PRIVACY-MODEL"
tags: ["wi", "s18", "ship-gate", "i18n", "wcag-2.2-aa", "axe-core", "lighthouse", "vale", "lychee", "ux-research", "prr", "single-phase-seal", "low-risk"]
---

# WI-S18-005 — S-18 Closing Ship Gate: i18n Native Speaker Review en-US/pt-BR/es-419 (Matching S-11 + S-15 + S-16 Alignment per Lote 10.16; Legal Local Review for Legal Terms in pt-BR/es-419 per Spec Contract §15 row 6) + WCAG 2.2 AA axe-core CI Gate 0 Violations Sustained (per Quality Standard 14.s18.4) + Manual Screen Reader Test + Lighthouse CI Gate ≥ 95 (Performance + A11y + Best Practices + SEO) em 5 Routes (`/`, `/docs/getting-started`, `/docs/sdk/python`, `/security`, `/pricing` per Spec Contract §5.6 R-S18-13 + Quality Standard 14.s18.1) + Vale Tone Consistency Lint CI Gate em `apps/docs/.vale/` Style Guide (per Quality Standard 14.s18.3) + lychee Broken-link CI Gate PR Fail se Broken-link + Weekly External Link Verification Cadence (per Quality Standard 14.s18.4) + UX Research Session 5 Dev Sample Finds Answer ≤ 30s (per Quality Standard 14.s18.2 Diátaxis Discoverability Test) + Completes Getting Started ≤ 5 min (per Spec Contract §6 EVT-018) + PRR LOW_RISK 3 Sign-offs Canonical (Owner + Final Approver + Docs Lead) + Cross-functional Publish Gate Separate (Finance Pricing + Legal Terms + Privacy Officer Compliance + Security Lead Security per Spec Contract §6) — Esses Sign-offs Count Toward Separate Publish Gate (Não Main PRR per Spec Contract §10 Anti-scope; Non-skippable per Waiver Policy §19) + Single-phase SEAL D+10

> **doc_status:** DRAFT · **work_status:** READY · **lane:** LOW_RISK
> **Parent:** [S-18](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S18-005 |
| Título | S-18 closing ship gate — i18n native speaker + WCAG 2.2 AA + Lighthouse + Vale + lychee + UX research + PRR LOW_RISK + cross-functional publish gate separate + single-phase SEAL D+10. |
| Sprint | S-18 |
| Lane | LOW_RISK |
| Forcing factors | none (closing WI; LOW_RISK lane single-phase SEAL D+10; DoD §6 todos critérios instant-verifiable at sprint close — Lighthouse score em 5 routes + WCAG axe-core 0 violations + Vale lint green + lychee broken-link green + cross-functional sign-offs coletados + UX research 5 devs em ≤ 30s + pricing calculator 10 scenarios validated; 2-week sprint LOW_RISK sem post-sprint observation window required) |

## 1. Intent

WI-S18-001..004 implementam features. **Este WI prova que o sistema docs production-grade completo funciona sob production-equivalent observation period at sprint close + i18n native speaker review + WCAG 2.2 AA + Lighthouse ≥ 95 + Vale + lychee + UX research session 5 dev sample**. É o ship gate do S-18. **Gating canônico**: este WI fecha em **single-phase SEAL D+10** (LOW_RISK lane; DoD §6 todos critérios instant-verifiable at sprint close; 2-week sprint sem post-sprint observation window required):

- **Single-phase SEAL D+10** (Sprint Review): docs URL live + custom domain SSL + Diátaxis taxonomy + Algolia DocSearch + getting started 5-min quickstart + REAPI auto-gen CI gate + 4-language code examples + 4 SDK guides + cross-functional sign-offs coletados (Finance/Legal/Privacy Officer/Security lead per relevant page) + i18n native speaker review en-US/pt-BR/es-419 + WCAG 2.2 AA axe-core 0 violations + Lighthouse ≥ 95 em 5 routes + Vale lint green + lychee broken-link green + UX research 5 devs ≤ 30s + completes getting started ≤ 5 min + PRR LOW_RISK 3 sign-offs canonical + cross-functional publish gate separate.

Deliverables 7-fold:

1. **i18n native speaker review** per locale en-US/pt-BR/es-419 (matching S-11 + S-15 + S-16 alignment per Lote 10.16):
   - **en-US** (default): native speaker review (English baseline).
   - **pt-BR** (LGPD primary): native speaker review + Legal local review for legal terms (per spec contract §15 row 6).
   - **es-419** (Latin America Spanish): native speaker review + Legal local review for legal terms.
   - Translation quality verified per locale; missing translation = build fail (Quality Standard 14.s18.5 i18n discipline).

2. **WCAG 2.2 AA axe-core CI gate 0 violations sustained** + manual screen reader test:
   - GitHub Actions workflow `.github/workflows/docs-a11y.yml` runs axe-core 0 violations gate.
   - Manual screen reader test (NVDA Windows + VoiceOver macOS + JAWS Windows commercial test).
   - Per Quality Standard 14.s18.4 + spec contract §5.6 R-S18-14.

3. **Lighthouse CI gate ≥ 95** (Performance + A11y + Best Practices + SEO) em 5 routes:
   - `/` (homepage).
   - `/docs/getting-started` (getting started page; WI-S18-002).
   - `/docs/sdk/python` (Python SDK guide; WI-S18-003).
   - `/security` (security page; WI-S18-004).
   - `/pricing` (pricing page; WI-S18-004).
   - Per Quality Standard 14.s18.1 + spec contract §5.6 R-S18-13.

4. **Vale tone consistency lint CI gate** em `apps/docs/.vale/`:
   - Vale style guide em `apps/docs/.vale/styles/` + `.vale.ini` config.
   - Tone rules: voice (active vs passive), terminology consistency (Bazel cap B), forbidden words.
   - PR fails se Vale violations (Quality Standard 14.s18.3 + spec contract §5.6 R-S18-15).

5. **lychee broken-link CI gate** PR fail se broken-link + weekly external link verification:
   - GitHub Actions workflow `.github/workflows/docs-lychee.yml` PR runs (internal links only fast).
   - Scheduled weekly cron runs (external links full verification).
   - Per Quality Standard 14.s18.4 + spec contract §5.6 R-S18-16.

6. **UX research session 5 dev sample** (per spec contract §6 EVT-018 + Quality Standard 14.s18.2):
   - 5 external dev sample (workshop or async session).
   - Diátaxis discoverability test: finds answer ≤ 30s.
   - Getting started: completes ≤ 5 min.
   - Findings documented em `specs/_audits/2026-XX-XX-ux-research-s18.md`.

7. **PRR LOW_RISK 3 sign-offs canonical + cross-functional publish gate separate**:
   - PRR `specs/04_sprints/S18/PRR-S18.md` (front matter per `prr` schema: feature_wi + capabilities + prod_target_date + work_status NOT_STARTED → IN_REVIEW → APPROVED).
   - 3 sign-offs canonical: Owner + Final Approver + Docs lead.
   - Cross-functional publish gate separate (não main PRR per spec contract §10 anti-scope): Finance pricing + Legal terms + Privacy Officer compliance + Security lead security per relevant page (CF-1 + CF-2 + CF-3 from WI-S18-004).

```yaml
# File: .github/workflows/docs-ci.yml (consolidated CI gates)
name: Docs CI Gates (LOW_RISK closing ship gate)
on: [pull_request, push]
jobs:
  vale:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Vale tone consistency lint (Quality Standard 14.s18.3)
        uses: errata-ai/vale-action@v2
        with:
          version: 3.x
          fail_on_error: true

  lychee:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: lychee broken-link check (Quality Standard 14.s18.4)
        uses: lycheeverse/lychee-action@v2
        with:
          args: --no-progress --internal-only apps/docs/

  lighthouse:
    runs-on: ubuntu-latest
    needs: [pr-preview-deploy]  # Lote 10.18 codex P1 fix — depend on PR preview deployment job
    steps:
      - uses: actions/checkout@v4
      - name: Lighthouse CI ≥ 95 em 5 routes em **PR preview build** (Quality Standard 14.s18.1; Lote 10.18 codex P1 canonical fix)
        uses: treosh/lighthouse-ci-action@v11
        with:
          # Canonical Lote 10.18 codex P1 fix: Lighthouse runs against PR preview deployment
          # (CF Pages preview branch URL `${{ steps.cf-pages-deploy.outputs.url }}`), NOT live docs.corelink.dev.
          # Prior version targeted live URL = could miss regressions in change under review.
          # PR preview = CF Pages auto-creates preview deployment per PR; URL from CF Pages API.
          urls: |
            ${{ steps.cf-pages-deploy.outputs.url }}/
            ${{ steps.cf-pages-deploy.outputs.url }}/docs/getting-started
            ${{ steps.cf-pages-deploy.outputs.url }}/docs/sdk/python
            ${{ steps.cf-pages-deploy.outputs.url }}/security
            ${{ steps.cf-pages-deploy.outputs.url }}/pricing
          configPath: '.lighthouserc.json'
          uploadArtifacts: true
      # Secondary check on production (live URL) runs nightly to detect drift post-merge,
      # NOT as PR gate. Configured separately em `.github/workflows/lighthouse-nightly.yml`.

  axe-core:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: WCAG 2.2 AA axe-core 0 violations (Quality Standard 14.s18.4 + R-S18-14)
        uses: dequelabs/axe-core-action@v1
        with:
          fail_on_violations: true
```

## 2. Narrative

Single-phase SEAL D+10 canonical em LOW_RISK lane (vs STANDARD two-phase D+20/D+50 quando 30d sustained needed; vs HIGH_RISK two-phase sempre per framework §33.5). DoD §6 todos critérios instant-verifiable at sprint close: Lighthouse score em 5 routes + WCAG axe-core 0 violations + Vale lint green + lychee broken-link green + cross-functional sign-offs coletados + UX research 5 devs em ≤ 30s + pricing calculator 10 scenarios validated. 2-week sprint LOW_RISK sem post-sprint observation window required.

Closing ship gate consolida 7 deliverables canonical para S-20 GA gate: docs production-grade live em `docs.corelink.dev` + 5-dev UX research success + Lighthouse ≥ 95 sustained + cross-functional publish gate sign-offs (Finance/Legal/Privacy Officer/Security lead per relevant page).

PRR LOW_RISK 3 sign-offs canonical (Owner + Final Approver + Docs lead) + cross-functional publish gate separate (não main PRR per spec contract §10 anti-scope; non-skippable per Waiver policy §19). Cross-functional sign-offs (CF-1 pricing + CF-2 security + CF-3 compliance) collected em WI-S18-004; closing WI verifies + documenta em PRR.

CONDITIONALLY_APPROVED waivers (typical em LOW_RISK lane S-18 per spec contract §19):
- W-X — Native-speaker review of stub TODO-marked translations pending D+10 (D-day = first external user onboarding); severity LOW. 3-locale presence requirement (en-US + pt-BR + es-419) is satisfied and **never** waived (per spec contract §19 — non-waivable).
- 5-dev UX research passing → 3-dev sample com plan to expand + ADR (per spec contract §19 — waivable).

Note: Cross-functional review gate (Finance + Legal + Privacy + Security) em pages relevant + Lighthouse ≥ 95 + WCAG 2.2 AA + auto-gen REAPI reference + 3-locale i18n (en-US + pt-BR + es-419, Lote 10.18 P1 tightening) + SBOM canonical public download (S-12 SLSA L3 release artifact; NDA-gated path NOT permitted) são **non-waivable** per spec contract §19.

**Risk justification LOW_RISK lane (zero forcing factors)**:
- Closing WI; LOW_RISK lane single-phase SEAL D+10.
- DoD §6 todos critérios instant-verifiable at sprint close.
- 2-week sprint LOW_RISK sem post-sprint observation window required.
- Não introduz tenant data flow path novo.
- Não há cripto-load-bearing controles novos.

## 3. Customer Impact & Journey

**Persona — Developer Advocate / Customer Success**:
- i18n 3 locales en-US (default) + pt-BR (LGPD primary) + es-419 native speaker reviewed + Legal local review for legal terms.
- WCAG 2.2 AA axe-core CI 0 violations sustained + manual screen reader test (a11y regulatory baseline).
- Lighthouse ≥ 95 em 5 routes (UX baseline for dev tools); CI gate < 95 fails PR; perf review weekly.
- Vale tone consistency lint CI + lychee broken-link CI weekly external verification.

**Persona — External Developer / Build Engineer (prospect)**:
- UX research 5 dev sample completes ≤ 5 min getting started + finds answer ≤ 30s Diátaxis discoverability test.

**Persona — Procurement / Compliance auditor**:
- Cross-functional publish gate sign-offs (Finance/Legal/Privacy Officer/Security lead per relevant page) documented em PRR.

## 4. Capability Mapping

- **CAP-DOCS-007** (i18n + a11y) — IMPLEMENTA primary closing.
- **CAP-DOCS-008** (CI lint + broken-link check) — IMPLEMENTA primary closing.
- **CAP-DOCS-001..006** — VALIDATES via Lighthouse + WCAG + UX research.
- Trace: `_spec_contract.md §4` + `remote_cache_product_profile.md` + `privacy_model.md`.

## 5. Tipo

Closing ship gate WI; LOW_RISK lane.

## 6. Escopo

### 6.1 In-scope

1. **i18n native speaker review** per locale (en-US/pt-BR/es-419):
   - en-US default native speaker review (English baseline; reviewer Docs lead or external).
   - pt-BR LGPD primary native speaker review + Legal local review for legal terms (per spec contract §15 row 6).
   - es-419 Latin America Spanish native speaker review + Legal local review for legal terms.
   - Translation files `apps/docs/i18n/<locale>/code.json` + `apps/docs/i18n/<locale>/docusaurus-plugin-content-docs/current/**/*.md` reviewed per locale.
   - Missing translation = build fail (Quality Standard 14.s18.5 i18n discipline).

2. **WCAG 2.2 AA axe-core CI gate** + manual screen reader test:
   - `.github/workflows/docs-a11y.yml` runs axe-core 0 violations gate (per Quality Standard 14.s18.4).
   - Manual screen reader test (NVDA Windows + VoiceOver macOS + JAWS Windows commercial test) — 5 critical pages tested.
   - Per spec contract §5.6 R-S18-14 + spec contract §6 EVT-018 a11y.

3. **Lighthouse CI gate ≥ 95** em 5 routes:
   - `.github/workflows/docs-lighthouse.yml` runs Lighthouse CI gate ≥ 95.
   - 5 routes: `/`, `/docs/getting-started`, `/docs/sdk/python`, `/security`, `/pricing`.
   - 4 pillars: Performance + A11y + Best Practices + SEO.
   - Per Quality Standard 14.s18.1 + spec contract §5.6 R-S18-13.

4. **Vale tone consistency lint CI gate** em `apps/docs/.vale/`:
   - Vale style guide em `apps/docs/.vale/styles/CoreLink/` (custom rules) + `.vale.ini` config.
   - Tone rules: voice (active vs passive), terminology consistency (Bazel cap B; Buck2 capital B; CoreLink one word), forbidden words (don't use "simply" / "easy" / "just" — accessibility tone).
   - PR fails se Vale violations (Quality Standard 14.s18.3 + spec contract §5.6 R-S18-15).

5. **lychee broken-link CI gate**:
   - `.github/workflows/docs-lychee.yml` PR runs (internal links only fast).
   - Scheduled weekly cron runs (external links full verification).
   - PR fails se broken-link em PR diff (per Quality Standard 14.s18.4 + spec contract §5.6 R-S18-16).

6. **UX research session 5 dev sample**:
   - 5 external dev sample (workshop or async session).
   - Diátaxis discoverability test: finds answer ≤ 30s (per Quality Standard 14.s18.2).
   - Getting started: completes ≤ 5 min (per spec contract §6 EVT-018).
   - Findings documented em `specs/_audits/2026-XX-XX-ux-research-s18.md`.
   - Iteration based on findings (post-sprint cadence; not blocking ship).

7. **PRR LOW_RISK 3 sign-offs canonical + cross-functional publish gate separate**:
   - PRR `specs/04_sprints/S18/PRR-S18.md` em PRR schema (feature_wi + capabilities + prod_target_date + work_status NOT_STARTED → IN_REVIEW → APPROVED).
   - 3 sign-offs canonical (LOW_RISK): Owner + Final Approver + Docs lead.
   - Cross-functional publish gate separate (não main PRR; non-skippable per Waiver policy §19):
     - CF-1 `/pricing`: Finance + Legal sign-off.
     - CF-2 `/security`: Security lead + Privacy Officer sign-off.
     - CF-3 `/compliance`: Legal + Privacy Officer sign-off.
   - **Promotion gate decision**: `APPROVED` | `CONDITIONALLY_APPROVED` (com waivers + ADR + expiry) | `REJECTED`.
   - **CONDITIONALLY_APPROVED waivers** (typical em LOW_RISK S-18 per spec contract §19):
     - W-X — Native-speaker review of stub TODO-marked translations pending D+10 (D-day = first external user onboarding); severity LOW. Never waives the 3-locale presence requirement (non-waivable per spec contract §19).
     - 5-dev UX research passing → 3-dev sample com plan to expand + ADR (waivable per spec contract §19).
   - **Non-waivable items** (per spec contract §19 — confirmed):
     - 3-locale i18n presence (en-US + pt-BR + es-419) — Lote 10.18 P1 tightening; prior "3 → 2 locales" allowance removida.
     - SBOM canonical public download (S-12 SLSA L3 release artifact) — Lote 10.18 P1 tightening; prior "SBOM via support email com NDA" allowance removida; this WI links to S-12 release artifacts (no duplicate SBOM publication path).

8. **Evidence pack** committed em `specs/_audits/`:
   - i18n native speaker review report (`2026-XX-XX-i18n-native-speaker-review.md`).
   - WCAG 2.2 AA axe-core CI evidence (`2026-XX-XX-wcag-22-aa-evidence.md`).
   - Manual screen reader test report (`2026-XX-XX-screen-reader-test.md`).
   - Lighthouse score per route ≥ 95 evidence (`2026-XX-XX-lighthouse-evidence.md`).
   - Vale lint CI evidence (`2026-XX-XX-vale-lint-evidence.md`).
   - lychee broken-link CI evidence (`2026-XX-XX-lychee-evidence.md`).
   - UX research session 5 dev sample report (`2026-XX-XX-ux-research-s18.md`).
   - Cross-functional sign-off log (`2026-XX-XX-cross-functional-signoffs-s18.md`).

### 6.2 Out-of-scope (deferred)

- 30d post-sprint observation (LOW_RISK single-phase SEAL D+10; sem observation window required).
- Customer case studies pre-GA (S-20 marketing prep).
- Video tutorials (post-GA backlog).
- Blog (post-GA Q1).

## 7. Anti-Scope

- Skip i18n native speaker review (Quality Standard 14.s18.5 i18n discipline; Legal local review for legal terms).
- Skip WCAG 2.2 AA axe-core CI gate (a11y regulatory baseline; non-waivable per spec contract §19).
- Skip Lighthouse ≥ 95 CI gate (UX baseline for dev tools; non-waivable per spec contract §19).
- Skip Vale tone consistency lint CI (Quality Standard 14.s18.3).
- Skip lychee broken-link CI (Quality Standard 14.s18.4).
- Skip UX research session 5 dev sample (Quality Standard 14.s18.2 Diátaxis discoverability test).
- Skip cross-functional publish gate sign-offs (CRITICAL gap; non-skippable per Waiver policy §19).
- Skip auto-gen REAPI reference CTRL-DOC-AUTO-GEN (non-waivable per spec contract §19).
- PRR LOW_RISK aprovado sem cross-functional publish gate sign-offs (CRITICAL gap).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: S-18 closing ship gate single-phase SEAL D+10

  Scenario: i18n native speaker review per locale (en-US/pt-BR/es-419)
    Given apps/docs/i18n/<locale>/ translation files
    When native speaker review per locale
    Then en-US default reviewed
    And pt-BR LGPD primary reviewed + Legal local review for legal terms
    And es-419 Latin America reviewed + Legal local review for legal terms
    And missing translation = build fail (Quality Standard 14.s18.5)

  Scenario: WCAG 2.2 AA axe-core CI gate 0 violations
    Given .github/workflows/docs-a11y.yml configured
    When PR runs
    Then axe-core 0 violations (Quality Standard 14.s18.4)
    And PR fails se ≥ 1 violation

  Scenario: Manual screen reader test
    Given 5 critical pages tested (NVDA + VoiceOver + JAWS)
    When manual test runs
    Then all critical interactions accessible
    And report committed em specs/_audits/

  Scenario: Lighthouse CI gate ≥ 95 em 5 routes
    Given .github/workflows/docs-lighthouse.yml configured
    When PR runs
    Then 5 routes (/, /docs/getting-started, /docs/sdk/python, /security, /pricing)
    And 4 pillars (Performance + A11y + Best Practices + SEO) ≥ 95
    And PR fails se any pillar < 95

  Scenario: Vale tone consistency lint CI gate
    Given apps/docs/.vale/ style guide configured
    When PR runs Vale lint
    Then tone rules enforced (active voice + terminology + forbidden words)
    And PR fails se Vale violations

  Scenario: lychee broken-link CI gate (PR + weekly)
    Given .github/workflows/docs-lychee.yml configured
    When PR runs
    Then internal links verified zero broken
    And weekly cron runs external links full verification
    And PR fails se broken-link em PR diff

  Scenario: UX research session 5 dev sample
    Given 5 external dev sample (workshop or async session)
    When UX research executed
    Then Diátaxis discoverability test: finds answer ≤ 30s (Quality Standard 14.s18.2)
    And getting started: completes ≤ 5 min (per spec contract §6 EVT-018)
    And findings documented em specs/_audits/2026-XX-XX-ux-research-s18.md

  Scenario: PRR LOW_RISK 3 sign-offs canonical
    Given specs/04_sprints/S18/PRR-S18.md
    When PRR review
    Then 3 sign-offs canonical (Owner + Final Approver + Docs lead)
    And work_status: NOT_STARTED → IN_REVIEW → APPROVED

  Scenario: Cross-functional publish gate separate (non-skippable)
    Given pricing/security/compliance pages PR submitted
    When cross-functional review
    Then CF-1 /pricing: Finance + Legal sign-off
    And CF-2 /security: Security lead + Privacy Officer sign-off
    And CF-3 /compliance: Legal + Privacy Officer sign-off
    And non-skippable per Waiver policy §19
    And CRITICAL post-mortem trigger se merge inadvertent

  Scenario: Single-phase SEAL D+10 (LOW_RISK)
    Given DoD §6 todos critérios instant-verifiable at sprint close
    When sprint review D+10
    Then SEAL ceremony executed
    And PRR LOW_RISK aprovado + cross-functional publish gate sign-offs collected
    And evidence pack committed em specs/_audits/

  Scenario: CONDITIONALLY_APPROVED waiver (typical LOW_RISK)
    Given S-18 close attempts CONDITIONALLY_APPROVED com waiver
    When waiver attempted
    Then native-speaker review of stub TODO-marked translations pending D+10 waivable (LOW severity; never waives 3-locale presence) per spec contract §19
    And 5-dev UX research → 3-dev sample com plan to expand waivable (com ADR) per spec contract §19
    And cross-functional review gate non-waivable (per spec contract §19)
    And Lighthouse ≥ 95 non-waivable
    And WCAG 2.2 AA non-waivable
    And auto-gen REAPI reference non-waivable
    And 3-locale i18n presence (en-US + pt-BR + es-419) non-waivable (Lote 10.18 P1; per spec contract §19)
    And SBOM canonical public download non-waivable (S-12 SLSA L3 release artifact; Lote 10.18 P1; per spec contract §19)
```

## 9. Design Decisions

### 9.1 Why single-phase SEAL D+10 (LOW_RISK)

- LOW_RISK lane single-phase SEAL canonical (vs STANDARD two-phase D+20/D+50; HIGH_RISK two-phase sempre).
- DoD §6 todos critérios instant-verifiable at sprint close (Lighthouse + WCAG + Vale + lychee + cross-functional sign-offs + UX research + pricing calculator validated).
- 2-week sprint LOW_RISK sem post-sprint observation window required.

### 9.2 Why 3 sign-offs canonical LOW_RISK (Owner + Final Approver + Docs lead)

- Per framework §33.5.4 + spec contract §6 sign-off line.
- LOW_RISK lane: 3 sign-offs canonical baseline.
- Engineer + QA + Product + Compliance officer + Privacy officer canonical em STANDARD lane são NÃO mandatory canonical em LOW_RISK (folded em PR review se applicable).
- Docs lead canonical em S-18 dado scope Diátaxis discipline + Vale lint + i18n native speaker + WCAG 2.2 AA + Lighthouse.

### 9.3 Why cross-functional publish gate separate (não main PRR)

- Per spec contract §10 anti-scope estrito + §9.6.
- Cross-functional sign-offs (Finance/Legal/Privacy Officer/Security lead per relevant page) count toward separate publish gate (não main PRR LOW_RISK).
- Non-skippable per Waiver policy §19.

### 9.4 Why Lighthouse ≥ 95 + WCAG 2.2 AA + auto-gen REAPI reference + 3-locale i18n + SBOM canonical public download non-waivable

- Per spec contract §19 Waiver policy: Cross-functional review gate (Finance + Legal + Privacy + Security) em pages relevant + Lighthouse ≥ 95 + WCAG 2.2 AA + Auto-gen REAPI reference + 3-locale i18n (en-US + pt-BR + es-419) + SBOM canonical public download são non-waivable.
- UX baseline for dev tools (Lighthouse).
- Accessibility regulatory baseline (WCAG 2.2 AA).
- Drift prevention (auto-gen REAPI reference).
- LGPD pt-BR canonical + LATAM es-419 alignment com S-11/S-15/S-16 mandatory (3-locale i18n; Lote 10.18 P1 tightening — prior "3 → 2 locales" allowance removida).
- S-12 SLSA L3 transparency baseline (SBOM canonical public download; Lote 10.18 P1 tightening — prior "SBOM via support email com NDA" allowance removida; NDA-gated SBOM = enterprise procurement friction inaceitável).

### 9.5 Why CONDITIONALLY_APPROVED waivers waivable (native-speaker review D+10 + 5-dev UX research → 3-dev)

- Per spec contract §19 Waiver policy waivable items.
- Native-speaker review of stub TODO-marked translations waivable until D+10 (D-day = first external user onboarding); severity LOW; never waives the 3-locale presence requirement (which is non-waivable per spec contract §19).
- 5-dev UX research passing → 3-dev sample com plan to expand waivable com ADR.

### 9.6 Why UX research 5 dev sample (não 10 ou 3)

- 5-dev sample = canonical em UX research practice (Nielsen Norman Group; saturation point).
- 5 external dev sample (workshop or async session); findings documented; iteration based on findings post-sprint.
- Quality Standard 14.s18.2 canonical: Diátaxis discoverability test 5 dev sample finds answer ≤ 30s.

### 9.7 ADR potencial?

- Não — closing WI consolidates canonical patterns from S-18 spec contract §19 Waiver policy + framework §33.5 lane sign-offs (no novel decision; ADR não necessário em LOW_RISK closing WI; CONDITIONALLY_APPROVED waivers documented em PRR if applicable com individual ADRs per waiver).

## 10. Completeness Criteria

- [ ] **10.s18.005.1** i18n native speaker review per locale en-US/pt-BR/es-419 + Legal local review for legal terms (per Completeness Criteria 10.s18.2).
- [ ] **10.s18.005.2** WCAG 2.2 AA axe-core CI gate 0 violations sustained (per Completeness Criteria 10.s18.8).
- [ ] **10.s18.005.3** Manual screen reader test 5 critical pages (NVDA + VoiceOver + JAWS).
- [ ] **10.s18.005.4** Lighthouse CI gate ≥ 95 em 5 routes (per Completeness Criteria 10.s18.7).
- [ ] **10.s18.005.5** Vale tone consistency lint CI gate (Quality Standard 14.s18.3).
- [ ] **10.s18.005.6** lychee broken-link CI gate PR + weekly cron (per Completeness Criteria 10.s18.1).
- [ ] **10.s18.005.7** UX research session 5 dev sample finds answer ≤ 30s + completes getting started ≤ 5 min (per Completeness Criteria 10.s18.3).
- [ ] **10.s18.005.8** PRR LOW_RISK 3 sign-offs canonical (Owner + Final Approver + Docs lead).
- [ ] **10.s18.005.9** Cross-functional publish gate separate (CF-1 + CF-2 + CF-3 sign-offs collected; non-skippable per Waiver policy §19).
- [ ] **10.s18.005.10** Evidence pack committed em `specs/_audits/`.
- [ ] **10.s18.005.11** Single-phase SEAL D+10 ceremony executed.

## 11. DoD

- [ ] i18n native speaker review en-US/pt-BR/es-419 + Legal local review.
- [ ] WCAG 2.2 AA axe-core CI gate 0 violations + manual screen reader test.
- [ ] Lighthouse ≥ 95 em 5 routes (4 pillars).
- [ ] Vale lint CI gate green.
- [ ] lychee broken-link CI gate green + weekly cron scheduled.
- [ ] UX research 5 dev sample completes ≤ 5 min + finds answer ≤ 30s.
- [ ] PRR LOW_RISK 3 sign-offs canonical aprovado.
- [ ] Cross-functional publish gate separate sign-offs collected (CF-1 + CF-2 + CF-3).
- [ ] Evidence pack committed em `specs/_audits/`.
- [ ] Adversarial scenarios 5+ documented cross-WI.

## 12. Invariants Validated

- **CTRL-PRIV-001** (zero PII em screenshots/examples) — VALIDATES via CI lint check (reuse from WI-S18-002 + WI-S18-003 + WI-S18-004).
- **CTRL-DOC-AUTO-GEN** (auto-gen drift prevention CI gate) — VALIDATES via REAPI reference auto-gen CI gate green (reuse from WI-S18-002).
- **Não introduz INVs novas** (sprint consumer; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| i18n native speaker review report | `specs/_audits/2026-XX-XX-i18n-native-speaker-review.md` | Markdown |
| WCAG 2.2 AA axe-core CI evidence | `specs/_audits/2026-XX-XX-wcag-22-aa-evidence.md` + `.github/workflows/docs-a11y.yml` | Markdown / YAML |
| Manual screen reader test report | `specs/_audits/2026-XX-XX-screen-reader-test.md` | Markdown |
| Lighthouse score per route ≥ 95 evidence | `specs/_audits/2026-XX-XX-lighthouse-evidence.md` + `.github/workflows/docs-lighthouse.yml` + `.lighthouserc.json` | Markdown / YAML / JSON |
| Vale lint CI evidence | `specs/_audits/2026-XX-XX-vale-lint-evidence.md` + `apps/docs/.vale/styles/CoreLink/` + `.vale.ini` | Markdown / Vale config |
| lychee broken-link CI evidence | `specs/_audits/2026-XX-XX-lychee-evidence.md` + `.github/workflows/docs-lychee.yml` | Markdown / YAML |
| UX research session report | `specs/_audits/2026-XX-XX-ux-research-s18.md` | Markdown |
| Cross-functional sign-off log | `specs/_audits/2026-XX-XX-cross-functional-signoffs-s18.md` | Markdown |
| PRR S-18 | `specs/04_sprints/S18/PRR-S18.md` | Markdown (`prr` schema) |

## 14. Quality Standards

- **14.s18.005.1** i18n discipline: missing translation = build fail; native speaker review per locale + Legal local review for legal terms (per Quality Standard 14.s18.5).
- **14.s18.005.2** Lighthouse ≥ 95 sustained (per Quality Standard 14.s18.1; CI gate < 95 fails PR).
- **14.s18.005.3** WCAG 2.2 AA axe-core 0 violations sustained (a11y regulatory baseline).
- **14.s18.005.4** Vale tone consistency lint CI gate (per Quality Standard 14.s18.3).
- **14.s18.005.5** lychee broken-link CI gate PR + weekly cron (per Quality Standard 14.s18.4).
- **14.s18.005.6** Cross-functional review gate canonical: Finance + Legal + Privacy Officer + Security lead per relevant page non-skippable per Waiver policy §19 (Quality Standard 14.s18.6).
- **14.s18.005.7** Cost regression gate: docs CI infra ≤ $50/mês (Lighthouse CI free + axe-core OSS + Vale OSS + lychee OSS + GitHub Actions free tier).

## 15. Test Plan

### Unit tests
- Vale lint config valid em `.vale.ini`.
- Lighthouse config valid em `.lighthouserc.json`.
- axe-core config valid.
- lychee config valid.

### Integration tests
- 5 routes Lighthouse ≥ 95 (CI green).
- WCAG 2.2 AA axe-core 0 violations (CI green).
- Vale lint green em PR.
- lychee broken-link green em PR (internal) + weekly cron green (external).
- i18n native speaker review report committed.
- Manual screen reader test report committed (NVDA + VoiceOver + JAWS).
- UX research session report committed (5 dev sample finds answer ≤ 30s + completes getting started ≤ 5 min).
- Cross-functional sign-off log committed (CF-1 + CF-2 + CF-3).
- PRR LOW_RISK 3 sign-offs canonical aprovado.

### Adversarial scenarios (5+; cross-WI summary)
1. Cross-functional review gate bypassed em pricing/security/compliance PR (developer mistake) → PR fails (non-skippable per Waiver policy §19; CRITICAL post-mortem trigger se merge inadvertent).
2. Lighthouse score < 95 em route inadvertent (perf regression) → CI gate fails; PR blocked; perf review weekly.
3. WCAG 2.2 AA axe-core ≥ 1 violation inadvertent (a11y regression) → CI gate fails; PR blocked.
4. Vale lint violations inadvertent (developer mistake) → CI gate fails; PR blocked.
5. lychee broken-link em PR diff inadvertent → CI gate fails; PR blocked.
6. i18n missing translation em locale (developer mistake) → build fails; PR blocked.
7. UX research 5 dev sample fails (finds answer > 30s OR getting started > 5 min) → discoverability post-mortem + IA review (Quality Standard 14.s18.2).
8. CONDITIONALLY_APPROVED waiver attempt non-waivable item (cross-functional gate / Lighthouse / WCAG / auto-gen REAPI reference) → waiver rejected (per spec contract §19).

## 16. Failure Modes

- **FM-DOC-CROSS-FUNCTIONAL-BYPASS** (cross-functional review gate bypassed): non-skippable per Waiver policy §19; CRITICAL post-mortem trigger se merge inadvertent (per spec contract §18).
- **FM-DOC-LIGHTHOUSE-REGRESSION** (Lighthouse score < 95 sustained > 7d): post-mortem (DX regression per spec contract §18).
- **FM-DOC-WCAG-REGRESSION** (axe-core violations introduced): CI gate enforces; post-mortem se sustained.
- **FM-DOC-I18N-DRIFT** (missing translation OR translation quality issues legal terms pt-BR/es-419): Legal local review enforces (per spec contract §15 row 6).
- **FM-DOC-UX-DISCOVERABILITY** (find answer > 30s sustained UX research): discoverability post-mortem + IA review (per spec contract §18).

## 17. Controls

- **CTRL-PRIV-001** (zero PII em screenshots/examples) — VALIDATES via CI lint check (reuse from WI-S18-002 + WI-S18-003 + WI-S18-004).
- **CTRL-DOC-AUTO-GEN** (auto-gen drift prevention CI gate) — VALIDATES via REAPI reference auto-gen CI gate green (reuse from WI-S18-002).
- **Cross-functional review gate canonical** (Finance + Legal + Privacy Officer + Security lead per relevant page) — VALIDATES via cross-functional publish gate separate (CF-1 + CF-2 + CF-3 sign-offs collected; non-skippable per Waiver policy §19).

## 18. Resilience Patterns

- N/A (closing WI; non-cripto-load-bearing).

## 19. Observability

- CI gates status (Lighthouse + axe-core + Vale + lychee + auto-gen drift); PR fails se any gate fails.
- Cross-functional sign-off log status (PR fails se sign-off missing per relevant page).
- UX research session findings tracked monthly post-sprint.

Não introduz métricas Prometheus per-tenant em closing WI (per observability_model §3.1; sprint consumer; docs telemetry separate em sprint.md §11 if applicable).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: cross-functional sign-offs documented em PRR + cross-functional sign-off log; non-repudiation via git commit history + GitHub Actions CI log.
- **Tampering**: docs source git-tracked + reviewer Docs lead + cross-functional sign-off per page.
- **Repudiation**: GitHub Actions CI gate log + git commit history + cross-functional sign-off log + PRR sign-off log.
- **Information disclosure**: zero customer data em screenshots/examples enforced via CI lint check (reuse from WI-S18-002 + WI-S18-003 + WI-S18-004).
- **DoS**: nenhuma (closing WI).
- **Elevation of privilege**: docs publish requires PR review + Docs lead sign-off + cross-functional sign-off per page (Finance/Legal/Privacy Officer/Security lead) + CI gates pass + PRR LOW_RISK aprovado.

**LINDDUN delta**:
- Linkability: nenhuma (closing WI; no per-tenant identifiers em examples).
- Identifiability: PAT format placeholder + tenant_id placeholder enforced via CI lint check (reuse).
- Non-repudiation: GitHub Actions CI gate log + cross-functional sign-off log + PRR sign-off log.
- Detectability: CI gates (Lighthouse + axe-core + Vale + lychee + auto-gen drift); cross-functional sign-off enforcement.
- Disclosure: SBOM Security lead review (CF-2); pentest exec summary public-safe (CF-2); compliance claims aligned canonical sources (CF-3).
- Unawareness: cross-functional review gate canonical (Finance + Legal + Privacy Officer + Security lead) per page sensible non-skippable.
- Non-compliance: LGPD + GDPR (lawful basis per processing) + CCPA/CPRA statements aligned canonical compliance_matrix.md (CF-3).

## 21. Dependencies

### Hard blockers
- **WI-S18-001 SEALED** (Docusaurus 3.x foundation + Diátaxis sidebar + i18n config + custom domain).
- **WI-S18-002 SEALED** (getting started + REAPI auto-gen + 4-language code examples).
- **WI-S18-003 SEALED** (4 SDK guides + client verify default-on documented).
- **WI-S18-004 SEALED** (compliance + security + pricing pages + cross-functional sign-offs CF-1 + CF-2 + CF-3 collected).

### Soft blockers
- 5 external dev sample availability (workshop or async session for UX research).
- Native speakers per locale availability (en-US + pt-BR + es-419) + Legal local review for legal terms.

### Outbound
- **S-19 (enterprise customer onboarding)**: may reuse public docs sections.
- **S-20 (GA gate)**: docs live em `docs.corelink.dev` + 5-dev UX research passed + Lighthouse ≥ 95 sustained + cross-functional publish gate sign-offs.

## 22. Effort PERT

O: 8h, M: 14h, P: 22h → PERT **14.3h** (per spec contract §12; closing ship gate i18n + WCAG + Lighthouse + Vale + lychee + UX research + PRR + cross-functional publish gate).

## 23. Cost Analysis

**Direct cost**:
- Lighthouse CI free: $0/mês.
- axe-core OSS: $0/mês.
- Vale OSS: $0/mês.
- lychee OSS: $0/mês.
- GitHub Actions free tier (CI gates): $0/mês.
- UX research session external developers: $0 (community workshop or async session; reciprocal feedback).
- Native speakers per locale + Legal local review: internal team or contractor; budget per Legal/Translation.
- JAWS Windows commercial test: ~$95 per license OR free 40-min trial (testing only).

**Total**: ~$10/mês adicional (JAWS commercial) ou $0 (trial).

**Indirect cost**: 0 GTM friction (docs production-ready) + 0 a11y regulatory exposure (WCAG 2.2 AA) + 0 dev experience friction (Lighthouse ≥ 95) + dev adoption baseline = priceless.

## 24. Post-mortem Hooks

- Cross-functional review bypassed em pricing/compliance/security PR → CRITICAL post-mortem + process reinforce (non-skippable gate violated).
- Lighthouse score < 90 sustained > 7d → post-mortem (DX regression per spec contract §18).
- WCAG 2.2 AA violations sustained > 7d → post-mortem (a11y regulatory baseline).
- UX research 5 dev sample fails (finds answer > 30s OR getting started > 5 min) → discoverability post-mortem + IA review.
- i18n missing translation OR translation quality issues legal terms → Legal local review reinforce (per spec contract §15 row 6).
- CONDITIONALLY_APPROVED waiver expiry triggered without remediation → expiry post-mortem + Waiver policy §19 reinforce.

## 25. Rollback / Recovery

Docs rollback: revert PR + redeploy CF Pages previous build. RTO ≤ 5min. CI gates retroactive enforcement via PR re-run. Cross-functional sign-off retroactive enforcement via re-collected sign-offs.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cross-functional review bypassed em pricing/security/compliance PR | L | H (PR review) | HIGH | M | LOW | Non-skippable per Waiver policy §19; PR fails se sign-off missing; CRITICAL post-mortem trigger se merge inadvertent |
| R-002 | Lighthouse score regression sustained > 7d | M | L | LOW | L | LOW | CI gate < 95 fails PR; perf review weekly (per spec contract §15 row 5) |
| R-003 | Translation quality issues legal terms (pt-BR/es-419) | M | L | MEDIUM (legal) | L | LOW | Native speaker + Legal local review per locale (per spec contract §15 row 6) |
| R-004 | UX research 5 dev sample fails (find answer > 30s OR getting started > 5 min) | M | M | LOW | M | LOW | Discoverability post-mortem + IA review; iteration based on findings (per spec contract §18) |
| R-005 | CONDITIONALLY_APPROVED waiver expiry without remediation | L | M | MEDIUM | L | LOW | Waiver policy §19 expiry ts + revalidation_trigger; expiry post-mortem |

## 27. Knowledge Transfer

- Tech talk (1h): "S-18 closing ship gate — i18n native speaker + WCAG 2.2 AA + Lighthouse ≥ 95 + Vale + lychee + UX research + PRR LOW_RISK + cross-functional publish gate separate".
- Doc `docs/internal/s18-closing-ship-gate.md` — closing ship gate procedure.
- Onboarding test (5 questions): single-phase SEAL D+10 rationale + 3 sign-offs canonical LOW_RISK + cross-functional publish gate non-skippable rationale + non-waivable items (cross-functional gate + Lighthouse + WCAG + auto-gen REAPI) + CONDITIONALLY_APPROVED waivers waivable items rationale.

## 28. Sign-off (LOW_RISK 3 canonical + cross-functional publish gate)

**Staffing reality (per ADR-0034 solo-tier; reuse from S-17)**:

| Status atual (2026-04-29) | Roles |
|---|---|
| **Confirmed (3)** | Owner (Gustavo Schneiter); Final Approver (Gustavo Schneiter); Engineer (Gustavo Schneiter — solo founder dual-hat folded em PR review) |
| **Pending Tier-1 hire/contract (LOW_RISK canonical)** | Docs lead |
| **Pending cross-functional publish gate** | Finance (CF-1 pricing); Legal (CF-1 pricing + CF-3 compliance); Privacy Officer (CF-2 security + CF-3 compliance); Security lead (CF-2 security) |

**Recommended path**: ADR-0034 solo-tier waiver + Option C parallel staffing track (Docs lead + Finance + Legal + Privacy Officer + Security lead).

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Docs lead | _TBD; emphatic — closing ship gate i18n + WCAG + Lighthouse + Vale + lychee + UX research + PRR LOW_RISK_ | _pending_ | _pending_ |

**Cross-functional publish gate separate (non-skippable per Waiver policy §19; reuse from WI-S18-004)**:

| # | Page | Required cross-functional sign-off | Status |
|---|---|---|---|
| CF-1 | `/pricing` | **Finance** + **Legal** | _pending_ |
| CF-2 | `/security` | **Security lead** + **Privacy Officer** | _pending_ |
| CF-3 | `/compliance` | **Legal** + **Privacy Officer** | _pending_ |

> Cross-functional sign-offs count toward separate publish gate (não main PRR per spec contract §10 anti-scope; non-skippable per Waiver policy §19). PRR LOW_RISK 3 sign-offs canonical em main PRR.

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S18-005 (cycle 12.S18.0; LOW_RISK lane closing ship gate; i18n native speaker en-US/pt-BR/es-419 + Legal local review + WCAG 2.2 AA axe-core + Lighthouse ≥ 95 em 5 routes + Vale lint + lychee broken-link + UX research 5 dev sample + PRR LOW_RISK 3 sign-offs canonical + cross-functional publish gate separate non-skippable + single-phase SEAL D+10). |

## 30. Anti-patterns evitados

- Skip i18n native speaker review (Quality Standard 14.s18.5 + Legal local review for legal terms).
- Skip WCAG 2.2 AA axe-core CI gate (a11y regulatory baseline; non-waivable per spec contract §19).
- Skip Lighthouse ≥ 95 CI gate (UX baseline; non-waivable per spec contract §19).
- Skip Vale tone consistency lint CI (Quality Standard 14.s18.3).
- Skip lychee broken-link CI (Quality Standard 14.s18.4).
- Skip UX research session 5 dev sample (Quality Standard 14.s18.2).
- Skip cross-functional publish gate sign-offs (CRITICAL gap; non-skippable per Waiver policy §19).
- Skip auto-gen REAPI reference CTRL-DOC-AUTO-GEN (non-waivable per spec contract §19).
- PRR LOW_RISK aprovado sem cross-functional publish gate sign-offs (CRITICAL gap).
- Two-phase SEAL em LOW_RISK (single-phase SEAL D+10 canonical; STANDARD/HIGH_RISK two-phase only).

---

**Fim WI-S18-005.**
