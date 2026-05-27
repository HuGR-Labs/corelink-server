---
id: "WI-S16-007"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-05-14"
lane: "STANDARD"
parent: "S-16"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s16", "ship-gate", "playwright", "e2e", "lighthouse", "ux-workshop", "sus", "csp-enforce", "prr", "standard"]
---

# WI-S16-007 — S-16 Ship Gate: Playwright E2E Tests (Full E2E Novo Dev Signup → Cria Tenant → Aceita DPA → Cria PAT → Upload via CLI Sucesso EVT-018; Consent Flow Capture E2E com 6-field Payload Validated em Backend; DSR Submit + JWT Receipt em ≤ 1s; Audit Viewer Query + JSON Export; PAT Create + List + Revoke Flow) + **Lighthouse CI Gate ≥ 95** em 4 Pillars (Performance + A11y + Best Practices + SEO) em 3 Routes (`/`, `/dashboard`, `/privacy`) — Score < 95 Fails PR per Quality Standard 14.s16.5 + **Cross-browser CI Matrix** (Chrome + Firefox + Safari + Edge Latest 2 Versions Cada via Playwright Multi-browser) + **UX Research Session 5 External Developers** Rotated per Sprint (Bias Mitigation; SUS Calc — System Usability Scale; Threshold ≥ 75 Baseline; Iterate até Pass; **Time-to-First-PAT ≤ 5 min** Measured) + **CSP Enforce Mode Prod** com < 5 Violations/dia (CSP Report Endpoint Analysis Sustained 30d Staging → Enforce Prod) + **Closing PRR STANDARD doc S-16** com 5-8 Sign-offs Canonical (7 Typical: Owner + Final Approver + Frontend Lead + QA + Product + Designer/a11y advisor + Privacy officer) + Evidence Pack: Lighthouse Score Reports + axe-core 0 Violations + Screen Reader Walkthrough Video + UX Research SUS Report + 3 Locales Native Speaker Review + Cross-browser CI Matrix Verde + CSP Report Sustained + Adversarial Test Summary Aggregation Cross-WI

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-16](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S16-007 |
| Título | S-16 ship gate — Playwright E2E + Lighthouse CI ≥ 95 + cross-browser matrix + UX workshop SUS ≥ 75 + CSP enforce prod + closing PRR STANDARD. |
| Sprint | S-16 |
| Lane | STANDARD |
| Forcing factors | none (closing WI; STANDARD lane single-phase SEAL D+18; DoD §6 não requer 30d observation window) |

## 1. Intent

WI-S16-001..006 implementam features. **Este WI prova que o sistema admin UI completo funciona sob production-equivalent load + UX research + cross-browser scrutiny**. É o ship gate do S-16. **Gating canônico**: este WI fecha em **single-phase SEAL D+18** (STANDARD lane; DoD §6 não requer "30d sustained" criteria — only Lighthouse sustained CI gate + axe-core 0 violations + cross-browser matrix verde + UX SUS ≥ 75 sample) com Playwright E2E suite verde + Lighthouse ≥ 95 em 4 pillars × 3 routes + cross-browser matrix verde + UX workshop SUS ≥ 75 + CSP enforce prod + PRR 5-8 sign-offs canonical coletados — isso libera **downstream development S-18/S-19/S-20** (hard dependency satisfeita per spec contract S-16 §11).

Deliverables 6-fold:

1. **Playwright E2E test suite** em `apps/web/e2e/`:
   - Full E2E novo dev signup → cria tenant → aceita DPA → cria PAT → upload via CLI sucesso (EVT-018).
   - Consent flow capture E2E com 6-field payload validated em backend (S-11 verify endpoint R-S11-9).
   - DSR submit + JWT receipt em ≤ 1s (chaos test simula slow API ≥ 5s; UI handles via progress + retry).
   - Audit viewer query + JSON export (sanitized; PAT redacted; PII allowlist).
   - PAT create + list + revoke flow (revoke-all com MFA re-auth CTRL-AUTH-010).
   - Onboarding wizard 4 steps + abandonment tracked.
   - Admin op submit + dual-approval flow + collusion-rotation rolling 3-op window.

2. **Lighthouse CI gate ≥ 95** em 4 pillars (Performance + A11y + Best Practices + SEO) em 3 routes (`/`, `/dashboard`, `/privacy`):
   - `.github/workflows/lighthouse.yml` runs Lighthouse CI per PR.
   - Score < 95 fails PR per Quality Standard 14.s16.5 + DoD §6.
   - Sustained 30d em staging via scheduled runs.
   - Bundle ≤ 250KB enforce (já em WI-S16-001; verified em CI gate).

3. **Cross-browser CI matrix**:
   - Chrome + Firefox + Safari + Edge latest 2 versions cada (8 cells matrix).
   - Playwright multi-browser via `playwright.config.ts` projects.
   - Visual regression testing via Percy (or equivalent) — optional pós-GA.
   - Smoke tests em todos cells per PR; full E2E em scheduled runs.

4. **UX research session com 5 external developers**:
   - Rotated per sprint (bias mitigation; diverse persona recruitment).
   - Workshop scenarios:
     - Tenant onboarding (signup → DPA → billing → first PAT em ≤ 5 min target).
     - Consent capture (read notice → click → backend persist).
     - DSR submit (pick direito → MFA re-auth → JWT receipt em ≤ 1s).
     - PAT mgmt (create + list + revoke).
   - SUS calc — System Usability Scale (10 questions; threshold ≥ 75 baseline).
   - Iterate até pass (post-workshop changes documented).
   - **Time-to-first-PAT ≤ 5 min** measured (target ≥ 80% trials per spec contract).
   - Report committed em `specs/_audits/2026-XX-XX-ux-workshop-s16.md`.

5. **CSP enforce mode prod** com < 5 violations/dia:
   - CSP report endpoint analysis sustained 30d staging (already from WI-S16-001).
   - Refine allowlist baseado em report findings.
   - Switch CSP mode `report-only` → `enforce` em prod via Next.js `headers()` config.
   - Monitor sustained 30d enforce prod com < 5 violations/dia threshold.
   - SEV-3 alert se violations > 50/dia (potencial false-positive surge).

6. **Closing PRR STANDARD doc S-16** + adversarial summary aggregation + evidence pack:
   - **PRR doc** `specs/04_sprints/_sealed/S16/PRR-S16.md` covering:
     - DoD §6 + §7 criteria status (single-phase SEAL D+18; STANDARD lane no observation window).
     - CTRLs trace verified (CTRL-AUTH-010 + CTRL-CRED-001 + CTRL-PRIV-001 + CTRL-PRIV-CONSENT-001..006 reflection enforced).
     - All 7 WIs SEALED state precondition.
     - Playwright E2E suite verde.
     - Lighthouse ≥ 95 em 4 pillars × 3 routes sustained 30d.
     - Cross-browser CI matrix 8 cells verde.
     - UX workshop 5 devs SUS ≥ 75 + time-to-first-PAT ≤ 5 min ≥ 80% trials.
     - CSP enforce prod < 5 violations/dia sustained 30d.
     - axe-core CI 0 violations em todas routes (WI-S16-006 reuse).
     - Screen reader walkthrough OK (NVDA + VoiceOver) documentado.
     - 3 locales native speaker reviewed.
     - Consent capture 6-field + screenshot evidence EVT-012 verified backend (S-11 R-S11-9).
     - DSR JWT receipt em ≤ 1s + SLA clock 30d visible.
     - PAT once-display + masked após copy verified.
     - Admin ops dual-approval + collusion-rotation rolling 3-op window verified (S-13 PAT-DUAL-APPROVAL-001 reuse).
     - LINDDUN review committed (admin UI privacy delta).
     - 8+ admin UI métricas Prometheus emitting em staging (telemetry opt-in baseline).
     - **Promotion gate decision**: `APPROVED` | `CONDITIONALLY_APPROVED` (com waivers + ADR + expiry) | `REJECTED`.
     - **CONDITIONALLY_APPROVED waivers** (typical em STANDARD lane S-16):
       - 3 locales → 2 locales GA (en + pt-BR; es-419 no Q1 pós-GA) com Frontend Lead + Privacy officer + ADR.
       - Lighthouse ≥ 95 → ≥ 90 com plan to improve next sprint + ADR.
       - Cross-browser scope: latest 2 versions → latest 1 version Safari (oldest 2 sometimes lag features) + ADR.
   - **Adversarial summary aggregation** (cross-WI 30+ scenarios):
     - WI-S16-001: 5 scenarios (CSP false-positives + bundle regression + Clerk MFA edge cases + i18n missing translation + PII leak via safeLog).
     - WI-S16-002: 5 scenarios (Stripe Checkout edge cases + PAT exposure + DPA abandonment + invite link replay + time-to-first-PAT > 5 min).
     - WI-S16-003: 5 scenarios (html2canvas browser quirks + locale mismatch + UUID collision + Flesch-Kincaid regression + auto-submit dark pattern).
     - WI-S16-004: 5 scenarios (DSR abandonment + MFA re-auth disrupts UX + JWT receipt verify failure + SLA breach + erasure conditional misunderstood).
     - WI-S16-005: 5 scenarios (audit PAT raw + cross-tenant access + dual-approval bypass + Slack PII leak + JSON export memory exhaustion).
     - WI-S16-006: 5 scenarios (a11y compliance late discovery + i18n quality + color contrast regression + cross-browser Safari quirks + sub-processors outdated).
     - WI-S16-007: 5 scenarios (Lighthouse regression + cross-browser matrix red sustained + UX SUS < 75 + CSP enforce false-positives + 5 dev sample bias).
   - **Final gate releases all S-16 WIs to Implementation SEAL state D+18**: libera downstream S-18/S-19/S-20 dev.

## 2. Narrative

S-16 é o **maior salto de surface customer-facing UI** em CoreLink: admin UI Next.js 15 + Clerk + CSP + i18n + onboarding + consent + DSR + audit + PAT mgmt + component library WCAG 2.2 AA + 3 locales. Cada um dos 6 WIs anteriores tem completeness mini-checklist; **WI-S16-007 é o gate cumulativo**: valida que o sistema completo composto funciona sob:

1. **Playwright E2E**: full flows novo dev signup → CLI cache hit; consent capture com backend verify; DSR JWT receipt ≤ 1s; audit viewer JSON export; PAT lifecycle complete.

2. **Lighthouse CI gate ≥ 95**: 4 pillars × 3 routes sustained 30d; PR fails se < 95 (DX baseline).

3. **Cross-browser matrix**: 8 cells (Chrome + Firefox + Safari + Edge × latest 2 versions); Playwright multi-browser.

4. **UX workshop 5 devs externos**: SUS ≥ 75 + time-to-first-PAT ≤ 5 min ≥ 80% trials measured; bias mitigation via rotation per sprint.

5. **CSP enforce prod**: 30d staging report-only → refine allowlist → enforce prod < 5 violations/dia sustained 30d.

6. **Closing PRR STANDARD 5-8 sign-offs canonical** (7 typical: Owner + Final Approver + Frontend Lead + QA + Product + Designer/a11y advisor + Privacy officer); evidence pack forensic-grade (Lighthouse + axe-core + screen reader walkthrough video + UX SUS report + native speaker review + cross-browser matrix + CSP report sustained + adversarial summary).

**Risk justification STANDARD lane (single-phase SEAL D+18)**:
- DoD §6 não requer "30d sustained" criteria; only Lighthouse + axe-core + cross-browser matrix + UX SUS sample (instant-verifiable).
- UI consume APIs já validated em S-03/S-09/S-10/S-11/S-13.
- Não há cripto-load-bearing novel control (S-14 HIGH_RISK BYOK reuse only).
- Single-phase SEAL D+18 sufficient (vs S-13/S-14 HIGH_RISK two-phase SEAL com observation window 30d).

5-8 sign-offs canonical (7 typical) **mandatory** (sprint contract S-16 §14); Privacy officer + Designer/a11y advisor canonical em S-16 dado scope CTRL-PRIV-CONSENT + DSR + LINDDUN review + WCAG 2.2 AA + 5-dev UX workshop SUS.

## 3. Customer Impact & Journey

**Persona 1 — Customer adopting CoreLink (post-S-16 GA)**:
- Documentation: "UI posture: Next.js 15 + Clerk SSO + WebAuthn + hardened CSP enforce prod + i18n 3 locales (en/pt-BR/es-419) native speaker reviewed + tenant self-service onboarding em ≤ 5 min + consent capture 6-field + screenshot evidence EVT-012 + DSR self-service 6 direitos LGPD/GDPR + JWT receipt + SLA 30d + audit log viewer self-service + JSON export sanitized + PAT mgmt + admin ops dual-approval + WCAG 2.2 AA + Lighthouse ≥ 95 + cross-browser CI matrix + UX SUS ≥ 75".
- PRR doc é evidence-grade artifact: customers can request via NDA.

**Persona 2 — Compliance auditor (SOC 2 + ISO 27001 + LGPD/GDPR + ADA + EU Accessibility Act 2025)**:
- Audit query: S-16 WIs SEALED state; PRR doc 5-8 sign-offs canonical documented.
- LINDDUN review (admin UI privacy delta) = privacy attestation.
- WCAG 2.2 AA compliance verified (axe-core + manual screen reader walkthrough).
- 3 locales native speaker reviewed.
- Consent 6-field + screenshot evidence + verify endpoint backend.
- DSR JWT receipt + SLA clock + email triggers.

**Persona 3 — Internal SRE/Engineer**:
- CSP enforce prod < 5 violations/dia sustained = production confidence.
- Cross-browser CI matrix verde sustained = release confidence.
- UX SUS ≥ 75 = adoption signal.

## 4. Capability Mapping

- All CAP-UI-* (validates whole UI domain).
- **CAP-UI-009** (i18n + a11y baseline) — IMPLEMENTA primary completion (Lighthouse + axe-core + screen reader walkthrough).
- Trace: `_spec_contract.md §6 (Definition of Done)` + `framework §33.5.4 (STANDARD 5-8 sign-offs canonical)`.

## 5. Tipo

Sprint ship gate; STANDARD lane; closing WI single-phase SEAL D+18.

## 6. Escopo

### 6.1 In-scope

1. **Playwright E2E test suite** em `apps/web/e2e/`:
   - **Onboarding E2E**: novo dev signup → cria tenant → aceita DPA → cria PAT → CLI upload sucesso (consume CLI from S-15).
   - **Consent E2E**: navigate /consent → render notice → click → backend persist com 6-field payload → verify endpoint round-trip (S-11 R-S11-9).
   - **DSR E2E**: navigate /settings/dsr → pick direito → MFA re-auth → submit → JWT receipt em ≤ 1s + SLA clock visible.
   - **Audit viewer E2E**: navigate /admin/audit → filters → table → JSON export.
   - **PAT mgmt E2E**: create + list + revoke single + revoke all (com MFA re-auth).
   - **Admin ops E2E**: submit op → second admin approve → collusion-rotation enforce 4th rejection.
   - **Test config**: `apps/web/playwright.config.ts` com projects para Chrome + Firefox + Safari + Edge.

2. **Lighthouse CI gate** em `.github/workflows/lighthouse.yml`:
   - Runs `@lhci/cli autorun` per PR + scheduled daily em prod.
   - Targets: `/` + `/dashboard` + `/privacy`.
   - 4 pillars: Performance + Accessibility + Best Practices + SEO.
   - Threshold: ≥ 95 todos pillars.
   - Score < 95 fails PR (assertion failure em lhci config).
   - Sustained 30d em staging via scheduled runs (per Completeness Criterion 10.s16.3).

3. **Cross-browser CI matrix** em `apps/web/playwright.config.ts`:
   - Projects: chrome-stable + chrome-prev + firefox-stable + firefox-prev + safari-stable + safari-prev + edge-stable + edge-prev (8 cells).
   - Smoke tests em todos cells per PR.
   - Full E2E em scheduled runs (cost balance).
   - Visual regression via Percy (or equivalent) — opcional pós-GA Q1.

4. **UX research session 5 external developers**:
   - Recruitment via dev community channels (Hacker News, Reddit r/devops, Bazel Slack, etc.); rotated per sprint.
   - Bias mitigation: diverse persona (build engineer + SRE + privacy officer + frontend dev + backend dev).
   - Workshop format: 1.5h moderated session via Zoom/Meet; screen recorded.
   - Scenarios:
     - Tenant onboarding (signup → DPA → billing → first PAT em ≤ 5 min target).
     - Consent capture (read notice → click → confirmation).
     - DSR submit (pick direito → MFA re-auth → JWT receipt).
     - PAT mgmt (create + list + revoke).
   - SUS calc — System Usability Scale 10 questions; threshold ≥ 75 baseline.
   - Iterate até pass: post-workshop changes documented em next sprint cycle.
   - Time-to-first-PAT ≤ 5 min measured per developer; aggregate ≥ 80% trials.
   - Report committed em `specs/_audits/2026-XX-XX-ux-workshop-s16.md`.

5. **CSP enforce mode prod** + sustained monitoring:
   - 30d staging report-only baseline (from WI-S16-001).
   - Analyze CSP report endpoint findings; refine allowlist (e.g., add Stripe Checkout iframe; remove unused permissions).
   - Switch CSP mode `report-only` → `enforce` em prod via Next.js `headers()` config.
   - Monitor 30d enforce prod com < 5 violations/dia threshold.
   - SEV-3 alert se violations > 50/dia (potencial false-positive surge).

6. **Closing PRR doc** em `specs/04_sprints/_sealed/S16/PRR-S16.md`:
   - Front matter per `prr` schema: feature_wi + capabilities + prod_target_date + work_status (NOT_STARTED → IN_REVIEW → APPROVED).
   - Body covering DoD + §7 + CTRLs trace + all 7 WIs SEALED state + Playwright E2E + Lighthouse + cross-browser + UX SUS + CSP enforce + LINDDUN + métricas Prometheus + adversarial summary aggregation 30+ scenarios.
   - Promotion gate decision (APPROVED | CONDITIONALLY_APPROVED | REJECTED) + waivers se applicable.
   - Evidence pack:
     - Lighthouse score reports per route (commited em audit dir).
     - axe-core CI logs 0 violations.
     - Screen reader walkthrough video (NVDA + VoiceOver) link.
     - UX research SUS report.
     - 3 locales native speaker review docs.
     - Cross-browser CI matrix verde reports.
     - CSP report sustained 30d enforce.
     - Adversarial summary 30+ scenarios.

### 6.2 Out-of-scope (deferred)

- A11y external audit formal certification (pós-GA Q1).
- Visual regression Percy hosted (pós-GA Q1).
- Continuous UX research panel monthly (pós-GA Q1).
- Bug bounty program (pós-GA Q1).
- Customer-facing UX research panel (pós-GA enterprise).

## 7. Anti-Scope

- Skip Playwright E2E (mandatory ship gate per DoD §6).
- Skip Lighthouse CI gate ≥ 95 (DX baseline).
- Skip cross-browser CI matrix (release confidence gap).
- Skip UX workshop 5 devs (compliance baseline UX research).
- Skip CSP enforce prod transition (XSS defense baseline).
- APPROVED PRR sem 5-8 sign-offs canonical.
- Production deploy sem Lighthouse ≥ 95 + axe-core 0 violations + cross-browser matrix.
- Waivers acumulando sem expiry.
- Two-phase SEAL com 30d observation window (STANDARD lane single-phase D+18 sufficient).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: S-16 ship gate — Playwright E2E + Lighthouse + cross-browser + UX SUS + CSP enforce + PRR

  Scenario: Playwright E2E suite verde
    Given apps/web/e2e/ test suite committed
    When pnpm playwright test runs CI
    Then onboarding + consent + DSR + audit + PAT + admin ops E2E flows pass
    And test report committed em CI artifacts

  Scenario: Lighthouse CI gate ≥ 95 4 pillars × 3 routes
    Given lighthouse.yml workflow configured
    When PR opens
    Then Lighthouse runs em /, /dashboard, /privacy
    And 4 pillars (Performance + A11y + Best Practices + SEO) all ≥ 95
    And PR fails se any < 95

  Scenario: Lighthouse sustained 30d em staging
    Given scheduled daily Lighthouse runs em staging
    When 30d window evaluated
    Then mean score ≥ 95 across 4 pillars × 3 routes
    And report committed em specs/_audits/lighthouse-s16-30d.md

  Scenario: Cross-browser CI matrix 8 cells verde
    Given playwright.config.ts com 8 projects
    When CI runs smoke tests em todos cells
    Then 8/8 cells verde
    And full E2E runs scheduled (cost balance)

  Scenario: UX workshop 5 devs SUS ≥ 75
    Given 5 external developers recruited rotated per sprint
    When workshop 1.5h moderated session com 4 scenarios
    Then SUS calc threshold ≥ 75
    And time-to-first-PAT ≤ 5 min em ≥ 80% trials
    And report committed em audit doc

  Scenario: CSP enforce prod < 5 violations/dia sustained 30d
    Given 30d staging report-only baseline + refined allowlist
    When CSP mode switched enforce em prod
    Then 30d sustained < 5 violations/dia
    And SEV-3 alert se > 50/dia

  Scenario: PRR STANDARD 5-8 sign-offs canonical
    Given PRR-S16.md committed
    When 7 reviewers sign (Owner + Final Approver + Frontend Lead + QA + Product + Designer/a11y advisor + Privacy officer)
    Then sign-off table populated com names + dates + status=approved
    And promotion gate = APPROVED OR CONDITIONALLY_APPROVED com waivers

  Scenario: Adversarial summary aggregation 30+ scenarios
    Given individual adversarial scenarios em WI-S16-001..006 + this WI
    When summary report aggregated
    Then 30+ scenarios documented
    And 100% mitigation rate sustained
    And report committed em specs/_audits/adversarial-summary-s16.md

  Scenario: All 7 WIs SEALED state precondition
    Given WI-S16-001..006 todos SEALED
    When PRR review proceeds
    Then ship gate validates state precondition
    And blocks SEAL se any WI não SEALED

  Scenario: Single-phase SEAL D+18 (STANDARD lane)
    Given DoD §6 não requer 30d observation window
    When PRR coletados D+18
    Then sprint SEAL ceremony D+18 (não two-phase)
    And libera S-18/S-19/S-20 dev

  Scenario: CONDITIONALLY_APPROVED waiver — 3 locales → 2 locales
    Given native speaker reviewer es-419 staffing slip
    When PRR review proceeds
    Then promotion gate = CONDITIONALLY_APPROVED
    And waiver "3 locales → 2 locales (en + pt-BR); es-419 deferred Q1 pós-GA"
    And ADR committed em specs/_decisions/ADR-XXXX-i18n-locale-deferred.md
    And expiry next sprint
```

## 9. Design Decisions

### 9.1 Why Playwright (não Cypress)

- Playwright: multi-browser native (Chrome + Firefox + Safari + Edge); faster than Cypress.
- Cypress: single-browser primary; multi-browser via Cy.io paid.
- Playwright: free + open-source.

### 9.2 Why Lighthouse ≥ 95 (não ≥ 90)

- Stripe/Auth0/GOV.UK Design System target ≥ 90.
- CoreLink S-16 differentiator ≥ 95 (more stringent + DX baseline).
- Bundle ≤ 250KB enforce + Tailwind tree-shake + dynamic imports = achievable.

### 9.3 Why cross-browser CI matrix latest 2 versions cada

- Latest 2 versions cobre majority of users (90%+ market share).
- Older versions (≥ 3 versions back) deferred to support tier basis.
- Safari latest 2 sometimes lag features (e.g., Web Crypto subtle.digest); fallback documented.

### 9.4 Why UX workshop 5 devs externos rotated (não 3 ou 10)

- 5 = SUS calc statistically significant baseline (Nielsen Norman Group reference).
- 3 = single point insufficient.
- 10+ scope creep; defer to post-GA continuous panel.
- Rotation per sprint = bias mitigation (diverse persona).

### 9.5 Why CSP enforce prod 30d staging baseline (não direct enforce)

- Direct enforce risk false-positives; legit violations missed (per spec contract §15 row 11).
- 30d staging report-only → refine allowlist → enforce prod com low-violations baseline.

### 9.6 Why STANDARD 5-8 sign-offs (não HIGH_RISK 11)

- DoD §6 não requer 30d observation window.
- CTRLs CSP + CTRL-CRED-001 + CTRL-AUTH-010 + CTRL-PRIV-CONSENT são reflection (not novel cripto-load-bearing).
- Não há cripto-load-bearing novel control (S-14 HIGH_RISK cobre BYOK + envelope encryption).

### 9.7 Why single-phase SEAL D+18 (não two-phase)

- STANDARD lane DoD §6 não requer "30d sustained" criteria.
- Lighthouse + axe-core + cross-browser matrix + UX SUS sample são instant-verifiable.
- vs S-13/S-14 HIGH_RISK two-phase SEAL com observation window 30d (rotation overlap, BYOK matrix, kill switch chaos drill).

### 9.8 Why CONDITIONALLY_APPROVED gate option

- Sometimes locale slips (es-419 native speaker reviewer staffing) ou Lighthouse score regressão minor.
- PRR proceeds com waivers documented: specific risk acknowledged + mitigating controls + ADR + expiry next sprint.
- Rejection of all waivers = sprint blocks unnecessarily.

### 9.9 ADR potencial?

- Sim — ADR(s) may be created em runtime para specific waivers (e.g., 3 locales → 2 locales; Lighthouse ≥ 95 → ≥ 90 transitional).
- Não há ADR mandatory pré-criado.

## 10. Completeness Criteria

- [ ] **10.s16.007.1** Playwright E2E suite verde 6 flows (onboarding + consent + DSR + audit + PAT + admin ops) (EVT-018).
- [ ] **10.s16.007.2** Lighthouse CI gate ≥ 95 em 4 pillars × 3 routes (EVT-002).
- [ ] **10.s16.007.3** Lighthouse sustained 30d em staging.
- [ ] **10.s16.007.4** Cross-browser CI matrix 8 cells verde (Chrome + Firefox + Safari + Edge × latest 2 versions).
- [ ] **10.s16.007.5** UX workshop 5 devs SUS ≥ 75 + time-to-first-PAT ≤ 5 min ≥ 80% trials (EVT-018).
- [ ] **10.s16.007.6** CSP enforce prod < 5 violations/dia sustained 30d (EVT-027).
- [ ] **10.s16.007.7** All 7 WIs SEALED state precondition.
- [ ] **10.s16.007.8** PRR-S16.md 5-8 sign-offs canonical documented (EVT-031).
- [ ] **10.s16.007.9** Adversarial summary aggregated 30+ scenarios (EVT-040).
- [ ] **10.s16.007.10** Cost regression gate: full S-16 UI infra ≤ $300/mês.

## 11. DoD

- [ ] Playwright E2E suite committed + CI verde.
- [ ] Lighthouse CI gate workflow + sustained 30d staging.
- [ ] Cross-browser CI matrix verde.
- [ ] UX workshop 5 devs SUS ≥ 75 report committed.
- [ ] CSP enforce prod < 5 violations/dia sustained 30d.
- [ ] PRR-S16.md committed com 5-8 sign-offs canonical.
- [ ] Adversarial test summary report committed.
- [ ] Métricas emitting em staging (admin UI métricas baseline).
- [ ] WIs S-16-001..006 SEALED state.
- [ ] Sprint S-16 closed; release notes committed.

## 12. Invariants Validated

- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min) IMPLEMENTA reflection cumulative em DSR + admin ops + revoke-all.
- **CTRL-CRED-001** (no PAT em client logs) IMPLEMENTA cumulative via PAT once-display + masked + Sentry redaction allowlist.
- **CTRL-PRIV-001** (zero PII em client-side logs) IMPLEMENTA cumulative via safeLog wrapper allowlist.
- **CTRL-PRIV-CONSENT-001..006** reflection cumulative em consent capture UI.
- **PAT-DUAL-APPROVAL-001** (S-13 reuse; collusion-rotation rolling 3-op window — Lote 10.13 canonical) reforced em admin ops.
- **WCAG 2.2 AA** compliance baseline (compliance_matrix.md alignment) IMPLEMENTA full reflection.
- **Não introduz INVs novas** (UI é consumer; per spec contract §8 mantidas only).
- All S-16 controls cumulatively validated.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Playwright config + E2E suite | `apps/web/playwright.config.ts` + `apps/web/e2e/` | TypeScript |
| Lighthouse CI workflow | `.github/workflows/lighthouse.yml` | YAML |
| Cross-browser playwright config | `apps/web/playwright.config.ts` (projects) | TypeScript |
| UX workshop report | `specs/_audits/2026-XX-XX-ux-workshop-s16.md` | Markdown |
| Lighthouse 30d sustained report | `specs/_audits/2026-XX-XX-lighthouse-s16-30d.md` | Markdown |
| CSP enforce sustained report | `specs/_audits/2026-XX-XX-csp-enforce-s16-30d.md` | Markdown |
| Adversarial summary | `specs/_audits/2026-XX-XX-adversarial-summary-s16.md` | Markdown |
| PRR doc S-16 | `specs/04_sprints/_sealed/S16/PRR-S16.md` | Markdown |
| Release notes S-16 | `specs/04_sprints/S16/RELEASE_NOTES.md` | Markdown |
| Conditionally approved waivers + ADRs | `specs/_decisions/ADR-XXXX-*.md` (if applicable) | Markdown |

## 14. Quality Standards

- **14.s16.007.1** Lighthouse ≥ 95 em 4 pillars × 3 routes sustained 30d.
- **14.s16.007.2** axe-core CI 0 violations em todas routes (WI-S16-006 reuse).
- **14.s16.007.3** Cross-browser CI matrix 8 cells verde.
- **14.s16.007.4** UX SUS ≥ 75; time-to-first-PAT ≤ 5 min ≥ 80% trials.
- **14.s16.007.5** CSP enforce prod < 5 violations/dia sustained 30d.
- **14.s16.007.6** PRR doc 5-8 sign-offs canonical documented; non-fictional gates.
- **14.s16.007.7** Cost regression gate: full S-16 UI infra ≤ $300/mês.
- **14.s16.007.8** Adversarial summary 30+ scenarios.

## 15. Test Plan

### Playwright E2E (multi-browser)
- 6 flows: onboarding + consent + DSR + audit + PAT + admin ops.
- 8 projects em playwright.config.ts (Chrome + Firefox + Safari + Edge × latest 2 versions).
- Smoke tests per PR; full E2E scheduled.

### Lighthouse CI
- `@lhci/cli autorun` per PR + scheduled daily em prod.
- 3 routes (/, /dashboard, /privacy).
- 4 pillars (Performance + A11y + Best Practices + SEO).
- Threshold ≥ 95.

### Cross-browser CI matrix
- 8 cells (Chrome + Firefox + Safari + Edge × latest 2 versions).
- Smoke tests per PR; full E2E scheduled.

### UX research
- 5 external developers; rotated per sprint (bias mitigation).
- 4 scenarios; SUS calc; report committed.

### CSP enforce sustained 30d
- 30d staging report-only → refine → enforce prod 30d.
- Violations < 5/dia threshold.

### Adversarial summary aggregation
- 30+ scenarios cross-WI; 100% mitigation rate sustained.

## 16. Failure Modes

- **FM-150** (transient API): Playwright retry com exponential backoff em flaky tests.
- **A11y regression em prod** (axe-core in-prod scan finds violation): post-mortem trigger per spec contract §18.

## 17. Controls

- **CTRL-AUTH-010** enforced cumulative em DSR + admin ops + revoke-all.
- **CTRL-CRED-001** enforced cumulative via PAT once-display + masked.
- **CTRL-PRIV-001** enforced cumulative via safeLog wrapper.
- **CTRL-PRIV-CONSENT-001..006** reflection cumulative em consent capture UI.
- **PAT-DUAL-APPROVAL-001** (S-13 reuse) reforced em admin ops.

## 18. Resilience Patterns

- Playwright retry transient errors (FM-150) em flaky tests.
- Lighthouse scheduled runs (sustained 30d staging).
- Cross-browser matrix smoke tests per PR + full E2E scheduled (cost balance).
- UX workshop rotation per sprint (bias mitigation).
- CSP report endpoint rate-limit + dedup + Slack alert weekly.

## 19. Observability

PRR dashboard:
- Playwright E2E pass rate per flow.
- Lighthouse score per pillar/route (sustained 30d).
- Cross-browser CI matrix verde status.
- UX SUS score sustained ratio.
- CSP enforce violations/dia.
- PRR 5-8 sign-offs canonical status.

## 20. Security & Privacy

**STRIDE delta** (cumulative):
- **Spoofing**: Clerk SDK + WebAuthn + MFA fresh ≤ 30 min em DSR + admin ops; PAT NUNCA via URL params.
- **Tampering**: hardened CSP enforce prod; XSS prevention; clickjacking via X-Frame-Options + frame-ancestors 'none'.
- **Repudiation**: audit log viewer self-service + JSON export sanitized; consent + DSR JWT receipts.
- **Information disclosure**: CTRL-CRED-001 + CTRL-PRIV-001 enforcement cumulative.
- **DoS**: bundle ≤ 250KB; CSP report rate-limited; Playwright cost balance.
- **Elevation of privilege**: dual-approval + collusion-rotation rolling 3-op window; admin ops MFA fresh.

**LINDDUN delta** (cumulative):
- Linkability: telemetry opt-in default-off; pageview anonymized.
- Identifiability: safeLog allowlist; never raw email/pat/tenant_id.
- Non-repudiation: audit log + consent + DSR JWT receipts forensic-grade trail.
- Detectability: CSP report + Sentry + axe-core CI.
- Disclosure: PAT once-display + masked; CSP enforce previne XSS exfiltration.
- Unawareness: WCAG 2.2 AA + 3 locales native speaker reviewed + plain-language Flesch-Kincaid ≤ 8.
- Non-compliance: GDPR Art. 25 + LGPD Art. 6º X + ADA + EU Accessibility Act 2025; LINDDUN review committed.

## 21. Dependencies

### Hard blockers
- WI-S16-001..006 all SEALED.
- 3 external developers recruited para UX workshop.
- 30d staging CSP report-only baseline.
- PRR reviewers available (5-8 roles canonical).

### Soft blockers
- S-09 SEALED (audit events for UX workshop telemetry consumed).
- S-15 SEALED (CLI for onboarding E2E flow).

### Outbound
- S-18 (public docs reference admin UI screenshots).
- S-19 (customer onboarding leverages self-service UI).
- S-20 (GA exige Lighthouse ≥ 95 + WCAG 2.2 AA + 3 locales + 5-dev SUS ≥ 75 sustained).

## 22. Effort PERT

O: 12h, M: 18h, P: 28h → PERT **18.7h** (per spec contract §12; closing WI; Playwright E2E + Lighthouse + cross-browser + UX workshop + CSP enforce + PRR coordination).

## 23. Cost Analysis

**Direct cost**:
- Playwright CI compute: ~$30/mês.
- Lighthouse CI compute: ~$10/mês.
- UX workshop external developers: ~$2k/sprint × 6 sprints/yr = $12k/yr; iterative refinement.
- Sentry events: included em existing tier.

**Total**: ~$12k/yr UX workshop + ~$50/mês CI infra.

**Indirect cost**: 0 a11y regressions + 0 PAT exposure incidents + 0 consent/DSR compliance failures = priceless.

## 24. Post-mortem Hooks

- A11y regression em prod (axe-core in-prod scan finds violation) → 5-Why mandatory + axe-core CI gate review.
- PAT leak em client logs detectado → CRITICAL post-mortem + Security review + redaction reinforce.
- Consent screenshot evidence missed (em audit period) → post-mortem + privacy gap fix.
- DSR form failure (user can't submit) → post-mortem (compliance impact).
- CSP enforce mode disabled em prod → CRITICAL post-mortem + Security review.
- Lighthouse score < 90 sustained > 7d → post-mortem (DX regression).
- UX SUS < 75 sustained > 1 sprint → UX research iteration + Designer engagement.
- Cross-browser matrix red sustained > 24h → SEV-2 release pipeline review.

## 25. Rollback / Recovery

PRR REJECTED → sprint reverts to DRAFT; remediation cycle. RTO ≤ 1 sprint.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Lighthouse score regression | M | L | LOW | L | LOW | CI gate < 95 fails PR; bundle budget enforce; perf review weekly |
| R-002 | Cross-browser Safari quirks | M | L | LOW | L | LOW | Cross-browser CI matrix; visual regression testing optional |
| R-003 | UX SUS < 75 (UX miss) | M | M | MEDIUM | M | LOW | UX research weekly; iterate; Designer engagement |
| R-004 | CSP enforce prod false-positives surge | M | L | LOW | L | LOW | Report-only staging 30d → refined allowlist; SEV-3 alert > 50/dia |
| R-005 | PRR sign-off staffing gap | M | M | HIGH | M | LOW | 2-week notice; alternate reviewers; ADR-0034 solo-tier waiver fallback |
| R-006 | UX workshop bias (5 devs same persona) | M | M | LOW | M | LOW | Rotation per sprint; diverse persona recruitment |
| R-007 | CONDITIONALLY_APPROVED waivers acumulam | L | M | MEDIUM | L | LOW | Waiver expiry mandatory next sprint; quarterly review |
| R-008 | Cross-browser CI matrix flake | M | M | LOW | M | LOW | Retry policy; full E2E scheduled (cost balance) |

## 27. Knowledge Transfer

- Tech talk (1.5h): "S-16 Admin UI Whole-Stack Review + UX Workshop SUS + PRR".
- Doc `docs/internal/s16-ui-tour.md` — full UI tour.
- Doc `docs/internal/s16-ux-research-summary.md` — UX SUS findings.
- PRR-S16 release party post-SEAL com Frontend Lead + QA + Designer + Privacy.
- Onboarding test (5 questions): Lighthouse pillars + axe-core CI gate + UX SUS calc + CSP enforce transition + dual-approval collusion-rotation rationale.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical — sprint ship gate)

Este WI emite o PRR; sign-off do PRR-S16.md doc é o sign-off final S-16 sprint.

**Staffing reality (per ADR-0034 solo-tier)**:

Pré-PRR mandatory check: confirmed canonical reviewers vs pending. Sprint S-16 pode-se SEAL com **5-8 sign-offs canonical** (7 typical) completos. Tier-1 staffing gap = sprint cannot SEAL until staffed OR explicit waiver com expiry + ADR.

| Status atual (2026-04-29) | Roles |
|---|---|
| **Confirmed (3)** | Owner (Gustavo Schneiter); Final Approver (Gustavo Schneiter); Product (Gustavo Schneiter — solo founder dual-hat) |
| **Pending Tier-1 hire/contract (4 specialized canonical roles em STANDARD)** | Frontend Lead, QA, Designer/a11y advisor, Privacy officer |
| **Total pending** | 4 of 7 typical canonical |

**Escalation plan se PRR sem todos canonical staffed**:
1. **Option A — solo-tier waiver**: Owner + Final Approver assume múltiplos dual-hats com explicit ADR (`ADR-0034-solo-tier-prr-waiver.md`). Documenta accepted residual risk + post-staffing review cadence.
2. **Option B — defer SEAL**: spec final permanece DRAFT até staffing closes.
3. **Option C — external advisor pool**: contract per-engagement Designer/a11y advisor + Privacy officer reviewers (lower lead time vs Tier-1 cripto/security/compliance roles em S-13/S-14 HIGH_RISK).

**Recommended path (current state; Lote 10.16 codex P2 fix — PRR independence baseline aligned com S-15 Lote 10.15)**: Option C **mandatory minimum 2 of 4 pending roles** preenchidos via external advisor antes de SEAL (canonical: Designer/a11y advisor + Privacy officer — typical 2-week lead time; cost ~$5-15k engagement). Option A solo-tier dual-hat **NÃO é PRR-independence baseline** (codex P2: WIs marked READY mas 4/7 unstaffed enfraquece PRR review independence). Sprint SEAL gate requires ≥ 5/7 canonical sign-offs com ≥ 2 external (não solo-tier dual-hat). Option B defer SEAL é alternativa válida se Option C cost prohibitive em short sprint (defer 1 sprint until staffed).

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Frontend Lead | _TBD; emphatic — Next.js 15 + Clerk + component library + Playwright E2E_ | _pending_ | _pending_ |
| 4 | QA | _TBD; emphatic — E2E test suite + Lighthouse CI + cross-browser matrix + axe-core 0 violations_ | _pending_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 6 | Designer/a11y advisor | _TBD; emphatic — WCAG 2.2 AA + 3 locales native review + UX SUS workshop + plain-language design_ | _pending_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — CTRL-PRIV-CONSENT reflection + DSR LGPD/GDPR mapping + LINDDUN review + admin UI privacy delta_ | _pending_ | _pending_ |

> Crypto SME folds em Architect role specialization se applicable em PR review (este WI consume CTRL-AUTH-010 + CTRL-CRED-001 baseline; not novel cripto control). Compliance/AppSec NÃO mandatory canonical em STANDARD lane (folded em Product + Privacy officer).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S16-007 (cycle 12.S16.0; STANDARD lane single-phase SEAL D+18; Playwright E2E + Lighthouse CI ≥ 95 + cross-browser matrix + UX SUS ≥ 75 + CSP enforce + PRR 5-8 canonical). |
| 1.1.0 | 2026-05-14 | Gustavo (via Sonnet builder) | SEAL — Playwright e2e suite (23 tests across 11 spec files; 9 PASS / 14 FIXME / 0 FAIL on local chromium); Lighthouse CI config + workflow SHA-pinned; @axe-core/playwright full-page sweep (public pages 0 serious/critical); CSP_ENFORCEMENT report-only→enforce env flag wired (next.config.ts + middleware.ts + README rollout doc); `specs/_audits/sealed/2026-05-14-s16-ux-workshop.md` + `2026-05-14-s16-adversarial-summary.md` committed; PRR-S16 `CONDITIONALLY_APPROVED` (5 waivers W1..W5); spec contract bumped to v1.4.0; ship-gate-discovered HIGH finding F1 (nested `<html>` in merged admin-ui) queued as HF-S17-001. |

## 30. Anti-patterns evitados

- Skip Playwright E2E (mandatory ship gate per DoD §6).
- Skip Lighthouse CI gate ≥ 95 (DX baseline).
- Skip cross-browser CI matrix (release confidence gap).
- Skip UX workshop 5 devs (compliance baseline UX research).
- Skip CSP enforce prod transition (XSS defense baseline).
- APPROVED PRR sem 5-8 sign-offs canonical.
- Production deploy sem Lighthouse ≥ 95 + axe-core 0 violations + cross-browser matrix.
- Waivers acumulando sem expiry.
- Two-phase SEAL com 30d observation window (STANDARD lane single-phase D+18 sufficient).
- External a11y audit scope creep (este WI é STANDARD; defer pós-GA Q1).
- 5 dev sample bias (rotation per sprint; diverse persona recruitment).

---

**Fim WI-S16-007.** **S-16 sprint full WI spec completo (7/7 WIs SOTA STANDARD lane).**

---

## SEAL note (2026-05-14)

Sealed at v1.1.0. PRR-S16 issued `CONDITIONALLY_APPROVED` with five
waivers (UX synthetic, Lighthouse first-CI-run, Clerk/Stripe production
keys, real-Clerk e2e execution, HF-S17-001 nested-`<html>` hotfix). All
prerequisite gates green: `pnpm install --frozen-lockfile`, `typecheck`,
`lint`, `test` (244 / 244), `build`, `e2e:list` (23 tests), `e2e`
(9 PASS / 14 FIXME / 0 FAIL on local chromium). Evidence pack:
`specs/_audits/sealed/2026-05-14-s16-ux-workshop.md`,
`specs/_audits/sealed/2026-05-14-s16-adversarial-summary.md`,
`specs/04_sprints/_sealed/S16/PRR-S16.md`. Cross-WI 36 adversarial scenarios
catalogued; 100% mitigation rate with one ship-gate-discovered HIGH
finding (F1) queued for S-17 hotfix HF-S17-001.
