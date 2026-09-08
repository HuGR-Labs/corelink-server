-- B-072 receiver lifecycle: deferred handoff delivery, PagerDuty outcomes, and
-- append-only webhook audit events.
--
-- 0043 was created before the receiver contract was wired. This migration
-- widens its region/vector checks and adds delivery state while preserving all
-- existing rows. The canonical drill_id is now also PagerDuty dedup_key:
-- `SP-<13-digit scheduled timestamp>`.

PRAGMA foreign_keys = OFF;

CREATE TABLE synthetic_page_drills_b072 (
    drill_id        TEXT    NOT NULL PRIMARY KEY,
    region          TEXT    NOT NULL CHECK (region IN ('americas', 'emea', 'apac', 'boundary_handoff')),
    severity        TEXT    NOT NULL CHECK (severity = 'sev2_synthetic'),
    engineer_slug   TEXT,
    emit_ts_ms      BIGINT  NOT NULL,
    ack_ts_ms       BIGINT,
    ack_vector      TEXT    CHECK (ack_vector IS NULL OR ack_vector IN ('mobile_push', 'sms', 'email', 'escalation')),
    mtta_ms         BIGINT,
    outcome         TEXT    NOT NULL CHECK (outcome IN ('acked', 'escalated', 'unacked')),
    correlation_id  TEXT    NOT NULL,
    schema_version  INTEGER NOT NULL DEFAULT 1,
    delivery_mode   TEXT    NOT NULL DEFAULT 'immediate' CHECK (delivery_mode IN ('immediate', 'deferred')),
    scheduled_at_ms BIGINT  NOT NULL,
    delivered_at_ms BIGINT,
    CONSTRAINT ack_after_emit CHECK (ack_ts_ms IS NULL OR ack_ts_ms >= emit_ts_ms),
    CONSTRAINT mtta_non_negative CHECK (mtta_ms IS NULL OR mtta_ms >= 0),
    CONSTRAINT unacked_fields_null CHECK (
        outcome != 'unacked'
        OR (ack_ts_ms IS NULL AND mtta_ms IS NULL AND ack_vector IS NULL)
    ),
    CONSTRAINT outcome_fields_complete CHECK (
        outcome = 'unacked'
        OR (engineer_slug IS NOT NULL AND ack_ts_ms IS NOT NULL
            AND mtta_ms IS NOT NULL AND ack_vector IS NOT NULL)
    ),
    CONSTRAINT deferred_boundary_only CHECK (
        delivery_mode != 'deferred' OR region = 'boundary_handoff'
    ),
    CONSTRAINT boundary_is_deferred CHECK (
        region != 'boundary_handoff' OR delivery_mode = 'deferred'
    ),
    CONSTRAINT delivered_after_emit CHECK (
        delivered_at_ms IS NULL OR delivered_at_ms >= emit_ts_ms
    )
);

INSERT INTO synthetic_page_drills_b072 (
    drill_id, region, severity, engineer_slug, emit_ts_ms, ack_ts_ms,
    ack_vector, mtta_ms, outcome, correlation_id, schema_version,
    delivery_mode, scheduled_at_ms, delivered_at_ms
)
SELECT
    drill_id, region, severity, engineer_slug, emit_ts_ms, ack_ts_ms,
    ack_vector, mtta_ms, outcome, correlation_id, schema_version,
    'immediate', emit_ts_ms, emit_ts_ms
FROM synthetic_page_drills;

DROP TABLE synthetic_page_drills;
ALTER TABLE synthetic_page_drills_b072 RENAME TO synthetic_page_drills;

CREATE INDEX IF NOT EXISTS idx_synthetic_drills_region_ts
    ON synthetic_page_drills (region, emit_ts_ms);
CREATE INDEX IF NOT EXISTS idx_synthetic_drills_outcome_ts
    ON synthetic_page_drills (outcome, emit_ts_ms);
CREATE INDEX IF NOT EXISTS idx_synthetic_drills_engineer
    ON synthetic_page_drills (engineer_slug)
    WHERE engineer_slug IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_synthetic_drills_pending_delivery
    ON synthetic_page_drills (delivery_mode, emit_ts_ms)
    WHERE delivery_mode = 'deferred';

CREATE TABLE IF NOT EXISTS synthetic_page_audit_events (
    event_id        TEXT    NOT NULL PRIMARY KEY,
    drill_id        TEXT    NOT NULL,
    event_type      TEXT    NOT NULL CHECK (event_type IN ('triggered', 'delivered', 'acked', 'escalated')),
    occurred_at_ms  BIGINT  NOT NULL,
    correlation_id  TEXT    NOT NULL,
    source_event_id TEXT    NOT NULL UNIQUE,
    engineer_slug   TEXT,
    FOREIGN KEY (drill_id) REFERENCES synthetic_page_drills (drill_id)
);

CREATE INDEX IF NOT EXISTS idx_synthetic_page_audit_drill_ts
    ON synthetic_page_audit_events (drill_id, occurred_at_ms);

PRAGMA foreign_keys = ON;
