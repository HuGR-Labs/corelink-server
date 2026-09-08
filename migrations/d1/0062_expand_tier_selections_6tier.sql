-- Migration 0062: widen the two legacy inline tier CHECKs to the canonical
-- six-tier taxonomy without replacing either physical table.
--
-- SQLite/D1 has no ALTER CHECK operation. A table rebuild would require a
-- DROP + RENAME window, which is not an additive migration and can lose rows,
-- indexes, triggers, or dependent objects. The tables created by 0039 are
-- therefore retained in place. SQLite's documented schema-editor escape hatch
-- (`writable_schema`) is used only to replace the CHECK expression in the
-- catalog SQL; no row, object name, index, foreign key, or view is removed.
--
-- Canonical accepted values:
--   tier_selections.tier: free | solo | starter | team | pro | max | enterprise
--   stripe_checkout_sessions.tier: solo | starter | team | pro | max
--
-- `team` remains accepted in both tables for backwards compatibility. The
-- catalog update is idempotent: once the old expression is gone, each UPDATE
-- affects zero rows on a replay. The schema-cookie dance forces the current
-- connection to reparse the catalog before subsequent INSERT/UPDATE statements
-- use the widened CHECKs. It changes no user-visible schema object.

PRAGMA writable_schema = ON;

-- Guarded, exact replacements. The WHERE clauses make a partial/foreign
-- schema impossible: only the two 0039 table definitions can be touched.
UPDATE sqlite_master
SET sql = replace(
    sql,
    'CHECK (tier IN (''free'', ''starter'', ''team'', ''pro'', ''enterprise''))',
    'CHECK (tier IN (''free'', ''solo'', ''starter'', ''team'', ''pro'', ''max'', ''enterprise''))'
)
WHERE type = 'table'
  AND name = 'tier_selections'
  AND instr(sql, 'CHECK (tier IN (''free'', ''starter'', ''team'', ''pro'', ''enterprise''))') > 0;

UPDATE sqlite_master
SET sql = replace(
    sql,
    'CHECK (tier IN (''starter'', ''team'', ''pro''))',
    'CHECK (tier IN (''solo'', ''starter'', ''team'', ''pro'', ''max''))'
)
WHERE type = 'table'
  AND name = 'stripe_checkout_sessions'
  AND instr(sql, 'CHECK (tier IN (''starter'', ''team'', ''pro''))') > 0;

PRAGMA writable_schema = OFF;

-- UPDATE sqlite_master does not invalidate SQLite's prepared schema cache.
-- Assigning two distinct values guarantees a cookie transition even when the
-- input database happens to use cookie 1. The cookie is internal metadata;
-- all table rows, columns, indexes, FKs, and the dependent drift view remain
-- byte-for-byte untouched.
PRAGMA schema_version = 0;
PRAGMA schema_version = 1;

-- Behavioral guard: a malformed/partial catalog edit must fail before the
-- migration is considered usable. The table is TEMP, so it never changes the
-- durable schema; its CHECK turns a failed postcondition into a transaction
-- error. IF NOT EXISTS + INSERT makes the guard replay-safe.
CREATE TEMP TABLE IF NOT EXISTS _migration_0062_schema_guard (
    ok INTEGER NOT NULL CHECK (ok = 1)
);

INSERT INTO _migration_0062_schema_guard (ok)
SELECT CASE
    WHEN (SELECT instr(sql, 'CHECK (tier IN (''free'', ''solo'', ''starter'', ''team'', ''pro'', ''max'', ''enterprise''))')
          FROM sqlite_master WHERE type = 'table' AND name = 'tier_selections') > 0
     AND (SELECT instr(sql, 'CHECK (tier IN (''solo'', ''starter'', ''team'', ''pro'', ''max''))')
          FROM sqlite_master WHERE type = 'table' AND name = 'stripe_checkout_sessions') > 0
    THEN 1
    ELSE 0
END AS migration_0062_schema_guard;

-- The catalog strings above are deliberately exact, but `sqlite_master.sql`
-- also retains comments.  A comment or literal containing the widened text
-- must not satisfy the guard while the executable CHECK remains narrow.
-- Probe both newly admitted values through the real constraints inside a
-- savepoint, then roll the probe rows back.  A rejected value aborts the
-- enclosing D1 transaction; a successful probe leaves no durable row.
SAVEPOINT _migration_0062_semantic_guard;

INSERT INTO tier_selections (
    tenant_id, tier, subscription_state, subscription_started_at_ms,
    correlation_id
)
VALUES
    ('__migration_0062_guard_solo__', 'solo', 'inactive', NULL, '__migration_0062_guard__'),
    ('__migration_0062_guard_max__', 'max', 'inactive', NULL, '__migration_0062_guard__');

INSERT INTO stripe_checkout_sessions (
    session_id, tenant_id, tier, created_at_ms, correlation_id
)
VALUES
    ('__migration_0062_guard_solo__', '__migration_0062_guard_solo__', 'solo', 0, '__migration_0062_guard__'),
    ('__migration_0062_guard_max__', '__migration_0062_guard_max__', 'max', 0, '__migration_0062_guard__');

ROLLBACK TO _migration_0062_semantic_guard;
RELEASE _migration_0062_semantic_guard;
