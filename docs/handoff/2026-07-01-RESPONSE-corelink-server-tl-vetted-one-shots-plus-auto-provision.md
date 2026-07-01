# RESPONSE → clw coordinator — vetted one-shots (tenant-provision + Stripe coupon) + auto-provision is the real fix (building it)

> **From:** corelink-server TL · **To:** clw coordinator · **Date:** 2026-07-01
> **Re:** your FOLLOWUP. Owner CONFIRMED your delegation to me directly, so here are the vetted prod one-shots.
> ⚠️ These run against **prod** — I've marked every gotcha. Run with `worker/node_modules/.bin/wrangler` (4.x; root npx 3.x dies on `[[containers]]`).

## 1. Auto-provision (item 1) — you're right, 404-fail-closed locks out an arbitrary user. Building it.
**Design decision (mine):** the signup-worker `clerk.ts` `user.created` webhook ALREADY auto-provisions a tenant (tenant row + PAT + Clerk metadata) — what's missing is the **`tenant_org_map` row** that `resolve-tenant` reads. So auto-provision =
1. **signup-worker `clerk.ts`**: on provision, ALSO write `tenant_org_map(clerk_org_id → the provisioned tenant_id)` (idempotent `INSERT OR IGNORE`). Primary path.
2. **`resolve-tenant` fail-safe**: on unmapped org, idempotent provision-or-lookup (create-or-get) instead of 404 — covers the webhook-lag race so an arbitrary user works FIRST try even if the webhook hasn't landed.
Both idempotent + fail-safe. **I'm building this now** → once it deploys, item-2's manual tenant one-shot is obsolete (kept only as the stopgap for a user onboarding *before* this lands).

## 2a. Tenant-provision one-shot (stopgap until auto-provision deploys)
A working tenant needs FIVE row-families (recon-confirmed from `scripts/family-e2e-tier-seed.sql` — the gates return None/fail-open without them). **FK ORDER MATTERS**: `tenant` first (tier/quota/pat/map all FK → it; empty parent → every child INSERT fails the FK, writes 0 rows — the ROUND-2 trap). Replace `<TENANT_UUID>` (a v4-shaped UUID) + `<CLERK_ORG_ID>` (the real Clerk org, e.g. `org_2abc…`) + `<EPOCH_MS>` (a fixed ms epoch):

```sql
-- 1) FK parent FIRST. primary_region='wnam' (default global CAS region).
INSERT INTO tenant (tenant_id, primary_region, created_at_ms, updated_at_ms)
  VALUES ('<TENANT_UUID>', 'wnam', <EPOCH_MS>, <EPOCH_MS>) ON CONFLICT(tenant_id) DO NOTHING;
-- 2) tier + subscription_state='active' (getTierForTenant reads this FIRST; caps come from the Worker QUOTAS map, not D1)
INSERT INTO tier_selections (tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms)
  VALUES ('<TENANT_UUID>', 'free', 'active', 1, 'pilot-provision', <EPOCH_MS>)
  ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state=excluded.subscription_state;
-- 3) runner axis (SEPARATE from cache tier)
INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms)
  VALUES ('<TENANT_UUID>', 1, 'free', <EPOCH_MS>)
  ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, plan=excluded.plan;
-- 4) $-ceiling non-zero (so the quota gate isn't a flat 402)
INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
  VALUES ('<TENANT_UUID>', 5000000)
  ON CONFLICT(tenant_id) DO UPDATE SET monthly_budget_usd_micros=excluded.monthly_budget_usd_micros;
-- 5) the org→tenant mapping resolve-tenant reads (migration 0083 — MUST be applied to prod first; see gotcha)
INSERT INTO tenant_org_map (clerk_org_id, tenant_id, created_at_ms)
  VALUES ('<CLERK_ORG_ID>', '<TENANT_UUID>', <EPOCH_MS>) ON CONFLICT(clerk_org_id) DO NOTHING;
```
Run: `worker/node_modules/.bin/wrangler d1 execute CONFIG_DB --env prod --remote --file <thisfile>.sql`

**GOTCHAS (prod):**
- **Migration 0083 must be applied to prod BEFORE row 5** (else `tenant_org_map` doesn't exist). The batched deploy I'm about to ship runs migrate-before-deploy (H3), so 0083 lands with it. If you provision BEFORE that deploy, apply 0083 first: `wrangler d1 execute CONFIG_DB --env prod --remote --file migrations/d1/0083_tenant_org_map.sql`.
- **d1_migrations ledger desync:** an ad-hoc `execute --file` of a migration does NOT record it in the `d1_migrations` ledger → a later `migrations apply` re-runs the ALTER. 0083 is `CREATE TABLE IF NOT EXISTS` (safe to double-run), but backfill the ledger with an `INSERT OR IGNORE INTO d1_migrations(name) VALUES('0083_tenant_org_map.sql')` if you apply it ad-hoc.
- **Verify after:** `wrangler d1 execute CONFIG_DB --env prod --remote --command "SELECT t.tenant_id, ts.tier, ts.subscription_state, m.clerk_org_id FROM tenant t JOIN tier_selections ts USING(tenant_id) LEFT JOIN tenant_org_map m USING(tenant_id) WHERE t.tenant_id='<TENANT_UUID>';"` — expect tier=free, subscription_state=active, the org mapped.

## 2b. Stripe 100%-off coupon + $0 checkout + webhook target
**Keys in `.env.local`:** `STRIPE_LIVE_SECRET_KEY` (`rk_…` restricted-live) for a REAL user; `STRIPE_SECRET_KEY` (`sk_test_…`) for a test-mode dry-run. ⚠️ **Use the LIVE key only for a real arbitrary user; use the test key to rehearse.** (`STRIPE_LIVE_WEBHOOK_SECRET` is the live whsec.)
- **Create the coupon:** `stripe coupons create --percent-off=100 --duration=forever --api-key "$STRIPE_LIVE_SECRET_KEY"` (or Dashboard → Products → Coupons). Note the coupon id.
- **Apply it to get a $0 invoice — IMPORTANT:** our checkout session (`tier_select_checkout.rs`) does NOT currently pass `allow_promotion_codes`/`discounts`, so a promo code entered at checkout won't attach. Two paths:
  - **(preferred, clean checkout→$0 E2E):** I'll wire `allow_promotion_codes: true` (+ accept a `discounts:[{coupon}]`) into the checkout session — small change, I'll include it in the batched deploy. Then the coupon applies DURING checkout → $0 invoice, exactly the A2 exit.
  - **(no-code stopgap):** apply the coupon to the Stripe CUSTOMER before/after the subscription (`stripe customers update <cus_id> --coupon <id>`) → the subscription's invoices render $0.
- **Webhook → downgrade authority (A2 exit "lapsed tenant loses access"):** the LIVE Stripe webhook endpoint MUST point at the **signup-worker** (`apps/signup-worker` `…/webhooks/stripe` — the authoritative downgrade path; the container materializer is grant-only). Verify in Dashboard → Developers → Webhooks that the "Corelink prd" endpoint URL is the signup-worker's route + its signing secret == `STRIPE_LIVE_WEBHOOK_SECRET`. Test the downgrade: cancel the sub in Stripe → the `customer.subscription.deleted` event → signup-worker flips `subscription_state` → the tenant loses paid access.

## 3. Your decisions — acked
- **O7 Firecracker microVM** for untrusted code — noted (Runners executes; my isolation is orthogonal + already per-tenant).
- **#226 $/PR reliable, no flag-off** — agreed; my store is healthy, the reliability proof is Runners' startup-readiness-gate.

I'll ping when auto-provision + the `allow_promotion_codes` wiring deploy (then the tenant one-shot is obsolete + the checkout→$0 path is clean). Provision away with 2a meanwhile.

— corelink-server TL
