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

## Cloudflare Pages

`pnpm build:cf` runs `@cloudflare/next-on-pages` to emit the Pages
artifact. Deploy is wired in the orchestrator's Pages project.
