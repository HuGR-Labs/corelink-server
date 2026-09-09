-- 0125_b083_common_purge_quarantine.sql
--
-- The common BYOK purge reconciler validates the complete physical identity
-- before it can issue an R2 DELETE.  An invalid identity is a durable
-- operator-visible safety fault, not a transient R2 failure.  Keep the
-- original immutable ledger row and record the exact rejection that caused
-- quarantine so the item cannot monopolize the bounded work queue forever.
--
-- This migration is applied once by the D1 migration ledger.  The ALTER TABLE
-- statements are intentionally not presented as raw-SQL re-execution-safe;
-- reruns are suppressed by the canonical migration runner's applied ledger.

ALTER TABLE byok_object_purge_item ADD COLUMN quarantine_reason TEXT;
ALTER TABLE byok_object_purge_item ADD COLUMN quarantined_at_ms INTEGER;

-- Preserve any already-quarantined 0121 rows when this additive migration is
-- applied after a partial rollout.  No physical identity is rewritten.
UPDATE byok_object_purge_item
   SET quarantine_reason = COALESCE(last_error, 'legacy purge quarantine'),
       quarantined_at_ms = COALESCE(verified_at_ms,
                                    (CAST(strftime('%s','now') AS INTEGER) * 1000))
 WHERE state = 'quarantined'
   AND (quarantine_reason IS NULL OR quarantined_at_ms IS NULL);

CREATE INDEX IF NOT EXISTS idx_byok_object_purge_quarantine
    ON byok_object_purge_item (state, quarantined_at_ms, purge_id);

CREATE TRIGGER IF NOT EXISTS trg_byok_purge_quarantine_evidence
BEFORE UPDATE OF state ON byok_object_purge_item
WHEN NEW.state = 'quarantined'
 AND (NEW.quarantine_reason IS NULL
      OR length(trim(NEW.quarantine_reason)) = 0
      OR NEW.quarantined_at_ms IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'BYOK purge quarantine requires durable reason and timestamp');
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_purge_quarantine_fields_forward_only
BEFORE UPDATE OF quarantine_reason, quarantined_at_ms ON byok_object_purge_item
WHEN OLD.state = 'quarantined'
 AND (NEW.quarantine_reason IS NOT OLD.quarantine_reason
      OR NEW.quarantined_at_ms IS NOT OLD.quarantined_at_ms)
BEGIN
    SELECT RAISE(ABORT, 'BYOK purge quarantine evidence is immutable');
END;
