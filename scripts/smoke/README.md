# Public-flip smoke harness

Two-layer smoke for the **public production surface** — the pages and APIs a
real customer hits on `corelink-app.humangr.com` and friends. Run it
**before and after every public-facing deploy** (admin-ui/OpenNext Worker,
main Worker, docs Pages, DNS/route changes) and as the **WP-5 acceptance
gate** for launch-cutover work.

## Why this exists (the azp incident, 2026-06-10)

A latent `401 clerk session azp invalid` (worker `M1-FIX-1` azp allowlist vs
tokens minted by a real browser Clerk session on the user-facing host
`corelink-app.humangr.com`) survived **every** prior smoke: the curl smokes
never carried a Clerk session, and the e2e suites ran against localhost or
the dead dotted hosts (`app.corelink.humangr.com` — see
`playwright.prod.config.ts` defaults), so no test ever exercised a
real-browser session token on the live public origin. This harness makes
that class of miss impossible: layer 1 pins every unauthenticated canonical
expectation per prod host, and layer 2 signs in through the real Clerk UI on
the real host and asserts **specifically** that no `401 azp invalid` appears
on the wire.

## Layer 1 — `public-flip-smoke.sh` (unauthenticated, no secrets)

```bash
bash scripts/smoke/public-flip-smoke.sh
```

Pure read-only curl GETs; **no env vars, no credentials** (CTRL-CRED-001).
Clear `[PASS]`/`[FAIL]` per probe; exit 0 only when all green.

| Host | Probe | Canonical expectation |
|---|---|---|
| corelink-app.humangr.com | `GET /` | 200 + security headers (XFO `DENY`, XCTO `nosniff`, HSTS, Referrer-Policy, Permissions-Policy, CSP w/ `frame-ancestors`) |
| corelink-app.humangr.com | `GET /upgrade?plan=solo` | 307 → `/en/upgrade?plan=solo`; redirect chain ends 200 (signed-out Clerk round-trip) |
| corelink-app.humangr.com | `GET /sign-up` | 200 |
| humangr.com | `GET /` | 200 + same security headers |
| corelink-docs.humangr.com | `GET /` | 200 |
| corelink-api.humangr.com | `GET /` | **404** fail-closed JSON (anything else = open unauthenticated surface or broken route binding) |
| corelink-api.humangr.com | `GET /health` | 200 + `"status":"ok"` |
| corelink-api.humangr.com | `GET /_health` | 200 |

## Layer 2 — `authenticated-smoke.spec.ts` (real Clerk session, Playwright)

Standalone — own `package.json`/configs here, **not** wired into
`apps/admin-ui`'s e2e configs. Runs from a clean checkout:

```bash
cd scripts/smoke
npm install
npx playwright install chromium
export SMOKE_USER_EMAIL=…       # never committed, never logged
export SMOKE_USER_PASSWORD=…
npm run smoke:authed
```

Coverage (serial, one real session, retries=0):

1. **Sign-in via the real Clerk UI** on `https://corelink-app.humangr.com/sign-in`
   (identifier → password → Continue), asserting a `__session` cookie lands
   on the app origin.
2. **Welcome/provisioning** — `/en/welcome` renders a welcome branch (PAT
   reveal or "already retrieved") and does NOT bounce to `/sign-up`; a
   session-wide network watcher asserts **no `401` with the
   `clerk session azp invalid` signature** appeared anywhere (the
   azp-incident regression assertion).
3. **Checkout** — `/upgrade?plan=solo` lands signed-in on
   `/en/upgrade?plan=solo`, the auto-fired `POST /api/checkout/session`
   returns 200 with a `https://checkout.stripe.com/…` `checkout_url` +
   `session_id`. **No payment ever occurs**: navigation to
   `checkout.stripe.com` is route-aborted, so the Stripe page never loads
   (run against the Stripe TEST-mode backend where possible regardless).
4. **Dashboard tabs** (`/en/customer/{usage,keys,billing,audit}` without
   5xx) — `test.skip` with `TODO(dashboard-wave)` until the dashboard wave
   ships; the 5xx collector is already wired.
5. **Zero 5xx** across the whole session, any host.

### Required env

| Var | Required | Meaning |
|---|---|---|
| `SMOKE_USER_EMAIL` | yes | Dedicated smoke-user email. Missing ⇒ hard error (never a silent green). |
| `SMOKE_USER_PASSWORD` | yes | Its password. Read from env only; never hardcoded, never logged/echoed. |
| `SMOKE_BASE_URL` | no | App origin override (default `https://corelink-app.humangr.com`). |

### Test-user contract

The smoke user must be a **dedicated, low-privilege** account (never a
personal/operator account): signed up through the real flow (so its tenant
is provisioned and the one-time PAT was already retrieved), **DPA
accepted** (otherwise the checkout POST 403s by design —
`INV-ONBOARD-DPA-FIRST`), password auth enabled in Clerk. Rotate its
password like any credential; it is intentionally allowlisted (not a
production-secrets-matrix row) in the secrets gates, same precedent as
`CORELINK_E2E_TOKEN`/`NEON_TEST_DSN`.

## When to run

- **Before flipping anything public** (DNS, Worker routes, Pages domains) —
  baseline must be green.
- **After every public-facing deploy** — admin-ui redeploy, main Worker
  deploy, docs deploy. Layer 1 always; layer 2 whenever auth, onboarding,
  billing, middleware, CSP, Clerk, or Worker route config changed.
- **WP-5 acceptance**: both layers green = accepted.
