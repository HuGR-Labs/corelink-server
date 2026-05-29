-- Migration 0055: Add clerk_user_id lookup column to the D1 `tenant` table (Stream-5).
--
-- The Stream-5 Clerk webhook auto-provision flow (signup-worker `user.created`
-- handler) writes the Clerk user id into `tenant.clerk_user_id` at provision
-- time. This column is the idempotency anchor — on a webhook retry, the
-- handler looks up the tenant by `clerk_user_id` and returns the cached
-- result without re-provisioning.
--
-- Column contract:
--   clerk_user_id  TEXT  — Clerk user id (e.g. "user_2abc..."). Nullable so
--   existing tenant rows (provisioned before this migration) are not
--   invalidated; the NOT NULL constraint is enforced at the application
--   layer (the signup handler always supplies this value for new tenants).
--
-- Uniqueness: UNIQUE INDEX (partial, WHERE clerk_user_id IS NOT NULL) —
-- same SQLite-idiomatic pattern as idx_tenant_signup_id_unique (migration
-- 0037). SQLite / D1 do not support ADD COLUMN ... UNIQUE inline.
--
-- D1 note: D1/SQLite does not support `ALTER TABLE ADD COLUMN IF NOT EXISTS`,
-- so this is a plain ADD COLUMN statement. Safe on any DB where the column
-- does not yet exist; idempotent in the sense that re-running on a DB
-- that already has the column will fail with "duplicate column name" —
-- wrangler migrations gate on the applied-migration ledger prevents
-- double-apply.
--
-- Additive policy (INV-ONBOARD-ATOMIC-PROVISIONING): no DROP, no destructive
-- ALTER. Pure ADD COLUMN migration.

ALTER TABLE tenant ADD COLUMN clerk_user_id TEXT;

-- Enforce uniqueness via a UNIQUE INDEX (the SQLite-idiomatic equivalent
-- of inline UNIQUE on an ALTER ADD COLUMN). Also serves as the covering
-- index for the webhook idempotency hot-path query:
-- `SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1`.
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_clerk_user_id
    ON tenant (clerk_user_id)
    WHERE clerk_user_id IS NOT NULL;
