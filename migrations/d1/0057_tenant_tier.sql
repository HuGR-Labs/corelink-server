-- CoreLink D1 (Cloudflare SQLite) — migration 0057 for tenant.tier column.
--
-- Adds a `tier` column to the `tenant` table so the Worker can enforce
-- per-tier storage and request quotas at the edge (before forwarding to the
-- Durable Object). The canonical tier taxonomy matches `tier_selections.tier`
-- (migration 0039) plus the `solo` alias that was renamed to `starter` in
-- S-19; the Worker quota module maps `starter → solo` quota class.
--
-- Canonical sources:
--   - worker/src/lib/quota.ts (quota definitions)
--   - migrations/d1/0039_tier_selection.sql (tier_selections table)
--   - docs/POSITIONING.md (tier pricing table)
--   - apps/admin-ui/src/lib/pricing.ts (matching quota constants)
--
-- Design notes:
--   - DEFAULT 'free' ensures every existing tenant row gets the most
--     restrictive tier on upgrade (zero risk of accidental quota bypass).
--   - The Worker quota check reads tier_selections.tier (per-tenant actual)
--     first, falling back to tenant.tier (default 'free') when no
--     tier_selections row exists (e.g. newly provisioned tenants).
--   - The CHECK constraint mirrors tier_selections.tier's canonical values
--     plus 'solo' (legacy alias, kept for backward compatibility with any
--     admin tooling that writes it directly).
--   - SQLite/D1 does NOT support ADD CONSTRAINT after CREATE TABLE; the
--     CHECK is inlined per ADR-0036 Rule 1. ALTER TABLE ADD COLUMN with
--     a CHECK constraint is supported in D1 (SQLite ≥ 3.37.0).
--
-- Invariants:
--   - INV-TENANT-ISOLATION (CRITICAL): tenant_id is the PK; the added
--     column carries no cross-tenant data.
--   - Backward compatible: existing rows get DEFAULT 'free'; no data loss.
--
-- Migration runner: see scripts/migrate_d1.sh.
-- Idempotency: SQLite does not support `ADD COLUMN IF NOT EXISTS`, so this
-- migration is guarded at the application layer by the migration runner's
-- sequential numbering (0057 runs exactly once).

ALTER TABLE tenant
  ADD COLUMN tier TEXT NOT NULL DEFAULT 'free'
    CHECK (tier IN ('free', 'solo', 'starter', 'team', 'pro', 'org', 'enterprise'));
