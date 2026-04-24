---
id: "SPEC-CONTRACT-S16"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s16", "ui", "admin", "frontend", "wcag-2.2-aa", "i18n", "consent", "dsr-ui", "standard", "sota-v1.1"]
---

# Spec Contract — S-16: Frontend Admin UI (Tenant Self-Service + Consent + DSR + Audit Viewer)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-16 |
| Nome | Frontend Admin UI |
| Lane | STANDARD |
| Lane forcing factors | n/a (STANDARD) — UI consume APIs já validated; CTRL-PRIV-CONSENT enforcement reside em S-11 backend; UI é capture frontend |
| Duração estimada | 3 semanas |
| WIs antecipados | 7 |
| SOTA target | Self-service UI WCAG 2.2 AA + 3 locales + Lighthouse ≥ 95 + accessible consent capture + DSR form + audit viewer |

## 1. Objetivo

Entregar **Frontend admin UI production-grade** (Next.js 15 + CF Pages) com **tenant self-service completo**: tenant onboarding flow (signup → DPA → billing setup; backend handlers em S-03/S-10), usage dashboard com Grafana proxy, audit log viewer queryando CloudEvents R2, consent management UI (CTRL-PRIV-CONSENT-001..006 capture com proof of informed), DSR request form (6 direitos S-11), PAT management, privacy notice page versioned, sub-processors page, billing overview. UI é essencial pra self-service; sem ela onboarding/suporte ficam manuais e GA scaling impossível.

**Por que SOTA:** competitors têm admin UI básico sem consent capture compliance-grade nem audit viewer self-service. CoreLink S-16 entrega: (a) consent form com 6-field proof of informed (CTRL-PRIV-CONSENT-001..006); (b) DSR form com identity re-auth + signed JWT receipt; (c) audit viewer com per-tenant filtering; (d) WCAG 2.2 AA + 3 locales. Reference: **GOV.UK Design System** (a11y patterns), **Stripe Dashboard** (billing UX), **Auth0 Dashboard** (PAT management).

## 2. Lane + forcing factors

- **Lane:** STANDARD (5–8 sign-offs).
- **Não FF-HR**: UI consome APIs validated em S-03/S-10/S-11; não introduz novo controle CRITICAL. Consent capture compliance está em CTRL-PRIV-CONSENT enforcement no backend; UI é captura frontend que mantém CTRL-PRIV-CONSENT-005 (locale match) + CTRL-PRIV-001 (zero PII em client-side logs).
- **Atenção em CSP**: hardened CSP enforcement crítico para evitar XSS that exfiltrates PAT.

## 3. Inherits_from

```yaml
inherits_from:
  - "AUTH-MODEL"                # PAT/Clerk integration + MFA re-auth
  - "PRIVACY-MODEL"             # CTRL-PRIV-CONSENT-001..006, CTRL-PRIV-001
  - "OBSERVABILITY-MODEL"       # client-side error tracking + privacy
  - "COMPLIANCE-MATRIX"         # WCAG 2.2 AA + LGPD + GDPR + ADA
  - "FAILURE-MODES"             # FM-150 (transient API), FM-160 (auth invalid)
  - "SECURITY-MODEL"            # CSP, XSS protection, CTRL-CRED-001 (no PAT em client logs)
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-UI-001** | Tenant onboarding flow | Signup → DPA acceptance → billing setup → first PAT created; backend handlers reuse S-03/S-10. |
| **CAP-UI-002** | Usage dashboard per-tenant | Métricas (CAS hit ratio, GB stored, GB egress, AC hit %), quota progress, plan tier. |
| **CAP-UI-003** | Audit log viewer self-service | CloudEvents R2 query proxy; filters por subject/type/ts; export JSON. |
| **CAP-UI-004** | Consent management UI | Captura 6-field proof per consent (notice_text_hash + version + locale + wording_id + ui_capture_ts + submission_ts) — CTRL-PRIV-CONSENT-001..006. |
| **CAP-UI-005** | DSR request form | 6 direitos (access/correction/erasure/portability/objection/consent_revoke); MFA re-auth; JWT receipt. |
| **CAP-UI-006** | PAT management | Create scoped PAT; list active; revoke; rotation. |
| **CAP-UI-007** | Privacy + Sub-processors pages | `/privacy` rendered semver; `/privacy/sub-processors` auto-generated. |
| **CAP-UI-008** | Billing overview | Current usage + quota progress + plan + invoice history (Stripe Customer Portal embed). |
| **CAP-UI-009** | i18n + a11y baseline | 3 locales (en/pt-BR/es) + WCAG 2.2 AA. |

## 5. Requirements específicos

### 5.1 Foundation (CAP-UI-001 + foundations)

- **R-S16-1**: Next.js 15 app em `apps/web/` deployed em CF Pages com:
  - SSR para `/dashboard`, `/audit`, `/billing` (privacy data; SSR auth-gated).
  - SSG para `/privacy`, `/sub-processors`, `/legal/*` (public, cacheable).
  - ISR para `/blog` (S-18).
- **R-S16-2**: Clerk SDK integration (reusa S-03 auth); MFA enforcement em routes sensíveis.
- **R-S16-3**: Hardened CSP `default-src 'none'` + explicit allowlists; report-only fase staging → enforce prod.

### 5.2 Dashboard + Audit (CAP-UI-002 + CAP-UI-003)

- **R-S16-4**: Usage dashboard via Grafana Cloud embedded panel ou métricas-as-code (decide via ADR-0017).
- **R-S16-5**: Audit log viewer query proxy `GET /api/audit?tenant_id=X&type=Y&since=Z`:
  - Backend reads CloudEvents R2 (S-09 audit bucket) via Worker.
  - Filters por subject/type/ts.
  - Export JSON button (audit-grade evidence).
  - Pagination cursor-based.

### 5.3 Consent + DSR (CAP-UI-004 + CAP-UI-005)

- **R-S16-6**: Consent UI captura full payload conforme S-11 R-S11-7:
  - Render notice versioned (`legal/privacy-notice/v<M.m>.md`); calcular `notice_text_hash = sha256(notice_text)` em frontend; envia ao backend.
  - Captura `ui_capture_ts` (browser ts) + `submission_ts` (server ts) — ambos em payload.
  - Captura `locale` from Accept-Language header.
  - Captura `wording_id` (UUID per A/B test variant).
  - **Screenshot evidence (EVT-012)** automática per consent capture for legal evidence retention.
- **R-S16-7**: DSR form com:
  - Identity re-auth: MFA code re-prompted antes submit (CTRL-AUTH-010).
  - Submit → confirmação em ≤ 1s + JWT receipt displayed + emailed.
  - SLA clock 30d visível pro user; email ao começar/avançar/completar.

### 5.4 PAT Management (CAP-UI-006)

- **R-S16-8**: Create PAT form:
  - Scope picker (read-only / read-write / admin) com explanation.
  - Expiry picker (30d / 90d / 1y / never).
  - Generated PAT exibido **apenas uma vez** + warning copy-protection.
  - List active: id + name + scope + last_used + expires.
  - Revoke single + revoke all.

### 5.5 Privacy + Sub-processors (CAP-UI-007)

- **R-S16-9**: `/privacy` renderiza markdown de `legal/privacy-notice/v<M.m>.md` (latest semver) via @next/mdx.
- **R-S16-10**: `/privacy/changelog` mostra diff entre versões.
- **R-S16-11**: `/privacy/sub-processors` auto-generated de `legal/sub-processors.md`.
- **R-S16-12**: Header notice de **mudança em sub-processor pendente** se broadcast triggered (S-11 R-S11-13).

### 5.6 i18n + a11y (CAP-UI-009)

- **R-S16-13**: 3 locales: en (default), pt-BR (LGPD primary), es (LATAM); detection via `Accept-Language`.
- **R-S16-14**: WCAG 2.2 AA compliance via:
  - axe-core CI test em todas as routes (zero violations).
  - Manual screen reader test (NVDA + VoiceOver) em consent + DSR forms.
  - Color contrast ≥ 4.5:1 normal; 3:1 large.
  - Keyboard navigation 100% (tab order logical).
  - ARIA landmarks corretos.

## 6. Definition of Done

- [ ] **WIs SEALED**: 7/7.
- [ ] **E2E test**: novo dev signup → cria tenant → aceita DPA → cria PAT → upload via CLI sucesso (EVT-018).
- [ ] **UI a11y** WCAG 2.2 AA compliance verificada via axe-core CI (0 violations) + manual screen reader test (EVT-019 added if not exists).
- [ ] **i18n**: en-US + pt-BR + es support inicial; matching Accept-Language; tested 3 locales (EVT-018).
- [ ] **Cross-browser**: Chrome, Firefox, Safari, Edge latest 2 versions cada (EVT-018).
- [ ] **Lighthouse score** ≥ 95 em Performance + A11y + Best Practices + SEO em 3 routes (`/`, `/dashboard`, `/privacy`) (EVT-002).
- [ ] **Consent UI**: 6-field payload captured + screenshot evidence (EVT-012) + verifiable backend (S-11 R-S11-9 verify endpoint) (EVT-012 + EVT-049).
- [ ] **DSR form**: submit → JWT receipt em ≤ 1s + SLA clock visible (chaos test simula slow API) (EVT-018).
- [ ] **PAT management**: create + list + revoke flow tested via Playwright (EVT-018).
- [ ] **CSP enforcement** prod: `default-src 'none'` + allowlists; CSP report endpoint capturing < 5 violations/dia (EVT-027).
- [ ] **Lighthouse a11y CI gate**: score < 95 fails PR (EVT-002).
- [ ] **PRR STANDARD**: Frontend lead + Privacy officer + Engineer + QA + Product + Designer + a11y advisor.
- [ ] **UX testing** com 5 devs externos: time-to-first-PAT ≤ 5 min; SUS score ≥ 75 (EVT-018).

## 7. Completeness Criteria (delta local)

- [ ] **10.s16.1** Consent form render respeita `notice_version` atual + gera `wording_id` único + EVT-012 screenshot evidence (EVT-012 + EVT-049).
- [ ] **10.s16.2** DSR form: submit → confirmação em ≤ 1s + SLA clock starts visível ao user (EVT-018).
- [ ] **10.s16.3** **Lighthouse ≥ 95** todos os pillars sustained 30d.
- [ ] **10.s16.4** **WCAG 2.2 AA** zero axe-core violations + screen reader walkthrough OK.
- [ ] **10.s16.5** **3 locales** detected via Accept-Language; native speaker review per language.
- [ ] **10.s16.6** **CSP enforce mode** prod com < 5 violations/dia (low false-positives).
- [ ] **10.s16.7** **Audit log viewer** query proxy + filters + export tested staged 30d.
- [ ] **10.s16.8** **UX SUS score ≥ 75** com 5 dev sample.

## 8. Invariants

### Mantidas (UI é consumer; CTRL-PRIV-CONSENT-005 enforcement em backend S-11)

- **CTRL-PRIV-CONSENT-005** (locale match com Accept-Language): UI capture locale conforme; backend (S-11) valida match.
- **CTRL-PRIV-001** (zero PII em logs client-side): no console.log/sentry capture com PII; redaction via `safeLog()` wrapper.
- **CTRL-CRED-001** (no PAT em client logs): PAT renderiza only-once + masked após copy.

## 9. Quality Standards (delta local)

- **14.s16.1 CSP `default-src 'none'`** + explicit allowlists (no `unsafe-inline`); enforcement prod com report endpoint.
- **14.s16.2 Zero dependencies** com known CVE HIGH/CRITICAL (npm audit + Dependabot).
- **14.s16.3 SSR/ISR** where appropriate; SSR para auth-gated; SSG para public; ISR para `/blog`.
- **14.s16.4 Bundle size budget**: main bundle ≤ 250KB gzipped; PR fails se exceeds.
- **14.s16.5 a11y CI gate**: axe-core 0 violations; manual screen reader sample monthly.
- **14.s16.6 i18n discipline**: missing translation = build fail; native speaker review per locale.
- **14.s16.7 Cross-browser CI** matrix (Chrome + Firefox + Safari + Edge latest 2).
- **14.s16.8 UX research**: 5 dev sample testing + SUS score ≥ 75 baseline; iterate até pass.
- **14.s16.9 PII redaction** em error tracking client-side (Sentry/equiv.); allowlist explicit fields.

## 10. Anti-scope

- ❌ Admin panel operacional interno (CoreLink ops) — `admin.corelink.dev` subdomain separado, não em S-16. (Cross-ref Sentry findings + customer support tools = pós-GA.)
- ❌ Mobile app nativo (iOS/Android) — anti-scope at GA; web responsive sufficient.
- ❌ Customer onboarding (full sales-led for enterprise) — S-19 (S-16 entrega self-service signup; enterprise é S-19 hand-off).
- ❌ A/B testing infrastructure (Optimizely/equiv.) — pós-GA.
- ❌ User analytics tracking (Mixpanel/Amplitude) — anti-scope; first-party telemetry only via opt-in.
- ❌ Live chat support widget — pós-GA.
- ❌ Real-time collaborative features (multi-user dashboards live edit) — pós-GA.
- ❌ White-label / theming custom — pós-GA enterprise.

## 11. Dependencies

### Hard blockers

- **S-03 SEALED** (Clerk auth + PAT).
- **S-10 SEALED** (billing data).
- **S-11 SEALED** (privacy backend + DSR API + consent ledger).
- **S-09 SEALED** (audit events R2 + métricas).
- **S-13 SEALED** (admin plane API for ops; UI é separate `admin.corelink.dev`).

### Soft blockers

- **S-14 SEALED** (BYOK setup wizard if customer-facing; deferrable).

### Outbound

- S-18 (public docs may embed UI screenshots).
- S-19 (customer onboarding leverages self-service; enterprise complement).
- S-20 (GA exige Lighthouse ≥ 95 + WCAG 2.2 AA).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S16-001** | Next.js 15 skeleton + Clerk + CSP + i18n base | scaffold; Clerk integration; CSP setup; i18n config + 3 locales base; deploy CF Pages | 14h | 22h | 36h | **23.0h** |
| **WI-S16-002** | Tenant onboarding flow (signup→DPA→billing→first PAT) | onboarding wizard; DPA accept UI; Stripe Checkout integration; first PAT create | 14h | 22h | 36h | **23.0h** |
| **WI-S16-003** | Usage dashboard + Grafana proxy + PAT management | dashboard layout; Grafana embed/proxy; ADR-0017; PAT CRUD; revoke flow | 16h | 24h | 38h | **25.0h** |
| **WI-S16-004** | Consent UI + DSR form (6 direitos + JWT receipt) + PII redaction | consent capture 6-field; DSR form; MFA re-auth flow; JWT receipt UI; SLA clock | 14h | 22h | 36h | **23.0h** |
| **WI-S16-005** | Audit log viewer + filters + export | viewer UI; query proxy backend; filters; pagination cursor; JSON export | 10h | 16h | 26h | **16.7h** |
| **WI-S16-006** | Privacy/sub-processors pages + i18n full + diff page | mdx render; semver versioning; sub-processors auto-gen; changelog page; broadcast banner | 8h | 14h | 22h | **14.3h** |
| **WI-S16-007** | a11y WCAG 2.2 AA + Lighthouse CI gate + 5-dev UX testing + SUS | axe-core CI; manual screen reader; Lighthouse gate; UX research session 5 devs; SUS calc | 12h | 18h | 28h | **18.7h** |

**Total PERT:** ~144h ≈ 18 dias work × 1 eng. Buffer 5 dias confere com 3 semanas.

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 5 dias.
- **Marcos:**
  - **D+5:** WI-001 + WI-002 SEALED (skeleton + onboarding).
  - **D+9:** WI-003 SEALED (dashboard + PAT).
  - **D+12:** WI-004 SEALED (consent + DSR).
  - **D+14:** WI-005 + WI-006 SEALED (audit viewer + privacy pages).
  - **D+17:** WI-007 SEALED (a11y + UX research).
  - **D+20:** Sprint review + sign-offs.

## 14. Critérios de promoção

- DoD complete + UX testing 5 devs externos com SUS ≥ 75.
- Lighthouse ≥ 95 sustained.
- WCAG 2.2 AA verified.
- 3 locales reviewed por native speakers.
- PRR STANDARD aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Next.js quirks com CF Pages runtime** (Edge runtime limitations) | M | M | MEDIUM | M | LOW | Test CF Pages Edge runtime early; documented compatibility matrix; fallback Nodejs runtime se needed. |
| **Clerk integration edge cases** (MFA + SSO combinations) | M | L | LOW | L | LOW | Comprehensive test matrix; Clerk support tier; ADR documented. |
| **a11y compliance late discovery** (audit findings post-launch) | M | L | LOW | L | LOW | axe-core CI gate from day 1 + manual review + WCAG 2.2 AA target (more strict than 2.1). |
| **CSP false-positives** em prod | M | L | LOW (operational) | L | LOW | CSP report-only mode em staging 14d → enforce prod com refined allowlist. |
| **i18n quality** (translation issues per locale) | M | L | LOW | L | LOW | Native speaker review + iterative refinement; missing-translation build fail. |
| **PAT exposure** em client logs (browser dev tools) | L | L | HIGH | L | LOW | PAT only-once display + masked + Sentry redaction allowlist. |
| **Lighthouse score regression** | M | L | LOW | L | LOW | CI gate < 95 fails PR; bundle budget enforce; perf review weekly. |
| **Cross-browser rendering issues** (Safari quirks) | M | L | LOW | L | LOW | Cross-browser CI matrix; visual regression testing (Percy or equivalent). |
| **Consent UI screenshot evidence falha** (browser API issues) | L | L | MEDIUM (compliance gap) | L | LOW | Server-side screenshot fallback via headless Chrome render proof; documented. |
| **DSR form abandonment** (UX miss) | M | L | LOW | L | LOW | UX research 5 devs; iterate; clear progress indicator. |
| **CSP report flooding** (legitimate violations missed) | M | L | LOW | L | LOW | Report endpoint with rate-limit + dedup + categorize; Slack alert weekly. |

## 16. Benchmarks SOTA externos

| Critério | Stripe Dashboard | Auth0 Dashboard | GOV.UK Design System | **CoreLink target S-16** |
|---|---|---|---|---|
| WCAG compliance | 2.1 AA | 2.1 AA | 2.2 AA | **2.2 AA** |
| Lighthouse Perf+A11y | ≥ 90 | ≥ 90 | ≥ 95 | **≥ 95** |
| i18n locales (3+) | Many | Many | EN+CY (Wales) | **3 (en/pt-BR/es) at GA** |
| Consent capture proof of informed | Limited | No | Yes | **Yes — 6-field payload + screenshot** |
| DSR self-service UI | Limited | Limited | Yes | **Yes — 6 direitos + MFA + JWT receipt** |
| Audit log viewer self-service | Yes | Yes | N/A | **Yes — CloudEvents query proxy + filters + export** |
| PAT management UX | Yes | Yes | N/A | **Yes — scope picker + expiry + masked once-display** |
| CSP `default-src 'none'` | Yes | Yes | Yes | **Yes — enforced prod** |
| SSR/ISR/SSG hybrid | Yes | Yes | Yes | **Yes — per-route strategy** |

**Veredito SOTA:** S-16 v1.1 atinge feature parity com Stripe/Auth0; vantagem em WCAG 2.2 AA (mais stringent que 2.1) + consent 6-field capture compliance.

## 17. References (RFCs, papers, standards)

- **WCAG 2.2** (W3C Recommendation, 2023) <https://www.w3.org/TR/WCAG22/>.
- **WAI-ARIA Authoring Practices 1.2** <https://www.w3.org/WAI/ARIA/apg/>.
- **GOV.UK Design System** <https://design-system.service.gov.uk/>.
- **Stripe Dashboard UX patterns**.
- **Auth0 Dashboard PAT management patterns**.
- **OWASP Application Security Verification Standard (ASVS) V14** — Configuration.
- **OWASP CSP Cheat Sheet**.
- **Next.js App Router Documentation** v15.
- **System Usability Scale (SUS)** <https://en.wikipedia.org/wiki/System_usability_scale>.
- **axe-core** accessibility testing engine.
- **Web Content Accessibility Guidelines (WCAG) 2.2** — Level AA conformance.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- a11y regression em prod (axe-core in-prod scan finds violation) → 5-Why obrigatório.
- PAT leak em client logs detectado → CRITICAL post-mortem + Security review + redaction reinforce.
- Consent screenshot evidence missed (em audit period) → post-mortem + privacy gap fix.
- DSR form failure (user can't submit) → post-mortem (compliance impact).
- CSP enforce mode disabled in prod → CRITICAL post-mortem + Security review.
- Lighthouse score < 90 sustained > 7d → post-mortem (DX regression).

## 19. Waiver policy

S-16 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ WCAG 2.2 AA compliance — accessibility regulatory baseline (ADA + EU Accessibility Act 2025).
- ❌ Consent capture 6-field proof — CTRL-PRIV-CONSENT compliance baseline.
- ❌ CSP `default-src 'none'` em prod — XSS defense baseline.
- ❌ PAT only-once display — credential exposure prevention.

Itens waivable com Frontend lead + Privacy officer + ADR:

- ⚠️ 3 locales → 2 locales GA (en + pt-BR); es no Q1 pós-GA.
- ⚠️ Lighthouse target ≥ 95 → ≥ 90 com plan to improve next sprint.
- ⚠️ Cross-browser scope: latest 2 versions → latest 1 version Safari (oldest 2 sometimes lag in features).

---

**Fim spec contract S-16 v1.1.0 SOTA.**
