-- Migration 0037: Enterprise inquiries + saga outbox (WI-S19-005)
--
-- Creates two tables:
--   enterprise_inquiries       — durable inquiry record (PII-aware,
--                                encrypted-at-rest payload columns).
--   enterprise_inquiry_outbox  — PAT-SAGA-001 atomic Slack + CRM
--                                dispatch transaction log; the worker
--                                drain reconciles in-flight rows after
--                                a crash and escalates rows that sit
--                                in `pending` past the 5min partial-
--                                state ceiling (RB-FM-ENTERPRISE-
--                                HANDOFF-PARTIAL).
--
-- D1 is the durable mirror for the in-process
-- `corelink-enterprise-inquiry` ledger. Production wiring runs a
-- quarterly reconcile against the HubSpot CRM API + Slack channel
-- history to detect drift (procedure deferred to PRR ship gate;
-- equivalent of docs/ops/d1-r2-reconcile.md for the inquiry ledger).
--
-- Security / privacy:
--   - `company`, `email`, `phone_optional`, `additional_notes`, and
--     `use_case` columns are AES-256-GCM encrypted-at-rest via the
--     S-14 BYOK envelope (`encrypted_payload_b64` carries the wrapped
--     blob; the per-column TEXT columns hold sanitised non-PII
--     surrogates: company name initials, email domain only, etc.) per
--     CTRL-PRIV-001.
--   - Lead score is derived on insert and stored in cleartext so the
--     Grafana lead-score histogram can hydrate without unsealing.
--   - PII is never written to the Prometheus surface; the metrics
--     carry only `{outcome, plan}` labels per INV-OBS-CARDINALITY-
--     BUDGET-RESPECTED.
--
-- Additive-only:
--   - Both tables are new; no existing table is altered.
--   - The migration is safely re-runnable via `CREATE TABLE IF NOT
--     EXISTS` + `INSERT OR IGNORE`.

-- ===========================================================
-- enterprise_inquiries — durable inquiry record
-- ===========================================================
CREATE TABLE IF NOT EXISTS enterprise_inquiries (
    inquiry_id              TEXT    NOT NULL PRIMARY KEY,
    -- Idempotency dedup token; UNIQUE so a re-submission with the
    -- same key short-circuits before any Slack / CRM / email side-
    -- effect.
    idempotency_key         TEXT    NOT NULL UNIQUE,
    -- Non-PII sanitised surrogate columns (full PII lives in
    -- `encrypted_payload_b64`).
    company_initials        TEXT    NOT NULL CHECK (length(company_initials) <= 8),
    role                    TEXT    NOT NULL CHECK (role IN (
                                'ciso', 'cto', 'cio', 'vp_eng', 'other'
                              )),
    email_domain            TEXT    NOT NULL CHECK (length(email_domain) <= 253),
    has_phone               INTEGER NOT NULL DEFAULT 0 CHECK (has_phone IN (0, 1)),
    expected_gb_per_month   BIGINT  NOT NULL CHECK (expected_gb_per_month >= 0),
    byok_requirements       TEXT    NOT NULL CHECK (byok_requirements IN (
                                'none', 'aws_kms', 'gcp_kms', 'azure_kv', 'vault'
                              )),
    residency_requirements  TEXT    NOT NULL CHECK (residency_requirements IN (
                                'none', 'us', 'eu', 'sam', 'apac', 'specific'
                              )),
    locale                  TEXT    NOT NULL CHECK (length(locale) <= 16),
    lead_score              INTEGER NOT NULL CHECK (lead_score BETWEEN 0 AND 1000),
    -- AES-256-GCM encrypted-at-rest payload (BYOK envelope per S-14).
    -- Carries `{company, email, phone, additional_notes, use_case}`.
    encrypted_payload_b64   TEXT    NOT NULL,
    status                  TEXT    NOT NULL CHECK (status IN (
                                'pending', 'committed', 'rolled_back', 'partial_escalated'
                              )),
    created_ms              BIGINT  NOT NULL,
    -- Server-side timestamp (ms since epoch) when the auto-reply
    -- email dispatched; NULL until SES confirms. The 24h SLA
    -- detector scans `replied_ms IS NULL AND created_ms < now - 24h`.
    replied_ms              BIGINT,
    -- Correlation id (PAT-CORRELATION-ID-001).
    correlation_id          TEXT    NOT NULL,
    CONSTRAINT inquiry_timestamps_consistent CHECK (
        replied_ms IS NULL OR replied_ms >= created_ms
    )
);

-- The 24h SLA detector worker scan: `replied_ms IS NULL AND
-- created_ms < now - 24h`. Index supports range scan on created_ms
-- with optional replied_ms NULL predicate.
CREATE INDEX IF NOT EXISTS idx_enterprise_inquiries_sla_scan
    ON enterprise_inquiries (replied_ms, created_ms);

CREATE INDEX IF NOT EXISTS idx_enterprise_inquiries_status_created
    ON enterprise_inquiries (status, created_ms);

CREATE INDEX IF NOT EXISTS idx_enterprise_inquiries_lead_score
    ON enterprise_inquiries (lead_score);

-- ===========================================================
-- enterprise_inquiry_outbox — saga PAT-SAGA-001 transaction log
-- ===========================================================
CREATE TABLE IF NOT EXISTS enterprise_inquiry_outbox (
    inquiry_id              TEXT    NOT NULL PRIMARY KEY,
    status                  TEXT    NOT NULL CHECK (status IN (
                                'pending', 'committed', 'rolled_back', 'partial_escalated'
                              )),
    slack_message_id        TEXT,
    crm_entry_id            TEXT,
    created_ms              BIGINT  NOT NULL,
    updated_ms              BIGINT  NOT NULL,
    CONSTRAINT outbox_timestamps_consistent CHECK (updated_ms >= created_ms),
    FOREIGN KEY (inquiry_id) REFERENCES enterprise_inquiries(inquiry_id)
);

-- Worker drain scan: rows in `pending` status past the 5min partial-
-- state ceiling. Index supports range scan on updated_ms within a
-- status equality predicate.
CREATE INDEX IF NOT EXISTS idx_enterprise_inquiry_outbox_status_updated
    ON enterprise_inquiry_outbox (status, updated_ms);
