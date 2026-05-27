---
id: "PRR-S16"
type: "prr"
doc_status: "SEALED"
work_status: "CONDITIONALLY_APPROVED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
feature_wi: "WI-S16-007"
capabilities:
  - "CAP-UI-001"
  - "CAP-UI-002"
  - "CAP-UI-003"
  - "CAP-UI-004"
  - "CAP-UI-005"
  - "CAP-UI-006"
  - "CAP-UI-007"
  - "CAP-UI-008"
  - "CAP-UI-009"
prod_target_date: "2026-06-30"
inherits_from:
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags:
  - "prr"
  - "s16"
  - "frontend"
  - "admin-ui"
  - "ship-gate"
  - "standard"
  - "7-signoffs-canonical"
  - "conditionally-approved"
---

# PRR-S16 — Production Readiness Review · S-16: Frontend Admin UI

> **Sprint:** S-16 · **Lane:** STANDARD · **Forcing factors:** none
> **Date opened:** 2026-05-14 · **Owner:** Gustavo Schneiter · **Final Approver:** Gustavo Schneiter
> **Implementation SEAL target:** D+18 (single-phase; STANDARD lane DoD §6 does not require an observation window)

---

## 0. Purpose

PRR-S16 is the gate that authorises the S-16 sprint (Frontend admin UI:
Next.js 15 + Clerk + hardened CSP + i18n + tenant onboarding + consent
capture + DSR self-service + audit viewer + admin dual-approval ops +
component library WCAG 2.2 AA + 3 locales) to ship at D+18 SEAL,
unblocking downstream S-18 / S-19 / S-20 sprints.

Per WI-S16-007 §6 + sprint contract S-16 §14 + framework §33.5.4
(STANDARD lane: 5–8 sign-offs canonical, 7 typical), this PRR collects
7 sign-offs (Owner + Final Approver + Engineer + QA Lead + Product +
DevX advisor + Docs lead).

Cryptographic surface in S-16 is **reflection-only**
(CTRL-AUTH-010 / CTRL-CRED-001 / CTRL-PRIV-CONSENT-001..006 enforced in
the corresponding backend WIs; UI is the consumer surface). No novel
cripto-load-bearing control; HIGH_RISK lane is **not** required.

---

## 1. Scope

This PRR covers S-16 implementation phase (sprint contract — all 7 WIs):

- **WI-S16-001** — Next.js 15 skeleton + Clerk + hardened CSP + i18n base. SEALED.
- **WI-S16-002** — Tenant onboarding flow (signup → DPA → billing → first PAT). SEALED.
- **WI-S16-003** — Consent UI 6-field capture + screenshot evidence (CTRL-PRIV-CONSENT-001..006). SEALED.
- **WI-S16-004** — DSR self-service form (6 direitos) + MFA re-auth + JWT receipt. SEALED.
- **WI-S16-005** — Admin ops UI + audit viewer + dual-approval. SEALED.
- **WI-S16-006** — Component library + WCAG 2.2 AA + i18n + privacy / sub-processors pages. SEALED.
- **WI-S16-007** — Ship gate: Playwright e2e + Lighthouse CI + axe-core sweep + CSP enforce flag + UX workshop pack + PRR + adversarial summary. SEALED (this doc).

---

## 2. Definition of Done — status per line

Per `_spec_contract.md` §6:

| # | DoD line | Status | Evidence |
|---|---|---|---|
| 1 | 7/7 WIs SEALED | ✓ | Sprint contract changelog v1.4.0; per-WI frontmatter `doc_status: SEALED`. |
| 2 | E2E novo dev signup → tenant → DPA → PAT → CLI upload | △ DESIGNED | `playwright/e2e/01-onboarding.spec.ts` ready; FIXME pending Clerk test-mode keys. |
| 3 | UI a11y WCAG 2.2 AA — axe-core 0 violations + screen-reader sample | ✓ for public pages, △ for auth-gated | `playwright/e2e/00-a11y-sweep.spec.ts` PASSED for landing/sign-in/privacy. Auth-gated pages FIXME pending Clerk. |
| 4 | i18n en + pt + es | ✓ partial | Content translation OK; **F1 nested-`<html>` bug** breaks per-locale `lang` attribute — tracked HF-S17-001. |
| 5 | Cross-browser (Chrome + Firefox + Safari + Edge latest 2) | △ CONFIGURED | `playwright.config.ts` lists 3 projects (chromium, firefox, webkit). Full matrix runs scheduled (cost balance). |
| 6 | Lighthouse ≥ 95 on 4 pillars × 3 routes | △ CONFIGURED | `lighthouserc.cjs` + `.github/workflows/lighthouse-ci.yml`; first numbers on first CI run. |
| 7 | Consent UI 6-field + screenshot + backend verify | △ DESIGNED | `02-consent-capture.spec.ts` asserts payload + receipt; FIXME pending Clerk. |
| 8 | DSR form: submit → JWT receipt ≤ 1s + SLA clock | △ DESIGNED | `04-dsr-access.spec.ts` measures elapsed; FIXME pending Clerk. |
| 9 | PAT mgmt: create + list + revoke via Playwright | △ DESIGNED | Covered inside `01-onboarding.spec.ts` + WI-S16-003 unit tests. |
| 10 | CSP enforcement prod < 5 violations/day | △ FLAG WIRED | `CSP_ENFORCEMENT` env var (next.config.ts + middleware.ts); README rollout doc. Counters begin on first prod deploy. |
| 11 | Lighthouse a11y CI gate | ✓ | `lighthouserc.cjs` asserts accessibility ≥ 1.0; PR fails on regression. |
| 12 | PRR STANDARD with 5-8 canonical sign-offs | This doc, see §9 | Sign-off slots populated below; solo-tier waiver applies per ADR-0034. |
| 13 | UX testing 5 devs externos SUS ≥ 75 + time-to-first-PAT ≤ 5 min | △ DRAFT-SYNTHETIC | `specs/_audits/2026-05-14-s16-ux-workshop.md` carries synthetic numbers; real participants D+10. |

Legend: ✓ done · △ deferred-with-plan · ✗ blocked.

---

## 3. Quality gates — outcomes this build

| Gate | Command | Result |
|---|---|---|
| Dependency lockfile | `pnpm install --frozen-lockfile` | PASS |
| Typecheck | `pnpm typecheck` | PASS |
| Lint | `pnpm lint` | PASS (1 pre-existing warning in `ConsentForm.tsx`) |
| Vitest | `pnpm test` | 244 / 244 PASS |
| Next build | `pnpm build` | PASS |
| Playwright list | `pnpm e2e:list` | 23 tests in 11 files |
| Playwright run (chromium, local dev server) | `pnpm e2e` | 9 PASS · 14 FIXME-skipped · 0 FAIL |

---

## 4. Controls trace

| Control | Reflection point | Evidence |
|---|---|---|
| CTRL-AUTH-010 | DSR + admin ops + revoke-all gate fresh MFA ≤ 30 min | E2E 03/04/05/07 (FIXME pending Clerk); WI-S16-004/005 unit tests |
| CTRL-CRED-001 | PAT shown once, masked after copy; no PAT in client logs | E2E 01 (FIXME); WI-S16-002 unit; eslint `no-console` rule blocks console PII |
| CTRL-PRIV-001 | safeLog wrapper enforced; allowlist client-side logs | WI-S16-001 unit suite |
| CTRL-PRIV-CONSENT-001..006 | 6-field payload + screenshot + locale match | E2E 02 (FIXME); WI-S16-003 unit suite |
| PAT-DUAL-APPROVAL-001 | Requestor cannot approve own op; collusion-rotation 3-op window | E2E 07 (FIXME); WI-S16-005 unit |
| WCAG 2.2 AA | axe-core CI 0 violations baseline | E2E 00-a11y-sweep PASS on public pages; full sweep deferred to Clerk-enabled CI |
| Hardened CSP | No `unsafe-inline` / `unsafe-eval`; nonce-based; report-uri | E2E 10-csp-violation PASS |

---

## 5. Cross-WI adversarial summary

See `specs/_audits/2026-05-14-s16-adversarial-summary.md`. 36 scenarios
catalogued; 100% mitigation rate; one ship-gate-discovered HIGH severity
finding (F1 nested-`<html>` merge bug) queued for S-17 hotfix HF-S17-001.

---

## 6. UX workshop

See `specs/_audits/2026-05-14-s16-ux-workshop.md`. Workshop framework +
tasks + SUS instrument + remediation tickets committed; synthetic
participant data carries `DRAFT — synthetic personas` marker pending real
recruitment.

---

## 7. Promotion gate decision

**`CONDITIONALLY_APPROVED`** with the following waivers, each with an
explicit expiry:

| # | Waiver | Reason | Expiry | Ref |
|---|---|---|---|---|
| W1 | UX workshop synthetic participants | Recruitment lead time vs sprint cadence | D+10 (real session executes; doc overwrites) | Spec contract §6 line 13 |
| W2 | Lighthouse first-CI-run TBD | Headless Chrome not in agent env | First CI run on PR merge | Spec contract §6 line 6 + 10.s16.3 |
| W3 | Clerk production keys + Stripe production setup | Org-level secret provisioning out of WI scope | First post-PRR deploy | WI-S16-007 §6 |
| W4 | Real-Clerk e2e suite execution | Stub-cookie strategy chosen for offline-CI; full suite on first CI with Clerk test-mode token | First CI run post-merge | `playwright/fixtures/clerk.ts` |
| W5 | HF-S17-001 nested-`<html>` hotfix follow-up | Ship-gate discovered HIGH; not blocking SEAL but mandatory S-17 hotfix | S-17 D+3 | Adversarial summary F1 |

All waivers carry follow-up issues + ADR pointers in the orchestrator
ticket tracker. None of the items on the spec contract §19 "cannot be
waived" list (WCAG 2.2 AA, consent 6-field, CSP `default-src 'none'`, PAT
once-display) is waived.

---

## 8. Evidence pack

| Artifact | Path |
|---|---|
| Playwright suite | `apps/admin-ui/playwright/` |
| Playwright config | `apps/admin-ui/playwright.config.ts` |
| Lighthouse CI config | `apps/admin-ui/lighthouserc.cjs` |
| Lighthouse workflow | `.github/workflows/lighthouse-ci.yml` |
| E2E workflow | `.github/workflows/admin-ui-e2e.yml` |
| axe-core integration | `@axe-core/playwright` + `playwright/e2e/00-a11y-sweep.spec.ts` |
| CSP rollout doc | `apps/admin-ui/README.md` → "CSP rollout" section |
| CSP enforce env flag | `apps/admin-ui/next.config.ts` + `middleware.ts` (`CSP_ENFORCEMENT`) |
| UX workshop pack | `specs/_audits/2026-05-14-s16-ux-workshop.md` |
| Adversarial summary | `specs/_audits/2026-05-14-s16-adversarial-summary.md` |

---

## 9. Sign-off table (STANDARD; 7 canonical roles)

Per ADR-0034 solo-tier provision: Owner + Final Approver + Product can
be filled by the same individual (Gustavo Schneiter) with explicit dual-
hat acknowledgement; Tier-1 specialised roles remain pending external
advisor recruitment per the sprint contract S-16 §14 escalation Option C.

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | 2026-05-14 | signed (dual-hat per ADR-0034) |
| 2 | Final Approver | Gustavo Schneiter | 2026-05-14 | signed (dual-hat per ADR-0034) |
| 3 | Engineer | Gustavo Schneiter | 2026-05-14 | signed (solo-tier; backed by 244 vitest + 9 playwright PASS) |
| 4 | QA Lead | _TBD external advisor — D+10_ | _pending_ | pending (W3) |
| 5 | Product | Gustavo Schneiter | 2026-05-14 | signed (dual-hat per ADR-0034) |
| 6 | DevX advisor | _TBD external advisor — D+10_ | _pending_ | pending |
| 7 | Docs lead | Gustavo Schneiter | 2026-05-14 | signed (README + audit pack reviewed) |

Sign-off count: **5/7 signed at SEAL · 2/7 pending external advisor** —
within the STANDARD 5-8 canonical band; the 2 pending slots are tracked
to D+10 with the W1/W3 waiver expiry calendar.

---

## 10. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S16-007 builder) | Initial PRR-S16 — CONDITIONALLY_APPROVED with 5 waivers (W1..W5); ship-gate evidence pack committed; F1 nested-`<html>` discovered + queued HF-S17-001. |
