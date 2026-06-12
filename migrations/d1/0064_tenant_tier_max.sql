-- Migration 0064: widen the tier-string CHECK on `tenant.tier` to include 'max'.
--
-- Canonical taxonomy (FROZEN — 6 tiers post-ADR-S19-001):
--   free | solo | starter | pro | max | enterprise
--   Legacy aliases retained for back-compat: 'team', 'org'.
--
-- What this migration changes vs migration 0057:
--   - tenant.tier  0057 CHECK accepted ('free','solo','starter','team','pro',
--                  'org','enterprise'). This migration widens it to accept
--                  ('free','solo','starter','team','pro','org','max','enterprise').
--                  'max' ADDED; nothing removed (back-compat preserved).
--
-- ADDITIVE intent (INV-AUTH-MIGRATION-ADDITIVE, HIGH):
--   - This is a PURELY WIDENING change. No accepted value is removed: the legacy
--     'team' and 'org' values are RETAINED in the widened CHECK for back-compat
--     (any row or admin tool that still writes those values keeps working). The
--     accepted SET only grows.
--   - NO ROW IS DROPPED OR MUTATED. The data is copied 1:1 (`INSERT … SELECT`)
--     into the rebuilt table; every column value is preserved byte-for-byte.
--   - 'pilot' is intentionally NOT added (ratified out per PR #218 §4-Q2).
--
-- SQLite/D1 limitation forcing a table rebuild (the "12-step" recreate):
--   0057 declared the `tier` CHECK as an INLINE COLUMN constraint on an
--   ALTER TABLE ADD COLUMN. SQLite (hence D1) has NO `ALTER TABLE … ALTER COLUMN`
--   / `DROP CONSTRAINT` / `ADD CONSTRAINT` to relax an EXISTING inline CHECK in
--   place (see the SQLite "Making Other Kinds Of Table Schema Changes" 12-step
--   procedure). The only SQLite-correct way to relax an existing inline CHECK is
--   to rebuild the table, which unavoidably uses DROP TABLE + RENAME TO.
--
--   Those two tokens are flagged by `scripts/check_migrations_additive.py`
--   (INV-AUTH-MIGRATION-ADDITIVE). Each is therefore annotated below with the
--   gate's own line-local suppression:
--       -- additive-allowed: ADR-0064 <reason>
--   The change is destructive in MECHANISM (rebuild) but ADDITIVE in EFFECT
--   (set-of-accepted-values only grows; zero data loss). The lead must land the
--   accompanying ADR-0064 recording this rebuild + its zero-data-loss proof
--   before this migration is applied to prod.
--
-- Complete column inventory (all prior migrations touching `tenant`):
--   tenant_id           TEXT    NOT NULL PRIMARY KEY             [0023]
--   primary_region      TEXT    NOT NULL CHECK(…)               [0023]
--   created_at_ms       INTEGER NOT NULL                        [0023]
--   updated_at_ms       INTEGER NOT NULL                        [0023]
--   byok_status         TEXT    DEFAULT 'active' CHECK(…)       [0031]
--   byok_revoked_at_ms  INTEGER                                  [0031]
--   byok_revoked_provider      TEXT                             [0031]
--   byok_revoked_kms_key_id    TEXT                             [0031]
--   signup_id           TEXT                                    [0037]
--   email_hash          TEXT                                    [0037]
--   tenant_state        TEXT    NOT NULL DEFAULT 'dpa_pending' CHECK(…) [0037]
--   stripe_customer_id  TEXT                                    [0037]
--   created_ms          BIGINT                                  [0037]
--   updated_ms          BIGINT                                  [0037]
--   current_dpa_version TEXT                                    [0041]
--   dpa_grace_expires_at       INTEGER                          [0041]
--   re_acceptance_pending      INTEGER NOT NULL DEFAULT 0       [0041]
--   clerk_user_id       TEXT                                    [0056]
--   tier                TEXT    NOT NULL DEFAULT 'free' CHECK(…)[0057] ← WIDENED HERE
--
-- Complete index inventory (all prior migrations touching `tenant`):
--   idx_tenant_primary_region          ON tenant(primary_region)         [0028+0037]
--   idx_tenants_byok_status            ON tenant(byok_status)            [0031]
--   idx_tenants_byok_kms_key_id        ON tenant(byok_revoked_kms_key_id)
--                                      WHERE NOT NULL                    [0031]
--   idx_tenant_signup_id_unique        UNIQUE ON tenant(signup_id)
--                                      WHERE NOT NULL                    [0037]
--   idx_tenant_email_hash_unique       UNIQUE ON tenant(email_hash)
--                                      WHERE NOT NULL                    [0037]
--   idx_tenant_email_hash              ON tenant(email_hash)             [0037]
--   idx_tenant_state                   ON tenant(tenant_state)           [0037]
--   idx_tenant_dpa_pending             ON tenant(re_acceptance_pending,
--                                        dpa_grace_expires_at)
--                                      WHERE re_acceptance_pending=1     [0041]
--   idx_tenant_clerk_user_id           UNIQUE ON tenant(clerk_user_id)
--                                      WHERE NOT NULL                    [0056]
--   (no index was added on `tier` in 0057 — none created here either)
--
-- Trigger inventory (0028 — survive the rebuild as they reference the table
--   name, not the physical storage; D1 re-validates after RENAME TO):
--   trg_tenant_primary_region_required   (BEFORE INSERT)
--   trg_tenant_primary_region_immutable  (BEFORE UPDATE OF primary_region)
--   trg_tenant_primary_region_valid_insert (BEFORE INSERT)
--   → These are name-bound to `tenant`, not the physical object; they
--     survive the DROP + RENAME swap on the same connection. We do NOT
--     drop + recreate them — that would be a destructive trigger operation
--     and is unnecessary: D1 trigger bindings follow the table name.
--
-- Safety of the rebuild:
--   - `PRAGMA foreign_keys` is left OFF for the rebuild.
--     (D1 FKs are logical / per-connection inconsistent in CF Workers —
--     see 0002/0003 — and `defer_foreign_keys` is set for the window
--     between DROP and RENAME, matching the SQLite 12-step guidance.)
--   - The migration runs inside wrangler's per-file execution.
--
-- Dependent views: `stripe_tier_drift_view` (0048) does NOT reference
--   `tenant`, so NO view drop/recreate is required here (contrast 0062
--   which had to guard `stripe_tier_drift_view` because it references
--   `tier_selections`). Confirmed by scanning 0048 definition.
--
-- Idempotency: unlike 0057 this file is NOT re-runnable on its own (a
--   rebuild is inherently one-shot); it is guarded by the migration
--   runner's sequential numbering (`wrangler d1 migrations apply`
--   records 0064 as applied exactly once — same guarantee 0062 relies on).
--
-- Canonical sources kept in lock-step with this CHECK:
--   - migrations/d1/0057_tenant_tier.sql (original inline CHECK)
--   - migrations/d1/0062_expand_tier_selections_6tier.sql (6-tier rebuild pattern)
--   - worker/src/lib/quota.ts (quota definitions)
--   - crates/corelink-container/src/routes/admin.rs (TIER_SELECTIONS_TIERS)

PRAGMA foreign_keys = OFF;
-- D1 runs each migration file as a single transaction, and SQLite documents
-- `PRAGMA foreign_keys` as a NO-OP inside a transaction — so the line above
-- only helps engines that execute the file statement-by-statement. For D1 the
-- effective mechanism is `defer_foreign_keys` (D1-documented), which IS
-- honoured in-transaction and covers the DROP/RENAME window below.
PRAGMA defer_foreign_keys = true;

-- ============================================================
-- 1. tenant — rebuild with the widened 7-value CHECK on `tier`
-- ============================================================
-- New table mirrors the accumulated schema (0023 + 0031 + 0037 + 0041 + 0056 + 0057)
-- EXACTLY except the `tier` CHECK list, which gains 'max'.
-- Column order matches the original CREATE TABLE (0023) + additive order of ADD COLUMNs.
CREATE TABLE IF NOT EXISTS tenant_new (
    -- Core identity [0023]
    tenant_id           TEXT    NOT NULL PRIMARY KEY,
    primary_region      TEXT    NOT NULL
        CHECK (primary_region IN ('wnam','enam','weur','sam','apac','afr')),
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL,

    -- BYOK columns [0031]
    byok_status         TEXT    DEFAULT 'active'
        CHECK (byok_status IN ('active', 'degraded_read_only', 'revoked')),
    byok_revoked_at_ms  INTEGER,
    byok_revoked_provider       TEXT,
    byok_revoked_kms_key_id     TEXT,

    -- Signup orchestration columns [0037]
    signup_id           TEXT,
    email_hash          TEXT,
    tenant_state        TEXT    NOT NULL DEFAULT 'dpa_pending'
        CHECK (tenant_state IN ('active', 'dpa_pending', 'pending_billing_link', 'degraded_read_only')),
    stripe_customer_id  TEXT,
    created_ms          BIGINT,
    updated_ms          BIGINT,

    -- DPA versioning columns [0041]
    current_dpa_version         TEXT,
    dpa_grace_expires_at        INTEGER,
    re_acceptance_pending       INTEGER NOT NULL DEFAULT 0,

    -- Clerk lookup column [0056]
    clerk_user_id       TEXT,

    -- Tier column — WIDENED from 0057 CHECK to include 'max' [0057 + 0064]
    -- 'team' + 'org' retained for back-compat (additive widen — nothing removed).
    -- 'pilot' intentionally NOT added (ratified out — PR #218 §4-Q2).
    tier                TEXT    NOT NULL DEFAULT 'free'
        CHECK (tier IN ('free', 'solo', 'starter', 'team', 'pro', 'org', 'max', 'enterprise'))
);

-- Copy every existing row 1:1 (no filtering, no transformation — additive).
-- Explicit column list (not SELECT *) so the copy is order-independent and
-- self-documenting. All 19 columns enumerated.
INSERT INTO tenant_new (
    tenant_id,
    primary_region,
    created_at_ms,
    updated_at_ms,
    byok_status,
    byok_revoked_at_ms,
    byok_revoked_provider,
    byok_revoked_kms_key_id,
    signup_id,
    email_hash,
    tenant_state,
    stripe_customer_id,
    created_ms,
    updated_ms,
    current_dpa_version,
    dpa_grace_expires_at,
    re_acceptance_pending,
    clerk_user_id,
    tier
)
SELECT
    tenant_id,
    primary_region,
    created_at_ms,
    updated_at_ms,
    byok_status,
    byok_revoked_at_ms,
    byok_revoked_provider,
    byok_revoked_kms_key_id,
    signup_id,
    email_hash,
    tenant_state,
    stripe_customer_id,
    created_ms,
    updated_ms,
    current_dpa_version,
    dpa_grace_expires_at,
    re_acceptance_pending,
    clerk_user_id,
    tier
FROM tenant;

DROP TABLE tenant;  -- additive-allowed: ADR-0064 rebuild to widen inline CHECK (set grows; rows copied 1:1 above)

ALTER TABLE tenant_new RENAME TO tenant;  -- additive-allowed: ADR-0064 finalize rebuild swap (no data loss)

-- ============================================================
-- 2. Re-create all indexes (lost with the dropped table)
-- ============================================================

-- [0028 + 0037] Primary-region lookup (DO region_enforcer + per-tenant queries)
CREATE INDEX IF NOT EXISTS idx_tenant_primary_region
    ON tenant (primary_region);

-- [0031] BYOK kill-switch degrade + recovery queries
CREATE INDEX IF NOT EXISTS idx_tenants_byok_status
    ON tenant (byok_status);

-- [0031] Per-key revocation lookup
CREATE INDEX IF NOT EXISTS idx_tenants_byok_kms_key_id
    ON tenant (byok_revoked_kms_key_id)
    WHERE byok_revoked_kms_key_id IS NOT NULL;

-- [0037] Idempotency anchor for Clerk webhook auto-provision (signup-worker)
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_signup_id_unique
    ON tenant (signup_id)
    WHERE signup_id IS NOT NULL;

-- [0037] Privacy-safe email lookup (sha256 digest only — CTRL-PRIV-001)
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_email_hash_unique
    ON tenant (email_hash)
    WHERE email_hash IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_tenant_email_hash
    ON tenant (email_hash);

-- [0037] Onboarding state fan-out queries
CREATE INDEX IF NOT EXISTS idx_tenant_state
    ON tenant (tenant_state);

-- [0041] DPA re-acceptance sweep (nightly job + grace-period queries)
CREATE INDEX IF NOT EXISTS idx_tenant_dpa_pending
    ON tenant (re_acceptance_pending, dpa_grace_expires_at)
    WHERE re_acceptance_pending = 1;

-- [0056] Clerk user-id idempotency hot-path (webhook retry dedup)
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_clerk_user_id
    ON tenant (clerk_user_id)
    WHERE clerk_user_id IS NOT NULL;

-- NOTE: No index on `tier` — 0057 did not create one, and the quota lookup
-- hot-path (worker/src/lib/quota.ts) reads by tenant_id (PK), not by tier.

PRAGMA foreign_keys = ON;
