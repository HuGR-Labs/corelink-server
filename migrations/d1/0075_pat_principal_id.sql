-- Migration 0075: pat.principal_id — associate a PAT with its issuing principal
-- (member seat) so SEAT REMOVAL can revoke exactly that member's tokens
-- (ADR-S33-001 WP-3).
--
-- The `pat` table keyed tokens by tenant only; a member's PATs were therefore
-- indistinguishable from the owner's, so seat-removal could not revoke "this
-- member's access". This adds the per-principal association.
--
-- For the tenant OWNER and legacy tokens this column is NULL (a removal targets a
-- specific non-NULL principal, so NULL-principal owner tokens are never swept).
-- New member PATs record `principal_id = the member's user_id`; seat-removal does
-- `UPDATE pat SET revoked_at_ms=? WHERE tenant_id=? AND principal_id=? AND
-- revoked_at_ms IS NULL`.
--
-- D1 note: D1/SQLite has no `ALTER TABLE ADD COLUMN IF NOT EXISTS`; this is a
-- pure additive ADD COLUMN (safe on existing rows — they default to NULL).

ALTER TABLE pat ADD COLUMN principal_id TEXT;

-- Covering index for the seat-removal revocation sweep + per-member lookups.
CREATE INDEX IF NOT EXISTS idx_pat_tenant_principal
    ON pat (tenant_id, principal_id);
