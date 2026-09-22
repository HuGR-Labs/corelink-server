-- Durable, authenticated webhook inbox. Additive-only; never deletes legacy markers.
-- 0132 follows the Terraform summary compatibility migration. Its effect
-- ledger remains immediately after it in 0133 (Refs #1629).
CREATE TABLE IF NOT EXISTS stripe_webhook_event_inbox (
    event_id TEXT NOT NULL PRIMARY KEY,
    event_type TEXT NOT NULL,
    raw_body_hex TEXT,
    payload_sha256 TEXT,
    stripe_created_at_ms BIGINT,
    state TEXT NOT NULL CHECK (state IN
        ('received', 'claimed', 'completed', 'quarantined', 'legacy_ambiguous')),
    fence INTEGER NOT NULL DEFAULT 0 CHECK (fence >= 0),
    claim_owner TEXT,
    claim_expires_at_ms BIGINT,
    received_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    terminal_at_ms BIGINT,
    last_error TEXT,
    CHECK (state = 'legacy_ambiguous' OR
           (raw_body_hex IS NOT NULL AND payload_sha256 IS NOT NULL
            AND length(payload_sha256) = 64)),
    CHECK ((state = 'claimed') =
           (claim_owner IS NOT NULL AND claim_expires_at_ms IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_stripe_webhook_inbox_claim
    ON stripe_webhook_event_inbox (state, claim_expires_at_ms);
CREATE INDEX IF NOT EXISTS idx_stripe_webhook_inbox_received
    ON stripe_webhook_event_inbox (received_at_ms);

-- Old marker rows prove neither a body nor a completed effect. Preserve them
-- as manual-reconciliation work; they are deliberately not auto-replayed.
INSERT INTO stripe_webhook_event_inbox
    (event_id, event_type, state, received_at_ms, updated_at_ms, last_error)
SELECT event_id, event_type, 'legacy_ambiguous', processed_at_ms, processed_at_ms,
       'legacy processed marker lacks payload and completion proof'
FROM stripe_webhook_events_processed
WHERE 1
ON CONFLICT(event_id) DO NOTHING;
