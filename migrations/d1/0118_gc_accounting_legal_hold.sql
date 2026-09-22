-- CoreLink D1 — GC finalization accounting and legal-hold fence.
--
-- The B-071 purge protocol already makes the R2/D1 operation resumable and
-- idempotent.  This follow-up keeps the final metadata delete safe for the
-- two repository-owned invariants that cannot be inferred from R2 alone:
--
--   * a tenant legal hold blocks every GC finalization attempt; and
--   * a successful finalization releases exactly the deleted blob's bytes
--     from tenant_storage_state in the same SQLite transaction as the
--     metadata delete and audit/candidate/intent updates.
--
-- Both guards are deliberately fail-closed.  A missing accounting row is
-- not treated as zero, because that would make the usage counter silently
-- diverge from blob_meta.  The R2 object may already be gone at this point;
-- leaving the intent in `r2_deleted` gives the reconciler a durable retry
-- boundary without claiming that reclaim completed.

-- A hold can be placed after the purge intent is acquired.  Check it at the
-- irreversible D1 boundary as well as in the caller, so an in-flight worker
-- cannot bypass a newly placed hold.
CREATE TRIGGER IF NOT EXISTS trg_gc_purge_legal_hold_guard
BEFORE DELETE ON blob_meta
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM gc_purge_intent
    WHERE tenant_id = OLD.tenant_id
      AND digest = OLD.digest
      AND state = 'r2_deleted'
)
AND EXISTS (
    SELECT 1 FROM tenant_legal_hold
    WHERE tenant_id = OLD.tenant_id
)
BEGIN
    SELECT RAISE(ABORT, 'gc_purge_blocked_by_legal_hold');
END;

-- Never finalize a purge if the authoritative accounting row is absent.  A
-- row is keyed by the blob's residency region, which is the same region used
-- by the CAS accounting path.
CREATE TRIGGER IF NOT EXISTS trg_gc_purge_accounting_required
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
      AND region = OLD.region
)
BEGIN
    SELECT RAISE(ABORT, 'gc_purge_accounting_state_missing');
END;

-- Keep the existing B-071 atomic finalize trigger as the single transaction
-- boundary.  `bytes_reclaimed_lifetime` records the physical bytes removed;
-- `bytes_used` is saturated independently so historical counter drift cannot
-- produce a negative value.
CREATE TRIGGER IF NOT EXISTS trg_gc_purge_finalize_accounting
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
       AND region = OLD.region;
END;
