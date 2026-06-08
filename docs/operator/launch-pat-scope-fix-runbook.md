# Runbook — PAT scope launch-blocker fix (additive) + owner-gated follow-ups

**Status:** code fix in this PR (no migration, no ADR). Data + secret + clone steps
are **owner-gated** — listed at the bottom.
**Discovered:** 2026-06-08, prepping the task-#23 prod back-fill. Verified against live
`corelink-prod-d1`.

## TL;DR

The D1 `pat.scope` column is constrained to `('read-write','read-only','admin')`
(migration 0037), but the production provisioning path (`signup-worker/clerk.ts`) was
writing `scope='cas:rw'`. **The first real self-serve signup would have failed its PAT
INSERT with a CHECK violation** and the customer would never have received a token.

Because auth migrations are **additive-only** (CI gate `check_migrations_additive.py`,
INV-AUTH-MIGRATION-ADDITIVE, HIGH — no destructive `pat` rebuild without an owner ADR),
the fix is **code-side, not schema-side** (owner chose this, "Option A"):

- `signup-worker/clerk.ts` now provisions `scope='read-write'` (CHECK-valid, **non-admin**
  → least privilege) instead of `cas:rw`.
- `scope.rs` `requires_cache_read/write` now additively accept `read-write` (→ rw) and
  `read-only` (→ read), alongside the existing `cas:rw|cas:r|cas:w|admin`. Every auth
  surface routes through these two functions (the unified `verify_capability` used by the
  Worker, OCI `/token` downscope, and all five adapters), so one change covers everything.
- The mint route (`internal_pat.rs`) already mapped `read-write`→cache-rw, and the signed
  PAT bitset is unchanged — no schema, no migration.

## Evidence (live prod, read-only, 2026-06-08)

| Probe | Result |
|---|---|
| `SELECT scope, count(*) FROM pat GROUP BY scope` | **25 rows, all `scope='admin'`** (legacy) |
| live `pat` DDL CHECK | `CHECK (scope IN ('read-write','read-only','admin'))` |

## Deploy (deploy-session / owner)

1. **Deploy the container + signup-worker** carrying this code. No DB migration is part of
   this fix. After deploy, a real signup persists `scope='read-write'`, which the CHECK
   accepts and `scope.rs` authorizes for cache rw.

## Owner-gated follow-ups (task #23)

- **(Optional) Least-privilege back-fill** of the 25 legacy admin tokens → `read-write`
  (no migration; `read-write` is already CHECK-valid):
  ```
  bash scripts/backfill-admin-scope-prod.sh          # dry-run: shows COUNT
  bash scripts/backfill-admin-scope-prod.sh --apply  # UPDATE admin → read-write
  ```
  Safe either way: `admin` already grants cache rw (superset in `scope.rs`); the back-fill
  just removes the over-privilege.
- **`CLERK_JWT_ISSUER` secret** — NOTE: task #23 called it `CLERK_ISSUER_URL`; the code
  reads **`CLERK_JWT_ISSUER`** (`corelink-clerk/src/env_config.rs`). Optional: unset ⇒ the
  issuer derives from the Clerk publishable key. Pinning (owner's call):
  ```
  printf '%s' "$CLERK_JWT_ISSUER" | wrangler secret put CLERK_JWT_ISSUER --env prod
  ```
- **Remove the stale work clone** (~600 MB; destructive — guard blocks the agent):
  ```
  rm -rf ~/Documents/HuGR/corelink-secfix
  ```

## What changed in this PR (no prod writes, no migration)

- `crates/corelink-container/src/scope.rs` — accept `read-write`/`read-only` (+ tests).
- `apps/signup-worker/src/webhooks/clerk.ts` (+ its tests, + the type doc) — provision
  `read-write`.
- `scripts/backfill-admin-scope-prod.sh` — gated, read-only-by-default back-fill (read-write).
- this runbook + CHANGELOG entry.
