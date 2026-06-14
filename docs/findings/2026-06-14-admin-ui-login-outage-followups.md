# admin-ui prod login outage (2026-06-14) — root causes + follow-ups

Prod login at `corelink-app.humangr.com` was **completely down** ("Application
error: a client-side exception") for every visitor. Diagnosed end-to-end with a
headless Chromium load against the live deploy. Three compounding causes were
fixed (see PR #267 / the bundle branch + CHANGELOG); this doc captures the
**non-obvious lessons + remaining follow-ups** so the debt is tracked, not lost.

## Root causes (fixed)

1. **CF zone rate-limit "Wave 32" 429'd the SPA's own JS chunks.** 10 req/10s/IP
   across all `corelink-*` hosts; a Next.js page load fires ~20 chunk requests
   from one IP → CF error 1015 → `Loading chunk failed` → crash. Fixed on the
   zone: exclude static-asset paths + 50 req/10s/IP dynamic. **This rule is a
   zone-WAF object — invisible to `git`.** (zone `73f57f6d…`, ruleset
   `c13b37aa…`, rule `260d907f…`.)
2. **Missing `<ClerkProvider>`** on `/sign-in` + `/sign-up` (outside the
   `(authenticated)` group) → `useSession … <ClerkProvider/>` throw. Fixed with
   co-located `ssr:false` wrappers.
3. **CSP gaps** — `script-src` missing `challenges.cloudflare.com` (Turnstile),
   `connect-src` missing `corelink-analytics.humangr.com`. Both added.
4. **Cost root cause** behind the rate-limit rule: `/_next/static` served
   `max-age=0` (cf-cache MISS every load). Fixed with `public/_headers`
   (immutable). Once edge-cached, origin load + CF cost drop.

## Follow-ups (open)

### FU-1 — `e2e-clerk-signup` is FALSE-GREEN for the frontend  *(quality debt)*
The scheduled prod e2e (`scripts/e2e-clerk-signup.sh`, 5 stages) exercises the
**backend** signup orchestration only: Clerk Backend API → signup-worker → D1
tenant row → PAT → publicMetadata → `/v1/users/me`. **It never loads the actual
`/sign-in` or `/sign-up` page in a browser**, so it stayed `success` on every run
through 2026-06-14 while the page crashed client-side. An HTTP-200 check cannot
catch a client-side exception (the HTML 200s; the JS throws on hydration).

**Recommendation:** add a browser render-smoke (Playwright) to the prod e2e cron:
load `/sign-in` + `/sign-up`, **fail on any `pageerror`** and assert the Clerk
widget mounts (`.cl-rootBox` / an `input`). This exact probe is what surfaced all
three root causes above — it would have paged on this outage. Keep it on the same
PagerDuty SEV-1 path as the backend e2e. (Implementation note: the runner already
has `playwright` + a local `chrome-headless-shell`; in CI install via
`playwright install --with-deps chromium`.)

### FU-2 — `/en/welcome` returns HTTP 500 (post-signup landing)
The signup `forceRedirectUrl` is `/en/welcome`. Unauthenticated, it returns
**500** (should redirect to sign-in — it is a protected route, not public and not
self-gated per `route-matcher.ts`). Likely either (a) the contaminated build that
was live, or (b) `auth()` (server component, `@clerk/nextjs/server`) throwing
because `clerkMiddleware` context isn't established for that route on the
OpenNext edge worker. **Re-test after the clean deploy lands**; if still 500,
the fix is in the middleware's clerkMiddleware path for protected routes (the
dynamic `import("@clerk/nextjs/server")` + `auth.protect()` branch in
`middleware.ts`). Blocks the real signup flow even once the page renders.

### FU-3 — local build contamination
`corelink-server/node_modules/` has a stray flat npm install (1257 dirs, 14 Mai)
with wrong `@sentry/core@6.19.7` + older `@clerk/*` NOT in the lockfile → manual
`pnpm cf:build` bundles broken deps (the `_optionalChain not exported` Sentry
warning). **Deploy prod via CI** (`gh workflow run admin-ui-deploy.yml --ref
<branch>`) — fresh checkout = clean. Optional cleanup: remove the stray
non-lockfile flat dirs from the root `node_modules`.

### FU-4 — deploy job timeout vs shared-Mac load
`admin-ui-deploy.yml` has `timeout-minutes: 20`. The first deploy of this fix was
**cancelled mid-build** because it ran while a 12-job PR-check storm (from the
same push) hammered the self-hosted Mac. Sequence deploys away from CI bursts;
consider bumping the timeout or reducing the per-PR check fan-out on admin-ui.
