-- WI-S14-006: BYOK tenant status columns for CMK revocation kill switch.
--
-- Adds byok_status + revocation tracking columns to `tenants` table.
-- D1 migration — additive only (no DROP / no data mutation).
--
-- INV-BYOK-CRYPTO-SOVEREIGNTY: byok_status = 'degraded_read_only' blocks
-- all writes and causes in-flight reads to fail after 5 min DEK cache TTL.
--
-- Revocation audit trail: byok_revoked_at_ms + byok_revoked_provider +
-- byok_revoked_kms_key_id stored for 7y (CTRL-AUDIT-005).

ALTER TABLE tenants ADD COLUMN byok_status TEXT DEFAULT 'active'
    CHECK (byok_status IN ('active', 'degraded_read_only', 'revoked'));

ALTER TABLE tenants ADD COLUMN byok_revoked_at_ms INTEGER;

ALTER TABLE tenants ADD COLUMN byok_revoked_provider TEXT;

ALTER TABLE tenants ADD COLUMN byok_revoked_kms_key_id TEXT;

-- Index for fast kill-switch degrade + recovery queries.
CREATE INDEX IF NOT EXISTS idx_tenants_byok_status
    ON tenants (byok_status);

-- Index for per-key revocation lookup.
CREATE INDEX IF NOT EXISTS idx_tenants_byok_kms_key_id
    ON tenants (byok_revoked_kms_key_id)
    WHERE byok_revoked_kms_key_id IS NOT NULL;
