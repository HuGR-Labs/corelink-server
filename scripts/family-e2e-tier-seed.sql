-- family-e2e per-tier seed fixture — corelink-server TL, 2026-06-16
-- For the clw ↔ corelink-server ↔ corelink-runners family-e2e (RELAY-family-e2e-to-server-TL.md).
--
-- Seeds ONE tenant per sellable tier so the per-tier matrix can run against
-- ACTIVE D1 gates (the container quota / byte-accounting / PAT gates return
-- None/fail-open WITHOUT these rows — clw cannot seed the server DB by design,
-- so this is the server-side fixture).
--
-- Apply to the TEST D1 (NOT prod):
--   wrangler d1 execute <TEST_DB> --file scripts/family-e2e-tier-seed.sql --remote
-- (use worker/node_modules/.bin/wrangler 4.x — the root npx wrangler 3.x dies on [[containers]]).
--
-- What the gates read (recon-confirmed):
--   * tier_selections    (0039 + 0062 6-tier expand) — getTierForTenant reads tier + subscription_state='active' FIRST
--     (worker/src/lib/quota.ts:135-189); the Worker maps the tier NAME → storage/request caps from its QUOTAS map
--     (quota.ts:84-93) — caps are NOT in D1, so seeding the tier name is sufficient for storage/request axes.
--   * runners_entitlement (0070) — per-tenant runner max_concurrency (SEPARATE axis from cache tier); introspect reads it.
--   * tenant_quota        (0066) — monthly $-ceiling in usd_micros; non-zero so the quota gate isn't a flat 402.
-- The $/request CAPS themselves are owner/runners-TL values; the numbers below are TEST fixtures (present + distinct
-- so clw can assert "the cap is read", not production economics).
--
-- Tenant ids are deterministic v4-shaped UUIDs (one per tier) so clw can hard-code them in the matrix.
-- created_at_ms / subscription_started_at_ms use a fixed launch-window epoch (2026-06-16T00:00:00Z = 1781568000000)
-- to keep the seed reproducible (no now()).

-- ── tenant parent rows (FK target) ───────────────────────────────────────────
-- `pat.tenant_id` and `tier_selections.tenant_id` both FK → tenant(tenant_id), so the
-- tenant rows MUST exist before the tier/quota/pat rows (an empty `tenant` table makes
-- every downstream INSERT fail the FK and write 0 rows — the exact ROUND-2 trap).
-- primary_region='wnam' (the default global CAS region); created/updated use the fixed epoch.
INSERT INTO tenant (tenant_id, primary_region, created_at_ms, updated_at_ms) VALUES
  ('00000000-0000-4000-8000-0000000f0001', 'wnam', 1781568000000, 1781568000000),
  ('00000000-0000-4000-8000-0000000f0002', 'wnam', 1781568000000, 1781568000000),
  ('00000000-0000-4000-8000-0000000f0003', 'wnam', 1781568000000, 1781568000000),
  ('00000000-0000-4000-8000-0000000f0004', 'wnam', 1781568000000, 1781568000000),
  ('00000000-0000-4000-8000-0000000f0005', 'wnam', 1781568000000, 1781568000000),
  ('00000000-0000-4000-8000-0000000f0006', 'wnam', 1781568000000, 1781568000000)
  ON CONFLICT(tenant_id) DO NOTHING;

-- ── free ─────────────────────────────────────────────────────────────────────
INSERT INTO tier_selections (tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0001', 'free', 'active', 1, 'family-e2e-seed-free', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state=excluded.subscription_state;
INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0001', 1, 'free', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, plan=excluded.plan;
INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
  VALUES ('00000000-0000-4000-8000-0000000f0001', 5000000)
  ON CONFLICT(tenant_id) DO UPDATE SET monthly_budget_usd_micros=excluded.monthly_budget_usd_micros;

-- ── solo ─────────────────────────────────────────────────────────────────────
INSERT INTO tier_selections (tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0002', 'solo', 'active', 1, 'family-e2e-seed-solo', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state=excluded.subscription_state;
INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0002', 2, 'solo', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, plan=excluded.plan;
INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
  VALUES ('00000000-0000-4000-8000-0000000f0002', 15000000)
  ON CONFLICT(tenant_id) DO UPDATE SET monthly_budget_usd_micros=excluded.monthly_budget_usd_micros;

-- ── starter ──────────────────────────────────────────────────────────────────
INSERT INTO tier_selections (tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0003', 'starter', 'active', 1, 'family-e2e-seed-starter', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state=excluded.subscription_state;
INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0003', 5, 'starter', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, plan=excluded.plan;
INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
  VALUES ('00000000-0000-4000-8000-0000000f0003', 35000000)
  ON CONFLICT(tenant_id) DO UPDATE SET monthly_budget_usd_micros=excluded.monthly_budget_usd_micros;

-- ── pro ──────────────────────────────────────────────────────────────────────
INSERT INTO tier_selections (tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0004', 'pro', 'active', 1, 'family-e2e-seed-pro', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state=excluded.subscription_state;
INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0004', 10, 'pro', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, plan=excluded.plan;
INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
  VALUES ('00000000-0000-4000-8000-0000000f0004', 50000000)
  ON CONFLICT(tenant_id) DO UPDATE SET monthly_budget_usd_micros=excluded.monthly_budget_usd_micros;

-- ── max ──────────────────────────────────────────────────────────────────────
INSERT INTO tier_selections (tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0005', 'max', 'active', 1, 'family-e2e-seed-max', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state=excluded.subscription_state;
INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0005', 25, 'max', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, plan=excluded.plan;
INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
  VALUES ('00000000-0000-4000-8000-0000000f0005', 149000000)
  ON CONFLICT(tenant_id) DO UPDATE SET monthly_budget_usd_micros=excluded.monthly_budget_usd_micros;

-- ── enterprise (unlimited tier — Worker maps to storageBytesMax=MAX_SAFE_INTEGER) ──
INSERT INTO tier_selections (tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0006', 'enterprise', 'active', 1, 'family-e2e-seed-ent', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state=excluded.subscription_state;
INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms)
  VALUES ('00000000-0000-4000-8000-0000000f0006', 100, 'enterprise', 1781568000000)
  ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, plan=excluded.plan;
INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
  VALUES ('00000000-0000-4000-8000-0000000f0006', 1000000000)
  ON CONFLICT(tenant_id) DO UPDATE SET monthly_budget_usd_micros=excluded.monthly_budget_usd_micros;

-- Verify:  SELECT t.tenant_id, t.tier, t.subscription_state, r.max_concurrency, q.monthly_budget_usd_micros
--          FROM tier_selections t LEFT JOIN runners_entitlement r USING(tenant_id)
--          LEFT JOIN tenant_quota q USING(tenant_id) WHERE t.correlation_id LIKE 'family-e2e-seed-%';
