# Decision record — production host scheme: **flat is canonical** (dotted is dead)

**Date:** 2026-06-09 · **Status:** Accepted (techlead decision) · **Owner-flags below**

## Context

A partial "flat-rename" (Wave 32 Phase H APPLY) migrated *some* configs to the
flat `corelink-<surface>.humangr.com` scheme (e.g. `scripts/smoke-prod-corelink.sh`)
but left **stale dotted `*.corelink.humangr.com` references** scattered across
other live-operational configs. Live evidence (curl, 2026-06-09):

| surface | dotted `<x>.corelink.humangr.com` | flat `corelink-<x>.humangr.com` |
|---|---|---|
| api    | ☠️ NXDOMAIN | ✅ live (`/health` 200) |
| app    | ☠️ NXDOMAIN | ✅ live (200) |
| docs   | ☠️ NXDOMAIN | ✅ live (200) |
| signup | ☠️ NXDOMAIN | ✅ live (resolves) |
| admin  | ☠️ NXDOMAIN | ✅ live (200) |

The dotted scheme does **not resolve**. The two scheduled money-path smokes
(`e2e-stripe-checkout`, `e2e-clerk-signup`) are green against the **flat** hosts;
`e2e-prod` was red since 2026-06-04 **solely** because it targeted the dead dotted
hosts (`ENOTFOUND docs.corelink.humangr.com`).

## Decision

**The flat scheme is the canonical production host scheme.** The live surface is
exactly 5 hosts (per `smoke-prod-corelink.sh` `DNS_PLAN`):

| host | target |
|---|---|
| `corelink-api.humangr.com`    | worker (`corelink-prod.…workers.dev`) |
| `corelink-signup.humangr.com` | worker |
| `humangr.com`  | worker |
| `corelink-app.humangr.com`    | pages (`corelink-admin-ui.pages.dev`) — the customer console UI |
| `corelink-docs.humangr.com`   | pages (`corelink-docs.pages.dev`) |

## Scope of the corrective fix (live-operational surface ONLY)

Fixed (PR `fix/live-host-scheme-flat-2026-06-09`):
- `.github/workflows/e2e-prod.yml` — `E2E_BASE_URL`/`E2E_DOCS_URL` defaults → flat.
- `apps/docs/functions/_middleware.ts` — `CANONICAL_HOST` (the live `*.pages.dev`
  301-redirect target was a **dead host**) → flat.
- `apps/docs/docusaurus.config.ts` — `SITE_URL` (canonical/sitemap) + "Admin" nav
  link → flat.
- `apps/admin-ui/.../customer/audit/visualization/page.tsx` — customer-facing docs
  link → flat.
- Test-fixture consistency: `corelink-stripe-real/src/portal.rs`,
  `apps/admin-ui/tests/middleware-pages.test.ts`.
- (follow-up) the docs unit tests + `static/CNAME` + `static/robots.txt` + several
  `src/pages/*` were also brought to the flat host.

## ⚠️ `corelink.dev` is a THIRD-PARTY domain — NOT ours

Discovered 2026-06-09: `corelink.dev` belongs to an **unrelated company** ("CoreLink
Development", an AI web/mobile dev agency in Missouri — its apex serves their marketing
site; the `*.corelink.dev` subdomains do not resolve). Several files aspirationally used
it. The docs `CNAME` / `robots.txt` / a unit test were fixed in PR #183; the
`static/.well-known/{security.txt,dnt-policy.txt}` (which misdirected **security +
privacy disclosure contacts** to `@corelink.dev` — a stranger) were moved off it to
`corelinksec@humangr.com` + the live `corelink-docs.humangr.com`. **Our product domain
is `humangr.com`.** Also dead (not just dotted): `corelink.humangr.com` (apex) and
`corelink.io` / `get.corelink.io` — only the flat `corelink-<surface>.humangr.com`
hosts are live.

## Explicitly NOT touched (correct as-is / out of scope)

- **Sealed audit records** (`specs/_audits/sealed/…`), sealed sprints — immutable history.
- **WebAuthn test vectors** (`crates/corelink-auth/tests/webauthn_*`) — `evil.corelink.humangr.com`
  is an **intentional adversarial origin**; rewriting breaks the test's meaning.
- `CHANGELOG.md`, `RELEASE-NOTES-*`, `ROADMAP-TO-GA.md`, historical specs.
- Internal / staging / observability hosts (`grafana`, `telemetry`, `staging`,
  region routers `weur/enam/…`).
- **`status.corelink.humangr.com`** — **deliberate**: operator CNAMEs it to BetterStack
  at launch-day (override via `STATUSPAGE_URL`); see `docusaurus.config.ts` §statuspage.

## ⚠️ Owner-flags (cannot be resolved from code alone)

1. **CSP Clerk host (AUTH-critical).** `apps/admin-ui/src/lib/csp.ts` allows
   `https://clerk.corelink.humangr.com` in `script-src`/`connect-src`/`frame-src`,
   but that host **does not resolve**. Either (a) the Clerk custom-domain CNAME is a
   launch-day DNS step, or (b) the CSP must list Clerk's **default** Frontend-API
   domain. If neither is true at launch, **the sign-in widget breaks** (CSP blocks the
   real Clerk origin). **Decide before launch.**
2. **`get.corelink.io` install host** — does not resolve; `e2e-prod`'s install-script
   test will stay red until it is provisioned (or that test is gated). Separate `.io`
   domain, not part of the flat humangr.com scheme.
3. **`scripts/dns-prod-plan.sh` / `dns-prod-verify.sh`** still encode **dotted** CNAMEs
   (contradicting the live flat reality + the already-flat `smoke-prod`). Audited in the
   cutover dry-run (task #36) — do **not** blind-edit the DNS mechanics.
4. **Customer docs `.mdx`** (quickstart / migrate guides, ×4 locales) still reference
   dotted/aspirational endpoints (`api.corelink…`, `reapi…`, `cache…`). The real
   customer endpoint is `corelink-api.humangr.com` (REAPI/CAS are **routes** under it,
   not subdomains). Deferred to PR 2 (per-ref judgment needed).
5. **Billing portal is a STUB.** `InMemoryCustomerHandler::portal_url`
   (`corelink-handler-customer/src/handler.rs:490`) returns a fake
   `https://billing.stripe.com/p/session/stub_<tenant>` URL — the customer
   self-service billing portal is **not wired to the real Stripe portal**. Money-path
   completeness item, separate from hostnames.
