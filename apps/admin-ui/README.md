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
| `script-src` | `'self' 'nonce-…' https://clerk.corelink.humangr.com` |
| `style-src` | `'self' 'nonce-…'` |
| `img-src` | `'self' data: https:` |
| `connect-src` | `'self' https://api.corelink.humangr.com https://clerk.corelink.humangr.com` |
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

## Deploy — Cloudflare Pages

### Architecture

`apps/admin-ui` deploys to the CF Pages project **`corelink-admin-ui`** (humangr-labs org)
via `@cloudflare/next-on-pages`. The GH Action `.github/workflows/admin-ui-deploy.yml`
builds and deploys automatically on every push to `main` that touches `apps/admin-ui/**`.

Routing precedence note: CF Worker routes take precedence over Pages custom domains on
the same CF zone. The `corelink-admin.humangr.com/*` route was removed from the root
`wrangler.toml` (Stream 3.11) so the Pages CNAME can resolve. Do NOT re-add a Worker
route for `corelink-admin.humangr.com` — it would shadow the Pages deployment.

### Build commands

```bash
# from inside apps/admin-ui
pnpm pages:build    # next build && npx @cloudflare/next-on-pages
                    # emits .vercel/output/static

pnpm pages:deploy   # wrangler pages deploy .vercel/output/static \
                    #   --project-name corelink-admin-ui
```

### One-time operator setup (manual steps)

#### 1. Create the CF Pages project

```bash
# Only needed once; the GH Action deploy step creates it if it doesn't exist.
wrangler pages project create corelink-admin-ui --production-branch main
```

#### 2. Set runtime secrets (server-side; never committed to git)

```bash
# Run once per secret; CF Pages runtime injects these into the Pages Functions.
wrangler pages secret put CLERK_SECRET_KEY        --project-name corelink-admin-ui
wrangler pages secret put STRIPE_SECRET_KEY       --project-name corelink-admin-ui
wrangler pages secret put SENTRY_AUTH_TOKEN       --project-name corelink-admin-ui  # optional
```

#### 3. Set build-time env vars (NEXT_PUBLIC_* — inlined into client bundle)

In the CF Pages dashboard (or GH Actions Settings → Variables):

| Variable | Value |
|---|---|
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | `pk_live_…` (from Clerk dashboard) |
| `NEXT_PUBLIC_CORELINK_API_URL` | `https://corelink-api.humangr.com` |
| `NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY` | `pk_live_…` (from Stripe dashboard) |
| `NEXT_PUBLIC_SENTRY_DSN` | `https://…@sentry.io/…` |

These are set as GH Repo Variables (`vars.*`) so the GH Action can inject them at build time.

#### 4. Wire the custom domain (MANUAL — CF Pages dashboard)

1. In the [CF Pages dashboard](https://dash.cloudflare.com) → Workers & Pages → `corelink-admin-ui`
2. Go to **Custom domains** → **Set up a custom domain**
3. Enter `corelink-admin.humangr.com` → Continue
4. CF will verify DNS; accept the suggested CNAME or create it manually:
   ```
   corelink-admin.humangr.com  CNAME  corelink-admin-ui.pages.dev  (proxied)
   ```
5. Wait for the "Active" badge (usually < 5 min; CF provisions a TLS cert automatically).

Prerequisite: the Worker route `corelink-admin.humangr.com/*` must be absent from the
main worker's wrangler.toml (done in Stream 3.11). If the route exists, CF Edge invokes
the Worker first and the Pages deployment is never reached.

#### 5. Update Clerk allowed redirect URLs (MANUAL — Clerk dashboard)

In [Clerk dashboard](https://dashboard.clerk.com) → Your application → **Domains**:

- Add `https://corelink-admin.humangr.com` as an **Allowed redirect origin**
- Ensure `https://corelink-admin.humangr.com/sign-in` and
  `https://corelink-admin.humangr.com/sign-up` are listed under redirect URLs.

Without this step, Clerk will block sign-in redirects from the custom domain.

#### 6. Smoke test

```bash
# After custom domain is active:
curl -I https://corelink-admin.humangr.com/api/health
# Expected: 200 OK with X-Content-Type-Options: nosniff

# pages.dev must 404 (E1 BLOCK middleware gate):
curl -I https://corelink-admin-ui.pages.dev/
# Expected: 404 Not Found
```

### Security

`functions/_middleware.ts` runs on every CF Pages Function request and **blocks** all
`*.pages.dev` traffic with `404 Not Found` before any downstream handler runs. This
prevents secret-exposure via the raw Pages subdomain (E1 security gate,
`specs/_audits/2026-05-28-security-exposure-review.md §E1`).

Production traffic must arrive exclusively via `corelink-admin.humangr.com`.
