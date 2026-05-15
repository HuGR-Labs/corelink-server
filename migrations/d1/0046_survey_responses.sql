-- Migration 0046: Customer-health survey responses (R-prep retention lane).
--
-- Closes the survey-infrastructure gap surfaced by
-- `marketing/retention/customer-health/NPS-SURVEY-SCHEDULE.md` (wave 1-6
-- schedule documented; storage + auth-without-password infra was missing).
--
-- # Why this table exists
--
-- The `corelink-survey` crate mints HMAC-SHA256-signed one-shot invite
-- tokens (`SurveyLinkSigner::sign_invite`). Click-through verifies the
-- token in constant time, validates the response payload, and persists
-- a row here. The recorder emits an audit envelope BEFORE this insert
-- (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER pattern) so the audit chain is
-- never ahead of or behind the response store.
--
-- # Four survey kinds (response_kind column)
--
--   - 'nps'          — 0..=10 NPS score (response_value JSON: `{"kind":"nps","score":<u8>}`)
--   - 'csat'         — 1..=5 CSAT score
--   - 'free_text'    — sanitized + length-capped UTF-8 (≤ 2048 bytes after trim)
--   - 'multi_choice' — ≤ 16 distinct option indices (each ≤ 255)
--
-- # Privacy-by-design
--
--   - `recipient_hash` is a salted SHA-256 digest. The raw email + raw
--     recipient id NEVER enter the database. Salt rotates with the
--     HMAC signing key (corelink-rotation-* lane).
--   - `ip_hash` and `user_agent_hash` are SHA-256 digests; raw IPs +
--     raw user-agent strings never land here.
--   - `token_jti` is the UUIDv7 minted by the signer; used as the
--     replay-rejection dedup key (UNIQUE constraint below).
--
-- # Idempotent / additive
--
-- Every statement uses `IF NOT EXISTS`. INV-AUTH-MIGRATION-ADDITIVE
-- holds; `scripts/check_migrations_additive.py` exits 0.
--
-- # Forward evolution
--
-- Adding new survey kinds is a value-only change in the application
-- layer's `response_kind` CHECK list and the corelink-survey crate's
-- `SurveyKind` enum (additive non_exhaustive). Existing rows are
-- compatible because the JSON in `response_value` carries the
-- discriminator inline.

CREATE TABLE IF NOT EXISTS survey_responses (
    -- Server-side row id (UUIDv7), minted at insert time.
    id              TEXT NOT NULL PRIMARY KEY,
    -- Tenant id (UUIDv7) — joined from the verified token payload.
    tenant_id       TEXT NOT NULL,
    -- Salted SHA-256 hash of recipient id (64-char lowercase hex).
    recipient_hash  TEXT NOT NULL,
    -- Survey slug (e.g. 'nps-w1-2026q2'). ASCII [A-Za-z0-9_-]{1,64}.
    survey_id       TEXT NOT NULL,
    -- Survey kind discriminator. CHECK keeps the column tight.
    response_kind   TEXT NOT NULL CHECK (
        response_kind IN ('nps', 'csat', 'free_text', 'multi_choice')
    ),
    -- Canonical JSON of the SurveyResponse variant. Parsed by analytics
    -- via `serde::Deserialize` on the SurveyResponse enum (tag = "kind").
    response_value  TEXT NOT NULL,
    -- Submitted-at unix ms (server-side clock at record() time).
    submitted_at_ms INTEGER NOT NULL,
    -- SHA-256(IP) — 64-char lowercase hex; raw IP never stored.
    ip_hash         TEXT NOT NULL,
    -- SHA-256(User-Agent) — 64-char lowercase hex; raw UA never stored.
    user_agent_hash TEXT NOT NULL,
    -- Token jti (UUIDv7). UNIQUE — enforces replay-rejection at the
    -- storage layer as a belt-and-suspenders complement to the
    -- in-memory dedup the recorder runs first.
    token_jti       TEXT NOT NULL UNIQUE
);

-- Analytics access pattern 1: per-survey aggregation (weekly NPS, CSAT
-- trend). Filters by survey_id + submitted_at_ms range.
CREATE INDEX IF NOT EXISTS idx_survey_responses_by_survey_time
    ON survey_responses (survey_id, submitted_at_ms);

-- Analytics access pattern 2: per-tenant cohort cuts (which tenants
-- responded last week, NPS score distribution by tenant tier).
CREATE INDEX IF NOT EXISTS idx_survey_responses_by_tenant_time
    ON survey_responses (tenant_id, submitted_at_ms);

-- Abuse pivot: same recipient submitting to many surveys (RB-SURVEY-ABUSE).
CREATE INDEX IF NOT EXISTS idx_survey_responses_by_recipient
    ON survey_responses (recipient_hash, submitted_at_ms);

-- Abuse pivot: per-IP burst detection (RB-SURVEY-ABUSE rate-limit lookup).
CREATE INDEX IF NOT EXISTS idx_survey_responses_by_ip_time
    ON survey_responses (ip_hash, submitted_at_ms);
