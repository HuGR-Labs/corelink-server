-- CoreLink D1 — migration 0107: reject new tenantless audit rows.
--
-- B-127 found that an INNER JOIN-based residency report silently dropped
-- 3,670 existing audit rows whose tenant had been removed. Most are retained
-- DSR evidence and must remain untouched; this migration does not rewrite or
-- delete historical rows. It closes the writer-side hole so future customer
-- rows cannot become unevaluable merely because a caller supplied an unknown
-- tenant_id.
--
-- The existing 0023 trigger only rejects a known tenant with a mismatched
-- region. In SQLite, `NEW.region != (SELECT ...)` evaluates UNKNOWN when the
-- tenant lookup returns NULL, and therefore permits a missing tenant. The new
-- trigger uses an explicit `NOT EXISTS` predicate and is additive: the old
-- trigger remains in place, while this second guard supplies the missing arm.
--
-- `_public` is the deliberate global audit namespace used by public revocation.
-- It has no customer tenant and is pinned to `wnam` by its writer. Re-emitting
-- an already-existing deterministic row is permitted so an idempotent DSR
-- retry still succeeds after the tenant has been erased; INSERT OR IGNORE
-- discards any changed values for that conflicting id.
--
-- INV-AUTH-MIGRATION-ADDITIVE: one new trigger only; no table/index/row rewrite.
-- Rollback is operationally reversible under change control by removing this
-- trigger (`DROP TRIGGER IF EXISTS trg_audit_outbox_tenant_residency_required`)
-- in a dedicated rollback operation; that destructive statement is kept out of
-- the forward migration to preserve the additive-only migration policy.

CREATE TRIGGER IF NOT EXISTS trg_audit_outbox_tenant_residency_required
BEFORE INSERT ON audit_outbox
FOR EACH ROW
WHEN NEW.tenant_id != '_public'
 AND NOT EXISTS (
     SELECT 1
     FROM tenant
     WHERE tenant_id = NEW.tenant_id
       AND primary_region = NEW.region
       AND primary_region IN ('wnam','enam','weur','sam','apac','afr')
 )
 AND NOT EXISTS (
     SELECT 1 FROM audit_outbox AS existing WHERE existing.id = NEW.id
 )
BEGIN
    SELECT RAISE(ABORT,
        'residency_unprovable: audit_outbox tenant_id must resolve to matching tenant.primary_region');
END;
