---
id: "AUDIT-S16-SPRINT-CLOSE-R1"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Review (Sonnet)"
tags: ["audit", "sprint-close", "s16", "adversarial"]
---

# Adversarial Sprint-Close Review — S-16 Round 1

Working tree: `corelink-server` @ main `f6b0546`. Scope: WI-S16-001..006 (WI-S16-007 explicitly excluded).
All gates were executed in `apps/admin-ui`.

## 1. Score

**7.4 / 10**

The implementation ships a remarkably complete, well-tested admin UI: 244 tests green across 59 files, typecheck clean, build clean, 27 jest-axe assertion files, hardened CSP with per-request nonce, PII-redacted logging, dual-approval + RbacGuard on every `/admin/*` route, ReAuthGate enforcing per-submit MFA on DSR + withdraw, 3 locales fully populated, screenshot evidence wired via dynamic-import `html2canvas`, server actions for token-handling boundaries. The architecture is genuinely good and the security baseline is materially better than typical SaaS dashboards. The deduction (-2.6) reflects three concrete, fixable gaps that block SEAL: (a) **every WI frontmatter still says `doc_status: "DRAFT"` / `work_status: "READY"`** — the work-status promotion to SEALED/DONE that the sprint-close brief asserts has not actually been applied to any of WI-S16-001..006; (b) the **spec contract (`_spec_contract.md`) is still `doc_status: "DRAFT"` and the WI table reflects the OLD mapping** (e.g. WI-S16-003 = "Usage dashboard + Grafana proxy + PAT management") even though code has shipped a totally different WI-S16-003 = consent UI — with no changelog row reconciling the swap; (c) a **duplicate `next.config.mjs` alongside the canonical `next.config.ts`** that strips the entire CSP `headers()` override and the next-intl plugin — Next.js 15 prefers `.ts` so currently harmless, but a stray config that nukes the hardened security headers is a foot-gun that must not survive SEAL.

## 2. P0 (must-fix before SEAL)

1. **WI frontmatter promotion not applied.** All six WIs ship with `doc_status: "DRAFT"`, `work_status: "READY"` — they are not SEALED on disk. The brief asserts they are merged + SEALED; the spec store contradicts that.
   - `specs/04_sprints/_sealed/S16/work_items/WI-S16-001-nextjs-skeleton-clerk-csp-i18n-base.md:3-4`
   - `specs/04_sprints/_sealed/S16/work_items/WI-S16-002-tenant-onboarding-flow-self-service-first-pat.md:3-4`
   - `specs/04_sprints/_sealed/S16/work_items/WI-S16-003-consent-ui-6-field-screenshot-evidence.md:3-4`
   - `specs/04_sprints/_sealed/S16/work_items/WI-S16-004-dsr-self-service-form-6-direitos-mfa-jwt-receipt.md:3-4`
   - `specs/04_sprints/_sealed/S16/work_items/WI-S16-005-admin-ops-ui-audit-viewer-dual-approval.md:3-4`
   - `specs/04_sprints/_sealed/S16/work_items/WI-S16-006-component-library-a11y-wcag-2-2-aa-i18n-privacy-pages.md:3-4`

2. **Spec contract still DRAFT and reflects the wrong WI mapping.** The contract was never reconciled with the actual WI re-mapping (Lote 10.16 deferred dashboard/Grafana, repurposed 003 to consent). The WI table in the contract still describes the pre-pivot scope. There is no changelog section in the contract recording the SEAL events for WI-S16-001..006.
   - `specs/04_sprints/_sealed/S16/_spec_contract.md:3` — `doc_status: "DRAFT"`
   - `specs/04_sprints/_sealed/S16/_spec_contract.md:210-216` — WI table shows old mapping (003 = "Usage dashboard + Grafana proxy + PAT management"; code ships consent UI under that ID)
   - No `## Changelog` / `## Spec Changes` section anywhere in the 313-line file.

3. **Duplicate `next.config.mjs` removes hardened headers + next-intl plugin.** `next.config.mjs` exists alongside `next.config.ts`. The `.mjs` file is a minimal `nextConfig = { reactStrictMode: true }` — it strips the static-CSP `headers()` override and the `withNextIntl` wrapper. Next.js 15 picks `.ts` first so the live build is unaffected today, but a future tooling change (or downstream `next-on-pages`) that reads `.mjs` would silently disable CSP fallback and break i18n routing.
   - `apps/admin-ui/next.config.mjs:1-9`
   - Canonical config: `apps/admin-ui/next.config.ts:13-30`

## 3. P1 (should-fix before SEAL)

1. **`ConsentDashboard` uses bare `<a href="/consent/...">` for internal nav** instead of `next/link`, and the hrefs omit the `[locale]` segment — clicking either link from `/pt/consent` will throw the user back to `/consent/<id>` (no locale), causing a hard reload + locale loss.
   - `apps/admin-ui/src/components/consent/ConsentDashboard.tsx:61` (`<a href={\`/consent/${r.id}\`}`)
   - `apps/admin-ui/src/components/consent/ConsentDashboard.tsx:63` (`<a href={\`/consent/withdraw/${r.id}\`}`)
   - Same file lines 73-74 correctly use `<Link href="/consent/new">` but also omit locale — inconsistent.

2. **Lint warning on production code.** `next lint` emits one `jsx-a11y/role-supports-aria-props` warning that survives every build. WCAG-AA sprint should not ship with any a11y-lint warning unsuppressed.
   - `apps/admin-ui/src/components/consent/ConsentForm.tsx:160` — `aria-readonly="true"` on a `<ul>` (role=list implicit; aria-readonly invalid here).

3. **No `/admin` index route.** `src/app/[locale]/admin/` contains only `audit/`, `ops/`, `tenants/` — there is no `page.tsx` for `/admin`. Hitting `/en/admin` directly returns a 404 with no breadcrumb back. Either add an admin landing page (preferred) or document the deliberate omission in WI-S16-005.

4. **`ConsentForm` accessibility nit on `<ul>` listed third-parties.** Same line as P1.2; per `jsx-a11y` the `aria-readonly` is meaningless. Either drop the attribute or change the element to something where `aria-readonly` is valid (e.g. a `<fieldset>` of disabled checkboxes).
   - `apps/admin-ui/src/components/consent/ConsentForm.tsx:160-164`

5. **Two parallel translation file sets.** `src/i18n/locales/{en,pt,es}.json` (used by `next-intl/request.ts`) and `src/i18n/messages-{en,pt,es}.json` (used by `messages.ts::t()`) both exist (each 117 / 125 lines respectively). Diverging keys will silently drift between flows. Pick one source of truth and consolidate before SEAL.
   - `apps/admin-ui/src/i18n/locales/en.json` and `apps/admin-ui/src/i18n/messages-en.json` (and pt/es siblings).

6. **`Drawer` raises React 19 dialog accessibility warnings in test output.** Three "Missing `Description` or `aria-describedby={undefined}` for {DialogContent}" warnings during `Drawer.test.tsx` — tests pass but the underlying Radix Dialog wrapper is not threading a description. Real assistive tech will surface this.
   - `apps/admin-ui/src/components/ui/Drawer.tsx` (Drawer test stderr in `pnpm test`)

7. **`safe-log.ts` emits via `console.log/info/warn/error` directly** (line 59). The sanitization is correct, but a future debug log added in any other module that bypasses `safeLog` won't be scrubbed. Consider eslint rule `no-console` (with `safeLog` allow-list) to prevent regression.
   - `apps/admin-ui/src/lib/safe-log.ts:59`

## 4. P2 (post-SEAL nice-to-have)

- Bundle size for `/onboarding/pat` is 5.24 kB route / 106 kB First Load (reasonable but `html2canvas` dynamic import means consent capture still inflates that flow ~80 kB on demand). Worth tracking after WI-S16-007 Lighthouse gate.
- `RbacGuard` checks for the literal string `"corelink-admin"`; consider an enum import to avoid stringly-typed bugs (`src/components/admin/RbacGuard.tsx:37,42`).
- `OpDetailView.tsx:79-80` hardcodes English strings ("Fresh MFA step-up required before signing. Approvals are disabled until you re-authenticate.") — should be moved to the i18n table for full 3-locale parity in admin surfaces.
- Five build pages flagged "Using edge runtime on a page currently disables static generation for that page" — acceptable but worth deliberate documentation.

## 5. Positives

- **CSP hardening is best-in-class.** `src/lib/csp.ts` rejects unsafe-inline, unsafe-eval, AND unsafe-hashes; per-request nonce generated via Web Crypto; report-only in dev, enforce in prod; helper `containsUnsafeDirective` exposed for test assertions.
- **PAT plaintext memory-only contract is genuinely defended.** `src/lib/onboarding-state.ts:6-10` documents the rule, and the `saveState` helper actively refuses to persist any value matching `/corelink_(prod|test)_/` — bug-for-bug guard.
- **ReAuthGate is wired on every DSR action AND on consent withdrawal.** `DsrActionPageClient.tsx:135-160` wraps the form, and `WithdrawForm.tsx:47-52` blocks submit until `mfaVerified`. Per-submit, not session-wide — matches CTRL-AUTH-010.
- **RbacGuard wraps 100% of `/admin/*` routes** (audit, audit/[event_id], ops, ops/[op_id], tenants, tenants/[tenant_id]) — verified by grep.
- **Operation-reason header is enforced in code path, not just doc.** `src/lib/admin-client.ts:79` throws if a mutating call is attempted without `X-Admin-Operation-Reason`.
- **6 consent fields modeled in types, validated in form, and screenshot evidence captured.** `consent-types.ts:24-37` enumerates CTRL-PRIV-CONSENT-001..006; `ConsentCaptureFlow.tsx:76-82` enforces the locale-match guard before submit.
- **244 tests / 59 files / 27 jest-axe files.** Component library coverage is excellent — every Radix primitive ships with both behavior and `toHaveNoViolations` tests.

## 6. Per-WI Sub-Scores

- **WI-S16-001 (Next.js skeleton + Clerk + CSP + i18n base):** 8.0/10 — CSP hardening + nonce middleware exemplary. -1.5 for the duplicate `next.config.mjs` foot-gun; -0.5 for two parallel locale-file conventions.
- **WI-S16-002 (Onboarding wizard + first PAT):** 8.5/10 — server-action token boundary, memory-only PAT, persistence guard, modal confirm-saved flow. Clean.
- **WI-S16-003 (Consent UI):** 7.5/10 — 6-field model + screenshot + JWT receipt + locale-match guard. -1.5 for bare `<a>` (P1.1) + lint warning (P1.2) + scope reassignment never reconciled in contract (P0.2).
- **WI-S16-004 (DSR self-service):** 8.5/10 — `<ReAuthGate>` per submit, six DSR action route, JWT receipt modal, SLA countdown, PII redaction. Solid.
- **WI-S16-005 (Admin ops UI):** 8.0/10 — `RbacGuard` + dual-approval card + operation-reason header + Merkle proof viewer. -1.5 for missing `/admin` index page + hardcoded English in `OpDetailView`.
- **WI-S16-006 (Component library + WCAG 2.2 AA + privacy/legal):** 8.0/10 — 13 UI primitives, Radix-based, ≥24px target sizes, focus-visible rings, jest-axe coverage, 3-locale legal markdown. -1.5 for Drawer description warning + the one persistent `jsx-a11y` warning.

---

**Gate evidence:**

- `pnpm install --frozen-lockfile` → exit 0.
- `pnpm typecheck` → exit 0 (zero TS errors).
- `pnpm lint` → exit 0 with **1 warning** at `ConsentForm.tsx:160` (P1.2 / P1.4).
- `pnpm test` → **244/244 passed across 59 files** (62.27s).
- `pnpm build` → exit 0 (`Compiled successfully`); 34 routes built; `/onboarding/pat` = 5.24 kB / 106 kB First Load.
- `python3 scripts/validate_specs.py` → 14 failures, **0 matching S16/S-16** (all are S11/S12/S13 ADRs unrelated to this sprint).

**SEAL recommendation:** **DO NOT SEAL** until P0.1, P0.2, P0.3 are addressed. P1 items can be deferred to WI-S16-007 close window with an explicit punchlist.
