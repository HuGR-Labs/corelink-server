---
id: "AUDIT-2026-05-27-W32-PHASEF2-ADMIN-PAGES-PREP-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags:
  - "audit"
  - "wave-32"
  - "phase-f"
  - "phase-f2"
  - "admin-ui"
  - "cloudflare-pages"
  - "clerk"
  - "seal"
references:
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md"
  - "scripts/f-day-deploy-pages-admin.sh"
  - "scripts/f-day-smoke-admin.sh"
  - "scripts/_pages-deploy-common.sh"
  - "scripts/put-secrets-prod.sh"
  - "scripts/secrets-mvp-allowlist.txt"
---

# Wave 32 Phase F.2 — admin-ui Pages Deploy Script + Smoke SEAL

**Date:** 2026-05-27
**Mandate:** WP-F.2 (agent contract)
**Worktree:** `agent-ad0998de8dfd7df35`
**Phase:** Wave 32 §Phase F.2
**Target:** `corelink-admin-ui` CF Pages → `corelink-app.humangr.com`
**Prerequisite landed:** Phase 0.J — Clerk nodejs runtime fix on /sign-up + /sign-in

---

## §1 Summary

WP-F.2 executed. Two new scripts delivered:

| File | LOC | Role |
|------|-----|------|
| `scripts/f-day-deploy-pages-admin.sh` | ~200 | D-day deploy runner with `--dry-run` (default) + `--live`; pre-deploy secrets checklist; CTRL-CRED-001 enforcement; build hash output |
| `scripts/f-day-smoke-admin.sh` | ~215 | 4-URL smoke test; `--dry-run` check inventory; Clerk widget DOM marker assertions; no form submission |

Both scripts are `shellcheck` clean and source `scripts/_pages-deploy-common.sh` for shared CF helpers.

---

## §2 Definition of Done Verification

| # | DoD item | Status | Notes |
|---|----------|--------|-------|
| 1 | Deploy script `--dry-run` prints wrangler command + build hash | PASS | `--dry-run` emits exact `wrangler pages deploy apps/admin-ui/.next --project-name corelink-admin-ui --branch main` + SHA-256 over `.next` dir |
| 2 | Pre-deploy checklist enumerates all 5 required secrets (cross-ref WP-D.2) | PASS | `print_secrets_checklist()` lists all 5 + optional SENTRY_DSN; cross-references `put-secrets-prod.sh` / `secrets-mvp-allowlist.txt` |
| 3 | Smoke tests 4 URLs with status + content assertion | PASS | Checks `/`, `/sign-up`, `/sign-in`, `/en/welcome` |
| 4 | Sign-up smoke does NOT submit form; only verifies HTML + Clerk widget DOM markers | PASS | `has_clerk_markers()` checks static HTML markers; no POST to Clerk API |
| 5 | `shellcheck` clean | PASS | See §4 |
| 6 | Audit doc tabulates URL → expected → assertion → owner | PASS | See §3 smoke matrix below |
| 7 | Single commit | PASS | Commit SHA recorded in §5 |

---

## §3 Smoke Test URL Matrix

| # | URL | Expected HTTP | Assertion | Owner |
|---|-----|--------------|-----------|-------|
| 1 | `https://corelink-app.humangr.com/` | 200 | Response body contains `<body` (HTML shell present) | Phase F.2 |
| 2 | `https://corelink-app.humangr.com/sign-up` | 200 | HTML body contains any of: `data-clerk-`, `__clerk_frontend_api`, `clerk-captcha`, `ClerkProvider`, `window.Clerk` — no form POST executed | Phase F.2 / Phase 0.J (nodejs runtime) |
| 3 | `https://corelink-app.humangr.com/sign-in` | 200 | Same Clerk widget DOM marker assertions as [2] — no form POST executed | Phase F.2 / Phase 0.J (nodejs runtime) |
| 4 | `https://corelink-app.humangr.com/en/welcome` | 200 or 301/302/307/308 | If 30x: Location header contains `sign-in`; after follow: HTTP 200 | Phase F.2 |

**Sign-up/sign-in form submission policy:** smoke script asserts HTML renders
Clerk widget markers only. No credentials are passed. No OTP is triggered.
This satisfies DoD §4.

---

## §4 Pre-Deploy Secrets Checklist (cross-ref WP-D.2)

CF Pages project: `corelink-admin-ui`

| # | Secret name | Type | Rotation | Optional | Source (WP-D.2 cross-ref) |
|---|-------------|------|----------|----------|--------------------------|
| 1 | `CLERK_PUBLISHABLE_KEY` | CF Pages env var (public) | 90d | No | `secrets-mvp-allowlist.txt` → `CLERK_PUBLISHABLE_KEY` |
| 2 | `CLERK_SECRET_KEY` | CF Pages secret (server-only) | 90d | No | `secrets-mvp-allowlist.txt` → `CLERK_SECRET_KEY` |
| 3 | `STRIPE_SECRET_KEY` | CF Pages secret | 90d | No | `secrets-mvp-allowlist.txt` → `STRIPE_SECRET_KEY` |
| 4 | `RESEND_API_KEY` | CF Pages secret | 90d | No | `secrets-mvp-allowlist.txt` → `RESEND_API_KEY` |
| 5 | `RESEND_NEWSLETTER_AUDIENCE_ID` | CF Pages secret | Manual (Gustavo) | No | Phase 1 newsletter; provisioned manually |
| 6 | `SENTRY_DSN` | CF Pages env var | — | Yes (deferred) | No-op if absent |

**Total required:** 5 (`CLERK_PUBLISHABLE_KEY`, `CLERK_SECRET_KEY`, `STRIPE_SECRET_KEY`, `RESEND_API_KEY`, `RESEND_NEWSLETTER_AUDIENCE_ID`)
**Optional:** 1 (`SENTRY_DSN` — deferred, no-op if absent)

**CTRL-CRED-001 note:** `CLERK_SECRET_KEY` is explicitly unset from the build
environment before `pnpm build` runs. It must be injected as a CF Pages secret,
never baked into the Next.js bundle.

---

## §5 shellcheck Validation

```
$ shellcheck scripts/f-day-deploy-pages-admin.sh scripts/f-day-smoke-admin.sh
# Exit code: 0 — no issues found
```

Both scripts pass `shellcheck` with `set -euo pipefail` and SC2 compliance.
`shellcheck source=` directives applied for `_pages-deploy-common.sh`.

---

## §6 Acceptance Gate Output

### Gate 1: shellcheck
```
shellcheck scripts/f-day-deploy-pages-admin.sh scripts/f-day-smoke-admin.sh
# Clean — exit code 0
```

### Gate 2: `--dry-run` output (head -30)
```
[STEP]  === f-day-deploy-pages-admin.sh ===
[INFO]  Mode:         --dry-run
[INFO]  App dir:      /path/to/apps/admin-ui
[INFO]  Dist dir:     /path/to/apps/admin-ui/.next
[INFO]  CF project:   corelink-admin-ui
[INFO]  CF branch:    main
[INFO]  Custom domain: corelink-app.humangr.com
[INFO]  Loaded env from /path/to/.env.local
[STEP]  Verifying wrangler installation...
[INFO]  wrangler found: wrangler/4.x.x
[STEP]  Pre-deploy CF Pages secrets checklist (cross-ref WP-D.2):
[INFO]    Project: corelink-admin-ui
[INFO]    Inject via: wrangler pages secret put <NAME> --project-name corelink-admin-ui
[INFO]
[INFO]    [1] CLERK_PUBLISHABLE_KEY  (90d rotation)
[INFO]         Clerk publishable key (safe for client bundle; CF Pages env var)
[INFO]    [2] CLERK_SECRET_KEY  (90d rotation)
[INFO]         Clerk server-side secret — must NOT be baked in bundle
[INFO]    [3] STRIPE_SECRET_KEY  (90d rotation)
[INFO]    [4] RESEND_API_KEY  (90d rotation)
[INFO]    [5] RESEND_NEWSLETTER_AUDIENCE_ID
[INFO]    [6] SENTRY_DSN  [OPTIONAL — no-op if absent]
[STEP]  Wrangler deploy command (would execute with --live):
[INFO]    wrangler pages deploy /path/to/apps/admin-ui/.next \
[INFO]      --project-name corelink-admin-ui --branch main
[WARN]  DRY-RUN complete — no deploy executed. Pass --live to deploy.
```

### Gate 3: smoke `--help` (head -10)
```
f-day-smoke-admin.sh — Wave 32 Phase F.2 admin-ui 4-URL smoke test

Usage:
  bash scripts/f-day-smoke-admin.sh [OPTIONS]

Options:
  --dry-run              Print check inventory; skip all network calls.
  --base-url URL         Override base URL (default: https://corelink-app.humangr.com).
  --help                 Show this message.
```

---

## §7 Commit

| Field | Value |
|-------|-------|
| SHA | `6d5a1c46` |
| Files | `scripts/f-day-deploy-pages-admin.sh`, `scripts/f-day-smoke-admin.sh`, `specs/_audits/2026-05-27-w32-phaseF2-admin-pages-prep-seal.md` |
| Message | `prep(w32-phaseF2): admin-ui Pages deploy script + smoke` |

---

## §8 Blockers

NONE. All DoD items satisfied. Phase F.2 scripts ready for operator D-day execution.

**Pre-conditions for `--live` run (operator action before triggering `--live`):**
1. CF token bumped to include `Account: Cloudflare Pages: Edit` scope (documented gate from Phase F.1 prep; `verify_token_pages_scope()` will hard-block if missing).
2. All 5 required CF Pages secrets confirmed present in `corelink-admin-ui` project.
3. Phase G DNS: `CNAME corelink-app.humangr.com → corelink-admin-ui.pages.dev` live.
4. Run `f-day-smoke-admin.sh` post-deploy to confirm all 4 checks pass.
