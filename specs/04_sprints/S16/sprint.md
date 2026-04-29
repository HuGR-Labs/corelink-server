---
id: "S-16"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
# lane_forcing_factors omitted: STANDARD lane permite empty (REG-LANE-003 só obriga se lane=HIGH_RISK; schema minItems:1 rejeita empty array)
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "FAILURE-MODES"
  - "SECURITY-MODEL"
tags: ["sprint", "s16", "ui", "admin", "frontend", "nextjs", "wcag-2.2-aa", "i18n", "consent", "dsr-ui", "audit-viewer", "csp", "standard"]
---

# Sprint S-16 — Frontend Admin UI (Next.js 15 + CF Pages) + Tenant Self-Service Onboarding (Signup → DPA → Billing → First PAT) + Usage Dashboard + Audit Log Viewer (CloudEvents R2 Query Proxy + Filters + JSON Export) + Consent UI 6-field Capture (CTRL-PRIV-CONSENT-001..006 + Screenshot Evidence EVT-012) + DSR Self-service Form (6 direitos LGPD/GDPR + MFA Re-auth + JWT Receipt + SLA Clock 30d) + PAT Management (Scope Picker + Once-display Masked + Revoke Flow) + Privacy/Sub-processors Pages (mdx semver + diff + broadcast banner) + WCAG 2.2 AA + 3 Locales (en/pt-BR/es) + Lighthouse ≥ 95 + CSP `default-src 'none'` + 5-dev UX Workshop SUS ≥ 75

> **doc_status:** DRAFT · **lane:** STANDARD · **Versão:** 1.0.0 · **2026-04-29**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 12.S16.0 SOTA elevation; STANDARD lane no forcing factors)

> **Phase boundary:** Fase 4 — Customer-facing self-service UI production-ready pré-GA.
> **STANDARD lane rationale (zero forcing factors):** UI consume APIs já validated em S-03 (Auth + PAT) + S-09 (audit events R2) + S-10 (billing) + S-11 (privacy backend + DSR API + consent ledger) + S-13 (admin plane API); não introduz novo path de tenant data ou novo controle CRITICAL. CTRL-PRIV-CONSENT enforcement (1..006) reside em backend S-11 (verify endpoint R-S11-9 + consent ledger D1); UI é capture frontend que mantém CTRL-PRIV-CONSENT-005 (locale match com Accept-Language) + CTRL-PRIV-001 (zero PII em client-side logs). CSP enforcement hardened é well-bounded surface (XSS defense baseline). WebAuthn admin step-up reusa S-03 cycle 9 SEAL (Clerk SDK + WebAuthn + PAT flows); dual-approval admin destructive ops reusa S-13 PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window (Lote 10.13 canonical). Não há cripto-load-bearing controles novos (S-14 HIGH_RISK BYOK reuse only).

---

## 1. Objetivo

Entregar **Frontend admin UI production-grade** (Next.js 15 + Cloudflare Pages) com **time-to-first-PAT ≤ 5 min** + **WCAG 2.2 AA compliance** + **3 locales** (en-US default + pt-BR LGPD primary + es-419 LATAM): tenant onboarding flow completo (signup → DPA acceptance → Stripe billing setup → first PAT created; backend handlers reuse S-03/S-10), usage dashboard per-tenant (CAS hit ratio, GB stored, GB egress, AC hit %, quota progress, plan tier — Grafana Cloud embedded panel ou métricas-as-code per ADR-0017), audit log viewer self-service (CloudEvents R2 query proxy via Worker; filters por subject/type/ts; pagination cursor-based; JSON export audit-grade evidence), consent management UI (captura 6-field proof of informed: `notice_text_hash` sha256 + `notice_version` + `locale` + `wording_id` UUID per A/B test variant + `ui_capture_ts` browser ts + `submission_ts` server ts — CTRL-PRIV-CONSENT-001..006 reflection; screenshot evidence EVT-012 automática per consent capture for legal evidence retention), DSR self-service form (6 direitos LGPD Art. 18 + GDPR Art. 15-22: access + correction + erasure + portability + objection + consent_revoke; identity re-auth via MFA code re-prompted antes submit per CTRL-AUTH-010 fresh ≤ 30 min; submit → confirmação em ≤ 1s + JWT receipt displayed + emailed; SLA clock 30d visível pro user; email ao começar/avançar/completar), PAT management (scope picker read-only/read-write/admin com explanation; expiry picker 30d/90d/1y/never; PAT generated exibido **apenas uma vez** + warning copy-protection + masked após copy; list active id + name + scope + last_used + expires; revoke single + revoke all), privacy + sub-processors pages (`/privacy` mdx semver versioned via @next/mdx; `/privacy/changelog` diff entre versões; `/privacy/sub-processors` auto-generated de `legal/sub-processors.md`; header notice de mudança em sub-processor pendente quando broadcast triggered S-11 R-S11-13), billing overview (current usage + quota progress + plan + invoice history Stripe Customer Portal embed), hardened CSP `default-src 'none'` enforcement prod (no `unsafe-inline`, no `eval`, nonce-based script tags, DOMPurify para qualquer user input rendered; report-only fase staging 14d → enforce prod com refined allowlist; CSP report endpoint < 5 violations/dia), telemetry opt-in default-off privacy-first (reuse pattern S-15; LINDDUN review).

Decomposição em 7 WIs: (1) **WI-S16-001** Next.js 15 skeleton em `apps/web/` deployed CF Pages com Clerk SDK integration (reusa S-03 auth; MFA enforcement em routes sensíveis), hardened CSP setup (report-only staging → enforce prod), i18n config + 3 locales base (en/pt-BR/es; `next-intl` ou equivalent; missing-translation = build fail per Quality Standard 14.s16.6), routing strategy SSR/ISR/SSG hybrid (SSR auth-gated `/dashboard|/audit|/billing`; SSG public `/privacy|/sub-processors|/legal/*`; ISR `/blog` deferred S-18); (2) **WI-S16-002** tenant onboarding flow (signup wizard step-by-step → DPA acceptance UI com texto versionado + capture timestamp → Stripe Checkout integration → first PAT created via PAT management widget reuse) + retention policy + flag toggle UI básica; (3) **WI-S16-003** consent management UI (captura full payload conforme S-11 R-S11-7: render notice versioned `legal/privacy-notice/v<M.m>.md` + calcular `notice_text_hash = sha256(notice_text)` em frontend + envia ao backend; captura `ui_capture_ts` + `submission_ts` + `locale` from Accept-Language + `wording_id` UUID; screenshot evidence EVT-012 automática via DOM-to-image fallback server-side headless Chrome render proof se browser API falhar); (4) **WI-S16-004** DSR self-service form (6 direitos com identity re-auth MFA fresh ≤ 30 min CTRL-AUTH-010; submit → JWT receipt em ≤ 1s + emailed; SLA clock 30d visível; email ao começar/avançar/completar) + DSR status viewer (request list + status + receipt download); (5) **WI-S16-005** admin operations UI (audit trail viewer com query proxy `GET /api/audit?tenant_id=X&type=Y&since=Z` reading CloudEvents R2 S-09 audit bucket via Worker; filters por subject/type/ts; export JSON; pagination cursor; admin op submit dual-approval flow reusing S-13 PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window); (6) **WI-S16-006** component library + accessibility WCAG 2.2 AA + i18n full (axe-core CI 0 violations; manual screen reader NVDA + VoiceOver em consent + DSR forms; color contrast ≥ 4.5:1 normal + 3:1 large; keyboard navigation 100% tab order logical; ARIA landmarks corretos; native speaker review per locale; `next-intl` discipline missing translation = build fail) + privacy/sub-processors pages mdx render + diff page + broadcast banner; (7) **WI-S16-007** ship gate: Playwright E2E tests (signup → DPA → PAT → CLI cache hit; consent flow capture; DSR submit + receipt; audit viewer query + export; PAT create + list + revoke); Lighthouse CI gate ≥ 95 em 4 pillars (Performance + A11y + Best Practices + SEO) em 3 routes (`/`, `/dashboard`, `/privacy`); cross-browser CI matrix (Chrome + Firefox + Safari + Edge latest 2 versions cada); UX research session 5 dev sample SUS calc ≥ 75; CSP enforce mode prod com < 5 violations/dia; closing PRR STANDARD 5-8 sign-offs canonical (7 typical: Owner + Final Approver + Frontend Lead + QA + Product + Designer/a11y advisor + Privacy officer). Implementa **CAP-UI-001..009** + reforça **CTRL-PRIV-CONSENT-005** (locale match) + **CTRL-PRIV-001** (zero PII em client-side logs) + **CTRL-CRED-001** (no PAT em client logs; PAT renderiza only-once + masked após copy) + **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min; reuse S-03 cycle 9 + S-13 dual-approval). Não introduz novas INVs (UI é consumer das backend invariants já enforced em S-03/S-09/S-10/S-11/S-13).

**Por que SOTA:** competitors (Stripe Dashboard, Auth0 Dashboard, GOV.UK Design System) têm admin UI básico — Stripe limita consent capture compliance-grade + DSR self-service; Auth0 não tem DSR self-service UI; GOV.UK Design System é WCAG 2.2 AA + a11y-grade mas sem audit viewer self-service. CoreLink S-16 entrega **feature parity + diferenciador**: (a) consent form com **6-field proof of informed** (`notice_text_hash` + `notice_version` + `locale` + `wording_id` + `ui_capture_ts` + `submission_ts`) + **screenshot evidence EVT-012** automática per consent capture for legal evidence retention (zero competitors); (b) DSR form com **6 direitos LGPD/GDPR self-service** + identity re-auth MFA + signed JWT receipt + SLA clock 30d visível pro user (zero competitors); (c) audit viewer self-service com per-tenant filtering + JSON export audit-grade evidence (Stripe/Auth0 parity); (d) WCAG 2.2 AA (mais stringent que 2.1; matching GOV.UK) + 3 locales reviewed por native speakers; (e) hardened CSP `default-src 'none'` enforcement prod (Stripe/Auth0 parity; XSS defense baseline). Reference: **GOV.UK Design System** (a11y patterns + DSR form), **Stripe Dashboard** (billing UX + PAT management), **Auth0 Dashboard** (PAT scope picker + audit viewer), **WCAG 2.2 W3C Recommendation** 2023, **WAI-ARIA Authoring Practices 1.2**.

## 2. Escopo

### 2.1 In-scope

- **WI-S16-001**: Next.js 15 app skeleton em `apps/web/` deployed em Cloudflare Pages com routing strategy SSR/ISR/SSG hybrid (SSR auth-gated `/dashboard|/audit|/billing|/settings`; SSG public `/privacy|/sub-processors|/legal/*`; ISR `/blog` deferred S-18); Clerk SDK integration (reusa S-03 cycle 9 SEAL — Clerk SSO + WebAuthn + PAT flows); MFA enforcement em routes sensíveis (`/settings/security|/audit|/dsr|/consent|/admin/*`); hardened CSP `default-src 'none'` setup (no `unsafe-inline`, no `eval`, nonce-based script tags via Next.js `headers()` config; report-only mode em staging 14d → enforce prod com refined allowlist após CSP report endpoint analysis); CSP report endpoint `/api/csp-report` rate-limited + dedup + Slack alert weekly; i18n config base (next-intl ou equivalent; 3 locales en/pt-BR/es; missing-translation = build fail per Quality Standard 14.s16.6; locale detection via `Accept-Language` header); bundle size budget enforcement ≤ 250KB gzipped main bundle (PR fails se exceeds; webpack-bundle-analyzer CI report); PII redaction wrapper `safeLog()` em error tracking client-side (Sentry/equiv.; allowlist explicit fields only — `request_id`, `user_id_hash`, `error_code`; never `email`, `pat`, `tenant_id` raw); `corelink_admin_ui_*` métricas Prometheus emitting via reverse-proxy (snake_case canonical per observability_model §3.1; `plan` label canonical; INV-OBS-CARDINALITY-BUDGET respeitado — NUNCA per-tenant labels).

- **WI-S16-002**: Tenant onboarding flow (signup wizard step-by-step com progress indicator clear → DPA acceptance UI com texto versionado `legal/dpa/v<M.m>.md` rendered via @next/mdx + capture timestamp + acceptance signed JWT receipt → Stripe Checkout integration via Stripe SDK + Stripe Customer Portal embed → first PAT created via PAT management widget reuse [scope picker read-only/read-write/admin com explanation per scope; expiry picker 30d/90d/1y/never; PAT exibido **apenas uma vez** + warning copy-protection + masked após copy + button "I've copied it"]); tenant self-service settings UI (config + retention policy picker + flag toggle pra dev/test scenarios); URL-shareable invite link para co-developers (signed JWT com tenant_id + role; reusa Clerk SDK invite flow).

- **WI-S16-003**: Consent management UI (captura full payload conforme S-11 R-S11-7 + CTRL-PRIV-CONSENT-001..006 reflection):
  - Render notice versioned (`legal/privacy-notice/v<M.m>.md` rendered via @next/mdx; `/privacy/changelog` diff entre versões via `react-diff-viewer` ou equivalent).
  - Captura `notice_text_hash = sha256(notice_text_rendered)` em frontend (Web Crypto API `crypto.subtle.digest`); envia ao backend POST `/api/consent` per S-11 R-S11-7 schema.
  - Captura `notice_version` (semver string from frontmatter mdx).
  - Captura `locale` from `navigator.language` ou Accept-Language header (CTRL-PRIV-CONSENT-005 enforcement em backend valida match — UI is reflection).
  - Captura `wording_id` (UUID v7 per A/B test variant; if no A/B test, deterministic UUID per `notice_version`).
  - Captura `ui_capture_ts` (browser `Date.now()` ms epoch) + `submission_ts` (server-assigned ts em backend).
  - **Screenshot evidence EVT-012** automática per consent capture: DOM-to-image client-side primary (html2canvas ou equivalent); server-side fallback via headless Chrome render proof se browser API falhar (R-S11-X risco mitigation).
  - Plain-language notice (Flesch-Kincaid grade ≤ 8 verified em CI + native speaker review per locale).
  - Click capture event-bound (no auto-submit; explicit user action required).
  - Consent revoke flow integrated em DSR form (WI-S16-004).

- **WI-S16-004**: DSR self-service form (6 direitos LGPD Art. 18 + GDPR Art. 15-22):
  - **Direito #1 Access** (LGPD Art. 18 II + GDPR Art. 15): user receives all dados pessoais structured.
  - **Direito #2 Correction** (LGPD Art. 18 III + GDPR Art. 16): user submits correction; reviewed.
  - **Direito #3 Erasure** (LGPD Art. 18 VI + GDPR Art. 17): user requests deletion; conditional logic (legal hold exceptions documented).
  - **Direito #4 Portability** (LGPD Art. 18 V + GDPR Art. 20): user receives JSON export structured.
  - **Direito #5 Objection** (GDPR Art. 21): user objects to processing.
  - **Direito #6 Consent revoke** (LGPD Art. 18 IX + GDPR Art. 7§3): user revokes prior consent.
  - Identity re-auth via MFA code re-prompted antes submit (CTRL-AUTH-010 fresh ≤ 30 min; reuse S-03 WebAuthn flow).
  - Submit → confirmação em ≤ 1s + signed JWT receipt displayed em UI + emailed via SES/equiv. (S-11 alignment).
  - SLA clock 30d visível pro user (countdown timer; email ao começar/avançar/completar per S-11 alignment).
  - DSR status viewer separate route `/settings/dsr` lista request_id + status + last_update + receipt download link.
  - Chaos test simula slow API response (≥ 5s API delay); UI shows progress indicator + retry option (FM-150 transient handled).

- **WI-S16-005**: Admin operations UI:
  - **Audit log viewer self-service** query proxy backend `GET /api/audit?tenant_id=X&type=Y&since=Z`:
    - Backend Worker reads CloudEvents R2 (S-09 audit bucket) via signed query.
    - Filters por subject (user_hash), type (event_type enum), ts (since/until).
    - Pagination cursor-based (cursor in/out via R2 query).
    - **Export JSON button** (audit-grade evidence; sanitized — PAT redacted; PII allowlist).
    - Render table with columns: ts, type, subject_hash, action, outcome, details_truncated.
  - **Admin op submit dual-approval flow** (reuse S-13 PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window):
    - Admin user submits destructive op (e.g., tenant delete, retention policy change, BYOK rotate).
    - Fresh MFA ≤ 30 min required (CTRL-AUTH-010).
    - Second admin approves via separate session (collusion-rotation enforcement: same approver max 3 ops within rolling window).
    - Op queued + audit event + Slack notification.

- **WI-S16-006**: Component library + accessibility WCAG 2.2 AA + i18n full + privacy/sub-processors pages:
  - **Component library** (Radix UI primitives ou equivalent + Tailwind CSS): Button, Input, Form, Modal, Tabs, Accordion, Toast, Spinner, Card, Table, Tooltip — all WAI-ARIA compliant.
  - **Accessibility WCAG 2.2 AA**:
    - Keyboard navigation 100% (tab order logical; focus indicators visible).
    - Screen reader test (NVDA + VoiceOver) em consent + DSR + audit viewer + PAT mgmt — manual walkthrough OK.
    - Color contrast ≥ 4.5:1 normal text; ≥ 3:1 large text; verified via axe-core + Stark plugin Figma design review.
    - ARIA landmarks corretos (`<nav>`, `<main>`, `<aside>`, `<header>`, `<footer>` semantic; `aria-label` em ambiguous regions).
    - axe-core CI test em todas as routes (zero violations gate; PR fails se any).
  - **i18n full 3 locales**:
    - en-US (default), pt-BR (LGPD primary), es-419 (LATAM).
    - Detection via `Accept-Language` header + manual locale switcher em footer.
    - Native speaker review per locale (community translator reviewed; iterative refinement).
    - Missing-translation = build fail per Quality Standard 14.s16.6 (CI gate).
  - **Privacy + sub-processors pages**:
    - `/privacy` renderiza markdown de `legal/privacy-notice/v<M.m>.md` (latest semver) via @next/mdx.
    - `/privacy/changelog` mostra diff entre versões via `react-diff-viewer`.
    - `/privacy/sub-processors` auto-generated de `legal/sub-processors.md`.
    - Header notice (banner) de **mudança em sub-processor pendente** se broadcast triggered (S-11 R-S11-13 alignment).

- **WI-S16-007**: Ship gate cumulative — Playwright E2E tests (full E2E novo dev signup → cria tenant → aceita DPA → cria PAT → upload via CLI sucesso EVT-018; consent flow capture E2E com 6-field payload validated em backend; DSR submit + JWT receipt em ≤ 1s; audit viewer query + JSON export; PAT create + list + revoke flow); Lighthouse CI gate ≥ 95 em 4 pillars (Performance + A11y + Best Practices + SEO) em 3 routes (`/`, `/dashboard`, `/privacy`) — score < 95 fails PR per Quality Standard 14.s16.5; cross-browser CI matrix (Chrome + Firefox + Safari + Edge latest 2 versions cada via Playwright multi-browser); UX research session com 5 external developers (rotated per sprint; bias mitigation; SUS calc — System Usability Scale; threshold ≥ 75 baseline; iterate até pass; time-to-first-PAT ≤ 5 min measured); CSP enforce mode prod com < 5 violations/dia (CSP report endpoint analysis sustained 30d staging → enforce prod); closing PRR STANDARD doc S-16 com 5-8 sign-offs canonical (7 typical: Owner + Final Approver + Frontend Lead + QA + Product + Designer/a11y advisor + Privacy officer); evidence pack: Lighthouse score reports + axe-core 0 violations + screen reader walkthrough video + UX research SUS report + 3 locales native speaker review + cross-browser CI matrix verde + CSP report sustained.

### 2.2 Anti-scope

- Admin panel operacional interno (CoreLink ops) — `admin.corelink.dev` subdomain separado; pós-GA Sentry findings + customer support tools.
- Mobile app nativo (iOS/Android) — anti-scope at GA; web responsive sufficient.
- Customer onboarding full sales-led for enterprise — S-19 (S-16 entrega self-service signup; enterprise é S-19 hand-off).
- A/B testing infrastructure (Optimizely/equiv.) — pós-GA.
- User analytics tracking (Mixpanel/Amplitude) — anti-scope; first-party telemetry only via opt-in.
- Live chat support widget — pós-GA.
- Real-time collaborative features (multi-user dashboards live edit) — pós-GA.
- White-label / theming custom — pós-GA enterprise.
- IDE extensions (VSCode plugin, JetBrains) — S-15/pós-GA.
- A11y external audit (formal certification) — pós-GA Q1; axe-core CI + manual screen reader sufficient para GA baseline.

## 3. Customer Impact & Journey

**JTBD:** "Como Build Engineer prospect novo, preciso completar **time-to-first-PAT ≤ 5 min** end-to-end self-service em UI: (a) signup via Clerk SSO ou email; (b) aceitar DPA versionado com texto plain-language; (c) configurar billing via Stripe Checkout; (d) criar primeiro PAT scoped read-write em ≤ 30s; (e) começar usando CLI/SDK reference. Como Privacy Officer cliente, preciso DSR self-service form com 6 direitos LGPD/GDPR + identity re-auth MFA + signed JWT receipt + SLA clock 30d visível para reduzir manual support tickets. Como Compliance auditor, preciso audit log viewer self-service per-tenant + JSON export audit-grade evidence + consent capture 6-field proof of informed + screenshot evidence EVT-012 para SOC 2 + LGPD/GDPR readiness. Como Acessibilidade auditor (ADA + EU Accessibility Act 2025), preciso WCAG 2.2 AA compliance verified via axe-core CI + manual screen reader (NVDA + VoiceOver) + color contrast ≥ 4.5:1 + 3 locales reviewed."

**CAPs entregues:** CAP-UI-001 (tenant onboarding flow) + CAP-UI-002 (usage dashboard per-tenant) + CAP-UI-003 (audit log viewer self-service) + CAP-UI-004 (consent management UI 6-field) + CAP-UI-005 (DSR request form 6 direitos) + CAP-UI-006 (PAT management) + CAP-UI-007 (privacy + sub-processors pages) + CAP-UI-008 (billing overview) + CAP-UI-009 (i18n + a11y baseline 3 locales WCAG 2.2 AA).

**Persona 1 — Build Engineer prospect novo**:
- Setup time benchmark: signup → first PAT ≤ 5 min (vs Stripe ~7 min, Auth0 ~6 min); measured via UX workshop sample 5 devs externos rotated per sprint + tracked monthly.
- UI ergonomics: DPA acceptance + Stripe Checkout + PAT creation seamless; PAT exibido apenas uma vez + masked após copy.
- Diferenciador competitivo: Bazel/Buck2 starter projects + FFI 3 languages from S-15 + complete self-service onboarding em S-16.

**Persona 2 — Privacy Officer cliente (Compliance auditor cliente)**:
- DSR self-service form 6 direitos LGPD Art. 18 + GDPR Art. 15-22 com identity re-auth MFA + JWT receipt em ≤ 1s + SLA clock 30d visível.
- Consent capture 6-field proof of informed (`notice_text_hash` + `notice_version` + `locale` + `wording_id` + `ui_capture_ts` + `submission_ts`) + screenshot evidence EVT-012 automática + verifiable backend (S-11 R-S11-9 verify endpoint).
- Privacy notice page versioned (`/privacy/v<M.m>` semver via @next/mdx; `/privacy/changelog` diff).
- Sub-processors page auto-generated; broadcast banner notice de mudança pendente.

**Persona 3 — Compliance auditor (SOC 2 + ISO 27001 + LINDDUN)**:
- Audit log viewer self-service per-tenant filtering por subject/type/ts; JSON export audit-grade evidence.
- Consent ledger D1 backend (S-11) verify endpoint; UI captura 6-field forwards correctly.
- DSR fulfillment SLA 30d tracked + emailed; signed JWT receipt audit trail forensic-grade.
- WCAG 2.2 AA compliance verified (more stringent than 2.1; ADA + EU Accessibility Act 2025 baseline).

**SLA addendum**:
- Time-to-first-PAT: ≤ 5 min (measured via UX workshop 5 external developers; tracked monthly).
- DSR submit confirmation: ≤ 1s + JWT receipt displayed + emailed.
- Consent capture: 6-field payload + screenshot evidence EVT-012 + backend verifiable per S-11 R-S11-9.
- Audit viewer query: ≤ 2s p95 latency on cursor pagination.
- Lighthouse score: ≥ 95 em 4 pillars sustained 30d em 3 routes.
- WCAG 2.2 AA: zero axe-core violations + screen reader walkthrough OK.
- Cross-browser: Chrome + Firefox + Safari + Edge latest 2 versions cada (CI matrix verde).
- UX research SUS score: ≥ 75 com 5 dev sample.
- CSP enforce prod: < 5 violations/dia (low false-positives).

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `auth_model.md` (Clerk SSO + WebAuthn + PAT flows S-03; MFA fresh ≤ 30 min CTRL-AUTH-010) + `privacy_model.md` (CTRL-PRIV-CONSENT-001..006 + CTRL-PRIV-001 zero PII em logs + DSR 6 direitos LGPD Art. 18 + GDPR Art. 15-22) + `observability_model.md §3.1` (Prometheus snake_case + `plan` label canonical; INV-OBS-CARDINALITY-BUDGET nunca per-tenant labels) + `compliance_matrix.md` (WCAG 2.2 AA + LGPD + GDPR + ADA + EU Accessibility Act 2025) + `failure_modes.md` (FM-150 transient API + FM-160 auth invalid) + `security_model.md` (CSP `default-src 'none'`; XSS protection; CTRL-CRED-001 no PAT em client logs).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S16-D1 | Next.js 15 skeleton + Clerk + CSP + i18n base | `apps/web/` + CF Pages deploy | App deployed staging; Clerk SDK integrated; MFA enforce em routes sensíveis; CSP report-only mode em staging; i18n 3 locales base; bundle ≤ 250KB; PII redaction wrapper `safeLog()` |
| S16-D2 | Tenant onboarding flow + self-service settings + first PAT | `apps/web/src/app/(onboarding)/*` + `apps/web/src/app/settings/*` | Signup → DPA → Stripe Checkout → first PAT em ≤ 5 min; URL-shareable invite link; settings UI config + retention policy + flag toggle |
| S16-D3 | Consent management UI 6-field + screenshot evidence | `apps/web/src/app/consent/*` + `apps/web/src/lib/consent.ts` | 6-field payload captured + EVT-012 screenshot evidence (client-side primary + server-side fallback); plain-language notice (Flesch-Kincaid ≤ 8); CTRL-PRIV-CONSENT-005 locale match |
| S16-D4 | DSR self-service form 6 direitos + MFA + JWT receipt + status viewer | `apps/web/src/app/(privacy)/dsr/*` | 6 direitos LGPD Art. 18 + GDPR Art. 15-22; identity re-auth MFA fresh ≤ 30 min; submit ≤ 1s + JWT receipt; SLA clock 30d visible; status viewer route |
| S16-D5 | Admin ops UI (audit viewer + dual-approval) | `apps/web/src/app/admin/*` + `apps/web/src/app/api/audit/route.ts` | Audit query proxy `GET /api/audit?tenant_id&type&since`; filters; pagination cursor; JSON export; admin op submit dual-approval reusing S-13 PAT-DUAL-APPROVAL-001 |
| S16-D6 | Component library + a11y WCAG 2.2 AA + i18n full + privacy/sub-processors | `apps/web/src/components/ui/*` + `apps/web/src/i18n/*` + `apps/web/src/app/(legal)/*` | Component library Radix UI + Tailwind; axe-core CI 0 violations; manual screen reader OK; color contrast ≥ 4.5:1; ARIA landmarks; 3 locales native speaker reviewed; `/privacy` mdx semver + changelog diff + sub-processors auto-gen + broadcast banner |
| S16-D7 | Ship gate — Playwright E2E + Lighthouse CI ≥ 95 + UX workshop 5 devs SUS ≥ 75 + CSP enforce + closing PRR | `apps/web/e2e/*` + `.github/workflows/lighthouse.yml` + `specs/04_sprints/S16/PRR-S16.md` | Playwright E2E suite verde; Lighthouse ≥ 95 em 4 pillars × 3 routes; cross-browser CI matrix; UX research SUS ≥ 75 com 5 devs; CSP enforce prod < 5 violations/dia; PRR 5-8 sign-offs canonical |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Auth Model (herda `auth_model.md`)

- Clerk SDK integration (S-03 cycle 9 SEAL — Clerk SSO + WebAuthn + PAT flows): UI consume via auth/cookie session.
- PAT format hybrid `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` (S-03 decision (a) HMAC + Argon2id verifier server-side; client transmits literal token via `Authorization: Bearer`). Admin UI **NUNCA** consume PAT via URL params — auth/cookie session canonical.
- WebAuthn admin step-up: admin destructive ops em UI requerem fresh MFA ≤ 30 min (CTRL-AUTH-010); reuse S-03 WebAuthn flow.
- Dual-approval admin destructive ops (S-13 PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window — Lote 10.13 canonical): same approver max 3 ops within rolling window; second admin approval via separate session.
- DSR identity re-auth: MFA code re-prompted antes submit (CTRL-AUTH-010 fresh ≤ 30 min).

### 6.2 Privacy Model (herda `privacy_model.md`)

- **CTRL-PRIV-CONSENT-001..006** — IMPLEMENTA reflection em consent capture UI; 6-field payload + screenshot evidence EVT-012 + backend verify (S-11 R-S11-9). Locale match canonical (CTRL-PRIV-CONSENT-005).
- **CTRL-PRIV-001** (zero PII em logs client-side) — IMPLEMENTA via `safeLog()` wrapper em error tracking (Sentry/equiv.); allowlist explicit fields only (`request_id`, `user_id_hash`, `error_code`); never `email`, `pat`, `tenant_id` raw, blob digests.
- DSR self-service form 6 direitos LGPD Art. 18 + GDPR Art. 15-22; SLA 30d tracked + emailed; signed JWT receipt audit trail.
- Privacy notice page versioned mdx (`/privacy/v<M.m>`); changelog diff; sub-processors auto-generated + broadcast banner notice (S-11 R-S11-13).

### 6.3 Security Model (herda `security_model.md`)

- **CSP `default-src 'none'`** — IMPLEMENTA hardened CSP enforcement prod (no `unsafe-inline`, no `eval`, nonce-based script tags via Next.js `headers()` config); report-only fase staging 14d → enforce prod com refined allowlist; CSP report endpoint < 5 violations/dia.
- **CTRL-CRED-001** (no PAT em client logs) — IMPLEMENTA via PAT renderiza only-once + masked após copy; never echoed em error messages; Sentry redaction allowlist explicit; never em telemetry payload.
- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min) — IMPLEMENTA via Clerk SDK MFA re-auth flow em consent + DSR + admin ops; reuse S-03.
- XSS prevention: DOMPurify para qualquer user input rendered (audit log details + DSR text); escape default em React; clickjacking via `X-Frame-Options: DENY` + `Content-Security-Policy: frame-ancestors 'none'`.
- CSRF: Next.js API routes + SameSite cookies + CSRF tokens em destructive POSTs.

### 6.4 Observability Model (herda `observability_model.md §3.1`)

UI emite métricas Prometheus snake_case com `plan` label canonical (NUNCA per-tenant labels per INV-OBS-CARDINALITY-BUDGET):

- `corelink_admin_ui_pageview_total{route, plan}` (counter; route ∈ enum routes; emitido se telemetry opt-in).
- `corelink_admin_ui_consent_capture_total{outcome, locale}` (counter; outcome ∈ ok|fail|abandoned).
- `corelink_admin_ui_dsr_submit_total{direito, outcome}` (counter; direito ∈ access|correction|erasure|portability|objection|consent_revoke).
- `corelink_admin_ui_pat_create_total{scope, outcome}` (counter; scope ∈ read_only|read_write|admin).
- `corelink_admin_ui_audit_query_total{outcome}` (counter).
- `corelink_admin_ui_csp_violation_total{directive}` (counter; directive ∈ enum CSP directives).
- `corelink_admin_ui_lighthouse_score{pillar, route}` (gauge CI; pillar ∈ performance|a11y|best_practices|seo).
- `corelink_admin_ui_axe_violation_total{rule_id}` (counter CI; rule_id from axe-core).

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total; **NUNCA per-tenant labels**).

Sentry/equiv. error tracking client-side com `safeLog()` wrapper PII redaction explicit allowlist (CTRL-PRIV-001).

### 6.5 Failure Modes (herda `failure_modes.md`)

- **FM-150** (transient API): UI implements retry com exponential backoff em mutations (consent submit + DSR submit + PAT create); progress indicator clear; chaos test simula slow API ≥ 5s + retry option.
- **FM-160** (auth invalid): clear error UI com next-action ("re-authenticate via /sign-in"); redirect to Clerk auth flow; never echoes PAT em error message.

### 6.6 Resilience Patterns (herda `resilience_patterns.md`)

- Retry com exponential backoff (FM-150) — 100ms..1s..10s + jitter 50%; max 3 retries default em mutations.
- Optimistic UI updates com rollback em falha (PAT create + DSR submit).
- Skeleton loaders em SSR auth-gated routes (perceived performance).
- Service Worker cache para SSG public routes (offline-first `/privacy|/sub-processors|/legal/*`).

### 6.7 Compliance Matrix (herda `compliance_matrix.md`)

- **WCAG 2.2 AA** (W3C Recommendation 2023) — IMPLEMENTA full compliance; axe-core CI 0 violations + manual screen reader OK + color contrast + ARIA landmarks + keyboard navigation 100%.
- **LGPD Art. 18** (6 direitos titular) — IMPLEMENTA DSR self-service form 6 direitos + JWT receipt + SLA 30d.
- **GDPR Art. 15-22** (data subject rights) — IMPLEMENTA via DSR self-service form same 6 direitos.
- **ADA + EU Accessibility Act 2025** — IMPLEMENTA via WCAG 2.2 AA baseline.

## 7. Definition of Done (lane STANDARD)

> **Single-phase SEAL D+18** (STANDARD lane; DoD §6 não requer "30d sustained" criteria — apenas Lighthouse sustained 30d em CI gate + axe-core CI 0 violations + cross-browser matrix verde + UX SUS ≥ 75 sample). Sem two-phase observation window (vs S-13/S-14 HIGH_RISK 30d sustained gates).

- [ ] **WIs SEALED**: 7/7 (EVT-031).
- [ ] **E2E test** novo dev signup → cria tenant → aceita DPA → cria PAT → upload via CLI sucesso (EVT-018).
- [ ] **UI a11y** WCAG 2.2 AA compliance verificada via axe-core CI 0 violations + manual screen reader test (NVDA + VoiceOver) (EVT-019).
- [ ] **i18n** en-US + pt-BR + es-419 support inicial; matching Accept-Language; tested 3 locales; native speaker reviewed (EVT-018).
- [ ] **Cross-browser** Chrome + Firefox + Safari + Edge latest 2 versions cada (EVT-018).
- [ ] **Lighthouse score** ≥ 95 em Performance + A11y + Best Practices + SEO em 3 routes (`/`, `/dashboard`, `/privacy`) sustained 30d (EVT-002).
- [ ] **Consent UI** 6-field payload captured + screenshot evidence EVT-012 + verifiable backend S-11 R-S11-9 verify endpoint (EVT-012 + EVT-049).
- [ ] **DSR form** submit → JWT receipt em ≤ 1s + SLA clock visible; chaos test simula slow API (EVT-018).
- [ ] **PAT management** create + list + revoke flow tested via Playwright (EVT-018).
- [ ] **CSP enforcement prod** `default-src 'none'` + allowlists; CSP report endpoint < 5 violations/dia (EVT-027).
- [ ] **Lighthouse a11y CI gate** score < 95 fails PR (EVT-002).
- [ ] **PRR STANDARD** 5-8 sign-offs canonical (7 typical: Owner + Final Approver + Frontend Lead + QA + Product + Designer/a11y advisor + Privacy officer).
- [ ] **UX testing** com 5 devs externos: time-to-first-PAT ≤ 5 min; SUS score ≥ 75 (EVT-018).
- [ ] **10.s16.1** Consent form render respeita `notice_version` atual + gera `wording_id` único + EVT-012 screenshot evidence (EVT-012 + EVT-049).
- [ ] **10.s16.2** DSR form submit → confirmação em ≤ 1s + SLA clock starts visível (EVT-018).
- [ ] **10.s16.3** Lighthouse ≥ 95 todos os pillars sustained 30d.
- [ ] **10.s16.4** WCAG 2.2 AA zero axe-core violations + screen reader walkthrough OK.
- [ ] **10.s16.5** 3 locales detected via Accept-Language; native speaker review per language.
- [ ] **10.s16.6** CSP enforce mode prod com < 5 violations/dia (low false-positives).
- [ ] **10.s16.7** Audit log viewer query proxy + filters + export tested staged 30d.
- [ ] **10.s16.8** UX SUS score ≥ 75 com 5 dev sample.
- [ ] **CTRL-PRIV-CONSENT-005** (locale match) reflection em UI; backend S-11 valida.
- [ ] **CTRL-PRIV-001** (zero PII em client logs) enforced via `safeLog()` wrapper allowlist.
- [ ] **CTRL-CRED-001** (no PAT em client logs) enforced via only-once display + masked + redaction allowlist.
- [ ] **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min) enforced via Clerk SDK MFA re-auth flow.
- [ ] **Não introduz INVs novas** (UI é consumer das backend invariants já enforced em S-03/S-09/S-10/S-11/S-13).
- [ ] **Cost regression gate**: UI infra ≤ $300/mês (CF Pages + Sentry + Lighthouse CI free tier amortized).
- [ ] **Métricas snake_case Prometheus**: 8+ admin UI métricas emitting em staging com label `plan` aplicável (NUNCA per-tenant labels per INV-OBS-CARDINALITY-BUDGET) (EVT-013).

## 8. Dependencies

### Hard blockers

- **S-03 SEALED** (Clerk auth + PAT format hybrid + WebAuthn flows).
- **S-09 SEALED** (audit events R2 bucket + métricas).
- **S-10 SEALED** (billing data + Stripe integration).
- **S-11 SEALED** (privacy backend + DSR API + consent ledger D1 + verify endpoint R-S11-9).
- **S-13 SEALED** (admin plane API + PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window).

### Soft blockers

- **S-14 SEALED** (BYOK setup wizard if customer-facing; deferrable — UI surface partial fallback).

### Outbound

- S-18 (public docs may embed UI screenshots + reference flows).
- S-19 (customer onboarding leverages self-service UI; enterprise complement sales-led).
- S-20 (GA exige Lighthouse ≥ 95 + WCAG 2.2 AA + 3 locales + 5-dev SUS ≥ 75).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-15 SEALED + S-13 SEALED hard blockers; STANDARD lane no two-phase SEAL).
- **D+5**: WI-S16-001 + WI-S16-002 SEALED (skeleton + onboarding).
- **D+9**: WI-S16-003 SEALED (consent UI 6-field + screenshot evidence).
- **D+12**: WI-S16-004 SEALED (DSR self-service form + status viewer).
- **D+14**: WI-S16-005 SEALED (admin ops UI + audit viewer + dual-approval).
- **D+16**: WI-S16-006 SEALED (component library + a11y WCAG 2.2 AA + i18n full + privacy pages).
- **D+18**: WI-S16-007 SEALED (E2E + Lighthouse + UX workshop + CSP enforce + closing PRR).
- **D+18**: **Single-phase SEAL ceremony** (STANDARD lane; DoD §6 não requer 30d observation window — Lighthouse sustained CI gate + axe-core 0 violations + cross-browser matrix + UX SUS sample sufficient instant-verifiable; PRR coletados; sprint review).
- **Total**: 3 semanas (15 dias úteis) + buffer 5 dias.

## 10. Risk Register

Ver `_spec_contract.md §15` (11 riscos: Next.js quirks com CF Pages Edge runtime, Clerk integration edge cases MFA + SSO, a11y compliance late discovery, CSP false-positives prod, i18n quality per locale, PAT exposure em client logs browser dev tools, Lighthouse score regression, cross-browser rendering Safari quirks, consent UI screenshot evidence falha browser API, DSR form abandonment UX miss, CSP report flooding legitimate violations missed).

## 11. Observability Plan

DASH-ADMIN-UI (novo dashboard, opt-in only via customer telemetry):
- Pageview rate per route (`corelink_admin_ui_pageview_total{route}`).
- Consent capture rate (`corelink_admin_ui_consent_capture_total`); abandonment ratio alert > 30%.
- DSR submit rate per direito (`corelink_admin_ui_dsr_submit_total{direito}`).
- PAT create rate per scope (`corelink_admin_ui_pat_create_total{scope}`).
- Audit query rate (`corelink_admin_ui_audit_query_total`).
- CSP violation counter per directive (`corelink_admin_ui_csp_violation_total{directive}`); alert > 50/dia.
- Lighthouse score gauge per pillar/route (`corelink_admin_ui_lighthouse_score{pillar, route}`); alert < 95.
- axe-core violation counter per rule (`corelink_admin_ui_axe_violation_total{rule_id}`); alert > 0.

Métricas listadas em §6.4 (8+); todas com label `plan` aplicável quando customer-emitted; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 12. Security & Privacy

**STRIDE delta** (vs S-13/S-14 backend baseline):
- **Spoofing**: Clerk SSO + WebAuthn + MFA fresh ≤ 30 min CTRL-AUTH-010; PAT NUNCA via URL params (auth/cookie session canonical); CSRF tokens em destructive POSTs; SameSite cookies.
- **Tampering**: hardened CSP `default-src 'none'` enforcement prod; XSS prevention via DOMPurify para user input rendered + React escape default; clickjacking via `X-Frame-Options: DENY` + `frame-ancestors 'none'`.
- **Repudiation**: audit log viewer self-service + JSON export audit-grade evidence; consent capture 6-field + screenshot EVT-012; DSR submit JWT receipt + emailed forensic-grade trail.
- **Information disclosure**: CTRL-CRED-001 enforced (PAT only-once display + masked + Sentry redaction allowlist); CTRL-PRIV-001 enforced (`safeLog()` wrapper PII allowlist); never tenant_id raw em client logs.
- **DoS**: CSP report endpoint rate-limited + dedup; CSRF tokens; bundle ≤ 250KB; Service Worker cache em SSG.
- **Elevation of privilege**: admin destructive ops fresh MFA + dual-approval (S-13 PAT-DUAL-APPROVAL-001); same approver max 3 ops rolling 3-op window.

**LINDDUN delta**:
- **Linkability**: telemetry opt-in default-off; quando on, NÃO inclui tenant_id (anonymized version + OS + route + outcome); cardinality budget respeitado.
- **Identifiability**: PII redaction wrapper `safeLog()` allowlist explicit fields only; never raw email/pat/tenant_id em client logs.
- **Non-repudiation**: consent capture 6-field forensic + screenshot evidence EVT-012; DSR JWT receipt + emailed; audit log JSON export sanitized.
- **Detectability**: CSP report endpoint + Sentry alerts; axe-core CI gate em PRs.
- **Disclosure**: PAT only-once display + masked após copy; never echoed em error messages; CSP `default-src 'none'` previne XSS exfiltration.
- **Unawareness**: privacy notice page versioned mdx + changelog diff; sub-processors broadcast banner; consent revoke flow integrated DSR.
- **Non-compliance**: GDPR Art. 25 (data protection by design — opt-in default-off telemetry); LGPD Art. 6º X (transparency); ADA + EU Accessibility Act 2025 (WCAG 2.2 AA); LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-admin-ui.md`.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** (per `_spec_contract.md §18`):

- a11y regression em prod (axe-core in-prod scan finds violation) → 5-Why mandatory.
- PAT leak em client logs detectado → CRITICAL post-mortem + Security review + redaction reinforce.
- Consent screenshot evidence missed (em audit period) → post-mortem + privacy gap fix.
- DSR form failure (user can't submit) → post-mortem (compliance impact).
- CSP enforce mode disabled em prod → CRITICAL post-mortem + Security review.
- Lighthouse score < 90 sustained > 7d → post-mortem (DX regression).
- 3 locales native speaker review skipped → post-mortem + i18n debt review.

## 14. Sign-off (STANDARD 5-8 canonical; 7 typical)

7 roles canonical para STANDARD lane (per framework §33.5.4 + spec contract §6 sign-off line): Owner + Final Approver + Frontend Lead + QA + Product + Designer/a11y advisor + Privacy officer.

**Lane rationale (STANDARD vs HIGH_RISK 11)**:
- UI consume APIs já validated em S-03/S-09/S-10/S-11/S-13; não introduz novo path tenant data flow direto.
- CTRL-PRIV-CONSENT enforcement reside backend S-11 (verify endpoint R-S11-9 + consent ledger D1); UI é capture frontend reflection.
- CTRL-AUTH-010 (MFA fresh ≤ 30 min) reuse S-03 cycle 9 SEAL.
- Dual-approval admin destructive ops reuse S-13 PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window.
- Não há cripto-load-bearing controles novos (S-14 HIGH_RISK BYOK reuse only).
- CSP enforcement hardened é well-bounded surface (XSS defense baseline; report-only staging → enforce prod).
- Não há regulatory residency exposure (S-14 cobre WNAM/ENAM/WEUR/SAM; S-16 é client-side UI).

Compliance/AppSec/Architect com Crypto SME specialization são **NÃO mandatory canonical** em STANDARD lane (folded em PR review se applicable; Privacy officer canonical em S-16 dado scope CTRL-PRIV-CONSENT + DSR + LINDDUN review; Designer/a11y advisor canonical dado WCAG 2.2 AA + 5-dev UX workshop SUS).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-16 (cycle 12.S16.0; spec contract v1.1.0 STANDARD lane base; Frontend Admin UI + Tenant Self-Service + Consent + DSR; 7 WIs; single-phase SEAL D+18). |

---

**Fim de S-16 sprint contract.**
