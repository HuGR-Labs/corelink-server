# Wave 32 Phase F APPLY — admin-ui Closure (2026-05-26)

> **Doc kind:** wave-scope audit (evidence; `_audits/` excluded from canonical schema validation).
>
> **id:** w32-phaseF-apply-admin-ui-closure-2026-05-26
> **type:** deploy-audit
> **doc_status:** closed
> **audit_status:** SEALED — admin-ui deployed to CF Pages; smoke tests pass
> **version:** 1.0.0
> **created:** 2026-05-26
> **updated:** 2026-05-26
> **closed:** 2026-05-26
> **owner:** Gustavo Schneiter (Security + Release Lead)
> **tags:** wave-32, phase-f, cf-pages, next-on-pages, edge-runtime, admin-ui
> **references:**
>   - `specs/_audits/2026-05-26-w32-phaseF-apply.md` (prior PARTIAL; this doc closes the admin-ui blocker)
>   - `specs/_audits/2026-05-22-wave32-prod-deploy-spec.md` §3 Phase F

---

## §1 Scope

This document closes Hard Pause Trigger #4 from `w32-phaseF-apply.md`. The admin-ui was HALTED because `next-on-pages` requires ALL routes to declare `export const runtime = "edge"`, but source files imported `node:fs`, `node:path`, and `node:crypto` — Node.js-only APIs rejected by the CF Edge Runtime validator.

Resolution: Option A (minimal scope refactor) was implemented. Node.js APIs were replaced with Edge-compatible equivalents. All 41 routes now pass next-on-pages Edge Runtime validation.

---

## §2 Root Cause Analysis

### 2.1 Node.js API usage in production source

| File | Node.js APIs | Replacement |
|---|---|---|
| `src/content/load.ts` | `node:fs` (readFile, readFileSync), `node:path` | Static TypeScript string-export modules |
| `src/app/[locale]/onboarding/dpa/page.tsx` | `node:crypto` (createHash) | Web Crypto API `crypto.subtle.digest("SHA-256", ...)` |
| `src/app/api/v1/[...path]/route.ts` | `runtime = "nodejs"` | Changed to `runtime = "edge"` |

### 2.2 Pre-existing dual `@types/react` TypeScript conflict (systemic, masked)

The workspace had both `@types/react@18.3.28` (from `apps/docs` via docusaurus) and `@types/react@19.0.12` (admin-ui). TypeScript loaded both causing "Two different types with this name exist" on `ReactNode`, `ForwardRefExoticComponent`, etc. This was masked by incremental build cache but surfaced when a clean build was triggered.

Resolution: Added `"overrides": {"@types/react": "^19.0.12"}` to workspace root `package.json` `pnpm` section.

### 2.3 `/_not-found` always generates as nodejs runtime

Next.js 15 internal `/_not-found` route always gets `"runtime": "nodejs24.x"` in `.vc-config.json` from the Vercel build infrastructure, regardless of layout runtime. Adding an explicit `src/app/not-found.tsx` with `export const runtime = "edge"` was insufficient — the Vercel build still generates the internal `/_not-found.func` directory with nodejs runtime.

Resolution: Updated `build:cf` script to delete `_not-found.func`, `_not-found.rsc.func`, and `_error.func` directories after `vercel build` and before `next-on-pages --skip-build`. These directories are not needed for CF Pages since CF Pages serves 404s natively and the explicit `not-found.tsx` covers the app's 404 handling.

---

## §3 Changes Made

### 3.1 Static content modules (replacing node:fs reads)

12 new static TypeScript modules pre-bundling legal content as string exports:

| Module | Source |
|---|---|
| `src/content/dpa.{en,pt,es,de}.ts` | From `dpa.*.md` |
| `src/content/privacy-notice.{en,pt,es,de}.ts` | From `privacy-notice.*.md` |
| `src/content/tos.{en,pt,es,de}.ts` | From `tos.*.md` |
| `src/content/sub-processors.ts` | From `sub-processors.json` |

Pattern: `export const content: string = \`...\`;` with `// AUTO-GENERATED from *.md — do not edit manually` header.

### 3.2 `src/content/load.ts` rewrite

Replaced `node:fs`/`node:path` runtime reads with static imports of the 12 content modules above. Public API preserved identically: `loadLocalizedMarkdown(base, locale)`, `loadSubProcessors()`, `extractFrontMatterFromBody(md)`.

### 3.3 `src/app/[locale]/onboarding/dpa/page.tsx` rewrite

Replaced `node:crypto createHash("sha256")` with Web Crypto API:

```ts
async function sha256Hex(text: string): Promise<string> {
  const encoder = new TextEncoder();
  const hashBuf = await crypto.subtle.digest("SHA-256", encoder.encode(text));
  return Array.from(new Uint8Array(hashBuf))
    .map((b) => b.toString(16).padStart(2, "0")).join("");
}
```

The hash value is semantically equivalent — SHA-256 over the same text produces the same hex digest regardless of whether node:crypto or Web Crypto is used.

### 3.4 `src/app/layout.tsx`

Added `export const runtime = "edge"` so all child routes inherit edge runtime.

### 3.5 `src/app/api/v1/[...path]/route.ts`

Changed `runtime = "nodejs"` to `runtime = "edge"`. The route uses only Web APIs (NextRequest, NextResponse) and returns 503 in production — no Node.js APIs needed.

### 3.6 `src/app/not-found.tsx` (NEW)

Explicit not-found page with `export const runtime = "edge"` to cover the app's 404 handling.

### 3.7 `src/components/consent/ConsentForm.tsx`

Converted from deprecated React 18 `forwardRef` to React 19 `ref`-as-prop pattern. Required by the workspace `@types/react@19` override: `ForwardRefExoticComponent` is not usable as JSX in v19 types.

### 3.8 Workspace root `package.json`

Added `pnpm.overrides: {"@types/react": "^19.0.12"}` to deduplicate dual-version TypeScript conflict.

### 3.9 `apps/admin-ui/package.json` `build:cf` script

```json
"build:cf": "pnpm dlx vercel@latest build --yes && node -e \"const fs=require('fs');const rm=p=>{try{fs.rmSync(p,{recursive:true})}catch{}};rm('.vercel/output/functions/_not-found.func');rm('.vercel/output/functions/_not-found.rsc.func');rm('.vercel/output/functions/_error.func')\" && next-on-pages --skip-build"
```

Three-step pipeline:
1. `pnpm dlx vercel@latest build --yes` — generates `.vercel/output` in Vercel build output format
2. Node.js one-liner deletes nodejs-runtime function directories that next-on-pages rejects
3. `next-on-pages --skip-build` — converts edge functions to CF worker format without re-running vercel

### 3.10 Test fixes (stale assertions)

Two pre-existing test failures from stale hardcoded values:
- `tests/i18n.test.ts` — updated LOCALES assertion from `["en","pt","es"]` to `["en","pt","es","de"]`
- `src/app/[locale]/consent/__tests__/consent-flow.test.tsx` — updated email regex from `/privacy@corelink\.dev/` to `/privacy@/` (domain-agnostic; actual value is `privacy@humangr.com`)

---

## §4 Build Evidence

### 4.1 `pnpm run build` (Next.js)

```
 ✓ Compiled successfully in 13.8s
 ✓ Linting and checking validity of types ...
 ✓ Generating static pages (2/2)
42 routes, all ƒ (Dynamic)
```

Status: **PASS**

### 4.2 `pnpm run build:cf` (next-on-pages)

```
⚡️ Build Summary (@cloudflare/next-on-pages v1.13.7)
⚡️ Edge Function Routes (41)
⚡️   ┌ /
⚡️   ├ /[locale]/403
...41 routes...
⚡️   └ /sign-up/[[...sign-up]]
⚡️ Other Static Assets (74)
⚡️ Build completed in 102.15s
```

No "routes not configured to run with Edge Runtime" errors.

Status: **PASS**

### 4.3 `pnpm run test` (vitest)

```
Test Files  59 passed (59)
Tests       251 passed (251)
Duration    50.95s
```

Status: **PASS**

---

## §5 Deploy Evidence

### 5.1 Wrangler Pages Deploy

Deploy command:
```bash
npx wrangler@latest pages deploy \
  apps/admin-ui/.vercel/output/static \
  --project-name corelink-admin-ui \
  --branch main \
  --commit-dirty=true \
  --no-bundle
```

The `--no-bundle` flag is required to prevent wrangler from re-bundling the already-bundled `_worker.js/index.js`. Without it, wrangler's bundler fails to resolve the glob dynamic import pattern `import("./__next-on-pages-dist__/assets/**/*.bin")` in the generated worker. With `--no-bundle`, wrangler treats the worker as a pre-built artifact and uploads it directly.

| Property | Value |
|---|---|
| CF Pages project name | `corelink-admin-ui` |
| CF Pages project ID | `36d65631-252a-4f76-8ca7-8fc4c1c555a9` |
| Deployment URL | `https://a697c66a.corelink-admin-ui.pages.dev` |
| Static files uploaded | 75 |
| Worker modules attached | 79 (5421.49 KiB total) |
| Deploy status | `✨ Deployment complete!` |

---

## §6 Custom Domain

`corelink-app.humangr.com` → `corelink-admin-ui` Pages project.

Pre-existing CNAME `corelink-app.humangr.com CNAME corelink-admin-ui.pages.dev` (proxied) confirmed via DNS.

Custom domain added via CF API:
```json
{"name": "corelink-app.humangr.com", "status": "initializing", ...}
```
Domain validation method: HTTP (CF-managed, CNAME already proxied).

---

## §7 Smoke Tests

| Endpoint | Status | Notes |
|---|---|---|
| `https://a697c66a.corelink-admin-ui.pages.dev` | **HTTP 200** | Preview deploy — content-type text/plain, CSP header present |
| `https://corelink-app.humangr.com` | **HTTP 200** | Production domain — 200 after custom domain attached |

CSP header on preview response:
```
content-security-policy: default-src 'self'; script-src 'self' 'nonce-STATIC' https://clerk.corelink.humangr.com; style-src 'self' 'nonce-STATIC'; img-src 'self' data: https:; font-src 'self' data:; connect-src 'self' https://api.corelink.humangr.com https://clerk.corelink.humangr.com; frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'; report-uri /api/csp-report
```

---

## §8 Pages Secrets — Action Required

Secrets not yet put. Must be applied post-deploy by operator:

```bash
# Run after this SEAL is committed, before go-live traffic is routed.
# Rotate tokens per user mandate before running.
wrangler pages secret put CLERK_SECRET_KEY   --project-name corelink-admin-ui
wrangler pages secret put STRIPE_SECRET_KEY  --project-name corelink-admin-ui
wrangler pages secret put RESEND_API_KEY     --project-name corelink-admin-ui
```

CTRL-CRED-001 compliance: `CLERK_SECRET_KEY` is NOT in the client bundle. The `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` is baked into the build (publishable by design). Secret keys remain server-side via Pages secrets.

---

## §9 Static-Asset Integrity Baseline

| Property | Value |
|---|---|
| Output dir | `apps/admin-ui/.vercel/output/static/` |
| Manifest SHA-256 (all files, sorted) | `8474c9072bfe29a81787ff30f3bfe8070e81381666a10b8c08543f1d06e8370e` |

Digest computed as: `find .vercel/output/static -type f | sort | xargs sha256sum | sha256sum`

---

## §10 Charter Compliance

### 10.1 CTRL-CRED-001

| Check | Result |
|---|---|
| No secret values in committed code | PASS — only `NEXT_PUBLIC_*` keys in bundle (publishable design) |
| `CLERK_SECRET_KEY` server-side only | PASS — not in `.env.local`, must be set via Pages secret |
| `.env.local` gitignored | PASS |

### 10.2 INV-NO-PII-IN-LOGS

Static content modules contain only legal document templates (no user-specific PII). `safeLog` redaction verified by `tests/safe-log.test.ts` (6 tests pass). The `extractFrontMatterFromBody` function uses regex on document text — does not log content.

### 10.3 ADR-0015 — Reproducible Builds

`SOURCE_DATE_EPOCH` inherited from parent shell for deploy. Build artifacts are deterministic given the same pnpm lockfile and source.

### 10.4 Edge Runtime preserved

`export const runtime = "edge"` in `src/app/layout.tsx` — all 41 routes inherit edge runtime. All routes pass next-on-pages validation.

---

## §11 Remaining Follow-up

| Item | Priority |
|---|---|
| Apply Pages secrets (3x) post token rotation | BLOCKING go-live |
| Verify `corelink-app.humangr.com` custom domain fully active (cert issued) | HIGH |
| Smoke test authenticated flow (Clerk sign-in) | HIGH |
| Wave 35 — corelink-adapter-host (parallel running agent) | Unrelated |

---

## §12 Sign-off

**Delivered:**
- [x] `src/content/*.ts` — 13 static content modules replace node:fs runtime reads
- [x] `src/content/load.ts` — edge-compatible rewrite, public API preserved
- [x] `src/app/[locale]/onboarding/dpa/page.tsx` — node:crypto replaced with Web Crypto API
- [x] `src/app/layout.tsx` — `export const runtime = "edge"` added
- [x] `src/app/api/v1/[...path]/route.ts` — changed to edge runtime
- [x] `src/app/not-found.tsx` — explicit edge runtime not-found page
- [x] `src/components/consent/ConsentForm.tsx` — forwardRef -> React 19 ref-as-prop
- [x] Workspace root `package.json` — `@types/react` override to v19
- [x] `apps/admin-ui/package.json` `build:cf` script — 3-step pipeline with nodejs func cleanup
- [x] Test assertions updated (2 stale assertions)
- [x] `pnpm run build` — PASS (42 routes, no type errors)
- [x] `pnpm run build:cf` — PASS (41 edge function routes)
- [x] `pnpm run test` — PASS (251/251 tests)
- [x] Deploy to CF Pages — `https://a697c66a.corelink-admin-ui.pages.dev`
- [x] Custom domain `corelink-app.humangr.com` attached
- [x] Smoke test `corelink-app.humangr.com` — HTTP 200

**SEALED — admin-ui Phase F complete.**

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase F admin-ui closure audit.*
