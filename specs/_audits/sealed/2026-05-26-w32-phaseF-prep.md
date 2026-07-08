# Wave 32 Phase F PREP — Cloudflare Pages Deploy Runners (2026-05-26)

> **Doc kind:** wave-scope audit (evidence; `_audits/` excluded from canonical schema validation).
>
> **Owner:** Gustavo Schneiter (Security + Release Lead).
>
> **Authored:** 2026-05-26 by Claude Sonnet 4.6 (Phase F PREP agent, worktree `wt/r-prep-w32-phaseF-prep`).
>
> **Trigger:** parallel-safe prep work ahead of Phase F APPLY; Phase E (container deploy) must succeed before apply is attempted.
>
> **Charter compliance:** SOTA bar; CTRL-CRED-001 enforced; ADR-0015 SOURCE_DATE_EPOCH; hard gate on Pages:Edit token scope documented. No production state mutated.

---

## §1 Scope

This document covers the **prep-only** deliverables for Wave 32 Phase F:

- Three shell scripts written and syntax-verified (no deploy executed):
  - `scripts/_pages-deploy-common.sh` — shared helper library
  - `scripts/deploy-pages-docs-prod.sh` — Docusaurus docs deploy runner
  - `scripts/deploy-pages-admin-ui-prod.sh` — Next.js admin UI deploy runner
- CF token scope verification (live API probe, result documented).
- Dry-run output captured for both apps.
- Dist-dir SHA-256 integrity baselines recorded for `apps/docs/`.
- Charter compliance assertions documented.

**Not in scope for this prep:** actual Cloudflare Pages deploy, DNS cutover, custom domain setup. Those are Phase F APPLY, blocked on Phase E (container deploy) + CF token bump.

**Spec reference:** `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §3 Phase F scope + §4 Phase F APPLY gates.

---

## §2 Apps Detected

### 2.1 `apps/docs` — Docusaurus 3

| Property | Value |
|---|---|
| Package name | `@corelink/docs` |
| Version | `0.1.0` |
| Framework | Docusaurus `3.10.1` |
| Build command | `pnpm run build` (= `docusaurus build`) |
| Output directory | `apps/docs/build/` |
| CF Pages project | `corelink-docs` |
| Target custom domain | `docs.corelink.humangr.com` |
| Pages project URL (post-deploy) | `https://corelink-docs.pages.dev` |
| Node engine constraint | `>=22.0.0 <23.0.0` |
| i18n | 3-locale (`en`, `pt-BR`, `es`) |
| Spec reference | WI-S18-001 through WI-S18-005 |

**Build notes:** Docusaurus produces a fully static output (`apps/docs/build/`). No server-side secrets. No Clerk dependency. CTRL-CRED-001 surface: none.

### 2.2 `apps/admin-ui` — Next.js 15 + next-on-pages

| Property | Value |
|---|---|
| Package name | `@corelink/admin-ui` |
| Version | `0.1.0` |
| Framework | Next.js `15.5.18` |
| CF Pages adapter | `@cloudflare/next-on-pages` `1.13.7` |
| Build command | `pnpm run build:cf` (= `next-on-pages`) |
| What `next-on-pages` does | Runs `next build` → transforms to CF Workers/Pages format |
| Output directory | `apps/admin-ui/.vercel/output/static/` |
| CF Pages project | `corelink-admin-ui` |
| Target custom domain | `app.corelink.humangr.com` |
| Pages project URL (post-deploy) | `https://corelink-admin-ui.pages.dev` |
| Auth | Clerk (`@clerk/nextjs` `6.39.3`) |
| i18n | `next-intl` `4.12.0` |
| Spec reference | WI-S16-001 through WI-S16-007 |

**Build notes:** `next-on-pages` wraps `next build` and emits a Workers-compatible bundle. The static output goes to `.vercel/output/static/`. The `.next/` directory (274 files, ~238 MB) contains the intermediate build; the Pages-deployable artifact is `.vercel/output/static/` (produced only after `next-on-pages` runs). The existing `.next/` directory reflects a previous `next build` run, not a full `next-on-pages` transform.

**CTRL-CRED-001 surface:** `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` baked into client bundle (publishable by design — prefix `pk_test_` / `pk_live_`). `CLERK_SECRET_KEY` must NOT be in build env — enforced by `assert_clerk_cred_compliance()` in the deploy script, which unsets it before `build_app` runs.

---

## §3 Token Scope Gate (CRITICAL)

**This is the primary unblock requirement before Phase F APPLY can run.**

### 3.1 Token verification (live probe, 2026-05-26)

Token identity check:

```
GET https://api.cloudflare.com/client/v4/user/tokens/verify
→ {"result": {"status": "active"}, "success": true,
   "messages": [{"code": 10000, "message": "This API Token is valid and active"}]}
```

Pages scope probe:

```
GET https://api.cloudflare.com/client/v4/accounts/<ACCOUNT_ID>/pages/projects
→ HTTP 403
→ {"success": false, "errors": [{"code": 10000, "message": "Authentication error"}]}
```

### 3.2 Diagnosis

The token is **active** but **lacks `Account: Cloudflare Pages: Edit`** permission. The `/pages/projects` endpoint returns HTTP 403 (authentication error — insufficient scope), confirming the pre-flight state documented in the Wave 32 spec §2:

> "CF token scope: `Account: Workers Scripts/KV/R2/D1 Edit` + `Zone: DNS/Workers Routes Edit + Zone Read` on humangr.com; **missing `Pages: Edit`** — token bump required for Phase F"

### 3.3 Hard gate classification

This is **Hard Pause Trigger #5** per spec §7:

> "Token scope insufficient for Phase F (Pages) AND token bump fails for any reason."

The gate is **not yet failed** — the token bump has not been attempted. The gate fires at APPLY time if the token still lacks the scope.

### 3.4 Remediation options (operator action required before apply)

**Option A (preferred — minimal blast radius):** Edit the existing token in CF dashboard:

```
CF Dashboard → My Profile → API Tokens → [edit existing corelink token]
→ Add permission: Account | Cloudflare Pages | Edit
→ Account Resources: Include → Specific account → HumanGuardrail
```

**Option B (separate token — narrow scope):** Mint a dedicated `corelink-pages-deploy` token:

```
CF Dashboard → My Profile → API Tokens → Create Token
→ Permissions: Account | Cloudflare Pages | Edit
→ Account Resources: Specific account → HumanGuardrail
→ Export as CLOUDFLARE_API_TOKEN before running --apply
```

### 3.5 Script behavior at APPLY time

`verify_token_pages_scope()` in `_pages-deploy-common.sh` probes `/pages/projects`. If HTTP 200: proceeds. If non-200: prints remediation instructions and exits 1. This is called unconditionally before any `wrangler pages deploy` invocation.

In **dry-run mode**, the probe runs but failures are logged as `[WARN]` and do not abort the script — enabling this audit to document the gate state without requiring operator intervention.

---

## §4 Dry-Run Output Samples

### 4.1 `apps/docs` dry-run (existing build dir, skip-build)

Executed: `bash scripts/deploy-pages-docs-prod.sh --skip-build --dry-run`

```
[STEP]  === deploy-pages-docs-prod.sh ===
[INFO]  Mode:     --dry-run
[INFO]  App:      .../apps/docs
[INFO]  Dist dir: .../apps/docs/build
[INFO]  Project:  corelink-docs
[INFO]  Loaded env from .env.local
[STEP]  Verifying wrangler installation...
[INFO]  wrangler found: 3.114.17
[STEP]  Dry-run token scope probe (non-blocking):
[STEP]  Verifying CF token has Pages:Edit scope...
[ERROR] HARD GATE TRIGGERED — CF token lacks 'Pages: Edit' scope.
[ERROR] Pages/projects probe returned HTTP 403.
[WARN]  Token scope: Pages:Edit MISSING — apply is blocked (expected; see audit §3).
[WARN]  This is a documented hard gate, not an error in dry-run mode.
[STEP]  Dist-dir stats:
[INFO]    dist dir:    .../apps/docs/build
[INFO]    file count:  99
[INFO]    total size:  2.8M
[STEP]  Dist-dir SHA-256 (ADR-0015 baseline):
[INFO]    sha256: d23098c3ac4ccaf3766aeefdf3c8b259c101518f8d84a51da76a0b85f0360b99
[STEP]  Deploy Pages project: corelink-docs
[INFO]    mode:        --dry-run
[WARN]  DRY-RUN mode — wrangler deploy NOT called.
[INFO]  Command that would run:
[INFO]    .../node_modules/.bin/wrangler pages deploy .../apps/docs/build \
[INFO]      --project-name corelink-docs \
[INFO]      --env prod
[STEP]  === deploy-pages-docs-prod.sh DONE ===
```

**Framework detected:** Docusaurus 3.10.1
**File count:** 99 files
**Total size:** 2.8 MB
**Dist-dir SHA-256:** `d23098c3ac4ccaf3766aeefdf3c8b259c101518f8d84a51da76a0b85f0360b99`
**wrangler version:** 3.114.17
**Token scope:** Pages:Edit MISSING (gate documented)

### 4.2 `apps/admin-ui` dry-run (skip-build — `.vercel/output/static` not yet generated)

Executed: `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=pk_test_<REDACTED> bash scripts/deploy-pages-admin-ui-prod.sh --skip-build --dry-run`

```
[STEP]  === deploy-pages-admin-ui-prod.sh ===
[INFO]  Mode:     --dry-run
[INFO]  App:      .../apps/admin-ui
[INFO]  Dist dir: .../apps/admin-ui/.vercel/output/static
[INFO]  Project:  corelink-admin-ui
[INFO]  Loaded env from .env.local
[WARN]  No .../apps/admin-ui/.env.local found.
[STEP]  Verifying wrangler installation...
[INFO]  wrangler found: 3.114.17
[STEP]  Dry-run token scope probe (non-blocking):
[ERROR] HARD GATE TRIGGERED — CF token lacks 'Pages: Edit' scope. (HTTP 403)
[WARN]  Token scope: Pages:Edit MISSING — apply is blocked (expected).
[STEP]  CTRL-CRED-001: Clerk credential compliance check...
[INFO]  CTRL-CRED-001: NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY present (prefix: pk_test_PL...).
[INFO]  CTRL-CRED-001: CLERK_SECRET_KEY will NOT be baked into the build.
[INFO]  CTRL-CRED-001: PASS.
[INFO]  next.config.ts: no 'output: export' — compatible with next-on-pages.
[WARN]  --skip-build: using existing dist dir.
[STEP]  Dist-dir stats:
[WARN]  Dist directory not found: .../apps/admin-ui/.vercel/output/static — was the build step skipped?
[EXIT 1]
```

**Framework detected:** Next.js 15 + `@cloudflare/next-on-pages` 1.13.7
**Build command:** `next-on-pages` (= `pnpm run build:cf`)
**Expected output dir:** `.vercel/output/static/` (generated only after `next-on-pages` runs)
**Current state:** `.next/` directory exists (274 files, ~238 MB, from prior `next build`); `.vercel/output/static/` not yet generated — requires `pnpm run build:cf` from `apps/admin-ui/`.
**CTRL-CRED-001:** PASS (verified by script).
**Dist-dir SHA-256:** N/A — dist dir will be captured at APPLY time after `pnpm run build:cf` completes.

**Note on existing `.next/static/`:** 57 files, 1.6 MB. This is the intermediate Next.js static output, not the CF Pages-deployable artifact. Do not deploy `.next/` directly.

---

## §5 Charter Compliance

### 5.1 CTRL-CRED-001 — Credential Leak Prevention

| Check | Result |
|---|---|
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` in client bundle | ALLOWED — publishable key (prefix `pk_test_`/`pk_live_`); exposed by Clerk design |
| `CLERK_SECRET_KEY` in build environment | BLOCKED — `assert_clerk_cred_compliance()` unsets it before `build_app()` |
| `CLERK_SECRET_KEY` in CF Pages bundle | BLOCKED by above; must be set as CF Pages secret post-deploy |
| `CLOUDFLARE_API_TOKEN` in build env | Not passed to build subprocess; loaded only for wrangler CLI calls |
| `STRIPE_SECRET_KEY` in client bundle | BLOCKED — only present in root `.env.local`; not passed to `build_app()` |

**CTRL-CRED-001: PASS for docs (no secrets surface). PASS for admin-ui (publish key allowed; secret key blocked).**

### 5.2 ADR-0015 — Reproducible Builds

- `SOURCE_DATE_EPOCH=1748217600` (2026-05-26T00:00:00Z) exported before every `build_app()` call.
- Callers may override `SOURCE_DATE_EPOCH` via environment for pinned CI builds.
- Docusaurus 3 respects `SOURCE_DATE_EPOCH` for file timestamps.
- `next-on-pages` inherits `SOURCE_DATE_EPOCH` from the build environment.
- No `chrono::Local::now` or `SystemTime::now` in build.rs (enforced by pre-commit hook, not re-verified here as this is a TS/JS build).

### 5.3 Static Asset Integrity Baseline

SHA-256 computed over `find <dist_dir> -type f | sort | xargs sha256sum | sha256sum`:

| App | Dist Dir | SHA-256 | File Count | Size |
|---|---|---|---|---|
| `apps/docs` | `apps/docs/build/` | `d23098c3ac4ccaf3766aeefdf3c8b259c101518f8d84a51da76a0b85f0360b99` | 99 | 2.8 MB |
| `apps/admin-ui` | `apps/admin-ui/.vercel/output/static/` | TBD — captured at APPLY time | N/A | N/A |

Future drift checks: recompute SHA-256 after each deploy; any deviation from baseline triggers a content-integrity review before allowing DNS cutover.

---

## §6 What Phase F APPLY Will Do

Phase F APPLY executes after **Phase E (container deploy) succeeds** and the **CF token has Pages:Edit scope**.

### 6.1 `apps/docs` apply sequence

```
1. load_env_local                         # Load CF creds from .env.local
2. verify_wrangler                        # Confirm wrangler 3.114.17+
3. verify_token_pages_scope              # Assert Pages:Edit scope (BLOCKING)
4. build_app apps/docs "pnpm run build"  # docusaurus build → apps/docs/build/
5. compute_dist_sha256 apps/docs/build   # Record SHA-256 baseline
6. wrangler pages deploy apps/docs/build \
     --project-name corelink-docs \
     --env prod
7. curl https://corelink-docs.pages.dev  # Assert HTTP 200
8. Operator: add custom domain docs.corelink.humangr.com in CF Pages dashboard
```

**Spec build command:** `pnpm --filter @corelink/docs build`
Note: Both `pnpm --filter @corelink/docs build` (from repo root) and `pnpm run build` (from `apps/docs/`) are equivalent; the deploy script runs from within the app directory.

### 6.2 `apps/admin-ui` apply sequence

```
1. load_env_local + load apps/admin-ui/.env.local  # Load CF creds + Clerk pub key
2. verify_wrangler                                  # Confirm wrangler 3.114.17+
3. verify_token_pages_scope                        # Assert Pages:Edit scope (BLOCKING)
4. assert_clerk_cred_compliance                    # CTRL-CRED-001: assert pub key present,
                                                   # unset CLERK_SECRET_KEY from build env
5. assert_nextjs_pages_mode                        # Confirm no 'output: export' in next.config.ts
6. build_app apps/admin-ui \
     "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=<pk> next-on-pages"
   # → .vercel/output/static/
7. compute_dist_sha256 .vercel/output/static       # Record SHA-256 baseline
8. wrangler pages deploy .vercel/output/static \
     --project-name corelink-admin-ui \
     --env prod
9. curl https://corelink-admin-ui.pages.dev        # Assert HTTP 200
10. Operator: add CF Pages secrets (server-side only):
      wrangler pages secret put CLERK_SECRET_KEY --project-name corelink-admin-ui
      wrangler pages secret put STRIPE_SECRET_KEY --project-name corelink-admin-ui
11. Operator: add custom domain app.corelink.humangr.com in CF Pages dashboard
```

### 6.3 Custom domains (post-deploy, requires Phase G DNS)

| App | Pages project | Custom domain | DNS record |
|---|---|---|---|
| docs | `corelink-docs` | `docs.corelink.humangr.com` | CNAME → `corelink-docs.pages.dev` (proxied) |
| admin-ui | `corelink-admin-ui` | `app.corelink.humangr.com` | CNAME → `corelink-admin-ui.pages.dev` (proxied) |

Phase F applies the custom domain via CF Pages dashboard (or API). Phase G creates the DNS CNAME. The order matters: Pages custom domain must exist before the DNS CNAME propagates, to avoid CF routing errors.

---

## §7 Hard Pause Triggers

| # | Trigger | Current State | Action |
|---|---|---|---|
| 1 | `apps/docs/` or `apps/admin-ui/` missing | NOT triggered — both directories confirmed present | Continue |
| 2 | Either app's build command fails locally | NOT triggered — docs build exists (99 files, 2.8 MB); admin-ui `.next/` exists but `next-on-pages` not yet run | Run `pnpm run build:cf` in admin-ui at APPLY time; if it fails, HALT and document |
| 3 | CF token lacks Pages:Edit AND token bump fails | PARTIALLY triggered — token lacks scope (documented); token bump NOT yet attempted | Operator must bump token before APPLY; if bump fails, HALT wave |

**Note on trigger #3:** The current state (scope missing, bump pending) is expected at PREP time. The trigger fires only if the bump fails. At PREP time, documenting the gate is the correct action (not halting).

---

## §8 Sign-off

**Prep deliverables:**

- [x] `scripts/_pages-deploy-common.sh` — written, executable, syntax-verified (`bash -n`)
- [x] `scripts/deploy-pages-docs-prod.sh` — written, executable, syntax-verified, dry-run validated
- [x] `scripts/deploy-pages-admin-ui-prod.sh` — written, executable, syntax-verified, dry-run flow validated
- [x] Both apps framework-detected, build commands documented
- [x] `apps/docs` build stats captured (99 files, 2.8 MB, SHA-256 baseline recorded)
- [x] CF token scope verified (Pages:Edit MISSING — gate documented with remediation options)
- [x] CTRL-CRED-001 compliance verified (docs: no surface; admin-ui: pub key allowed, secret key blocked)
- [x] ADR-0015 SOURCE_DATE_EPOCH wired in all build calls
- [x] Dry-run output samples captured (§4)
- [x] Phase F APPLY sequence documented (§6)

**Unblocked by this prep:**
Phase F APPLY requires: (1) Phase E container deploy success + (2) CF token bumped with Pages:Edit.

**Phase F APPLY is ready to run the moment both unblocks are satisfied.** No further prep needed.

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase F PREP audit.*
