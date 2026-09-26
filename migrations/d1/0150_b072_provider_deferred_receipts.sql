-- B-072: terminal receipt for a synthetic drill whose alert provider is
-- explicitly deferred. Kept separate from the PagerDuty lifecycle projection
-- so its constrained `outcome` and append-only audit vocabularies stay intact.
CREATE TABLE IF NOT EXISTS synthetic_page_provider_receipts (
    drill_id                TEXT    NOT NULL PRIMARY KEY,
    scheduled_at_ms         BIGINT  NOT NULL,
    correlation_id          TEXT    NOT NULL,
    provider_mode           TEXT    NOT NULL CHECK (provider_mode = 'provider_deferred'),
    outcome                 TEXT    NOT NULL CHECK (outcome = 'provider_deferred'),
    scheduler_worker_revision TEXT   NOT NULL CHECK (length(scheduler_worker_revision) BETWEEN 1 AND 200),
    serving_sha             TEXT    NOT NULL CHECK (length(serving_sha) = 40 AND serving_sha NOT GLOB '*[^0-9a-fA-F]*'),
    receiver_worker_revision TEXT   NOT NULL CHECK (length(receiver_worker_revision) BETWEEN 1 AND 200),
    receiver_result         TEXT    NOT NULL CHECK (receiver_result = 'persisted_provider_deferred'),
    recorded_at_ms          BIGINT  NOT NULL,
    FOREIGN KEY (drill_id) REFERENCES synthetic_page_drills_b072 (drill_id)
);

CREATE TABLE IF NOT EXISTS synthetic_page_provider_audit_events (
    event_id       TEXT NOT NULL PRIMARY KEY,
    drill_id       TEXT NOT NULL UNIQUE,
    event_type     TEXT NOT NULL CHECK (event_type = 'provider_deferred'),
    occurred_at_ms BIGINT NOT NULL,
    correlation_id TEXT NOT NULL,
    source_event_id TEXT NOT NULL UNIQUE,
    FOREIGN KEY (drill_id) REFERENCES synthetic_page_provider_receipts (drill_id)
);

CREATE INDEX IF NOT EXISTS idx_synthetic_page_provider_receipts_scheduled
    ON synthetic_page_provider_receipts (scheduled_at_ms, serving_sha);
