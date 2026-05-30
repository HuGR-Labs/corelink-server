# Tier 3 GTM Wave — Deploy Plan

> Tag: `tier3-gtm-wave-sealed-2026-05-30` at `b59c4862`. 41 commits ahead of
> `origin/main` (38 from the 18-agent wave + 3 cherry-picks from prior-session
> branches). Repo state impeccable: only `main` + harness-locked worktrees
> remain locally; all merged branches deleted.

## What's NOT yet deployed (the actual side-effecting work)

| Change | Surface | Risk |
|---|---|---|
| `/_health` reports storage backing (A4) | main worker container | low — additive field in JSON |
| `/v1/audit/:tenant/export` route fix (A3) | main worker container | low — adds path segment, no consumer change |
| `(authenticated)/layout.tsx` welcome group (A1) | admin-ui Worker | medium — moves files; rebuild required |
| Public `/pricing` page (B1) | admin-ui Worker | low — new public route |
| Customer dashboard MVP (B5) | admin-ui Worker | low — pages were already wired |
| PatRevealCard + signup forceRedirect (B4) | admin-ui Worker | low — new component + Clerk option |
| **Per-tier quota enforcement (B2)** | main worker + D1 migration 0057 | **medium-high — runs on every authed request** |
| `BILLING_DB` alias binding (cherry-pick) | signup-worker | low — adds binding only |
| Smoke script fixes (A2 + C10) | operational | none — local script |
| Docs/blog/examples (C1/C2/C3/C6/C7/C8/C9 + legacy ops docs) | none until docs site rebuild | none — static |

## Mandatory pre-deploy: D1 migration 0057

`migrations/d1/0057_tenant_tier.sql` adds `tenant.tier` column (default
`free`). **Must apply BEFORE deploying main worker** (B2 quota path reads
the column; without it the worker fails-open per the test design, but the
explicit ALTER is the cleaner sequence).

```sh
worker/node_modules/.bin/wrangler d1 execute CONFIG_DB \
  --env prod --remote \
  --file=migrations/d1/0057_tenant_tier.sql
```

Idempotency check first (SQLite can't `IF NOT EXISTS` on ADD COLUMN):

```sh
worker/node_modules/.bin/wrangler d1 execute CONFIG_DB --env prod --remote \
  --command "SELECT name FROM pragma_table_info('tenant') WHERE name='tier'"
```

If empty → migration safe; if returns `tier` → skip the migration (already applied).

## Deploy order (sequential, mandatory)

1. **D1 migration 0057** (above) — fast, reversible (ALTER TABLE ... DROP COLUMN if needed)
2. **Main worker** (`wrangler deploy --env prod`) — picks up:
   - `/_health` storage field (A4)
   - `/v1/audit/:tenant/export` route fix (A3)
   - Per-tier quota enforcement (B2)
3. **Smoke against prod** with `CORELINK_SMOKE_TOKEN` — verify quota path doesn't break existing PATs
4. **Admin-ui Worker** (`wrangler deploy` from `apps/admin-ui/`) — picks up:
   - `/pricing` route
   - `(authenticated)/welcome` move
   - PatRevealCard + customer dashboard
5. **Signup-worker** (`wrangler deploy --config apps/signup-worker/wrangler.toml`) — picks up:
   - `BILLING_DB` alias binding (stripe webhook fix from legacy cherry-pick)
6. **Re-run smoke** + **manual sign-up E2E** (#369) + **Stripe checkout E2E** (#370)

## Rollback

Each Worker can be rolled back via `wrangler rollback <version-id>`:
- Main worker prev: `33d61b18` (pre-wave)
- Admin-ui Worker prev: pre-wave deploy
- Signup-worker prev: `c0e27681`

D1 migration rollback:
```sh
worker/node_modules/.bin/wrangler d1 execute CONFIG_DB --env prod --remote \
  --command "ALTER TABLE tenant DROP COLUMN tier"
```
(SQLite ≥ 3.35 supports DROP COLUMN. D1 runs SQLite ≥ 3.37.)

## Why this is NOT auto-deployed

- B2's quota path runs on every authed request. A typo in the SQL or a
  D1 outage would error every CAS PUT/GET. Needs verified migration + smoke
  before customer traffic hits it.
- Admin-ui has had 2 deploy iterations this session (Pages → Worker + post-OpenNext
  Clerk fix). Another deploy must coincide with someone who can browser-test
  /sign-in, /sign-up, /pricing, /en/welcome rendering.
- Signup-worker change is benign (new binding) but BILLING_DB binding will
  cause stripe.ts to attempt writes — the migration of `tenant_billing` table
  must already be applied (per `02df9a17` commit body that mentions
  migrations 0055+0056 already applied; verify before deploy).

## Pre-deploy checklist

- [ ] `git push origin main --follow-tags` succeeds
- [ ] `bash scripts/smoke-prod-corelink.sh` (without token) ≤ 3 failures (current baseline)
- [ ] `wrangler d1 execute CONFIG_DB --env prod --remote --command "SELECT name FROM sqlite_master WHERE name IN ('tenant_billing','tier_selections','pat','tenant')"` returns 4 rows
- [ ] Apply migration 0057 (above)
- [ ] Verify `SELECT name FROM pragma_table_info('tenant') WHERE name='tier'` returns `tier`
- [ ] Operator picks a low-traffic window
- [ ] Operator confirms ready

## After deploy

- [ ] Re-run smoke with `CORELINK_SMOKE_TOKEN` set; ≤ 2 failures (external status pages only)
- [ ] Curl `/_health` and confirm `"storage":"r2"` field present
- [ ] Curl `/v1/audit/<tenant>/export?from_ms=0&to_ms=N` with PAT and confirm non-404
- [ ] Browser: load `/en/pricing` and verify 5 cards render
- [ ] Browser: sign up new Clerk user; verify post-signup redirect to `/welcome` with PatRevealCard
- [ ] D1 check: new tenant row has `tier='free'`
- [ ] Trigger a $1 Stripe Checkout in test mode; verify `tenant_billing` row appears via BILLING_DB binding
- [ ] Update memory: tier3-gtm-wave-sealed → mark as DEPLOYED with timestamp
