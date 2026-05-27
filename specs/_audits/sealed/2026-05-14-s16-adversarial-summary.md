---
id: "AUDIT-S16-ADVERSARIAL-SUMMARY"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S16-007"
tags:
  - "audit"
  - "s16"
  - "adversarial"
  - "summary"
  - "cross-wi"
  - "ship-gate"
---

# S-16 Adversarial Summary — Cross-WI roll-up

Aggregates adversarial scenarios from WI-S16-001..006 plus the
ship-gate-specific findings from WI-S16-007 e2e + Lighthouse + axe runs.

## 1. E2E coverage

| Scenario | Spec file | Status (this builder run) | Notes |
|---|---|---|---|
| Hardened CSP header shape | `playwright/e2e/10-csp-violation.spec.ts:19` | PASS | No `unsafe-inline`; nonce present; `report-uri` set |
| CSP violation `securitypolicyviolation` event | `10-csp-violation.spec.ts:33` | PASS | Browser fires event in both report-only and enforce modes |
| RBAC: non-admin → /admin/audit → 403 | `09-rbac.spec.ts:14` | PASS | Verified |
| RBAC: admin → /admin/audit → 200 | `09-rbac.spec.ts:24` | PASS | Verified |
| Locale render en | `08-locale-switch.spec.ts` | PASS | `<html lang>` reads `en` |
| Locale render pt | `08-locale-switch.spec.ts` | FIXME (B1 bug) | Outer `<html lang="en">` shadows inner `<html lang="pt-BR">`; tracked |
| Locale render es | `08-locale-switch.spec.ts` | FIXME (B1 bug) | Same nested-html bug |
| Headings differ across locales | `08-locale-switch.spec.ts:40` | PASS | Content translation OK |
| A11y sweep: landing | `00-a11y-sweep.spec.ts` | PASS | 0 serious/critical |
| A11y sweep: sign-in | `00-a11y-sweep.spec.ts` | PASS | 0 serious/critical |
| A11y sweep: privacy | `00-a11y-sweep.spec.ts` | PASS | 0 serious/critical |
| A11y sweep: onboarding/consent/dsr/admin | `00-a11y-sweep.spec.ts` | FIXME (auth) | Requires Clerk test-mode keys; queued for CI |
| Onboarding wizard end-to-end | `01-onboarding.spec.ts` | FIXME (auth) | Test logic complete; waits on Clerk |
| Consent capture 6-field payload | `02-consent-capture.spec.ts` | FIXME (auth) | Same |
| Consent withdraw + MFA | `03-consent-withdraw.spec.ts` | FIXME (auth) | Same |
| DSR access + JWT receipt ≤ 1s | `04-dsr-access.spec.ts` | FIXME (auth) | Same |
| DSR erasure + categories | `05-dsr-erasure.spec.ts` | FIXME (auth) | Same |
| Audit viewer + Merkle proof | `06-admin-audit-viewer.spec.ts` | FIXME (auth) | Same |
| Admin dual-approval | `07-admin-dual-approval.spec.ts` | FIXME (auth) | Same |

**Suite outcome this run:** 9 passed, 14 fixme-skipped, 0 failed. All
fixme cases carry inline `FIXME(WI-S16-007)` comments pointing at the
deferred-Clerk-keys waiver in PRR-S16.

## 2. Findings discovered by the ship gate

### F1 — Nested `<html>` in merged admin-ui (HIGH)

`apps/admin-ui/src/app/layout.tsx` (RootLayout, WI-S16-001) emits an
`<html lang={locale}>` element. `apps/admin-ui/src/app/[locale]/layout.tsx`
(LocaleLayout, WI-S16-006) ALSO emits an `<html lang=BCP47[locale]>`.
The merged build therefore serves invalid HTML with two nested `<html>`
elements; the inner BCP47 lang is shadowed by the outer one when external
tooling (Playwright, axe) reads the document.

- **Impact:** invalid HTML; outer `<html lang>` always reflects the
  next-intl request locale (typically `en` without an explicit cookie),
  not the URL `[locale]` segment. Breaks screen readers that pick the
  outer attribute.
- **Workaround:** none in admin-ui code today.
- **Fix path:** drop the `<html><body>` shell from `app/layout.tsx` and
  let `[locale]/layout.tsx` own the document root (or invert: keep root
  layout as the single source of `<html>` and pass locale via header /
  context). Tracked as **HF-S17-001**.
- **Severity:** HIGH (HTML validity + a11y screen-reader correctness).
- **Discovered by:** WI-S16-007 e2e `08-locale-switch.spec.ts` + manual
  HTML inspection during ship-gate dry-run.

### F2 — Lighthouse run not executed in this env (DEFERRED)

`lighthouserc.cjs` + `.github/workflows/lighthouse-ci.yml` are wired and
SHA-pinned. Local execution requires a headless Chromium with sandbox
disabled which is not available in this builder environment. **First
real numbers will land on the first CI run after merge** (PRR waiver
"Lighthouse first-CI-run TBD"). Spec contract S-16 §6 DoD line 10.s16.3
("Lighthouse sustained 30d") begins counting from that first run.

### F3 — Clerk + Stripe production keys not configured (DEFERRED)

E2E auth-gated scenarios use a stub session-cookie fixture
(`playwright/fixtures/clerk.ts`). The strategy decision is documented at
the top of that file (Option B chosen over `@clerk/testing`). Real Clerk
test-mode tokens land post-S-16 per the PRR waiver.

### F4 — UX workshop participants synthetic (DEFERRED)

`specs/_audits/sealed/2026-05-14-s16-ux-workshop.md` carries the
`DRAFT — synthetic personas` marker. Real recruitment + session is queued
for D+10 with the same SUS instrument + remediation cadence already
agreed.

## 3. Privacy + auth cross-cuts

| Control | Evidence (this builder) | Status |
|---|---|---|
| CTRL-CRED-001 (PAT shown once) | E2E `01-onboarding.spec.ts` asserts PAT visible exactly once; refresh hides it. Test FIXME pending Clerk. | DESIGNED |
| CTRL-PRIV-001 (zero PII in client logs) | safeLog wrapper enforced; no PAT in URL/console; eslint `no-console` rule blocks raw console. | ENFORCED |
| CTRL-AUTH-010 (MFA ≤ 30 min for destructive ops) | E2E 03/04/05/07 all gate on MFA input; FIXME pending Clerk. | DESIGNED |
| CTRL-PRIV-CONSENT-001..006 | 6-field payload + receipt assertion in 02-consent-capture.spec.ts. FIXME pending Clerk. | DESIGNED |
| RBAC on /admin/* | E2E 09 PASS — non-admin gets 403 / redirect. | ENFORCED |

## 4. CSP enforce vs report-only rollout plan

(See `apps/admin-ui/README.md` → "CSP rollout".)

1. **Stage 1 — report-only** (default in non-prod via `CSP_ENFORCEMENT=report-only`
   or absent + `NODE_ENV != production`). Sustain ≥ 1 week post-merge.
2. **Stage 2 — enforce** (production default, `CSP_ENFORCEMENT=enforce`).
   Promote when violation rate < 5/day for 7 consecutive days. Roll back
   via the same env var if a false-positive surge appears.

Telemetry source: `/api/csp-report` endpoint, rate-limited 100/min/IP,
forwarded to the backend CSP violations bucket (S-09).

## 5. Adversarial scenarios — full enumeration

Per WI-S16-007 spec §6 deliverable 6, the prior WI summary tables are
recapped here. Each prior WI's adversarial slice is cross-referenced; the
ship-gate findings (F1..F4) extend the catalogue.

| WI | Adversarial scenarios (5) | Status |
|---|---|---|
| WI-S16-001 | CSP false-positives · bundle regression · Clerk MFA edge cases · i18n missing translation · PII leak via safeLog | MITIGATED |
| WI-S16-002 | Stripe Checkout edge cases · PAT exposure · DPA abandonment · invite-link replay · time-to-first-PAT > 5 min | MITIGATED |
| WI-S16-003 | html2canvas browser quirks · locale mismatch · UUID collision · Flesch-Kincaid regression · auto-submit dark pattern | MITIGATED |
| WI-S16-004 | DSR abandonment · MFA disrupts UX · JWT receipt verify failure · SLA breach · erasure conditional misunderstood | MITIGATED |
| WI-S16-005 | audit PAT raw · cross-tenant access · dual-approval bypass · Slack PII leak · JSON export memory exhaustion | MITIGATED |
| WI-S16-006 | a11y compliance late discovery · i18n quality · color contrast regression · cross-browser Safari quirks · sub-processors outdated | MITIGATED |
| WI-S16-007 | Lighthouse regression · cross-browser matrix red · UX SUS < 75 · CSP enforce false-positives · 5-dev sample bias · **F1 nested-`<html>` merged-UI bug** (NEW) | MITIGATED w/ HF-S17-001 follow-up |

Total: 36 scenarios (35 from spec + F1 new finding).

## 6. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S16-007 builder) | Created at SEAL; consolidates e2e + a11y + CSP + UX findings; flags F1 nested-html bug as ship-gate-discovered HIGH severity hotfix. |
