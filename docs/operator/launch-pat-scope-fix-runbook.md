# Runbook — PAT scope vocabulary fix (launch-blocker) + owner-gated follow-ups

**Status:** code ready (this PR). Deploy + data + secret steps are **owner / deploy-session gated** — listed at the bottom.
**Discovered:** 2026-06-08, while prepping the task-#23 prod back-fill. Verified against live `corelink-prod-d1`.

## TL;DR

The D1 `pat.scope` CHECK constraint and the auth code disagreed on the scope
vocabulary. The constraint only allowed `('read-write','read-only','admin')`,
but both the code (`scope.rs`) and the production provisioning path
(`signup-worker/clerk.ts`) speak `('cas:rw','cas:r','cas:w','admin')`. **The
first real self-serve signup would have failed its PAT INSERT with a CHECK
violation** and the customer would never have received a token.

Migration `0061_pat_scope_colon_grammar.sql` corrects the constraint. Nothing
else in the code needed to change — the code was already right; the prod schema
was stale.

## Evidence (live prod, read-only, 2026-06-08)

| Probe | Result |
|---|---|
| `SELECT scope, count(*) FROM pat GROUP BY scope` | **25 rows, all `scope='admin'`** (legacy; `cas:rw` path never wrote) |
| live `pat` DDL CHECK | `CHECK (scope IN ('read-write','read-only','admin'))` |
| any table FK-referencing `pat` | none (DROP-rebuild is dependency-safe) |
| d1_migrations ledger head | `0056` applied → **0057–0061 pending** on prod |

Local proof (python3 sqlite3, exact prod DDL + 25 seeded admin rows):
`INSERT scope='cas:rw'` → **REJECTED** before 0061; after 0061 the 25 rows + 3
named indexes are preserved, `cas:rw/cas:r/cas:w/admin` are accepted, an unknown
scope is still rejected, and the `admin→cas:rw` back-fill UPDATE becomes valid.

## Deploy order (deploy-session / owner)

1. **Apply pending D1 migrations to prod** — applies 0057–0061 in order:
   ```
   bash scripts/apply-d1-migrations-prod.sh        # or scripts/d1-migration-runner.sh --env prod
   ```
   0061 rebuilds `pat` with the corrected CHECK. Idempotent via the d1_migrations ledger.
2. **(Owner-gated) Least-privilege back-fill** of the 25 legacy admin tokens —
   ONLY valid after step 1:
   ```
   bash scripts/backfill-admin-scope-prod.sh          # dry-run: shows COUNT
   bash scripts/backfill-admin-scope-prod.sh --apply  # UPDATE admin → cas:rw
   ```
   Optional/safe: `admin` already grants cache rw (superset in `scope.rs`), so
   the 25 tokens keep working either way. The back-fill is about least
   privilege, not correctness — owner's call on real customer tokens.

## Owner-gated items still pending (task #23)

- **`CLERK_JWT_ISSUER` secret** — NOTE: task #23 called it `CLERK_ISSUER_URL`;
  the code actually reads **`CLERK_JWT_ISSUER`** (`corelink-clerk/src/env_config.rs`).
  It is **optional**: when unset the issuer is derived from the Clerk
  publishable key (`https://{frontend_api}`). Setting it explicitly *pins* the
  issuer (and enables a multi-issuer allowlist via comma-separation). The value
  already exists in `.env.local`. Pin-vs-derive is an owner decision; if pinning:
  ```
  printf '%s' "$CLERK_JWT_ISSUER" | wrangler secret put CLERK_JWT_ISSUER --env prod
  ```
  (It is NOT yet in the secrets allowlist/matrix — adding it there is part of
  the pin decision, not done here to avoid a self-authored loosening of the
  gated secrets matrix.)
- **Remove the stale work clone** (~600 MB, destructive — guard blocks the agent):
  ```
  rm -rf ~/Documents/HuGR/corelink-secfix
  ```

## What changed in this PR (all reversible, no prod writes)

- `migrations/d1/0061_pat_scope_colon_grammar.sql` — the CHECK-vocabulary fix.
- `scripts/backfill-admin-scope-prod.sh` — gated, read-only-by-default back-fill.
- this runbook + CHANGELOG entry.
