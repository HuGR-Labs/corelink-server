---
id: "WI-S16-001"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
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
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s16", "ui", "nextjs", "clerk", "csp", "i18n", "scaffolding", "cf-pages", "standard"]
---

# WI-S16-001 — Next.js 15 Skeleton em `apps/web/` Deployed em Cloudflare Pages com Clerk SDK Integration (Reusa S-03 Cycle 9 SEAL — Clerk SSO + WebAuthn + PAT Flows; MFA Enforce em Routes Sensíveis `/settings/security|/audit|/dsr|/consent|/admin/*` per CTRL-AUTH-010 Fresh ≤ 30 min) + Hardened CSP `default-src 'none'` Setup (No `unsafe-inline`, No `eval`, Nonce-based Script Tags via Next.js `headers()` Config; Report-only Mode Staging 14d → Enforce Prod com Refined Allowlist após CSP Report Endpoint Analysis) + CSP Report Endpoint `/api/csp-report` Rate-limited + Dedup + Slack Alert Weekly + i18n Config Base 3 Locales (en-US default + pt-BR LGPD primary + es-419 LATAM; next-intl ou Equivalent; Missing-translation = Build Fail per Quality Standard 14.s16.6; Locale Detection via `Accept-Language` Header) + Routing Strategy SSR/ISR/SSG Hybrid (SSR Auth-gated `/dashboard|/audit|/billing|/settings`; SSG Public `/privacy|/sub-processors|/legal/*`; ISR `/blog` Deferred S-18) + Bundle Size Budget ≤ 250KB Gzipped Main Bundle (PR Fails se Exceeds; webpack-bundle-analyzer CI Report) + PII Redaction Wrapper `safeLog()` em Error Tracking Client-side per CTRL-PRIV-001 (Allowlist Explicit Fields Only — `request_id`, `user_id_hash`, `error_code`; Never `email`, `pat`, `tenant_id` Raw)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-16](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S16-001 |
| Título | Foundation Next.js 15 skeleton + Clerk SDK + hardened CSP + i18n config + routing strategy + bundle budget + PII redaction wrapper. |
| Sprint | S-16 |
| Lane | STANDARD |
| Forcing factors | none (UI consume APIs S-03 já validated; CSP enforcement well-bounded; não introduz novo path tenant data flow) |

## 1. Intent

Foundation WI do S-16. Entrega o **Next.js 15 app skeleton** em `apps/web/` deployed em Cloudflare Pages com Clerk SDK integration (reusa S-03 cycle 9 SEAL — Clerk SSO + WebAuthn + PAT flows), hardened CSP `default-src 'none'` setup (report-only staging → enforce prod), i18n config base 3 locales (en/pt-BR/es), routing strategy SSR/ISR/SSG hybrid, bundle size budget enforcement, e PII redaction wrapper `safeLog()` em error tracking client-side. Foundation layer para WI-S16-002..007.

```typescript
// File: apps/web/next.config.mjs
import { withPlausibleProxy } from 'next-plausible';

const nonce = () => Buffer.from(crypto.randomUUID()).toString('base64');

const cspHeaders = (mode: 'report-only' | 'enforce') => ({
  key: mode === 'enforce' ? 'Content-Security-Policy' : 'Content-Security-Policy-Report-Only',
  value: [
    "default-src 'none'",
    "script-src 'self' 'nonce-{NONCE}' https://clerk.corelink.dev",
    "style-src 'self' 'nonce-{NONCE}'",
    "img-src 'self' data: https://images.clerk.dev",
    "font-src 'self' data:",
    "connect-src 'self' https://api.corelink.dev https://clerk.corelink.dev",
    "frame-ancestors 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "report-uri /api/csp-report"
  ].join('; ')
});

export default {
  experimental: { instrumentationHook: true },
  i18n: { locales: ['en-US', 'pt-BR', 'es-419'], defaultLocale: 'en-US', localeDetection: true },
  async headers() {
    return [{
      source: '/:path*',
      headers: [
        cspHeaders(process.env.NODE_ENV === 'production' ? 'enforce' : 'report-only'),
        { key: 'X-Frame-Options', value: 'DENY' },
        { key: 'X-Content-Type-Options', value: 'nosniff' },
        { key: 'Strict-Transport-Security', value: 'max-age=63072000; includeSubDomains; preload' },
        { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
        { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=()' }
      ]
    }];
  }
};
```

## 2. Narrative

Next.js 15 + Cloudflare Pages é a stack escolhida (Edge runtime; SSR/ISR/SSG hybrid; CF Pages free tier baseline). Clerk SDK (S-03 cycle 9 SEAL) provê SSO + WebAuthn + PAT flows com MFA enforcement em routes sensíveis. Hardened CSP `default-src 'none'` é XSS defense baseline (Stripe/Auth0 parity). i18n 3 locales (en-US default + pt-BR LGPD primary + es-419 LATAM) reuse pattern S-11 privacy notice locales matching. Bundle ≤ 250KB gzipped enforce em CI (webpack-bundle-analyzer). PII redaction wrapper `safeLog()` em error tracking client-side per CTRL-PRIV-001 (allowlist explicit fields only).

Este WI é foundation: define app workspace member `apps/web/`, scaffolds Next.js 15 com routing strategy, integra Clerk SDK, define CSP headers via `next.config.mjs`, define i18n config, define bundle budget CI gate, e implementa `safeLog()` wrapper PII redaction allowlist.

**Risk justification STANDARD lane (zero forcing factors)**:
- UI consume APIs S-03 já validated; não introduz novo path tenant data flow.
- CSP report-only staging → enforce prod is bounded surface (well-defined refinement cycle).
- Clerk SDK reuse single Rust+JS truth (Clerk SSO + WebAuthn + PAT já tested em S-03 cycle 9 SEAL).
- Bundle budget + PII redaction wrapper é well-bounded discipline (PR gate enforcement).

## 3. Customer Impact & Journey

**Persona 1 — Build Engineer prospect novo**:
- App skeleton load < 2s p95 latency (CF Edge runtime; SSR auth-gated routes).
- Clerk SSO sign-in seamless; MFA enforce em routes sensíveis transparent.
- 3 locales detected via Accept-Language header; manual switcher em footer.

**Persona 2 — Security Engineer**:
- CSP `default-src 'none'` enforcement prod previne XSS exfiltration.
- CSP report endpoint rate-limited + dedup + Slack alert weekly = monitoring sustained.
- PII redaction wrapper `safeLog()` allowlist explicit; never raw email/pat/tenant_id em client logs.

## 4. Capability Mapping

- **CAP-UI-009** (i18n + a11y baseline 3 locales WCAG 2.2 AA) — IMPLEMENTA primary i18n config + locale detection; a11y baseline em WI-S16-006.
- **CAP-UI-001..008** — IMPLEMENTA foundation layer (skeleton + Clerk + CSP + routing); features em WI-S16-002..006.
- Trace: `_spec_contract.md §4 + §5.1 (R-S16-1..3)` + `auth_model.md` (Clerk SSO + WebAuthn + PAT flows S-03 cycle 9 SEAL) + `security_model.md` (CSP `default-src 'none'`; CTRL-CRED-001) + `privacy_model.md` (CTRL-PRIV-001 zero PII em logs).

## 5. Tipo

Feature WI; STANDARD lane; foundation layer.

## 6. Escopo

### 6.1 In-scope

1. **Next.js 15 app workspace member** `apps/web/`:
   - `package.json` com dependencies: next@15.x, react@18.x, @clerk/nextjs, next-intl (or react-i18next), @next/mdx, tailwindcss, radix-ui primitives, sentry/equiv. SDK.
   - Workspace member em root `package.json` (pnpm/npm/yarn workspaces).
   - Deploy target: Cloudflare Pages (Edge runtime primary; Nodejs runtime fallback se Edge limitations).

2. **Clerk SDK integration** (reusa S-03 cycle 9 SEAL):
   - `@clerk/nextjs` configured com `CLERK_PUBLISHABLE_KEY` + `CLERK_SECRET_KEY` via env vars.
   - Sign-in flow: `/sign-in` route via `<SignIn />` component.
   - Sign-up flow: `/sign-up` route via `<SignUp />` component.
   - WebAuthn passkey enrolment via Clerk SDK MFA factor.
   - MFA enforcement em routes sensíveis `/settings/security|/audit|/dsr|/consent|/admin/*` via middleware `clerkMiddleware()` + `auth().protect()` checks; fresh MFA ≤ 30 min via `lastVerifiedAt` claim check (CTRL-AUTH-010).
   - PAT consumption via `Authorization: Bearer` header forward em API routes; auth/cookie session canonical em UI (NUNCA via URL params).

3. **Hardened CSP `default-src 'none'` setup**:
   - CSP headers via Next.js `headers()` config (vide §1 narrative example):
     - `default-src 'none'` (deny by default).
     - `script-src 'self' 'nonce-{NONCE}' https://clerk.corelink.dev` (no `unsafe-inline`, no `eval`).
     - `style-src 'self' 'nonce-{NONCE}'`.
     - `img-src 'self' data: https://images.clerk.dev`.
     - `font-src 'self' data:`.
     - `connect-src 'self' https://api.corelink.dev https://clerk.corelink.dev`.
     - `frame-ancestors 'none'` (clickjacking defense).
     - `base-uri 'self'`.
     - `form-action 'self'`.
     - `report-uri /api/csp-report`.
   - Nonce gen per request (cryptographically random; injected via Next.js middleware).
   - **Report-only mode em staging 14d** → analyze CSP report endpoint findings → refine allowlist → **enforce mode em prod** após low-violations baseline (< 5/dia).
   - Additional security headers: `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Strict-Transport-Security: max-age=63072000`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy: camera=(), microphone=(), geolocation=()`.

4. **CSP report endpoint** `/api/csp-report`:
   - Rate-limited (≤ 100 reports/min per IP).
   - Dedup via canonical hash (directive + violated-uri + source-file).
   - Persist em D1 ou KV temporary (7d retention; CSP analysis only).
   - Slack alert weekly digest (top violations + new directives + frequency).
   - Reduces report flooding mitigation per spec contract §15 row 11.

5. **i18n config base 3 locales**:
   - `next-intl` (or equivalent) configured com locales `['en-US', 'pt-BR', 'es-419']`, default `'en-US'`, `localeDetection: true` via `Accept-Language` header.
   - Translation files: `apps/web/src/messages/{en-US,pt-BR,es-419}.json`.
   - Missing-translation = build fail (CI gate per Quality Standard 14.s16.6).
   - Locale switcher component em footer (manual override; cookie persisted).
   - Plain-language baseline (Flesch-Kincaid grade ≤ 8 verified em CI per locale; native speaker review per locale em WI-S16-006).

6. **Routing strategy SSR/ISR/SSG hybrid**:
   - **SSR auth-gated** (private data; auth required):
     - `/dashboard` (usage dashboard).
     - `/audit` (audit log viewer).
     - `/billing` (billing overview).
     - `/settings/*` (settings + PAT mgmt + DSR + consent revoke).
     - `/admin/*` (admin ops).
   - **SSG public** (cacheable; no auth):
     - `/privacy` (privacy notice mdx).
     - `/privacy/changelog` (diff between versions).
     - `/privacy/sub-processors` (auto-generated).
     - `/legal/dpa` (DPA terms).
     - `/legal/tos` (Terms of Service).
   - **ISR** (deferred S-18):
     - `/blog` (incremental static regeneration; revalidate 60s).

7. **Bundle size budget ≤ 250KB gzipped main bundle**:
   - `webpack-bundle-analyzer` CI report em PR.
   - PR fails se main bundle exceeds 250KB gzipped (per Quality Standard 14.s16.4).
   - Tree-shake aggressive (Next.js automatic + manual import discipline).
   - Code splitting per route (dynamic imports).
   - No moment.js (use date-fns or native Intl); no lodash full import (cherry-pick).

8. **PII redaction wrapper `safeLog()` em error tracking client-side** (CTRL-PRIV-001):
   - File `apps/web/src/lib/safe-log.ts`.
   - API: `safeLog(level, msg, context)` where `context: Record<string, unknown>` filtered via allowlist.
   - **Allowlist** explicit fields only: `request_id`, `user_id_hash` (sha256), `error_code`, `route`, `cli_version` (if applicable).
   - **Denylist explicit** (assert never present): `email`, `pat`, `tenant_id` raw, `blob_digest`, `password`, `mfa_code`.
   - Wrapper integrates Sentry/equiv. SDK; calls `Sentry.captureMessage()` with sanitized context.
   - Property test: feed 100 random objects with denylisted keys; assert wrapper strips them all.

### 6.2 Out-of-scope (deferred)

- Tenant onboarding flow (WI-S16-002).
- Consent management UI (WI-S16-003).
- DSR self-service form (WI-S16-004).
- Admin operations UI + audit viewer (WI-S16-005).
- Component library + a11y full + privacy/sub-processors pages (WI-S16-006).
- E2E tests + Lighthouse CI + UX workshop + closing PRR (WI-S16-007).

## 7. Anti-Scope

- CSP `unsafe-inline` ou `unsafe-eval` (XSS regression baseline).
- Skip MFA enforcement em routes sensíveis (CTRL-AUTH-010 violation).
- PAT em URL params (CTRL-CRED-001 violation; auth/cookie session canonical).
- Bundle size > 250KB gzipped (DX regression).
- Missing-translation tolerated em CI (i18n drift).
- PII raw em error tracking (CTRL-PRIV-001 violation).
- Skip CSP report endpoint (monitoring gap).
- Skip security headers (X-Frame-Options + HSTS + Referrer-Policy baseline).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Next.js 15 skeleton + Clerk + CSP + i18n + bundle budget + PII redaction

  Scenario: App skeleton deployed em CF Pages staging
    Given apps/web/ workspace member configured
    When `pnpm build` + CF Pages deploy runs
    Then app accessible em https://staging.corelink.dev
    And health check route /api/health returns 200

  Scenario: Clerk SDK integrated com SSO + WebAuthn + MFA
    Given CLERK_PUBLISHABLE_KEY + CLERK_SECRET_KEY env vars set
    When user navigates /sign-in
    Then Clerk <SignIn /> component rendered
    And SSO providers + email + WebAuthn passkey options visible
    And MFA enforced em routes sensíveis via middleware

  Scenario: MFA fresh ≤ 30 min em route sensível
    Given user authenticated > 30 min ago without recent MFA
    When user navigates /admin/audit
    Then redirect to MFA re-prompt
    And after MFA verified < 30 min, route accessible
    And lastVerifiedAt claim updated

  Scenario: CSP report-only mode em staging
    Given app deployed staging com NODE_ENV=development
    When user loads any route
    Then HTTP header `Content-Security-Policy-Report-Only` present
    And directive `default-src 'none'` enforced
    And violations reported para /api/csp-report

  Scenario: CSP enforce mode em prod
    Given app deployed prod com NODE_ENV=production
    When user loads any route
    Then HTTP header `Content-Security-Policy` present (enforce)
    And inline script without nonce blocked
    And eval() blocked

  Scenario: CSP report endpoint rate-limited + dedup
    Given /api/csp-report endpoint configured
    When 200 violations submitted em 1 minuto same IP
    Then 100 accepted; 100 rate-limited
    And dedup via hash (directive + violated-uri + source-file)
    And weekly Slack digest emitted

  Scenario: i18n 3 locales detection via Accept-Language
    Given user com Accept-Language: pt-BR
    When user navigates /
    Then UI rendered em pt-BR
    And messages from src/messages/pt-BR.json loaded

  Scenario: Missing translation = build fail
    Given key `dashboard.title` ausente em es-419.json
    When `pnpm build` runs CI
    Then build fails com error "missing translation: dashboard.title (es-419)"
    And PR cannot merge

  Scenario: Bundle budget ≤ 250KB gzipped
    Given main bundle build complete
    When webpack-bundle-analyzer report runs CI
    Then main bundle ≤ 250KB gzipped
    And PR fails se exceeds

  Scenario: PII redaction wrapper safeLog() strips denylist
    Given context = { email: "x@y.com", request_id: "req-123", pat: "corelink_..." }
    When safeLog("error", "test", context) called
    Then Sentry capture sanitized context
    And email + pat stripped
    And request_id retained

  Scenario: SSR auth-gated route redirects unauth
    Given user não autenticado
    When user navigates /dashboard
    Then redirect to /sign-in com redirect_url=/dashboard

  Scenario: SSG public route cacheable sem auth
    Given user não autenticado
    When user navigates /privacy
    Then route accessible sem auth
    And HTTP cache-control: public, max-age=300

  Scenario: Security headers em todas routes
    Given any route loaded
    Then HTTP header X-Frame-Options: DENY
    And HTTP header Strict-Transport-Security: max-age=63072000
    And HTTP header X-Content-Type-Options: nosniff
    And HTTP header Referrer-Policy: strict-origin-when-cross-origin
```

## 9. Design Decisions

### 9.1 Why Next.js 15 + CF Pages (não Vite + Vercel)

- CF Pages alinhado com CoreLink stack (Workers + R2 + D1); Edge runtime faster cold-start.
- Next.js 15 App Router: SSR/ISR/SSG hybrid native; server components + streaming.
- Ecosystem maduro (i18n libs, Clerk SDK, mdx).

### 9.2 Why Clerk SDK reuse S-03 (não Auth0 ou WorkOS)

- Já SEALED em S-03 cycle 9 (SSO + WebAuthn + PAT flows); reuse evita drift.
- Bem integrado com Next.js (`@clerk/nextjs` middleware).
- WebAuthn passkey nativo + MFA fresh ≤ 30 min via `lastVerifiedAt` claim.

### 9.3 Why CSP report-only staging → enforce prod (não direct enforce)

- CSP enforcement direct em prod risk false-positives; legit violations missed (per spec contract §15 row 11).
- Report-only staging 14d → analyze + refine allowlist → enforce prod.
- CSP report endpoint rate-limited + dedup + Slack alert weekly = monitoring sustained.

### 9.4 Why next-intl (não react-i18next)

- next-intl: native App Router support; SSR-friendly; type-safe via codegen.
- react-i18next: client-only legacy; less ergonomic em SSR.
- Trade-off: lock-in next-intl; mitigated via thin abstraction layer.

### 9.5 Why bundle ≤ 250KB gzipped (não 500KB)

- Lighthouse perf ≥ 95 dependency (large bundle = TBT/LCP regression).
- CF Pages cold-start cost amortized via small bundle.
- Discipline forces tree-shake + dynamic imports + code splitting.

### 9.6 Why safeLog() wrapper (não direct Sentry)

- Sentry direct API exposes raw context; PII leak risk.
- `safeLog()` wrapper enforces allowlist explicit + denylist assertion.
- Property test guarantees no PII in production telemetry.

### 9.7 ADR potencial?

- Não. Patterns reused (Next.js 15 standard + Clerk SDK from S-03 + CSP standard hardening). No novel architecture decision em S-16-001.

## 10. Completeness Criteria

- [ ] **10.s16.001.1** Next.js 15 app skeleton em `apps/web/` deployed CF Pages staging (EVT-018).
- [ ] **10.s16.001.2** Clerk SDK integrated com SSO + WebAuthn + MFA enforcement em routes sensíveis (EVT-018).
- [ ] **10.s16.001.3** Hardened CSP `default-src 'none'` headers configured; report-only staging mode (EVT-027).
- [ ] **10.s16.001.4** CSP report endpoint `/api/csp-report` rate-limited + dedup + Slack alert weekly (EVT-027).
- [ ] **10.s16.001.5** i18n config base 3 locales (en/pt-BR/es) com locale detection via Accept-Language (EVT-018).
- [ ] **10.s16.001.6** Missing-translation = build fail CI gate (EVT-002).
- [ ] **10.s16.001.7** Routing strategy SSR/ISR/SSG hybrid configured per spec.
- [ ] **10.s16.001.8** Bundle budget ≤ 250KB gzipped enforce CI (webpack-bundle-analyzer report).
- [ ] **10.s16.001.9** PII redaction wrapper `safeLog()` allowlist + property test 0 PII em telemetry (EVT-049).
- [ ] **10.s16.001.10** Security headers (X-Frame-Options + HSTS + Referrer-Policy + X-Content-Type-Options + Permissions-Policy) em todas routes.

## 11. DoD

- [ ] App skeleton em `apps/web/` workspace; CF Pages deploy verde.
- [ ] Clerk SDK integration tested (SSO + WebAuthn + MFA enforcement).
- [ ] CSP report-only staging mode em CSP report endpoint receiving + analyzing.
- [ ] i18n 3 locales base configured com translation files committed.
- [ ] Bundle ≤ 250KB gzipped CI gate verde.
- [ ] PII redaction wrapper `safeLog()` implemented + property test passing.
- [ ] Security headers verified via tools (e.g., securityheaders.com test grade A+).
- [ ] Tests: unit (CSP header generator + safeLog wrapper + auth middleware) + integration (Clerk sign-in + MFA enforce + i18n locale detection) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-CRED-001** (no PAT em client logs) reforced via PAT NUNCA via URL params + auth/cookie session canonical + safeLog allowlist.
- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min) reforced via Clerk middleware + lastVerifiedAt claim check.
- **CTRL-PRIV-001** (zero PII em client-side logs) reforced via safeLog wrapper allowlist explicit.
- **INV-OBS-CARDINALITY-BUDGET** respeitado (8+ admin UI métricas em §6.4 spec contract; NUNCA per-tenant labels).
- **Não introduz INVs novas** (UI é consumer; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Next.js app | `apps/web/` | TypeScript |
| Next.js config | `apps/web/next.config.mjs` | JavaScript |
| Clerk middleware | `apps/web/src/middleware.ts` | TypeScript |
| CSP headers config | `apps/web/src/lib/csp.ts` | TypeScript |
| CSP report endpoint | `apps/web/src/app/api/csp-report/route.ts` | TypeScript |
| i18n config | `apps/web/src/i18n/config.ts` | TypeScript |
| Translation files | `apps/web/src/messages/{en-US,pt-BR,es-419}.json` | JSON |
| safeLog wrapper | `apps/web/src/lib/safe-log.ts` | TypeScript |
| CF Pages config | `apps/web/wrangler.toml` | TOML |
| GitHub Actions workflow | `.github/workflows/deploy-web.yml` | YAML |

## 14. Quality Standards

- **14.s16.001.1** CSP `default-src 'none'` enforce prod com < 5 violations/dia.
- **14.s16.001.2** Bundle ≤ 250KB gzipped main bundle (CI gate).
- **14.s16.001.3** Test coverage ≥ 80% (CSP + safeLog + auth middleware).
- **14.s16.001.4** SAST: npm audit + Dependabot zero CVEs HIGH/CRITICAL.
- **14.s16.001.5** SSR auth-gated routes redirect unauth via Clerk middleware verified.
- **14.s16.001.6** i18n missing-translation = build fail (CI gate).
- **14.s16.001.7** Security headers grade A+ em securityheaders.com.

## 15. Test Plan

### Unit tests (≥ 80% coverage)
- CSP header generator: nonce embedded; directives canonical.
- safeLog wrapper: allowlist enforced; denylist stripped; property test 100 random objects.
- Clerk middleware: protected routes redirect unauth; MFA fresh ≤ 30 min check.
- i18n locale detection: Accept-Language parsed; fallback default `en-US`.

### Integration tests
- Clerk sign-in flow: SSO + email + WebAuthn passkey enrolment.
- MFA enforce: redirect to re-prompt em route sensível após > 30 min.
- CSP report endpoint: rate-limit + dedup + persist + Slack digest.
- i18n locale switcher: cookie persisted + UI re-rendered.

### Negative scenarios (≥ 4)
1. **PAT em URL param tentativa**: rejected via auth middleware; safeLog não logs raw.
2. **Inline script sem nonce**: blocked em prod CSP enforce.
3. **Missing translation key**: build fails em CI.
4. **Bundle > 250KB**: PR fails CI gate.
5. **Email em context safeLog**: stripped from Sentry capture.
6. **MFA stale > 30 min**: redirect to re-prompt antes route sensível.

### Cross-OS / Cross-browser
- Manual smoke test em Chrome + Firefox + Safari + Edge latest 2 versions; full matrix em WI-S16-007.

## 16. Failure Modes

- **FM-150** (transient API): Clerk SDK + CSP report endpoint retry com exponential backoff (3 retries default; configurable).
- **FM-160** (auth invalid): clear error UI; redirect to Clerk sign-in flow; never echoes PAT em error message.

## 17. Controls

- **CTRL-CRED-001** (no PAT em client logs): PAT NUNCA via URL params; auth/cookie session canonical; safeLog allowlist; never em telemetry payload.
- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min): Clerk middleware + lastVerifiedAt claim check em routes sensíveis.
- **CTRL-PRIV-001** (zero PII em client-side logs): safeLog wrapper allowlist explicit + denylist assertion + property test.

## 18. Resilience Patterns

- Retry transient errors (FM-150): exponential backoff em mutations; progress indicator clear.
- Skeleton loaders em SSR auth-gated routes (perceived performance).
- Service Worker cache para SSG public routes (offline-first; deferred WI-S16-006).
- CSP report endpoint rate-limit + dedup (DoS resilience).

## 19. Observability

UI métricas Prometheus snake_case com `plan` label canonical (NUNCA per-tenant labels):
- `corelink_admin_ui_pageview_total{route, plan}` counter (emitido se telemetry opt-in).
- `corelink_admin_ui_csp_violation_total{directive}` counter (CSP report endpoint).
- `corelink_admin_ui_lighthouse_score{pillar, route}` gauge (CI; reported em WI-S16-007).
- Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas; NUNCA per-tenant labels).

Sentry/equiv. error tracking client-side com `safeLog()` wrapper PII redaction explicit allowlist (CTRL-PRIV-001).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Clerk SSO + WebAuthn + MFA fresh ≤ 30 min CTRL-AUTH-010; PAT NUNCA via URL params; SameSite cookies.
- **Tampering**: hardened CSP `default-src 'none'` enforce prod; XSS prevention; clickjacking via X-Frame-Options + frame-ancestors 'none'.
- **Repudiation**: Clerk audit events backend + CSP report endpoint logged.
- **Information disclosure**: CTRL-CRED-001 (PAT only-once display em PAT mgmt WI-S16-002); CTRL-PRIV-001 (safeLog allowlist).
- **DoS**: CSP report endpoint rate-limited + dedup; bundle ≤ 250KB.
- **Elevation of privilege**: MFA enforce em routes sensíveis; admin dual-approval em WI-S16-005.

**LINDDUN delta**:
- Linkability: telemetry opt-in default-off (S-15 pattern); pageview anonymized (route + plan; NUNCA tenant_id).
- Identifiability: safeLog allowlist; never raw email/pat/tenant_id em client logs.
- Disclosure: CSP enforce previne XSS exfiltration; Sentry redaction allowlist explicit.

## 21. Dependencies

### Hard blockers
- S-03 SEALED (Clerk auth + PAT format hybrid + WebAuthn flows).

### Soft blockers
- S-09 SEALED (CSP report endpoint persist em D1 ou KV).
- CF Pages account configured + DNS staging.corelink.dev pointed.

### Outbound
- WI-S16-002 (tenant onboarding flow consume skeleton).
- WI-S16-003 (consent UI consume skeleton).
- WI-S16-004 (DSR form consume skeleton).
- WI-S16-005 (admin ops UI consume skeleton).
- WI-S16-006 (component library + a11y consume skeleton).
- WI-S16-007 (E2E + Lighthouse + closing PRR consume skeleton).

## 22. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; foundation skeleton + Clerk + CSP + i18n + routing + bundle + PII redaction).

## 23. Cost Analysis

- CF Pages: free tier (≤ 500 builds/mês; ≤ 100k requests/dia free).
- Clerk: free tier MAU ≤ 10k; growth tier $25/mês after.
- Sentry: free tier ≤ 5k events/mês; growth tier $26/mês after.
- Total: ~$50/mês incremental at growth tier.

## 24. Post-mortem Hooks

- CSP enforce mode disabled em prod → CRITICAL post-mortem + Security review.
- PII leak em client logs detectado → CRITICAL post-mortem + Privacy review + redaction reinforce.
- MFA enforcement bypass em route sensível → CRITICAL post-mortem + AppSec review.
- Bundle > 250KB sustained > 1 sprint → DX regression post-mortem.
- Missing translation deployed em prod → i18n debt review.

## 25. Rollback / Recovery

CF Pages deploy regression detected → revert via CF Pages rollback (previous deploy remains accessible); Next.js patch release com fix.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Next.js 15 quirks com CF Pages Edge runtime | M | M | MEDIUM | M | LOW | Test CF Pages Edge runtime early; documented compatibility matrix; fallback Nodejs runtime se needed |
| R-002 | Clerk integration MFA + SSO edge cases | M | L | LOW | L | LOW | Comprehensive test matrix; Clerk support tier; reuse S-03 cycle 9 SEAL patterns |
| R-003 | CSP false-positives prod (legitimate broken) | M | L | LOW | L | LOW | Report-only staging 14d → refine allowlist → enforce prod |
| R-004 | Bundle > 250KB regression | M | L | LOW | L | LOW | CI gate fails PR; weekly perf review; tree-shake discipline |
| R-005 | i18n quality issues per locale | M | L | LOW | L | LOW | Native speaker review (WI-S16-006); missing-translation = build fail |
| R-006 | PII leak via safeLog bypass | L | L | HIGH | L | LOW | Property test 100 random objects; denylist assertion; PR review enforced |
| R-007 | CSP report flooding (legitimate violations missed) | M | L | LOW | L | LOW | Rate-limit + dedup + categorize; Slack alert weekly |

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink Admin UI Foundation — Next.js 15 + Clerk + CSP".
- Doc `docs/internal/admin-ui-architecture.md` — overview.
- Onboarding test (3 questions): CSP directives + safeLog allowlist + Clerk MFA fresh ≤ 30 min.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Frontend Lead | _TBD; emphatic — Next.js 15 + Clerk SDK + CSP + i18n config_ | _pending_ |
| 4 | QA | _TBD; emphatic — Clerk integration + CSP report endpoint + i18n locale tests_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | Designer/a11y advisor | _TBD; emphatic — locale switcher UX + security headers UX impact_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — safeLog allowlist + CTRL-PRIV-001 reflection + LINDDUN review_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical. Compliance/AppSec/Architect com Crypto SME specialization são NÃO mandatory canonical em STANDARD lane (folded em PR review se applicable).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S16-001 (cycle 12.S16.0; foundation Next.js 15 skeleton + Clerk + CSP + i18n + bundle budget + PII redaction). |

## 30. Anti-patterns evitados

- CSP `unsafe-inline` ou `unsafe-eval` (XSS regression baseline).
- Skip MFA enforcement em routes sensíveis (CTRL-AUTH-010 violation).
- PAT em URL params (CTRL-CRED-001 violation).
- Bundle size > 250KB gzipped (DX regression).
- Missing-translation tolerated em CI (i18n drift).
- PII raw em error tracking (CTRL-PRIV-001 violation).
- Skip CSP report endpoint (monitoring gap).
- Skip security headers baseline.
- Direct CSP enforce prod sem report-only staging refinement (false-positives risk).

---

**Fim WI-S16-001.**
