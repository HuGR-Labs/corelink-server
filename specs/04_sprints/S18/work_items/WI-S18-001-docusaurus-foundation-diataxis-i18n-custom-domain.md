---
id: "WI-S18-001"
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
  - "PRIVACY-MODEL"
tags: ["wi", "s18", "docs", "docusaurus", "diataxis", "algolia", "i18n", "custom-domain", "cf-pages", "low-risk"]
---

# WI-S18-001 — Docusaurus 3.x Foundation em `apps/docs/` Deployed CF Pages Custom Domain `docs.corelink.dev` + SSL Let's Encrypt + Diátaxis Taxonomy Sidebar (Tutorials Learning-oriented + How-to Task-oriented + Reference Information-oriented + Explanation Understanding-oriented; PR Review Checks Taxonomy Fit per Quality Standard 14.s18.2) + Algolia DocSearch Integration + Versioning (Latest + 1 Prior Major; Docusaurus Native) + Edit on GitHub Link per Page + i18n Config 3 Locales en-US (Default) + pt-BR (LGPD Primary) + es-419 (Matching S-11 + S-15 + S-16 Alignment per Lote 10.16) + Foundation para WI-S18-002..005

> **doc_status:** DRAFT · **work_status:** READY · **lane:** LOW_RISK
> **Parent:** [S-18](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S18-001 |
| Título | Docusaurus 3.x foundation + Diátaxis taxonomy + Algolia DocSearch + i18n 3 locales + custom domain `docs.corelink.dev`. |
| Sprint | S-18 |
| Lane | LOW_RISK |
| Forcing factors | none (LOW_RISK; foundation WI; docs sprint não toca tenant data flow path; Docusaurus 3.x open-source canonical; CF Pages free tier baseline; Algolia DocSearch free tier OSS) |

## 1. Intent

Foundation WI do S-18. Entrega o **Docusaurus 3.x foundation** em `apps/docs/` deployed CF Pages custom domain `docs.corelink.dev` + SSL Let's Encrypt automated, Diátaxis taxonomy sidebar 4 categories canonical (tutorial / how-to / reference / explanation; PR review checks taxonomy fit per Quality Standard 14.s18.2), Algolia DocSearch integration (free tier OSS), versioning latest + 1 prior major (Docusaurus native), edit on GitHub link per page, i18n config 3 locales en-US (default) + pt-BR (LGPD primary) + es-419 (matching S-11 + S-15 + S-16 alignment per Lote 10.16). Foundation layer para WI-S18-002..005 (getting started + REAPI auto-gen + SDK guides + compliance/security/pricing pages + closing ship gate).

```typescript
// File: apps/docs/docusaurus.config.ts
import type { Config } from '@docusaurus/types';

const config: Config = {
  title: 'CoreLink',
  tagline: 'Multi-tenant content-addressable cache on Cloudflare',
  url: 'https://docs.corelink.dev',
  baseUrl: '/',
  organizationName: 'humangr-labs',
  projectName: 'corelink-server',
  i18n: {
    defaultLocale: 'en-US',
    locales: ['en-US', 'pt-BR', 'es-419'],
  },
  themeConfig: {
    algolia: {
      appId: process.env.ALGOLIA_APP_ID,
      apiKey: process.env.ALGOLIA_SEARCH_API_KEY, // public search-only key
      indexName: 'corelink',
      contextualSearch: true,
    },
    navbar: {
      title: 'CoreLink',
      items: [
        { type: 'docSidebar', sidebarId: 'tutorials', label: 'Tutorials' },
        { type: 'docSidebar', sidebarId: 'howto', label: 'How-to' },
        { type: 'docSidebar', sidebarId: 'reference', label: 'Reference' },
        { type: 'docSidebar', sidebarId: 'explanation', label: 'Explanation' },
        { to: '/security', label: 'Security' },
        { to: '/compliance', label: 'Compliance' },
        { to: '/pricing', label: 'Pricing' },
        { type: 'localeDropdown', position: 'right' },
        { type: 'docsVersionDropdown', position: 'right' },
      ],
    },
  },
  presets: [
    [
      'classic',
      {
        docs: {
          editUrl: 'https://github.com/humangr-labs/corelink-server/edit/main/apps/docs/',
          versions: { current: { label: 'Latest' } },
        },
      },
    ],
  ],
};
export default config;
```

## 2. Narrative

Docusaurus 3.x é canonical reference em docs frameworks (Stripe + Linear-tier baseline) com built-in i18n + versioning + edit on GitHub + sidebars + Algolia DocSearch integration. Diátaxis taxonomy é canonical reference em docs IA (information architecture) per <https://diataxis.fr/> — 4 categories (tutorial learning-oriented + how-to task-oriented + reference information-oriented + explanation understanding-oriented) força disciplina. CF Pages custom domain `docs.corelink.dev` deploy free tier + SSL Let's Encrypt automated. Algolia DocSearch free tier OSS para dev tools.

Foundation pattern: WI-S18-002..005 consumirão this scaffold (getting started em tutorial; how-to integrate Bazel CI em how-to; REAPI v2 reference em reference; "Why content-addressable cache?" em explanation; SDK guides distributed across categories conforme content type). PR review checks Diátaxis taxonomy fit per Quality Standard 14.s18.2 (every page categorized; reviewer Docs lead).

i18n 3 locales canonical en-US (default) + pt-BR (LGPD primary) + es-419 (matching S-11 + S-15 + S-16 alignment per Lote 10.16). Native speaker review per locale em WI-S18-005 (closing ship gate; Legal local review for legal terms).

**Risk justification LOW_RISK lane (zero forcing factors)**:
- Foundation WI; docs sprint não toca tenant data flow path; Docusaurus 3.x open-source canonical.
- CF Pages free tier baseline; Algolia DocSearch free tier OSS.
- i18n config setup (locales + defaultLocale); native speaker review separate WI-S18-005 closing gate.
- Não há cripto-load-bearing controles novos.

## 3. Customer Impact & Journey

**Persona — External Developer / Build Engineer (prospect)**:
- Diátaxis taxonomy discoverability: tutorial learning-oriented + how-to task-oriented + reference information-oriented + explanation understanding-oriented (find answer ≤ 30s WI-S18-005 UX research baseline).
- Algolia DocSearch integration: search-as-you-type contextual search.
- Versioning latest + 1 prior major: backward compat docs reference.
- Edit on GitHub per page: community contribution friction reduced.
- i18n 3 locales: en-US (default) + pt-BR (LGPD primary) + es-419 (matching S-11 + S-15 + S-16 alignment).

## 4. Capability Mapping

- **CAP-DOCS-001** (getting started 5-min quickstart) — IMPLEMENTA foundation (scaffold; content em WI-S18-002).
- **CAP-DOCS-006** (Diátaxis taxonomy) — IMPLEMENTA primary.
- **CAP-DOCS-007** (i18n + a11y) — IMPLEMENTA primary i18n config (a11y axe-core CI em WI-S18-005).
- Trace: `_spec_contract.md §4` + `remote_cache_product_profile.md` (REAPI v2 API semantics base).

## 5. Tipo

Foundation WI; LOW_RISK lane.

## 6. Escopo

### 6.1 In-scope

1. **Docusaurus 3.x scaffold** em `apps/docs/`:
   - `package.json` com `@docusaurus/core` + `@docusaurus/preset-classic` + `@docusaurus/theme-search-algolia`.
   - `docusaurus.config.ts` com title + tagline + url + baseUrl + organizationName + projectName.
   - `sidebars.ts` com 4 sidebars Diátaxis (tutorials + howto + reference + explanation).
   - `static/` com favicon + logo placeholder.

2. **CF Pages deploy** custom domain `docs.corelink.dev`:
   - GitHub Actions workflow `.github/workflows/docs-deploy.yml` build + deploy CF Pages.
   - Custom domain `docs.corelink.dev` configured CF Pages dashboard.
   - SSL Let's Encrypt automated (CF native).
   - Preview deploys per PR.

3. **Diátaxis taxonomy sidebar** 4 categories canonical:
   - **Tutorials** (learning-oriented): "Build your first cached project in 5 min" placeholder (content em WI-S18-002).
   - **How-to** (task-oriented): "How to configure BYOK", "How to integrate Bazel CI" placeholders.
   - **Reference** (information-oriented): REAPI v2 reference (auto-gen WI-S18-002), CLI reference (WI-S18-003), SDK reference (WI-S18-003).
   - **Explanation** (understanding-oriented): "Why content-addressable cache?", "How dedup works" placeholders.
   - PR review checks Diátaxis taxonomy fit per Quality Standard 14.s18.2 (Docs lead reviewer; PR template enforces category).

4. **Algolia DocSearch integration**:
   - Algolia DocSearch free tier OSS application.
   - Public search-only API key em build-time `ALGOLIA_SEARCH_API_KEY` (never admin key em build).
   - `contextualSearch: true` (filter por locale + version).
   - Index update via Algolia crawler scheduled.

5. **Versioning** (Docusaurus native):
   - `docs/` = latest (current).
   - `versioned_docs/version-1.x/` = 1 prior major (frozen).
   - `versions.json` lista versions disponíveis.
   - Navbar `docsVersionDropdown` para switcher.

6. **Edit on GitHub link per page**:
   - `editUrl: 'https://github.com/humangr-labs/corelink-server/edit/main/apps/docs/'` em preset config.
   - "Edit this page" footer link automated.

7. **i18n config 3 locales**:
   - `defaultLocale: 'en-US'`.
   - `locales: ['en-US', 'pt-BR', 'es-419']` (matching S-11 + S-15 + S-16 alignment per Lote 10.16).
   - `i18n/<locale>/code.json` translation files placeholder (native speaker review WI-S18-005).
   - Navbar `localeDropdown` para switcher.

### 6.2 Out-of-scope (deferred)

- Native speaker translation review per locale (WI-S18-005 closing ship gate).
- WCAG 2.2 AA axe-core CI gate (WI-S18-005).
- Lighthouse ≥ 95 CI gate (WI-S18-005).
- Vale tone consistency lint CI (WI-S18-005).
- lychee broken-link CI (WI-S18-005).
- REAPI auto-gen via `protoc-gen-doc` (WI-S18-002).
- SDK guides content (WI-S18-003).
- Compliance + security + pricing pages (WI-S18-004).
- UX research session 5 dev (WI-S18-005).

## 7. Anti-Scope

- Skip Diátaxis taxonomy (every page categorized; PR review checks).
- Custom docs framework (use Docusaurus 3.x canonical).
- Skip i18n config 3 locales (en-US/pt-BR/es-419 matching S-11 + S-15 + S-16).
- Skip versioning (Docusaurus native; latest + 1 prior major).
- Skip edit on GitHub per page (community contribution baseline).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Docusaurus 3.x foundation + Diátaxis taxonomy + Algolia DocSearch + i18n 3 locales + custom domain

  Scenario: Docusaurus 3.x scaffold em apps/docs/
    Given apps/docs/package.json com @docusaurus/core ^3.x
    When npm install
    Then dependencies resolved
    And docusaurus build succeeds
    And static site generated em apps/docs/build/

  Scenario: CF Pages deploy custom domain docs.corelink.dev
    Given GitHub Actions workflow docs-deploy.yml
    When push to main
    Then CF Pages build + deploy executed
    And custom domain docs.corelink.dev resolves
    And SSL Let's Encrypt automated valid
    And preview deploys per PR working

  Scenario: Diátaxis taxonomy sidebar 4 categories
    Given sidebars.ts com 4 sidebars (tutorials + howto + reference + explanation)
    When docs build
    Then navbar shows 4 category dropdowns
    And PR review checks Diátaxis taxonomy fit per Quality Standard 14.s18.2

  Scenario: Algolia DocSearch integration
    Given themeConfig.algolia configured com appId + apiKey + indexName
    When user types em search box
    Then Algolia DocSearch returns results contextual
    And public search-only API key (never admin key em build)

  Scenario: Versioning latest + 1 prior major
    Given docs/ + versioned_docs/version-1.x/ + versions.json
    When user clicks docsVersionDropdown
    Then switcher shows Latest + 1.x
    And links route correctly per version

  Scenario: Edit on GitHub link per page
    Given editUrl configured em preset
    When user clicks "Edit this page" footer
    Then GitHub edit URL opens em correct file path

  Scenario: i18n config 3 locales (en-US + pt-BR + es-419)
    Given defaultLocale en-US + locales [en-US, pt-BR, es-419]
    When user clicks localeDropdown
    Then switcher shows 3 locales
    And translations resolved per locale (placeholder; native speaker review WI-S18-005)

  Scenario: Diátaxis taxonomy PR review check
    Given new docs page added em PR
    When reviewer Docs lead reviews
    Then PR template enforces category (tutorial/how-to/reference/explanation)
    And PR fails if category missing or wrong

  Scenario: SSL custom domain
    Given custom domain docs.corelink.dev configured CF Pages
    When user visits https://docs.corelink.dev
    Then SSL valid Let's Encrypt automated
    And HSTS header present
```

## 9. Design Decisions

### 9.1 Why Docusaurus 3.x (não MkDocs / Hugo / custom)

- Docusaurus 3.x é canonical reference em docs frameworks (Stripe + Linear-tier baseline).
- Built-in i18n + versioning + edit on GitHub + sidebars + Algolia DocSearch integration.
- React-based; idiomatic em CoreLink stack (TypeScript + React).
- MkDocs Python-based (CoreLink stack TypeScript-first); Hugo Go-based (less idiomatic).
- Custom framework anti-scope (per S-18 anti-scope; reuse open-source canonical).

### 9.2 Why CF Pages custom domain (não Vercel / Netlify)

- CF Pages free tier baseline; CoreLink stack already CF-native (Workers + R2 + D1 + KV).
- Custom domain `docs.corelink.dev` SSL Let's Encrypt automated.
- Preview deploys per PR free tier.

### 9.3 Why Diátaxis taxonomy (não freeform IA)

- Diátaxis canonical reference em docs IA (per <https://diataxis.fr/>).
- 4 categories força disciplina: tutorial learning-oriented + how-to task-oriented + reference information-oriented + explanation understanding-oriented.
- Stripe + Linear + GOV.UK use Diátaxis or equivalent.
- PR review checks taxonomy fit (Quality Standard 14.s18.2).

### 9.4 Why Algolia DocSearch (não ElasticSearch / Lunr)

- Algolia DocSearch free tier OSS para dev tools.
- Built-in Docusaurus integration.
- Search-as-you-type contextual.
- ElasticSearch overkill; Lunr inferior UX.

### 9.5 Why versioning latest + 1 prior major (não all versions)

- Docusaurus native versioning.
- Latest = current; 1 prior major = backward compat reference.
- All versions = maintenance overhead (post-GA roadmap).

### 9.6 Why i18n 3 locales en-US + pt-BR + es-419 (matching S-11 + S-15 + S-16)

- Lote 10.16 alignment (canonical 3 locales across S-11 + S-15 + S-16 + S-18).
- en-US default (English baseline).
- pt-BR LGPD primary (Brazil regulatory).
- es-419 (Latin America Spanish; matching CoreLink target market).
- Native speaker review per locale + Legal local review for legal terms (WI-S18-005 closing ship gate).

### 9.7 ADR potencial?

- Não — Docusaurus 3.x + Diátaxis + Algolia DocSearch são canonical references em S-18 spec contract §17 (no novel decision; ADR não necessário).

## 10. Completeness Criteria

- [ ] **10.s18.001.1** Docusaurus 3.x scaffold em `apps/docs/` (build succeeds).
- [ ] **10.s18.001.2** CF Pages deploy custom domain `docs.corelink.dev` + SSL Let's Encrypt automated (EVT-018).
- [ ] **10.s18.001.3** Diátaxis taxonomy sidebar 4 categories (tutorials + howto + reference + explanation).
- [ ] **10.s18.001.4** Algolia DocSearch integrated (contextualSearch=true; public search-only API key).
- [ ] **10.s18.001.5** Versioning latest + 1 prior major (docsVersionDropdown).
- [ ] **10.s18.001.6** Edit on GitHub link per page (editUrl configured).
- [ ] **10.s18.001.7** i18n config 3 locales en-US + pt-BR + es-419 (localeDropdown).
- [ ] **10.s18.001.8** PR review template enforces Diátaxis taxonomy fit (Docs lead reviewer; per Quality Standard 14.s18.2).
- [ ] **10.s18.001.9** Preview deploys per PR working.

## 11. DoD

- [ ] Docusaurus 3.x scaffold em `apps/docs/` build succeeds.
- [ ] CF Pages deploy custom domain `docs.corelink.dev` SSL valid.
- [ ] Diátaxis taxonomy sidebar 4 categories + PR review template.
- [ ] Algolia DocSearch integrated.
- [ ] Versioning + edit on GitHub + i18n config 3 locales.
- [ ] Adversarial scenarios 3+ documented.

## 12. Invariants Validated

- **CTRL-PRIV-001** (zero PII em screenshots/examples) — IMPLEMENTA foundation; fixture pipeline em WI-S18-004 (compliance/security/pricing pages).
- **Não introduz INVs novas** (sprint consumer; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Docusaurus scaffold | `apps/docs/package.json`, `apps/docs/docusaurus.config.ts`, `apps/docs/sidebars.ts` | TypeScript / JSON |
| GitHub Actions deploy workflow | `.github/workflows/docs-deploy.yml` | YAML |
| i18n config | `apps/docs/i18n/<locale>/code.json` × 3 (placeholder) | JSON |
| PR review template Diátaxis taxonomy | `.github/PULL_REQUEST_TEMPLATE/docs.md` | Markdown |
| Versioned docs scaffold | `apps/docs/versioned_docs/version-1.x/` | Markdown |

## 14. Quality Standards

- **14.s18.001.1** Diátaxis discipline: every page categorized; PR review checks taxonomy fit (per Quality Standard 14.s18.2).
- **14.s18.001.2** Cost regression gate: docs infra ≤ $50/mês (CF Pages free tier + Algolia DocSearch free tier OSS).
- **14.s18.001.3** Custom domain SSL Let's Encrypt automated (CF native).

## 15. Test Plan

### Unit tests
- Docusaurus build succeeds (`docusaurus build` zero errors).
- i18n config 3 locales loaded.
- Sidebars 4 categories Diátaxis loaded.

### Integration tests
- CF Pages deploy succeeds (GitHub Actions workflow green).
- Custom domain docs.corelink.dev resolves SSL valid.
- Algolia DocSearch returns results em search box.
- Versioning switcher works (latest ↔ 1.x).
- localeDropdown switcher works (en-US ↔ pt-BR ↔ es-419).

### Adversarial scenarios (3+)
1. Algolia admin API key inadvertent em build (CI lint check via grep; PR fails).
2. Diátaxis taxonomy missing em new page PR (PR template enforces; PR fails).
3. CF Pages deploy fails custom domain (SSL not provisioned); rollback via DNS revert.

## 16. Failure Modes

- Docusaurus build fails em CI → PR blocked.
- CF Pages deploy fails custom domain SSL → rollback via DNS revert.
- Algolia DocSearch index update fails → manual reindex via Algolia dashboard.

## 17. Controls

- **CTRL-PRIV-001** (zero PII em logs/screenshots) — foundation; fixture pipeline reuse em WI-S18-004.
- PR review template enforces Diátaxis taxonomy fit (Quality Standard 14.s18.2).

## 18. Resilience Patterns

- N/A (docs sprint consumer; non-cripto-load-bearing).

## 19. Observability

- CF Pages deployment status (last deploy ts; alert se > 7d sem deploy).
- Docusaurus build CI status (PR fails se build fails).
- Custom domain SSL valid (CF Pages native).

Não introduz métricas Prometheus per-tenant em foundation WI (per observability_model §3.1; sprint consumer; docs telemetry separate em WI-S18-005 if applicable).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Algolia DocSearch auth via API key in build-time (public search-only key; never admin key em build).
- **Tampering**: docs source git-tracked + reviewer Docs lead.
- **Repudiation**: GitHub Actions deploy log + git commit history.
- **Information disclosure**: nenhuma (foundation WI; content em WI-S18-002..004).
- **DoS**: CF Pages free tier rate-limited; Algolia DocSearch free tier query limited.
- **Elevation of privilege**: docs publish requires PR review + Docs lead sign-off.

**LINDDUN delta**:
- Linkability: nenhuma (foundation WI; content em WI-S18-002..004).
- Identifiability: nenhuma (foundation WI; PAT examples em WI-S18-002 placeholder format).
- Non-repudiation: GitHub Actions deploy log.
- Detectability: CF Pages deployment status; build CI status.
- Disclosure: Algolia public search-only API key (never admin).

## 21. Dependencies

### Hard blockers
- Cloudflare account access (CF Pages + custom domain DNS).
- Algolia DocSearch application approved (OSS free tier).

### Soft blockers
- Branding assets (logo + favicon) — placeholder OK em foundation; refine em WI-S18-005.

### Outbound
- WI-S18-002 (getting started + REAPI auto-gen consumes Diátaxis sidebar).
- WI-S18-003 (SDK guides consumes sidebar reference category).
- WI-S18-004 (compliance + security + pricing pages consumes navbar links).
- WI-S18-005 (i18n native speaker + WCAG + Lighthouse + Vale + lychee + UX research + PRR closing ship gate).

## 22. Effort PERT

O: 12h, M: 18h, P: 28h → PERT **18.7h** (per spec contract §12; foundation WI; Docusaurus scaffold + CF Pages deploy + Diátaxis sidebars + Algolia + i18n config + versioning + edit on GitHub).

## 23. Cost Analysis

**Direct cost**:
- CF Pages free tier: $0/mês.
- Custom domain docs.corelink.dev DNS: ~$15/year (already owned domain).
- Algolia DocSearch free tier OSS: $0/mês.
- GitHub Actions free tier (public repo): $0/mês.

**Total**: ~$2/mês ($15/year amortized).

**Indirect cost**: 0 GTM friction (docs production-ready foundation) + dev adoption baseline = priceless.

## 24. Post-mortem Hooks

- CF Pages deploy fails custom domain → 5-Why + DNS verification + SSL provisioning review.
- Algolia DocSearch admin API key inadvertent em build → CRITICAL post-mortem (security; rotate key + audit log).
- Diátaxis taxonomy bypassed em PR → PR template reinforce + Docs lead review reinforce.

## 25. Rollback / Recovery

CF Pages deploy rollback: revert PR + redeploy previous build. RTO ≤ 5min. Recovery via DNS revert se custom domain SSL issue (Cloudflare API).

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | CF Pages deploy fails custom domain SSL | L | H | LOW | L | LOW | DNS verification + SSL provisioning review; rollback via DNS revert |
| R-002 | Algolia DocSearch admin API key em build | L | M | MEDIUM | L | LOW | CI lint check via grep ALGOLIA_ADMIN_API_KEY; PR fails; public search-only key only em build |
| R-003 | Diátaxis taxonomy bypassed em PR | M | M | LOW | M | LOW | PR template enforces category; Docs lead reviewer; PR fails se category missing |

## 27. Knowledge Transfer

- Tech talk (30min): "CoreLink docs foundation — Docusaurus 3.x + Diátaxis + i18n + CF Pages".
- Doc `docs/internal/s18-docs-foundation.md` — docs publish procedure.
- Onboarding test (3 questions): Diátaxis 4 categories + i18n locales canonical + Algolia DocSearch public-only key rationale.

## 28. Sign-off (LOW_RISK 3 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Docs lead | _TBD; emphatic — Diátaxis discipline + Docusaurus 3.x foundation + i18n config_ | _pending_ | _pending_ |

> Cross-functional review gate (Finance/Legal/Privacy/Security) NÃO aplicável em WI-S18-001 (foundation WI; content sensitive em WI-S18-004 compliance/security/pricing pages onde cross-functional gate canonical mandatory).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S18-001 (cycle 12.S18.0; LOW_RISK lane; Docusaurus 3.x foundation + Diátaxis taxonomy + Algolia DocSearch + i18n 3 locales + custom domain `docs.corelink.dev`). |

## 30. Anti-patterns evitados

- Custom docs framework (use Docusaurus 3.x canonical).
- Skip Diátaxis taxonomy discipline (PR review enforces).
- Skip i18n config 3 locales (matching S-11 + S-15 + S-16 alignment).
- Skip versioning (Docusaurus native; latest + 1 prior major).
- Algolia admin API key em build (CI lint check enforces public-only).

---

**Fim WI-S18-001.**
