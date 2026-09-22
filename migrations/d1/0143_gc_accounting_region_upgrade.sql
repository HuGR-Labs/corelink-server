-- CoreLink D1 — upgrade the already-deployed GC accounting trigger bodies.
--
-- 0118_gc_accounting_legal_hold.sql originally keyed accounting with
-- blob_meta.region.  That value is macro residency (for example, `wnam`),
-- whereas tenant_storage_state uses the five-region GC partition.  The
-- authoritative partition is already fenced in gc_purge_intent.gc_region.
--
-- SQLite's CREATE TRIGGER IF NOT EXISTS preserves an existing trigger body,
-- so deployments that applied the original 0118 need this forward migration.
-- Only the two accounting triggers are replaced; the legal-hold guard and
-- B-071 finalization trigger remain in force throughout this transaction.

DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required;

CREATE TRIGGER trg_gc_purge_accounting_required
BEFORE DELETE ON blob_meta
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM gc_purge_intent
    WHERE tenant_id = OLD.tenant_id
      AND digest = OLD.digest
      AND state = 'r2_deleted'
)
AND NOT EXISTS (
    SELECT 1 FROM tenant_storage_state
    WHERE tenant_id = OLD.tenant_id
      AND region = (
          SELECT gc_region FROM gc_purge_intent
          WHERE tenant_id = OLD.tenant_id
            AND digest = OLD.digest
            AND state = 'r2_deleted'
      )
)
BEGIN
    SELECT RAISE(ABORT, 'gc_purge_accounting_state_missing');
END;

DROP TRIGGER IF EXISTS trg_gc_purge_finalize_accounting;

CREATE TRIGGER trg_gc_purge_finalize_accounting
AFTER DELETE ON blob_meta
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM gc_purge_intent
    WHERE tenant_id = OLD.tenant_id
      AND digest = OLD.digest
      AND state = 'r2_deleted'
)
BEGIN
    UPDATE tenant_storage_state
       SET bytes_used = MAX(0, bytes_used - OLD.size_bytes),
           bytes_reclaimed_lifetime = bytes_reclaimed_lifetime
               + OLD.size_bytes,
           bytes_used_updated_at_ms = MAX(
               bytes_used_updated_at_ms,
               (SELECT updated_at FROM gc_purge_intent
                WHERE tenant_id = OLD.tenant_id
                  AND digest = OLD.digest
                  AND state = 'r2_deleted')
           ),
           updated_at_ms = MAX(
               updated_at_ms,
               (SELECT updated_at FROM gc_purge_intent
                WHERE tenant_id = OLD.tenant_id
                  AND digest = OLD.digest
                  AND state = 'r2_deleted')
           )
     WHERE tenant_id = OLD.tenant_id
       AND region = (
           SELECT gc_region FROM gc_purge_intent
           WHERE tenant_id = OLD.tenant_id
             AND digest = OLD.digest
             AND state = 'r2_deleted'
       );
END;
