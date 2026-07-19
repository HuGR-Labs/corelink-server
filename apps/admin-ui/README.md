# `@corelink/admin-ui`

Next.js 15 admin UI for CoreLink — foundation skeleton per **WI-S16-001**.

This package is the entrypoint for tenant self-service: onboarding, consent
capture, DSR submission, PAT management, audit log viewer. Follow-on work
items (WI-S16-002..007) layer business UX on top of this skeleton.

## Status

- Foundation layer: **scaffolded** (Next.js 15 App Router, Clerk auth shim,
  hardened CSP, 3-locale i18n, CI workflow, vitest suite).
- Business UX (onboarding, consent, DSR, PAT CRUD, audit viewer): deferred
  to WI-S16-002..007.

## Quickstart

```bash
# from repo root
pnpm install

# admin-ui dev server (http://localhost:3000)
pnpm --filter @corelink/admin-ui dev

# from inside apps/admin-ui
pnpm dev
pnpm build
pnpm test
pnpm typecheck
pnpm lint
```

Copy `.env.example` to `.env.local` and populate the Clerk keys before
running auth-gated routes.

## CSP architecture

CoreLink admin UI enforces a hardened Content-Security-Policy with **no
`unsafe-inline` and no `unsafe-eval`**.

- `src/lib/csp.ts` exposes `buildCspHeaderValue(nonce)` and the static
  security headers (HSTS, X-Frame-Options, etc.).
- `middleware.ts` generates a cryptographically random per-request nonce
  via Web Crypto, forwards it on `x-nonce` request header (so the layout
  can pass it to `<Script>` children), and attaches the CSP header to
  every response.
- Header mode is gated by `CSP_ENFORCEMENT` (WI-S16-007 deliverable 4):
  - `CSP_ENFORCEMENT=report-only` → `Content-Security-Policy-Report-Only`
    (staging baseline; collect violations for ≥ 30d, refine allowlist).
  - `CSP_ENFORCEMENT=enforce` → `Content-Security-Policy` (production
    default once baseline converges < 5 violations/dia).
  - Unset → `enforce` when `NODE_ENV=production`, else `report-only`.
- Violations are POSTed to `/api/csp-report` (rate-limited 100/min/IP,
  forwarded to `${NEXT_PUBLIC_CORELINK_API_URL}/v1/csp-violations`).

### CSP rollout (S-16 → S-17)

1. **Stage 1 — Report-only baseline.** Production deploys with
   `CSP_ENFORCEMENT=report-only` for ≥ 1 week. Violations are aggregated;
   any directive triggering > 5/day is investigated.
2. **Stage 2 — Enforce.** Flip `CSP_ENFORCEMENT=enforce` (the production
   default) once the violation rate is sustained < 5/day. Roll back via
   the same env var if a false-positive surge appears.

The Playwright e2e suite (`playwright/e2e/10-csp-violation.spec.ts`)
asserts the header shape — `default-src`, no `unsafe-inline`, `nonce-`
present, `report-uri` set — regardless of the mode in effect.

### Allowlist baseline

| Directive | Allowlist |
|---|---|
| `default-src` | `'self'` |
| `script-src` | `'self' 'nonce-…' https://clerk.corelink-app.humangr.com` |
| `style-src` | `'self' 'nonce-…'` |
| `img-src` | `'self' data: https:` |
| `connect-src` | `'self' https://api.corelink.humangr.com https://clerk.corelink-app.humangr.com` |
| `frame-ancestors` | `'none'` |
| `form-action` | `'self'` |
| `base-uri` | `'self'` |
| `object-src` | `'none'` |

## i18n

Three locales ship at MVP: `en` (default), `pt`, `es`. Files live in
`src/i18n/locales/`. To add a new locale:

1. Create `src/i18n/locales/<code>.json` mirroring the `en.json` key set.
2. Add the code to the `LOCALES` tuple in `src/i18n/request.ts`.
3. Add it to the `i18n.test.ts` matrix so missing-key regressions fail CI.

Missing translation keys are caught at test time (`pnpm test`) and at
typecheck time via next-intl's strict mode.

## Privacy + auth invariants

- **CTRL-PRIV-001**: zero PII in client-side logs. Use `safeLog(level, msg,
  ctx)` from `src/lib/safe-log.ts`; only allowlisted keys survive.
- **CTRL-CRED-001**: PATs never appear in URL params, telemetry payloads,
  or `console.log`. The eslint rule `no-console` blocks raw console use.
- **CTRL-AUTH-010**: routes that mutate or read sensitive data require
  fresh MFA ≤ 30 min. Enforced by the Clerk middleware (full hookup in
  WI-S16-002).

## CI

`.github/workflows/admin-ui-ci.yml` runs typecheck, lint, vitest, and the
Next build. SHA-pinned actions per repo policy.

## Deploy — Cloudflare Worker

### Architecture

`apps/admin-ui` deploys to the Cloudflare **Worker** named **`corelink-admin-ui`**
(HumanGuardrail org) via `@opennextjs/cloudflare`. The GH Action
`.github/workflows/admin-ui-deploy.yml` builds and deploys automatically on every
push to `main` that touches `apps/admin-ui/**`.

> **Migrated Pages → Worker.** `@cloudflare/next-on-pages` 1.13.7 cannot render
> Next 15 SSR with `next-intl` + middleware + `ClerkProvider` — every SSR page route
> returned HTTP 500. The supported path is `@opennextjs/cloudflare` + `wrangler deploy`,
> configured in [`wrangler.toml`](./wrangler.toml) (`main = .open-next/worker.js`).

Routing note: the custom domain `humangr.com` is bound **directly to
this Worker** in `wrangler.toml`
(`routes = [{ pattern = "humangr.com", custom_domain = true }]`), so
`wrangler deploy` provisions it. The legacy `corelink-admin-ui` Pages project no longer
serves this hostname.

### Build commands

```bash
# from inside apps/admin-ui
pnpm cf:build    # opennextjs-cloudflare build (next build + OpenNext adapter)
                 # emits .open-next/worker.js + .open-next/assets

pnpm cf:deploy   # wrangler deploy  (reads name/main/routes from wrangler.toml)
                 # orchestrator-only — do NOT run locally
```

### One-time operator setup (manual steps)

#### 1. Set runtime secrets (server-side; never committed to git)

```bash
# Run once per secret; the Worker runtime injects these at request time.
# The first `wrangler deploy` (or the lines below) creates the Worker.
wrangler secret put CLERK_SECRET_KEY        # --name read from wrangler.toml
wrangler secret put STRIPE_SECRET_KEY
wrangler secret put SENTRY_AUTH_TOKEN       # optional
```

#### 2. Set build-time env vars (NEXT_PUBLIC_* — inlined into client bundle)

In the CF Worker dashboard (or GH Actions Settings → Variables):

| Variable | Value |
|---|---|
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | `pk_live_…` (from Clerk dashboard) |
| `NEXT_PUBLIC_CORELINK_API_URL` | `https://corelink-api.humangr.com` |
| `NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY` | `pk_live_…` (from Stripe dashboard) |
| `NEXT_PUBLIC_SENTRY_DSN` | `https://…@sentry.io/…` |

These are set as GH Repo Variables (`vars.*`) so the GH Action can inject them at build time.

#### 3. Custom domain (automatic via wrangler.toml)

The custom domain is declared in [`wrangler.toml`](./wrangler.toml) as a Worker
custom-domain route, so `wrangler deploy` (the GH Action deploy step) attaches it and
CF provisions the TLS cert automatically — no manual dashboard step:

```toml
routes = [
  { pattern = "humangr.com", custom_domain = true }
]
```

Prerequisite: the `humangr.com` zone must be on the same Cloudflare account
(`CF_ACCOUNT_ID`). The hostname was previously owned by the `corelink-admin-ui` Pages
project and detached on migration day, so it is free for the Worker to claim. Do NOT
add a `humangr.com/*` route to any *other* Worker (e.g. the root
`wrangler.toml`) — a second route on the same hostname would shadow this one.

#### 4. Update Clerk allowed redirect URLs (MANUAL — Clerk dashboard)

In [Clerk dashboard](https://dashboard.clerk.com) → Your application → **Domains**:

- Add `https://humangr.com` as an **Allowed redirect origin**
- Ensure `https://humangr.com/corelink/sign-in` and
  `https://humangr.com/corelink/sign-up` are listed under redirect URLs.

Without this step, Clerk will block sign-in redirects from the custom domain.

#### 5. Smoke test

```bash
# After the custom domain is active:
curl -I https://humangr.com/corelink/api/health
# Expected: 200 OK with X-Content-Type-Options: nosniff
```

### Security

Production traffic must arrive exclusively via `humangr.com`.

> **Open item (post Pages → Worker migration):** the default Worker subdomain
> (`*.workers.dev`) is a secret-exposure surface and should be either disabled for
> this Worker or blocked at the edge. The legacy `*.pages.dev` block lived in
> `functions/_middleware.ts` — a **CF Pages Functions** file that does **not** execute
> under the OpenNext Worker runtime. The root `middleware.ts` (CSP + Clerk only) does
> not host-block. Tracked separately from the deploy fix; see the E1 security gate
> (`specs/_audits/2026-05-28-security-exposure-review.md §E1`).
