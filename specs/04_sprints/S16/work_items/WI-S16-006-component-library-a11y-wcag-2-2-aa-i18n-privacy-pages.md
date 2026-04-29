---
id: "WI-S16-006"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-16"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s16", "ui", "component-library", "a11y", "wcag-2.2-aa", "i18n", "axe-core", "screen-reader", "privacy-pages", "sub-processors", "standard"]
---

# WI-S16-006 — Component Library (Radix UI Primitives + Tailwind CSS): Button + Input + Form + Modal + Tabs + Accordion + Toast + Spinner + Card + Table + Tooltip — All WAI-ARIA Compliant + **Accessibility WCAG 2.2 AA** (Keyboard Navigation 100% Tab Order Logical + Focus Indicators Visible; Screen Reader Test NVDA + VoiceOver em Consent + DSR + Audit Viewer + PAT Mgmt — Manual Walkthrough OK; Color Contrast ≥ 4.5:1 Normal Text + ≥ 3:1 Large Text Verified via axe-core + Stark Plugin Figma Design Review; ARIA Landmarks Corretos `<nav>` + `<main>` + `<aside>` + `<header>` + `<footer>` Semantic + `aria-label` em Ambiguous Regions; **axe-core CI Test em Todas Routes Zero Violations Gate** PR Fails se Any) + **i18n Full 3 Locales** en-US Default + pt-BR LGPD Primary + es-419 LATAM (Detection via Accept-Language Header + Manual Locale Switcher Footer; Native Speaker Review per Locale Community Translator Reviewed Iterative Refinement; Missing-translation = Build Fail per Quality Standard 14.s16.6 CI Gate) + **Privacy + Sub-processors Pages**: `/privacy` Renderiza Markdown de `legal/privacy-notice/v<M.m>.md` (Latest Semver) via @next/mdx + `/privacy/changelog` Mostra Diff entre Versões via react-diff-viewer + `/privacy/sub-processors` Auto-generated de `legal/sub-processors.md` + Header Notice (Banner) de **Mudança em Sub-processor Pendente** se Broadcast Triggered (S-11 R-S11-13 Alignment)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-16](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S16-006 |
| Título | Component library + a11y WCAG 2.2 AA + i18n full 3 locales + privacy/sub-processors pages. |
| Sprint | S-16 |
| Lane | STANDARD |
| Forcing factors | none (a11y + i18n + privacy pages são compliance baseline; WCAG 2.2 AA target ADA + EU Accessibility Act 2025) |

## 1. Intent

Entregar **component library WAI-ARIA compliant** + **WCAG 2.2 AA full** + **i18n native speaker reviewed 3 locales** + **privacy/sub-processors pages mdx versioned**. Completes UX surface; baseline GA conversion S-20.

```typescript
// File: apps/web/src/components/ui/button.tsx
import * as React from 'react';
import { Slot } from '@radix-ui/react-slot';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'destructive' | 'ghost';
  size?: 'sm' | 'md' | 'lg';
  asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ variant = 'primary', size = 'md', asChild, className, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button';
    return (
      <Comp
        ref={ref}
        className={cn(buttonVariants({ variant, size }), className)}
        // WAI-ARIA compliant: aria-label fallback if children empty; focus indicators visible
        {...props}
      />
    );
  }
);
Button.displayName = 'Button';
```

## 2. Narrative

WCAG 2.2 AA é baseline regulatory para ADA + EU Accessibility Act 2025 — mais stringent que 2.1 (target competitors Stripe/Auth0). GOV.UK Design System é gold-standard a11y patterns; CoreLink S-16 entrega parity + 3 locales nativos reviewed.

Component library reuse Radix UI primitives (gold-standard accessible primitives) + Tailwind CSS para styling. axe-core CI gate em todas routes (zero violations PR fails). Manual screen reader test (NVDA + VoiceOver) em consent + DSR + audit viewer + PAT mgmt (compliance-critical surfaces). i18n full com native speaker review per locale; missing-translation = build fail.

Privacy + sub-processors pages: `/privacy` mdx semver versioned via @next/mdx; `/privacy/changelog` diff entre versões; `/privacy/sub-processors` auto-generated; broadcast banner se mudança pendente (S-11 R-S11-13).

**Risk justification STANDARD lane**:
- WCAG 2.2 AA compliance + a11y + i18n são compliance baseline (não introduz cripto-load-bearing).
- axe-core CI + manual screen reader é well-bounded discipline.
- Privacy pages mdx render é static rendering (no novel architecture).
- Sub-processors page auto-generated (single source of truth `legal/sub-processors.md`).

## 3. Customer Impact & Journey

**Persona 1 — End-user (assistive tech)**:
- Screen reader (NVDA + VoiceOver) walkthrough OK em consent + DSR + audit + PAT mgmt.
- Keyboard navigation 100% (tab order logical; focus indicators visible).
- Color contrast ≥ 4.5:1 normal; ≥ 3:1 large.
- ARIA landmarks corretos em todas routes.

**Persona 2 — End-user (3 locales)**:
- en-US default + pt-BR LGPD primary + es-419 LATAM auto-detected via Accept-Language.
- Manual locale switcher em footer (cookie persisted).
- Native speaker reviewed per locale (community translator reviewed; iterative refinement).
- Missing-translation = build fail (no partial coverage shipped).

**Persona 3 — Privacy Officer cliente / Compliance auditor**:
- Privacy notice page versioned mdx (`/privacy/v<M.m>` semver); diff entre versões em `/privacy/changelog`.
- Sub-processors page auto-generated de `legal/sub-processors.md` (single source of truth).
- Broadcast banner se mudança em sub-processor pendente (S-11 R-S11-13).

**Persona 4 — Acessibilidade auditor (ADA + EU Accessibility Act 2025)**:
- WCAG 2.2 AA compliance verified via axe-core CI 0 violations + manual screen reader walkthrough OK.
- Color contrast verified via axe-core + Stark Figma plugin design review.
- Keyboard nav 100%; ARIA landmarks; focus indicators visible.

## 4. Capability Mapping

- **CAP-UI-007** (Privacy + Sub-processors pages) — IMPLEMENTA primary; mdx semver + diff + auto-gen + broadcast banner.
- **CAP-UI-009** (i18n + a11y baseline) — IMPLEMENTA primary; WCAG 2.2 AA + 3 locales native speaker reviewed.
- Trace: `_spec_contract.md §4 + §5.5 (R-S16-9..12) + §5.6 (R-S16-13..14)` + `compliance_matrix.md` (WCAG 2.2 AA + LGPD + GDPR + ADA + EU Accessibility Act 2025) + `S-11` (R-S11-13 broadcast trigger).

## 5. Tipo

Feature WI; STANDARD lane; component library + a11y + i18n + privacy pages.

## 6. Escopo

### 6.1 In-scope

1. **Component library** em `apps/web/src/components/ui/`:
   - **Primitives** (Radix UI primitives + Tailwind CSS):
     - `Button` (variants: primary/secondary/destructive/ghost; sizes: sm/md/lg).
     - `Input` (text/email/password/number; aria-describedby for help text + errors).
     - `Form` (react-hook-form + zod validation; aria-invalid + aria-required).
     - `Modal` (Radix Dialog; aria-modal + focus trap + ESC close).
     - `Tabs` (Radix Tabs; arrow key navigation; aria-selected + aria-controls).
     - `Accordion` (Radix Accordion; aria-expanded + aria-controls).
     - `Toast` (Radix Toast; aria-live polite/assertive).
     - `Spinner` (aria-label "Loading"; aria-busy).
     - `Card` (semantic article wrapper).
     - `Table` (sortable; aria-sort; row select with aria-selected).
     - `Tooltip` (Radix Tooltip; aria-describedby; keyboard accessible).
     - `Select` (Radix Select; arrow key navigation; combobox pattern).
     - `Checkbox` + `Radio` (Radix Checkbox/RadioGroup; aria-checked).
   - **All WAI-ARIA compliant** per W3C ARIA Authoring Practices 1.2.
   - Storybook (or Histoire) catalog em `apps/web/src/components/ui/.storybook/`; component-level testing.

2. **Accessibility WCAG 2.2 AA**:
   - **Keyboard navigation 100%**:
     - Tab order logical (DOM order matches visual flow).
     - Focus indicators visible (≥ 2px outline + sufficient contrast).
     - Skip-to-main-content link em header.
     - Escape key closes modals.
     - Arrow keys navigate em tabs/select/menu.
   - **Screen reader test** (manual walkthrough OK; documented em `specs/_audits/2026-XX-XX-screen-reader-walkthrough-s16.md`):
     - NVDA (Windows) em consent + DSR + audit viewer + PAT mgmt.
     - VoiceOver (macOS) em consent + DSR + audit viewer + PAT mgmt.
     - All flows fully usable via screen reader (no visual-only cues).
   - **Color contrast** ≥ 4.5:1 normal text; ≥ 3:1 large text (≥ 18pt or ≥ 14pt bold):
     - Verified via axe-core CI.
     - Verified via Stark Figma plugin em design review.
     - Tailwind CSS theme tokens canonical (no ad-hoc hex codes).
   - **ARIA landmarks corretos**:
     - `<header>` (top-level navigation).
     - `<nav>` (primary nav + aria-label "Primary navigation").
     - `<main>` (page content).
     - `<aside>` (sidebar; aria-label "Sidebar").
     - `<footer>` (legal links + locale switcher).
     - `aria-label` em ambiguous regions (e.g., search input).
   - **axe-core CI gate em todas routes**:
     - `@axe-core/playwright` em E2E suite.
     - Zero violations gate (PR fails se any).
     - Continuous monitoring em staging via axe-core scheduled runs.

3. **i18n full 3 locales**:
   - **en-US** (default).
   - **pt-BR** (LGPD primary; SP/RJ Brazilian Portuguese conventions).
   - **es-419** (LATAM Spanish; broader than es-ES for LATAM market).
   - **Detection**: `Accept-Language` header primary; manual locale switcher em footer (cookie persisted).
   - **Native speaker review per locale** (community translator reviewed; iterative refinement):
     - en-US: Gustavo Schneiter (Owner) primary.
     - pt-BR: native speaker reviewer (TBD; emphatic — primary LGPD market).
     - es-419: native speaker reviewer (TBD; emphatic — LATAM market).
     - Reviews iterative; documented em `specs/_audits/2026-XX-XX-i18n-native-review-s16.md`.
   - **Missing-translation = build fail** per Quality Standard 14.s16.6 CI gate (already em WI-S16-001 baseline; here verified em CI).
   - **Translation files** em `apps/web/src/messages/{en-US,pt-BR,es-419}.json` (full per-locale completeness).
   - **Plain-language Flesch-Kincaid grade ≤ 8** per locale verified em CI (using `text-statistics` ou equivalent for English; `Indice Legibilidade Flesch (BR adaptation)` for pt-BR; `Fernández-Huerta` for es).

4. **Privacy + sub-processors pages**:
   - **`/privacy`** renderiza markdown de `legal/privacy-notice/v<M.m>.md` (latest semver) via @next/mdx:
     - Frontmatter: `version: "1.2.0"`, `effective_date: "2026-04-01"`, `locale_supported: ["en-US", "pt-BR", "es-419"]`.
     - Renders content + version banner ("Effective <date>; current version <semver>").
     - Cross-link to changelog: "View changes in v1.2.0 vs v1.1.0".
   - **`/privacy/changelog`** mostra diff entre versões via `react-diff-viewer`:
     - Lists all versions chronologically (latest first).
     - Per pair, render diff (additions/deletions).
     - Click entry to expand full diff view.
   - **`/privacy/sub-processors`** auto-generated de `legal/sub-processors.md`:
     - Markdown source canonical (single source of truth).
     - Auto-rendered table: name + region + role + DPA URL + last_updated.
     - Build script `scripts/regenerate_sub_processors.py` runs em CI from `legal/sub-processors.md` source.
   - **Header notice (banner) de mudança em sub-processor pendente** se broadcast triggered (S-11 R-S11-13 alignment):
     - Banner UI: "We are updating our sub-processors. New sub-processor X effective <date>. Click to review.".
     - Triggered via S-11 R-S11-13 broadcast event (e.g., new sub-processor onboarded; existing rotated).
     - Banner dismissable per user (cookie persisted); reappears se new broadcast triggered.

5. **Accessibility documentation**:
   - `apps/web/docs/accessibility.md` — overview WCAG 2.2 AA + commitments.
   - `apps/web/docs/keyboard-shortcuts.md` — list of keyboard shortcuts.

### 6.2 Out-of-scope (deferred)

- Closing PRR + Lighthouse + UX workshop (WI-S16-007).
- Component library Storybook deploy public (pós-GA Q1).
- A11y external audit formal certification (pós-GA Q1).
- Additional locales (fr, de, ja, etc.) — pós-GA enterprise.

## 7. Anti-Scope

- Skip axe-core CI gate (a11y regression risk).
- Skip manual screen reader test (compliance gap; ADA + EU Accessibility Act).
- Color contrast < 4.5:1 normal text (WCAG 2.2 AA violation).
- Tab order non-logical (WCAG 2.4.3 violation).
- Missing ARIA landmarks (WCAG 2.4.1 violation).
- Skip native speaker review per locale (i18n quality gap).
- Missing-translation tolerated em build (i18n drift).
- Privacy notice non-versioned (compliance gap).
- Sub-processors page hardcoded (not auto-gen) — single source of truth violation.
- Broadcast banner missed (S-11 R-S11-13 alignment gap).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Component library + a11y WCAG 2.2 AA + i18n full + privacy pages

  Scenario: Component library WAI-ARIA compliant
    Given Storybook catalog rendered
    When component tested via axe-core
    Then 0 violations em todos primitives (Button, Input, Form, Modal, etc.)
    And aria-label/aria-describedby/role attributes corretos

  Scenario: axe-core CI gate em todas routes
    Given E2E Playwright suite runs CI
    When @axe-core/playwright runs em todas routes
    Then 0 violations gate
    And PR fails se any violation

  Scenario: Keyboard navigation 100%
    Given user navigates via Tab key only
    When traversing /dashboard / /audit / /consent / /dsr / /admin/ops
    Then tab order matches visual flow
    And focus indicators visible (≥ 2px outline)
    And ESC closes modals; arrow keys navigate em tabs/select

  Scenario: Screen reader walkthrough OK
    Given NVDA (Windows) running
    When user navigates consent + DSR + audit + PAT mgmt
    Then all flows fully usable via screen reader
    And no visual-only cues (all conveyed via aria-label or text)
    Given VoiceOver (macOS) running
    Then same coverage verified

  Scenario: Color contrast ≥ 4.5:1 normal + 3:1 large
    Given Tailwind CSS theme rendered
    When axe-core checks contrast
    Then normal text ≥ 4.5:1 contrast ratio
    And large text (≥ 18pt or ≥ 14pt bold) ≥ 3:1
    And no ad-hoc hex codes (theme tokens canonical)

  Scenario: ARIA landmarks corretos
    Given user navigates any route
    Then <header> / <nav> / <main> / <aside> / <footer> semantic present
    And aria-label em ambiguous regions
    And skip-to-main-content link em header

  Scenario: i18n 3 locales full coverage
    Given translation files en-US + pt-BR + es-419 committed
    When build runs CI
    Then no missing keys (missing-translation = build fail)
    And native speaker review per locale documented em audit doc

  Scenario: Locale detection via Accept-Language
    Given user sends Accept-Language: pt-BR
    When user navigates /
    Then UI rendered em pt-BR
    And locale switcher in footer overrides via cookie

  Scenario: Plain-language Flesch-Kincaid ≤ 8 per locale
    Given notice + form labels rendered em pt-BR
    When CI computes readability metric
    Then grade ≤ 8 (BR adaptation)
    And native speaker review confirms appropriate tone

  Scenario: /privacy renderiza mdx semver
    Given legal/privacy-notice/v1.2.0.md committed
    When user navigates /privacy
    Then version banner "Effective <date>; v1.2.0" displayed
    And content rendered via @next/mdx

  Scenario: /privacy/changelog mostra diff
    Given v1.0 + v1.1 + v1.2 committed
    When user navigates /privacy/changelog
    Then versions listed chronologically (latest first)
    And per pair diff (react-diff-viewer)

  Scenario: /privacy/sub-processors auto-generated
    Given legal/sub-processors.md committed (Cloudflare + Stripe + Clerk + Sentry)
    When build runs scripts/regenerate_sub_processors.py
    Then /privacy/sub-processors renderiza table com name + region + role + DPA URL
    And single source of truth

  Scenario: Broadcast banner sub-processor change pendente
    Given S-11 R-S11-13 broadcast triggered (new sub-processor X)
    When user navigates any route
    Then banner "We are updating our sub-processors. New sub-processor X effective <date>" visible
    And banner dismissable (cookie persisted)
    And reappears se new broadcast triggered
```

## 9. Design Decisions

### 9.1 Why Radix UI primitives (não Headless UI ou shadcn from scratch)

- Radix UI: gold-standard accessible primitives (WAI-ARIA Authoring Practices 1.2 reference impl).
- Headless UI: less mature; Tailwind-tied.
- shadcn from scratch: NIH effort; reinventing wheel.

### 9.2 Why Tailwind CSS + theme tokens canonical (não CSS-in-JS)

- Tailwind: utility-first; theme tokens canonical (no drift).
- CSS-in-JS: runtime cost; bundle bloat.
- Theme tokens canonical = consistent contrast + spacing.

### 9.3 Why WCAG 2.2 AA (não 2.1 AA)

- ADA + EU Accessibility Act 2025 baseline trending to 2.2.
- 2.2 vs 2.1: adds 9 new success criteria (focus appearance, drag movements, target size, etc.); more stringent baseline.
- Competitors Stripe/Auth0 still at 2.1; CoreLink S-16 differentiator.

### 9.4 Why 3 locales (en-US + pt-BR + es-419)

- en-US: default global; English market primary.
- pt-BR: LGPD primary (Brazilian market regulatory requirement).
- es-419: LATAM Spanish (broader than es-ES; covers Mexico + Argentina + Chile + Colombia primary markets).
- Additional locales (fr, de, ja, etc.) deferred pós-GA enterprise.

### 9.5 Why mdx (não plain markdown ou JSON)

- mdx: markdown + JSX; allows components em notice (e.g., `<DPALink />`, `<ReadMoreButton />`).
- Plain markdown: limited; no interactive components.
- JSON: not human-friendly for legal review.

### 9.6 Why sub-processors auto-gen (não hardcoded UI)

- Single source of truth `legal/sub-processors.md`; UI rendering automatic.
- Hardcoded UI = drift risk + manual sync burden.

### 9.7 ADR potencial?

- Não. Patterns reused (Radix UI + Tailwind + @next/mdx + native speaker review). No novel architecture decision.

## 10. Completeness Criteria

- [ ] **10.s16.006.1** Component library 13+ primitives implemented + Storybook catalog (EVT-018).
- [ ] **10.s16.006.2** axe-core CI gate em todas routes; 0 violations (EVT-019).
- [ ] **10.s16.006.3** Manual screen reader test (NVDA + VoiceOver) em consent + DSR + audit + PAT mgmt OK (EVT-019).
- [ ] **10.s16.006.4** Color contrast ≥ 4.5:1 normal + 3:1 large via axe-core + Stark Figma plugin.
- [ ] **10.s16.006.5** Keyboard navigation 100% tab order logical + focus indicators visible.
- [ ] **10.s16.006.6** ARIA landmarks corretos + skip-to-main-content link.
- [ ] **10.s16.006.7** i18n 3 locales full coverage; native speaker reviewed per locale (EVT-018).
- [ ] **10.s16.006.8** Plain-language Flesch-Kincaid ≤ 8 per locale CI gate.
- [ ] **10.s16.006.9** /privacy mdx semver + /privacy/changelog diff + /privacy/sub-processors auto-gen.
- [ ] **10.s16.006.10** Broadcast banner sub-processor pendente integrated S-11 R-S11-13.

## 11. DoD

- [ ] Component library deployed staging Storybook catalog.
- [ ] axe-core CI gate verde 0 violations todas routes.
- [ ] Manual screen reader walkthrough documented em audit doc.
- [ ] i18n 3 locales full coverage; native speaker reviews documented.
- [ ] Privacy + changelog + sub-processors pages rendered + tested.
- [ ] Broadcast banner integrated S-11 R-S11-13.
- [ ] Tests: unit (component primitives + locale switcher + sub-processors render) + integration (axe-core E2E todas routes; manual screen reader walkthrough) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-PRIV-001** (zero PII em client-side logs) reforced via consistency com WI-S16-001 baseline.
- **Não introduz INVs novas** (UI é consumer; per spec contract §8).
- **WCAG 2.2 AA** compliance baseline (compliance_matrix.md alignment).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Component library | `apps/web/src/components/ui/` | TypeScript |
| Storybook config | `apps/web/.storybook/` | TypeScript |
| Translation files (full) | `apps/web/src/messages/{en-US,pt-BR,es-419}.json` | JSON |
| Privacy notice mdx | `legal/privacy-notice/v1.2.0.md` | Markdown+JSX |
| Privacy changelog page | `apps/web/src/app/(legal)/privacy/changelog/page.tsx` | TypeScript |
| Sub-processors auto-gen script | `scripts/regenerate_sub_processors.py` | Python |
| Sub-processors source | `legal/sub-processors.md` | Markdown |
| Broadcast banner component | `apps/web/src/components/sub-processor-banner.tsx` | TypeScript |
| A11y documentation | `apps/web/docs/accessibility.md` | Markdown |
| Screen reader walkthrough audit | `specs/_audits/2026-XX-XX-screen-reader-walkthrough-s16.md` | Markdown |
| i18n native review audit | `specs/_audits/2026-XX-XX-i18n-native-review-s16.md` | Markdown |

## 14. Quality Standards

- **14.s16.006.1** axe-core CI 0 violations em todas routes.
- **14.s16.006.2** Test coverage ≥ 80% (component primitives + locale switcher + sub-processors render).
- **14.s16.006.3** SAST: Radix UI + react-diff-viewer + @next/mdx zero CVEs HIGH/CRITICAL.
- **14.s16.006.4** Manual screen reader walkthrough OK em compliance-critical surfaces (consent + DSR + audit + PAT mgmt).
- **14.s16.006.5** Color contrast ≥ 4.5:1 normal text; ≥ 3:1 large text (axe-core + Stark Figma plugin).
- **14.s16.006.6** Native speaker review per locale documented (community translator reviewed; iterative refinement).
- **14.s16.006.7** Missing-translation = build fail (CI gate per Quality Standard 14.s16.6).
- **14.s16.006.8** Plain-language Flesch-Kincaid ≤ 8 per locale (BR adaptation for pt-BR; Fernández-Huerta for es-419).

## 15. Test Plan

### Unit tests (≥ 80% coverage)
- Component primitives: ARIA attributes corretos; keyboard handlers wired.
- Locale switcher: cookie persisted; UI re-renders.
- Sub-processors render: parses `legal/sub-processors.md` + table generated.
- Broadcast banner: displays when S-11 R-S11-13 event triggered; dismissable cookie.

### Integration tests (E2E staging)
- axe-core E2E em todas routes (zero violations).
- Manual screen reader walkthrough (NVDA + VoiceOver) em consent + DSR + audit + PAT mgmt; documented.
- i18n 3 locales coverage: navigate em pt-BR + es-419 + en-US; all keys present.
- Privacy + changelog + sub-processors pages rendered correctly.

### Negative scenarios (≥ 4)
1. **Missing translation key**: build fails CI.
2. **Color contrast < 4.5:1**: axe-core CI fails PR.
3. **Tab order non-logical**: keyboard navigation test fails.
4. **Screen reader announces visual-only cue**: walkthrough fails; iteration required.
5. **Hardcoded sub-processor in UI**: drift detected; auto-gen script enforced em CI.
6. **Broadcast banner missed when S-11 R-S11-13 triggered**: integration test fails.

### Cross-browser
- axe-core E2E em Chrome + Firefox + Safari + Edge; full matrix em WI-S16-007.

## 16. Failure Modes

- **FM-150** (transient API): broadcast banner refetch retry com exponential backoff.
- **A11y regression em prod** (axe-core in-prod scan finds violation): post-mortem trigger per spec contract §18.

## 17. Controls

- **CTRL-PRIV-001** (zero PII em client logs) consumed.
- **WCAG 2.2 AA compliance** (compliance_matrix.md baseline) — IMPLEMENTA full reflection.

## 18. Resilience Patterns

- Service Worker cache em SSG public routes (offline-first `/privacy|/sub-processors|/legal/*`).
- Retry transient errors (FM-150) em broadcast banner refetch.
- Native speaker review per locale (iterative refinement; gap-fill via community translators).

## 19. Observability

UI métricas Prometheus snake_case:
- `corelink_admin_ui_axe_violation_total{rule_id}` counter (alert > 0).
- `corelink_admin_ui_locale_switch_total{from, to}` counter.
- `corelink_admin_ui_privacy_changelog_view_total` counter.
- `corelink_admin_ui_sub_processors_view_total` counter.
- `corelink_admin_ui_broadcast_banner_dismissed_total` counter.

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: locale switcher cookie signed (no plaintext drift).
- **Tampering**: privacy notice mdx versioned + immutable per release; `legal/sub-processors.md` source canonical.
- **Repudiation**: privacy changelog forensic-grade trail.
- **Information disclosure**: ARIA labels never expose secrets; sub-processors page public legal evidence.
- **DoS**: Service Worker cache (offline-first SSG); broadcast banner rate-limited refetch.

**LINDDUN delta**:
- **Unawareness**: WCAG 2.2 AA compliance + native speaker review per locale + plain-language Flesch-Kincaid ≤ 8.
- **Non-compliance**: ADA + EU Accessibility Act 2025 + LGPD Art. 9º (transparency em locale primário) + GDPR Art. 12 (transparent information); LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-component-library-s16.md`.

## 21. Dependencies

### Hard blockers
- WI-S16-001 SEALED (skeleton + Clerk + CSP + i18n base).
- WI-S16-003 SEALED (consent UI consume component primitives).
- WI-S16-004 SEALED (DSR form consume component primitives).
- WI-S16-005 SEALED (audit viewer + admin ops consume component primitives).
- S-11 SEALED (R-S11-13 broadcast event + privacy notice mdx versioned).

### Soft blockers
- Native speaker reviewers identified pt-BR + es-419 (community translators OR contract).

### Outbound
- WI-S16-007 (E2E axe-core + Lighthouse + UX workshop + closing PRR).

## 22. Effort PERT

O: 8h, M: 14h, P: 22h → PERT **14.3h** (per spec contract §12; component library + a11y + i18n full + privacy pages + broadcast banner).

## 23. Cost Analysis

- Native speaker reviewers: ~$500/sprint × 2 (pt-BR + es-419) = $1k/sprint; iterative.
- Storybook hosting (private): free (CF Pages).
- Total: ~$1k/sprint reviewers + ~$0/mês infra.

## 24. Post-mortem Hooks

- A11y regression em prod (axe-core in-prod scan finds violation) → 5-Why mandatory per spec contract §18.
- Missing translation deployed em prod → i18n debt review.
- Color contrast regression → a11y review + theme tokens audit.
- Sub-processors page outdated > 7d → auto-gen script broken; SEV-2 post-mortem.
- Broadcast banner missed em prod → S-11 R-S11-13 integration gap; SEV-2.

## 25. Rollback / Recovery

A11y regression detected → revert via CF Pages rollback; component library backward-compatible (no breaking changes em Storybook catalog).

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | A11y compliance late discovery (post-launch audit findings) | M | L | LOW | L | LOW | axe-core CI gate from day 1 + manual screen reader review + WCAG 2.2 AA target |
| R-002 | i18n quality issues per locale | M | L | LOW | L | LOW | Native speaker review + iterative refinement; missing-translation = build fail |
| R-003 | Color contrast regression em theme update | L | M | LOW | L | LOW | Theme tokens canonical (no ad-hoc); axe-core CI gate |
| R-004 | Cross-browser rendering issues (Safari quirks) | M | L | LOW | L | LOW | Visual regression testing (Percy or equivalent); cross-browser CI matrix em WI-S16-007 |
| R-005 | Sub-processors page outdated (auto-gen script broken) | L | M | MEDIUM | L | LOW | CI runs script per build; integration test |
| R-006 | Native speaker reviewer staffing slip | M | M | LOW | M | LOW | Community translators + contract fallback; iterative refinement allowed |

## 27. Knowledge Transfer

- Tech talk (1.5h): "CoreLink Component Library + WCAG 2.2 AA + i18n Native Review + Privacy Pages mdx".
- Doc `apps/web/docs/accessibility.md` — overview + commitments.
- Doc `apps/web/docs/keyboard-shortcuts.md` — list.
- Onboarding test (5 questions): WCAG 2.2 vs 2.1 differences + axe-core CI gate + native speaker review per locale + Flesch-Kincaid grade target + sub-processors auto-gen rationale.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Frontend Lead | _TBD; emphatic — component library + Radix UI integration + Storybook_ | _pending_ |
| 4 | QA | _TBD; emphatic — axe-core CI + manual screen reader walkthrough + i18n locale tests_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | Designer/a11y advisor | _TBD; emphatic — WCAG 2.2 AA compliance + color contrast + ARIA landmarks + Stark Figma plugin design review_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — privacy notice mdx versioning + sub-processors auto-gen + broadcast banner S-11 R-S11-13 alignment_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical. Designer/a11y advisor canonical em S-16-006 (a11y compliance-critical).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S16-006 (cycle 12.S16.0; component library + a11y WCAG 2.2 AA + i18n full 3 locales + privacy/sub-processors pages). |

## 30. Anti-patterns evitados

- Skip axe-core CI gate (a11y regression risk).
- Skip manual screen reader test (compliance gap; ADA + EU Accessibility Act).
- Color contrast < 4.5:1 normal text (WCAG 2.2 AA violation).
- Tab order non-logical (WCAG 2.4.3 violation).
- Missing ARIA landmarks (WCAG 2.4.1 violation).
- Skip native speaker review per locale (i18n quality gap).
- Missing-translation tolerated em build (i18n drift).
- Privacy notice non-versioned (compliance gap).
- Sub-processors page hardcoded (single source of truth violation).
- Broadcast banner missed (S-11 R-S11-13 alignment gap).
- Ad-hoc hex codes (theme tokens canonical).
- Visual-only cues (a11y violation; screen reader gap).

---

**Fim WI-S16-006.**
