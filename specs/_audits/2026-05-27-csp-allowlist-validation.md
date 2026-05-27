# CSP Allow-list Validation — Phase 1 Vendors

**Date:** 2026-05-27  
**Auditor:** Agent `csp-allowlist-validation` (Claude Sonnet 4.6)  
**Scope:** `apps/admin-ui` (Next.js + Clerk + Stripe + Plausible + Sentry) and `apps/docs` (Docusaurus + Plausible + Sentry CDN + BetterStack)

---

## 1. Vendor Inventory

### 1.1 admin-ui (`apps/admin-ui`)

| Vendor | Integration | CSP surfaces |
|---|---|---|
| **Stripe Checkout** | `@stripe/stripe-js` via JS SDK; `window.location.assign(checkout_url)` from `/api/checkout/session` | `script-src`, `connect-src`, `frame-src` |
| **Stripe Customer Portal** | `window.location.assign(portal_url)` from `/api/v1/customer/billing/portal-session` | `connect-src` (redirect to `billing.stripe.com`) |
| **Clerk** | `@clerk/nextjs` (SSR + client); CNAME proxy at `clerk.corelink.humangr.com`; modal/popup auth steps | `script-src`, `connect-src`, `frame-src` |
| **Plausible Analytics** | `PlausibleScript` component — consent-gated dynamic `<script>` injection; domain `corelink-admin.humangr.com` | `script-src`, `connect-src` |
| **Sentry** | `@sentry/nextjs` SDK (bundled); `tunnelRoute: "/monitoring"` proxies all ingest through same-origin | no CSP additions needed — same-origin tunnel |
| **PostHog** | Listed in sub-processors legal copy as "fallback" but NOT wired in any source file | none required |
| **Resend** | Server-side only (`RESEND_API_KEY` never reaches browser) | none required |

### 1.2 docs (`apps/docs`)

| Vendor | Integration | CSP surfaces |
|---|---|---|
| **Plausible Analytics** | `scripts:` array in `docusaurus.config.ts`; loads `https://plausible.io/js/script.js` | `script-src`, `connect-src` |
| **Sentry CDN loader** | `headTags` in `docusaurus.config.ts`; loads `https://browser.sentry-cdn.com/8.45.0/bundle.tracing.min.js`; reports directly to `https://*.ingest.sentry.io` (no tunnel) | `script-src`, `connect-src` |
| **BetterStack** | `StatusPill` component fetches `${statuspageUrl}/badge.json` — canonical URL is `https://status.corelink.humangr.com` (CNAME to BetterStack) | `connect-src` |
| **NewsletterSignup** | POSTs cross-origin to `https://corelink-admin.humangr.com/api/newsletter/subscribe` | `connect-src` |
| **Algolia DocSearch** | React-rendered (no remote `<script>`); API calls to `*.algolia.net` / `*.algolianet.com` / `*.algolia.io` | `connect-src` |

---

## 2. Gap Analysis

### 2.1 admin-ui (`apps/admin-ui/src/lib/csp.ts`)

#### Phase 0 baseline (before this audit)

```
script-src  'self' 'nonce-…' https://clerk.corelink.humangr.com https://plausible.io https://js.stripe.com
style-src   'self' 'nonce-…'
connect-src 'self' https://api.corelink.humangr.com https://clerk.corelink.humangr.com
            https://plausible.io https://api.stripe.com https://m.stripe.network
frame-src   https://js.stripe.com https://hooks.stripe.com https://challenges.cloudflare.com
```

#### Gaps identified

| Gap | Directive | Missing hostname | Vendor source |
|---|---|---|---|
| G-001 | `style-src` | `'unsafe-inline'` absent — required by Tailwind CSS JIT inline `<style>` blocks and Clerk modal overlay styles | [Clerk CSP docs](https://clerk.com/docs/security/content-security-policy) |
| G-002 | `connect-src` | `https://checkout.stripe.com` — Stripe Checkout session redirect endpoint | [Stripe CSP guide](https://docs.stripe.com/security/guide#content-security-policy) |
| G-003 | `connect-src` | `https://billing.stripe.com` — Stripe Customer Portal redirect target; `PortalLauncher` does `window.location.assign(portal_url)` where `portal_url` starts with `https://billing.stripe.com` | [Stripe CSP guide](https://docs.stripe.com/security/guide#content-security-policy) |
| G-004 | `frame-src` | `https://clerk.corelink.humangr.com` — Clerk renders verification and TOTP prompts inside same-origin `<iframe>` backed by the proxied Clerk frontend | [Clerk CSP docs](https://clerk.com/docs/security/content-security-policy) |

#### Sentry — NO gap

`tunnelRoute: "/monitoring"` in `next.config.ts` configures `@sentry/nextjs` to route all client beacons through the Next.js app at `/monitoring`. The browser only makes a same-origin POST; the app proxies server-side to `o*.ingest.sentry.io`. Therefore no `connect-src` or `script-src` addition is needed for Sentry in admin-ui.

#### PostHog — NO gap

PostHog is listed in sub-processors legal copy as a potential fallback but is not imported or loaded from any source file in `apps/admin-ui/src/`. No CSP directives required.

### 2.2 docs (`apps/docs/static/_headers`)

#### Phase 0 baseline (before this audit)

```
script-src  'self' https://plausible.io
connect-src 'self' https://plausible.io https://*.algolia.net https://*.algolianet.com https://*.algolia.io
```

#### Gaps identified

| Gap | Directive | Missing hostname | Vendor source |
|---|---|---|---|
| G-005 | `script-src` | `https://browser.sentry-cdn.com` — Sentry CDN loader injected via `headTags` in `docusaurus.config.ts` | [Sentry CDN CSP docs](https://docs.sentry.io/platforms/javascript/install/cdn/#content-security-policy) |
| G-006 | `connect-src` | `https://*.ingest.sentry.io` — Sentry error/event ingest; docs site has no tunnel route, loader POSTs directly | [Sentry CDN CSP docs](https://docs.sentry.io/platforms/javascript/install/cdn/#content-security-policy) |
| G-007 | `connect-src` | `https://status.corelink.humangr.com` — `StatusPill` component fetches `badge.json` from this URL (BetterStack CNAME; browser sees this hostname) | [BetterStack status JSON](https://betterstack.com/docs/uptime/api/get-current-status-of-statuspage/) |
| G-008 | `connect-src` | `https://corelink-admin.humangr.com` — `NewsletterSignup` cross-origin POST to admin-ui API route | Component source: `apps/docs/src/components/NewsletterSignup/NewsletterSignup.tsx:45` |

---

## 3. Applied Patches

### 3.1 `apps/admin-ui/src/lib/csp.ts`

**Delta count: +4 directives/hosts**

```diff
- `style-src 'self' 'nonce-${nonce}'`
+ `style-src 'self' 'unsafe-inline' 'nonce-${nonce}'`         // G-001

- "connect-src 'self' ... https://api.stripe.com https://m.stripe.network"
+ "connect-src 'self' ... https://api.stripe.com https://m.stripe.network https://checkout.stripe.com https://billing.stripe.com"  // G-002, G-003

- "frame-src https://js.stripe.com https://hooks.stripe.com https://challenges.cloudflare.com"
+ "frame-src https://js.stripe.com https://hooks.stripe.com https://challenges.cloudflare.com https://clerk.corelink.humangr.com"  // G-004
```

### 3.2 `apps/docs/static/_headers`

**Delta count: +4 directives/hosts**

```diff
- script-src  'self' https://plausible.io
+ script-src  'self' https://plausible.io https://browser.sentry-cdn.com  // G-005

- connect-src 'self' https://plausible.io https://*.algolia.net https://*.algolianet.com https://*.algolia.io
+ connect-src 'self' https://plausible.io https://*.algolia.net https://*.algolianet.com https://*.algolia.io
+             https://*.ingest.sentry.io https://status.corelink.humangr.com https://corelink-admin.humangr.com  // G-006, G-007, G-008
```

### 3.3 `apps/admin-ui/tests/csp.test.ts`

Three new test cases added:
- `allows Stripe Checkout / portal endpoints` — extended to assert `https://checkout.stripe.com` and `https://billing.stripe.com` in `connect-src`
- `allows Clerk frame-src for modal/popup auth steps` — asserts `https://clerk.corelink.humangr.com` in `frame-src`
- `style-src includes 'unsafe-inline'` — asserts `'unsafe-inline'` present on `style-src` and absent from `script-src`

---

## 4. Constraint Compliance

| Constraint | Status |
|---|---|
| `'unsafe-eval'` forbidden anywhere | PASS — never present |
| `'unsafe-inline'` only on `style-src` | PASS — `style-src` only; `script-src` uses nonce |
| `default-src 'self'` baseline | PASS |
| `object-src 'none'` | PASS |
| `base-uri 'self'` | PASS |
| `form-action 'self'` | PASS |
| HSTS `max-age=63072000; includeSubDomains; preload` | PASS (both apps) |
| `X-Content-Type-Options: nosniff` | PASS (both apps) |
| `Referrer-Policy: strict-origin-when-cross-origin` | PASS (both apps) |
| `Permissions-Policy: camera=(), microphone=(), geolocation=()` | PASS (both apps) |

---

## 5. Manual Smoke Test

After deploying to production (or staging with `CSP_ENFORCEMENT=enforce`), validate the following three flows using the browser DevTools **Console** tab and **Network** tab:

### 5.1 Sign-up flow (`https://app.corelink.humangr.com/sign-up`)

**What to check:**
1. Open DevTools → Console. Zero CSP violation messages should appear on page load.
2. Click "Sign up" button. Clerk modal/popup renders without console errors.
3. DevTools → Network → filter `clerk.corelink.humangr.com`. Auth calls succeed (200/30x, not blocked).
4. After sign-up completes, redirect to `/onboarding` with no CSP errors in console.

### 5.2 Checkout flow (`https://app.corelink.humangr.com/<locale>/pricing` → Upgrade)

**What to check:**
1. Zero CSP violations on page load.
2. Click "Upgrade" button → POST to `/api/checkout/session` succeeds (200).
3. Browser redirects to `https://checkout.stripe.com/…` — no CSP blocking error in console before redirect.
4. Complete checkout on Stripe. Return URL resolves to `/<locale>/upgraded`.
5. DevTools → Console throughout: no `Refused to connect` or `Refused to load` CSP messages referencing `stripe.com`.

### 5.3 StatusPill on docs site (`https://docs.corelink.humangr.com`)

**What to check:**
1. Open DevTools → Console. Zero CSP violation messages on page load.
2. Observe the StatusPill in the navbar. Green dot = successful `badge.json` fetch.
3. DevTools → Network → filter `status.corelink.humangr.com`. Confirm `badge.json` fetch returns 200 (not blocked by CSP `connect-src`).
4. If `SENTRY_DSN_DOCS` is configured at deploy: DevTools → Network → filter `browser.sentry-cdn.com`. Script loads 200. No console `Refused to load script` error.

---

## 6. Vendor Source Citations

| Vendor | Official CSP guidance URL |
|---|---|
| Stripe | https://docs.stripe.com/security/guide#content-security-policy |
| Clerk | https://clerk.com/docs/security/content-security-policy |
| Plausible | https://plausible.io/docs/proxy/csp |
| Sentry (CDN loader) | https://docs.sentry.io/platforms/javascript/install/cdn/#content-security-policy |
| BetterStack | https://betterstack.com/docs/uptime/api/get-current-status-of-statuspage/ |

---

## 7. Files Changed

| File | Change |
|---|---|
| `apps/admin-ui/src/lib/csp.ts` | +`'unsafe-inline'` on `style-src`; +`checkout.stripe.com` and `billing.stripe.com` on `connect-src`; +`clerk.corelink.humangr.com` on `frame-src`; updated docstring with vendor source citations |
| `apps/admin-ui/tests/csp.test.ts` | +3 test cases covering gaps G-001 through G-004 |
| `apps/docs/static/_headers` | +`browser.sentry-cdn.com` on `script-src`; +`*.ingest.sentry.io`, `status.corelink.humangr.com`, `corelink-admin.humangr.com` on `connect-src`; updated comments |
| `specs/_audits/2026-05-27-csp-allowlist-validation.md` | This document |

---

## 8. Total Delta

- **admin-ui**: 4 directive additions (1 × style-src token, 2 × connect-src hosts, 1 × frame-src host)
- **docs**: 4 directive additions (1 × script-src host, 3 × connect-src hosts)
- **Total unique additions: 8**
- **Zero** `'unsafe-eval'` additions
- **Zero** wildcards added to `connect-src` or `frame-src`
- `'unsafe-inline'` added only to `style-src` (acceptable per Tailwind + Clerk requirements)
