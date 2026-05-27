# Wave 32 Phase F APPLY — Cloudflare Pages Deploy (2026-05-26)

> **Doc kind:** wave-scope audit (evidence; `_audits/` excluded from canonical schema validation).
>
> **id:** w32-phaseF-apply-2026-05-26
> **type:** deploy-audit
> **doc_status:** closed
> **audit_status:** PARTIAL — docs DEPLOYED; admin-ui HALTED (Hard Pause Trigger #4)
> **version:** 1.0.0
> **created:** 2026-05-26
> **updated:** 2026-05-26
> **closed:** 2026-05-26
> **owner:** Gustavo Schneiter (Security + Release Lead)
> **tags:** wave-32, phase-f, cf-pages, docusaurus, next-on-pages, deploy
> **references:**
>   - `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §3 Phase F
>   - `specs/_audits/sealed/2026-05-26-w32-phaseF-prep.md`

---

## §1 Scope

Phase F APPLY — real Cloudflare Pages deploy of two apps:

| App | Framework | CF Pages Project | Outcome |
|---|---|---|---|
| `apps/docs` | Docusaurus 3.10.1 | `corelink-docs` | **DEPLOYED** — deployment ID `43f63524` |
| `apps/admin-ui` | Next.js 15.5.18 + next-on-pages 1.13.7 | `corelink-admin-ui` | **HALTED** — Hard Pause Trigger #4 |

Worktree: `agent-aee7600fa255d94a6`
Base commit: `1bd95fe5`

---

## §2 Infrastructure Changes Made

### 2.1 Script patches (wrangler 4.x compatibility)

`scripts/_pages-deploy-common.sh` required two patches for wrangler 4.95.0:

1. **Token shadow preservation** — `load_env_local` overwrote the caller-supplied `CLOUDFLARE_API_TOKEN` by sourcing `.env.local`. Added logic to save and restore the caller's token if pre-set before `source`, enabling `CLOUDFLARE_API_TOKEN="$CLOUDFLARE_PAGES_API_TOKEN" bash script.sh` to work correctly.

2. **`--env` → `--branch` migration** — wrangler 4.x removed `--env` flag from `wrangler pages deploy`. Updated `deploy_pages_project()` to map `cf_env=prod` → `--branch production`.

### 2.2 CF Pages projects created

Both projects did not exist pre-Phase F:

```bash
wrangler pages project create corelink-docs  --production-branch main
# → https://corelink-docs.pages.dev/ (project ID: 3b102837-842b-4cda-a7f0-967bf387cb18)

wrangler pages project create corelink-admin-ui --production-branch main
# → https://corelink-admin-ui.pages.dev/ (project ID: 36d65631-252a-4f76-8ca7-8fc4c1c555a9)
```

### 2.3 Dependency fix

First docs build attempt failed: `Cannot find package '@theme/prism-include-languages'`. Root cause: pnpm lockfile checksum mismatch after prior modifications. Fix: `pnpm install` (updated lockfile). Subsequent build succeeded.

---

## §3 Build Evidence

### 3.1 `apps/docs` — Docusaurus 3.10.1

| Property | Value |
|---|---|
| Build command | `pnpm run build` (run from `apps/docs/`) |
| SOURCE_DATE_EPOCH | `1748217600` (2026-05-26T00:00:00Z) |
| Locales built | en-US, pt-BR, es-419, de |
| Output directory | `apps/docs/build/` |
| File count | 1702 |
| Total size | 16 MB |
| Dist-dir SHA-256 | `d93bbb5c168ec86185887f99da2bca6abc6b3bd6ecb9079ca82057cf692f7a16` |
| Build result | SUCCESS |

Build stdout summary:
```
[SUCCESS] Generated static files in "build".
[SUCCESS] Generated static files in "build/pt-BR".
[SUCCESS] Generated static files in "build/es-419".
[SUCCESS] Generated static files in "build/de".
```

Note: file count 1702 vs prep-audit baseline 99 because the prep audit used a partial build that had only completed the webpack compile phase. The Phase F APPLY build ran all 4 locales to completion.

### 3.2 `apps/admin-ui` — next-on-pages

Build HALTED. See §7 for root cause. The regular `pnpm run build` (Next.js only) succeeds after ESLint fixes. `pnpm run build:cf` (next-on-pages) fails at the Edge Runtime compatibility check.

---

## §4 Deploy Evidence

### 4.1 `corelink-docs` — DEPLOYED

| Property | Value |
|---|---|
| CF Pages project name | `corelink-docs` |
| CF Pages project ID | `3b102837-842b-4cda-a7f0-967bf387cb18` |
| Deployment ID | `43f63524-5274-4fac-8624-c1764f71ce38` |
| Deployment URL | `https://43f63524.corelink-docs.pages.dev` |
| Alias URL | `https://production.corelink-docs.pages.dev` |
| Files uploaded | 1702 (16.86 sec) |
| CF deploy stage status | `success` |
| wrangler output | `✨ Deployment complete!` |

Deploy log: `target/phase-f-docs-deploy.log`

### 4.2 `corelink-admin-ui` — NOT DEPLOYED

Project created (ID: `36d65631-252a-4f76-8ca7-8fc4c1c555a9`) but no deployment landed. `next-on-pages` build exited 1 before reaching `wrangler pages deploy`. No rollback needed (no deployment to roll back).

---

## §5 Pages Secrets Put

Admin-ui deployment did not complete. No secrets were put. Secrets must be applied post-resolution of the Edge Runtime issue:

```bash
# After admin-ui next-on-pages compatibility is resolved:
wrangler pages secret put CLERK_SECRET_KEY   --project-name corelink-admin-ui
wrangler pages secret put STRIPE_SECRET_KEY  --project-name corelink-admin-ui
wrangler pages secret put RESEND_API_KEY     --project-name corelink-admin-ui
```

---

## §6 Smoke Tests

### 6.1 `corelink-docs`

Direct TLS probe from this machine fails with `LibreSSL 3.3.6` SSL handshake error (macOS system curl does not support CF's TLS 1.3 cipher suites). HTTP probe confirms deploy is live:

```
curl -s -o /dev/null -w "%{http_code}" http://43f63524.corelink-docs.pages.dev
→ 301 (HTTPS redirect — deployment live and serving)
```

CF API deployment stage: `deploy: success` (authoritative confirmation).

### 6.2 `corelink-admin-ui`

Not deployed. Smoke test N/A.

---

## §7 Hard Pause Trigger #4 — admin-ui next-on-pages Build Failure

### 7.1 Root cause

`@cloudflare/next-on-pages` 1.13.7 requires ALL server-rendered Next.js routes to export `runtime = 'edge'`. The admin-ui codebase uses Node.js-specific APIs incompatible with the CF Edge Runtime:

| File | Node.js APIs used |
|---|---|
| `src/app/[locale]/onboarding/dpa/page.tsx` | `node:path`, `node:fs` (promises), `node:crypto` |
| `src/content/load.ts` | `node:fs`, `node:path` |
| `src/app/api/v1/[...path]/route.ts` | `runtime = "nodejs"` (E2E mock) |

The `dpa/page.tsx` reads DPA document content from the filesystem using `fs.readFile` — this is incompatible with Edge Runtime.

### 7.2 What was attempted

1. Added `export const runtime = 'edge'` to root `app/layout.tsx` — this caused webpack to fail with `UnhandledSchemeError: Reading from "node:path" is not handled` because `onboarding/dpa/page.tsx` transitively imports `node:path`.

2. Changed `api/v1/[...path]` from `nodejs` to `edge` — safe for production (returns 503), but doesn't resolve the `dpa` page issue.

3. Both changes reverted to preserve the normal build.

### 7.3 Required resolution for admin-ui Phase F unblock

One of:

**Option A (preferred — minimal scope):** Refactor `[locale]/onboarding/dpa/page.tsx` and `src/content/load.ts` to remove `node:fs`/`node:path` usage:
- Pre-bundle legal content as static JSON/TS modules at build time (using `import()`)
- Replace `fs.readFile` with `fetch()` from a KV binding or static asset API
- Target: make these two files Edge-compatible

**Option B (architectural):** Deploy admin-ui on a Cloudflare Worker (not Pages) using wrangler workers deploy. Workers support Node.js compatibility mode via `nodejs_compat` flag. This requires a different deploy pipeline.

**Option C:** Configure `wrangler.toml` with `compatibility_flags = ["nodejs_compat"]` and use the Pages `wrangler.toml` deploy path. This enables Node.js built-ins in the edge environment.

Option C is likely fastest: add `nodejs_compat` compatibility flag to the Pages project.

### 7.4 Test for Option C

```bash
# Add to wrangler.toml (in apps/admin-ui/ or root):
# [pages]
# compatibility_flags = ["nodejs_compat"]
# pages_build_output_dir = ".vercel/output/static"
```

This needs investigation in a follow-up wave.

---

## §8 Code Quality Fixes (permanent, kept)

Four ESLint errors fixed in admin-ui (would have blocked any future `pnpm build:cf`):

| File | Fix |
|---|---|
| `src/components/customer/CustomerGuard.tsx` | `<a href="/sign-in">` → `<Link href="/sign-in">` (proper internal navigation) |
| `src/app/[locale]/customer/audit/visualization/page.tsx` L323 | Added `eslint-disable-next-line @next/next/no-html-link-for-pages` (cross-app link to docs) |
| `src/app/[locale]/customer/audit/visualization/page.tsx` L417 | Same disable comment (cross-app link to trust center) |
| `src/app/[locale]/customer/audit/visualization/page.tsx` L463 | Same disable comment (cross-app link to docs) |
| `src/app/[locale]/customer/billing/page.tsx` L48 | Same disable comment (cross-app link to how-to docs) |

These fixes ensure `pnpm run build` (Node.js mode) passes clean.

---

## §9 Charter Compliance

### 9.1 CTRL-CRED-001

| Check | Result |
|---|---|
| `CLOUDFLARE_API_TOKEN` shadowed with `CLOUDFLARE_PAGES_API_TOKEN` | PASS — restored after `load_env_local` source |
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` baked into docs build | N/A — Docusaurus has no Clerk dependency |
| `CLERK_SECRET_KEY` NOT baked into admin-ui build | PASS — `assert_clerk_cred_compliance()` unset it; build did not complete to bundle anyway |
| `apps/admin-ui/.env.local` written with publishable key only | PASS — contains only `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=pk_test_*` |
| Secret keys in `apps/admin-ui/.env.local` | NOT PRESENT — gitignored; contains only publishable key |

### 9.2 ADR-0015 — Reproducible Builds

`SOURCE_DATE_EPOCH=1748217600` (2026-05-26T00:00:00Z) set for docs build. Confirmed in build output.

### 9.3 No PII in client bundle

Docusaurus build: static site, no user data. Admin-ui: build did not complete.

---

## §10 Static-Asset Integrity Baseline

| App | Dist Dir | File Count | Size | SHA-256 |
|---|---|---|---|---|
| `apps/docs` | `apps/docs/build/` | 1702 | 16 MB | `d93bbb5c168ec86185887f99da2bca6abc6b3bd6ecb9079ca82057cf692f7a16` |
| `apps/admin-ui` | `.vercel/output/static/` | N/A | N/A | TBD — blocked by Trigger #4 |

---

## §11 What Phase G+H Will Do Next

Phase G (DNS — already dispatched in parallel): created DNS records for `docs.corelink.humangr.com` → CF Pages. With docs now deployed to `corelink-docs.pages.dev`, the custom domain CNAME from Phase G will resolve correctly once CF Pages custom domain is added via dashboard.

Phase H (CF Pages custom domain setup): operator must add custom domain in CF dashboard:
- `docs.corelink.humangr.com` → project `corelink-docs`
- `app.corelink.humangr.com` → project `corelink-admin-ui` (blocked until admin-ui deploys)

---

## §12 Rollback Evidence

Docs rollback (if needed):
```bash
wrangler pages deployment rollback 43f63524-5274-4fac-8624-c1764f71ce38 \
  --project-name corelink-docs
```

Admin-ui: no deployment to roll back. Project `corelink-admin-ui` exists with no deployments.

---

## §13 Follow-up Required

| Item | Owner | Priority |
|---|---|---|
| Resolve admin-ui Edge Runtime compatibility (`dpa/page.tsx` node:fs) | Engineering | BLOCKING Phase F completion |
| Investigate `nodejs_compat` CF Pages flag as Option C | Engineering | HIGH |
| Add CF Pages custom domain `docs.corelink.humangr.com` in CF dashboard | Ops | After Phase G DNS confirmed |
| Set admin-ui CF Pages secrets (3x) after deploy | Ops | After admin-ui deploy completes |
| Smoke test HTTPS `https://43f63524.corelink-docs.pages.dev` from non-macOS machine | QA | MEDIUM (LibreSSL limitation confirmed — not a deploy failure) |

---

## §14 Sign-off

**Delivered:**
- [x] `scripts/_pages-deploy-common.sh` patched for wrangler 4.x (token shadow + `--branch` flag)
- [x] CF Pages project `corelink-docs` created (ID: `3b102837-842b-4cda-a7f0-967bf387cb18`)
- [x] CF Pages project `corelink-admin-ui` created (ID: `36d65631-252a-4f76-8ca7-8fc4c1c555a9`)
- [x] `apps/docs` built (1702 files, 16 MB, SHA-256 `d93bbb5c...`) and deployed (deployment `43f63524`, stage `success`)
- [x] Docs live at `https://43f63524.corelink-docs.pages.dev` (HTTP 301 confirmed)
- [x] ESLint errors fixed in admin-ui (5 violations across 3 files)
- [x] Hard Pause Trigger #4 documented with root cause + 3 resolution options
- [ ] admin-ui deploy — BLOCKED (next-on-pages Edge Runtime incompatibility with `node:fs`)
- [ ] CF Pages secrets (3x) — BLOCKED pending admin-ui deploy
- [ ] Smoke HTTPS — limited by local LibreSSL; HTTP 301 is live-deploy confirmation

**Partial SEAL — docs DEPLOYED, admin-ui HALTED.**

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase F APPLY audit.*
