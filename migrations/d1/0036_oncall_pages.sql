-- Migration 0036: Oncall rotations + pages (WI-S17-005)
--
-- Creates three tables:
--   oncall_rotations  — per-tier rotation roster + history
--   oncall_shifts     — individual shift assignments (7d cap; 2w protection)
--   oncall_pages      — per-engineer page log for fatigue scoring
--
-- D1 is the durable mirror for the in-process `corelink-oncall`
-- RotationLedger. Production wiring runs a quarterly reconcile against
-- the PagerDuty API to detect drift (procedure deferred to PRR ship
-- gate; equivalent of docs/ops/d1-r2-reconcile.md for the oncall
-- ledger).
--
-- Security / privacy:
--   - engineer_id stored as opaque slug (e.g. `eng-001`); display
--     names live only in the handoff template (specs/_templates/
--     oncall_handoff.md) per CTRL-PRIV-001.
--   - Fatigue metrics derived from this table carry only `{tier,
--     in_rotation}` labels (no per-engineer cardinality on the
--     Prometheus surface) per INV-OBS-CARDINALITY-BUDGET-RESPECTED.

-- ===========================================================
-- oncall_rotations — per-tier rotation roster
-- ===========================================================
CREATE TABLE IF NOT EXISTS oncall_rotations (
    -- Canonical tier id: 'tier-1' | 'tier-2' | 'tier-3'.
    tier                TEXT    NOT NULL CHECK (tier IN ('tier-1', 'tier-2', 'tier-3')),
    -- Rotation cadence in seconds; canonical 604800 (7d) per
    -- Google SRE Workbook Ch 8 + WI §1 / §6.1.1.
    rotation_seconds    BIGINT  NOT NULL CHECK (rotation_seconds <= 604800),
    -- Schema version (bump on additive change).
    schema_version      INTEGER NOT NULL DEFAULT 1,
    -- Last reconcile timestamp (ms since epoch) against PagerDuty
    -- Schedule API.
    last_reconcile_ms   BIGINT  NOT NULL DEFAULT 0,
    PRIMARY KEY (tier)
);

-- ===========================================================
-- oncall_shifts — individual shift assignments
-- ===========================================================
CREATE TABLE IF NOT EXISTS oncall_shifts (
    shift_id            TEXT    NOT NULL PRIMARY KEY,
    tier                TEXT    NOT NULL CHECK (tier IN ('tier-1', 'tier-2', 'tier-3')),
    engineer_id         TEXT    NOT NULL,
    start_ms            BIGINT  NOT NULL,
    end_ms              BIGINT  NOT NULL,
    -- Correlation id (PAT-CORRELATION-ID-001) from the audit chain.
    -- D1 compatibility: column defs must precede table-level CONSTRAINTs.
    correlation_id      TEXT    NOT NULL,
    -- 7-day shift cap canonical (SHIFT_CAP_SECONDS in corelink-oncall).
    CONSTRAINT shift_cap_7d CHECK (end_ms - start_ms <= 604800000),
    CONSTRAINT shift_positive_duration CHECK (end_ms > start_ms),
    FOREIGN KEY (tier) REFERENCES oncall_rotations(tier)
);

CREATE INDEX IF NOT EXISTS idx_oncall_shifts_engineer
    ON oncall_shifts (engineer_id);

CREATE INDEX IF NOT EXISTS idx_oncall_shifts_tier_start
    ON oncall_shifts (tier, start_ms);

CREATE INDEX IF NOT EXISTS idx_oncall_shifts_end_ms
    ON oncall_shifts (end_ms);

-- ===========================================================
-- oncall_pages — per-engineer page log (rolling 30d retained;
-- older rows pruned via daily GC cron deferred to PRR ship gate)
-- ===========================================================
CREATE TABLE IF NOT EXISTS oncall_pages (
    page_id             TEXT    NOT NULL PRIMARY KEY,
    engineer_id         TEXT    NOT NULL,
    severity            TEXT    NOT NULL CHECK (severity IN ('sev0', 'sev1', 'sev2', 'sev3')),
    ts_ms               BIGINT  NOT NULL,
    -- True if engineer was in an active shift at ts_ms (derived once
    -- at insert via oncall_shifts join; updated on shift backfill).
    in_rotation         INTEGER NOT NULL DEFAULT 0 CHECK (in_rotation IN (0, 1)),
    -- True if ts_ms falls within the 00:00-06:00 UTC sleep-hour
    -- window (canonical Grafana dashboard label per WI §6.1.4).
    sleep_hour          INTEGER NOT NULL DEFAULT 0 CHECK (sleep_hour IN (0, 1)),
    -- Correlation id from the incident response chain.
    correlation_id      TEXT    NOT NULL,
    -- MTTA / MTTR (ms) once the incident is acknowledged / resolved.
    mtta_ms             BIGINT,
    mttr_ms             BIGINT,
    CONSTRAINT mtta_non_negative CHECK (mtta_ms IS NULL OR mtta_ms >= 0),
    CONSTRAINT mttr_non_negative CHECK (mttr_ms IS NULL OR mttr_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_oncall_pages_engineer_ts
    ON oncall_pages (engineer_id, ts_ms);

CREATE INDEX IF NOT EXISTS idx_oncall_pages_severity_ts
    ON oncall_pages (severity, ts_ms);

CREATE INDEX IF NOT EXISTS idx_oncall_pages_in_rotation
    ON oncall_pages (in_rotation, ts_ms);

-- Seed the canonical 3-tier rotation cadence.
INSERT OR IGNORE INTO oncall_rotations (tier, rotation_seconds, schema_version, last_reconcile_ms)
VALUES
    ('tier-1', 604800, 1, 0),
    ('tier-2', 604800, 1, 0),
    ('tier-3', 604800, 1, 0);
