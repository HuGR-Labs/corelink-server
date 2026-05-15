-- Migration 0044: Drata evidence sync ledger (R5-prep SOC 2 Drata pipeline)
--
-- Creates one table:
--   drata_evidence_sent — durable, append-only idempotency ledger for
--   the `corelink-drata-sync` daily-cron evidence push to the Drata
--   REST API.
--
-- D1 is the source-of-truth for the `corelink-drata-sync` crate's
-- `IdempotencyLedger` trait. The CF Cron Worker invokes
-- `SyncRunner::run_daily` at 03:00 UTC; each surviving record is
-- POSTed to Drata, the receipt id is returned, and one row is
-- appended here. Subsequent cron ticks short-circuit on the SHA-256
-- hash lookup so Drata never sees a duplicate (CTRL-AUDIT-DRATA-001).
--
-- Replay safety:
--   - record_sha256 is PRIMARY KEY → INSERT-OR-IGNORE makes the table
--     fail-CLOSED against double-write races between two worker
--     replicas; the receipt of the first writer wins.
--   - The audit chain (`corelink.compliance.drata_evidence_sent`)
--     carries the same hash so chain replay can reconstruct any push
--     even after this table is rotated / archived.
--
-- Security / privacy (CTRL-PRIV-001):
--   - No tenant identifiers, customer emails, or PAT strings are
--     stored here. The hash is computed over the canonical
--     EvidenceRecord JSON (metadata-only payload). Drata receipts are
--     opaque ids assigned by Drata.
--   - api_key column is NOT stored. The Drata Bearer token lives in
--     the secrets manager and is referenced only by `api_key_redacted`
--     in the audit envelope.

CREATE TABLE IF NOT EXISTS drata_evidence_sent (
    -- SHA-256 hex of the canonical EvidenceRecord JSON. Idempotency key.
    record_sha256   TEXT    NOT NULL PRIMARY KEY,
    -- Stream: 'audit_logs' | 'access_reviews' | 'credential_management'
    -- | 'change_management' | 'incident_response' | 'vulnerability_management'.
    stream          TEXT    NOT NULL CHECK (stream IN (
        'audit_logs',
        'access_reviews',
        'credential_management',
        'change_management',
        'incident_response',
        'vulnerability_management'
    )),
    -- Opaque Drata-assigned receipt id (e.g. `rcp_xxxxx`).
    receipt_id      TEXT    NOT NULL,
    -- Wall-clock timestamp (ms since epoch) of the successful push.
    sent_at_ms      BIGINT  NOT NULL,
    -- Audit-chain correlation id (PAT-CORRELATION-ID-001) linking this
    -- row to the `corelink.compliance.drata_evidence_sent` envelope.
    correlation_id  TEXT    NOT NULL,
    -- Schema version (bump on additive change).
    schema_version  INTEGER NOT NULL DEFAULT 1,
    CONSTRAINT sent_at_non_negative CHECK (sent_at_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_drata_evidence_stream_ts
    ON drata_evidence_sent (stream, sent_at_ms);

CREATE INDEX IF NOT EXISTS idx_drata_evidence_receipt
    ON drata_evidence_sent (receipt_id);

-- Backlog-monitoring view: any record older than 24h not yet appearing
-- in the audit chain is the SLA breach for RB-DRATA-SYNC-FAILURE.
-- (View only; the CF worker exports backlog age as a Prometheus gauge
-- separately — see specs/_runbooks/RB-DRATA-SYNC-FAILURE.md §3.)
CREATE VIEW IF NOT EXISTS drata_evidence_sent_backlog_24h AS
SELECT
    stream,
    COUNT(*)                 AS sent_count_24h,
    MIN(sent_at_ms)          AS oldest_sent_at_ms,
    MAX(sent_at_ms)          AS newest_sent_at_ms
FROM drata_evidence_sent
WHERE sent_at_ms >= (CAST(strftime('%s', 'now') AS BIGINT) * 1000) - 86400000
GROUP BY stream;
