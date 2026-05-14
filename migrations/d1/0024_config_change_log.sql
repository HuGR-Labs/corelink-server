-- D1 migration: config_change_log (WI-S13-001)
--
-- Stores 90-day audit history of every config-singleton write (update +
-- rollback). Serves as the rollback source-of-truth when the DO needs
-- to restore a historical payload.
--
-- Invariants enforced:
--   INV-AUDIT-APPEND-ONLY (CRITICAL): rows are INSERT-only; no UPDATE/DELETE
--   except the daily purge cron which removes rows where
--   created_at_ms < now - 90d.
--
--   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL): this INSERT is batched
--   with the DO trigger + audit_outbox INSERT in the same D1 db.batch()
--   call. If the batch fails, the DO write is rolled back (fail-CLOSED).
--
-- Retention: 90d hard (SOC 2 CC8.1). Older versions available in R2
-- long-term audit archive (CTRL-AUDIT-005 7y retention) if needed.
--
-- Cron: daily 02:00 UTC purges rows where created_at_ms < now - 90d.

CREATE TABLE IF NOT EXISTS config_change_log (
    version          INTEGER PRIMARY KEY,       -- monotonically increasing; 1-origin
    payload_hash     BLOB(32)    NOT NULL,       -- SHA-256 of canonical JCS JSON
    payload          BLOB        NOT NULL,       -- canonical JCS JSON (serde_jcs RFC 8785)
    actor_user_id    BLOB(16)    NOT NULL,       -- UUIDv7 bytes (admin user_id)
    actor_email_hash BLOB(32)    NOT NULL,       -- SHA-256(email) — pseudonymized
    mfa_ts_ms        INTEGER     NOT NULL,       -- MFA completion timestamp (Unix ms)
    created_at_ms    INTEGER     NOT NULL,       -- row creation timestamp (Unix ms)
    change_type      TEXT        NOT NULL        -- 'update' | 'rollback'
                     CHECK (change_type IN ('update', 'rollback')),
    previous_version INTEGER                     -- NULL for genesis (v1)
);

CREATE INDEX IF NOT EXISTS idx_config_change_log_created_at
    ON config_change_log(created_at_ms);

-- Purge cron helper view: selects rows outside 90d window.
-- Cron worker runs: DELETE FROM config_change_log WHERE created_at_ms < ?
-- where ? = UNIXEPOCH('now') * 1000 - 90 * 86400 * 1000
