-- Migration 0042: Synthetic page weekly drill records (WI-S20-006)
--
-- Creates one table:
--   synthetic_page_drills — per-drill emit + ack record for the
--   weekly synthetic SEV-2 drill run by `corelink-synthetic-pager`.
--
-- D1 is the durable mirror for the `corelink-synthetic-pager`
-- in-process `InMemoryDrillRecorder`. Production wiring runs the CF
-- Cron Worker `[triggers] crons = ["0 14 * * 1"]` at 14:00 UTC Monday
-- (mid-Americas-shift) and rotates region selection across a 4-week
-- cycle so every region (Americas / EMEA / APAC) is exercised at
-- least once per 28d.
--
-- Security / privacy:
--   - engineer_slug stored as opaque slug (e.g. `eng-001`); display
--     names live only in the handoff template (specs/_templates/
--     oncall_handoff.md) per CTRL-PRIV-001.
--   - MTTA metrics derived from this table carry only `{region,
--     ack_vector, outcome}` labels (no per-engineer cardinality on the
--     Prometheus surface) per INV-OBS-CARDINALITY-BUDGET-RESPECTED.
--   - drill_id is the canonical PagerDuty incident dedup key; webhook
--     receiver upserts on it.

CREATE TABLE IF NOT EXISTS synthetic_page_drills (
    -- Canonical drill id ('SP-<UUIDv4 stripped>').
    drill_id        TEXT    NOT NULL PRIMARY KEY,
    -- Region paged: 'americas' | 'emea' | 'apac'.
    region          TEXT    NOT NULL CHECK (region IN ('americas', 'emea', 'apac')),
    -- Synthetic severity (only one canonical value at GA; reserved
    -- for additive growth per WI §2.1).
    severity        TEXT    NOT NULL CHECK (severity = 'sev2_synthetic'),
    -- On-call engineer slug captured at ack time. NULL when the drill
    -- closed without ack (Unacked outcome).
    engineer_slug   TEXT,
    -- Emit timestamp (ms since epoch). The cron worker stamps this
    -- before POSTing the PagerDuty Events API v2 trigger.
    emit_ts_ms      BIGINT  NOT NULL,
    -- Ack timestamp (ms since epoch). NULL when Unacked.
    ack_ts_ms       BIGINT,
    -- Ack vector: 'mobile_push' | 'sms' | 'email'. NULL when Unacked.
    ack_vector      TEXT    CHECK (ack_vector IS NULL OR ack_vector IN ('mobile_push', 'sms', 'email')),
    -- MTTA in milliseconds. NULL when Unacked.
    mtta_ms         BIGINT,
    -- Final classification: 'acked' | 'escalated' | 'unacked'.
    outcome         TEXT    NOT NULL CHECK (outcome IN ('acked', 'escalated', 'unacked')),
    -- Audit-chain correlation id (PAT-CORRELATION-ID-001).
    correlation_id  TEXT    NOT NULL,
    -- Schema version (bump on additive change).
    schema_version  INTEGER NOT NULL DEFAULT 1,
    -- Time-ordering invariants.
    CONSTRAINT ack_after_emit CHECK (ack_ts_ms IS NULL OR ack_ts_ms >= emit_ts_ms),
    CONSTRAINT mtta_non_negative CHECK (mtta_ms IS NULL OR mtta_ms >= 0),
    -- Unacked drills MUST carry NULL ack/mtta/vector fields.
    CONSTRAINT unacked_fields_null CHECK (
        outcome != 'unacked'
        OR (ack_ts_ms IS NULL AND mtta_ms IS NULL AND ack_vector IS NULL)
    ),
    -- Acked / Escalated MUST have a non-null engineer_slug + ack
    -- fields populated.
    CONSTRAINT acked_fields_complete CHECK (
        outcome = 'unacked'
        OR (engineer_slug IS NOT NULL AND ack_ts_ms IS NOT NULL
            AND mtta_ms IS NOT NULL AND ack_vector IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_synthetic_drills_region_ts
    ON synthetic_page_drills (region, emit_ts_ms);

CREATE INDEX IF NOT EXISTS idx_synthetic_drills_outcome_ts
    ON synthetic_page_drills (outcome, emit_ts_ms);

CREATE INDEX IF NOT EXISTS idx_synthetic_drills_engineer
    ON synthetic_page_drills (engineer_slug)
    WHERE engineer_slug IS NOT NULL;
