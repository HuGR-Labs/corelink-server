-- Effect keys make a claimed webhook safe to replay after a process crash.
-- Ordinal reservation: 0132 is the next free D1 ordinal after 0131 on
-- integration base e70d9d9349f1ae96d71fd918817af8ad2bcd4762 (Refs #1629).
CREATE TABLE IF NOT EXISTS stripe_webhook_event_effects (
    event_id TEXT NOT NULL PRIMARY KEY,
    effect_key TEXT NOT NULL UNIQUE,
    payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64),
    fence INTEGER NOT NULL CHECK (fence > 0),
    effect_kind TEXT NOT NULL,
    applied_at_ms BIGINT NOT NULL,
    CHECK (length(effect_key) > 0)
);

CREATE INDEX IF NOT EXISTS idx_stripe_webhook_effects_applied
    ON stripe_webhook_event_effects (applied_at_ms);
