-- Migration 0044: Stripe webhook idempotency table (R2-12 / WI-S19-004).
--
-- Stripe retries webhook deliveries up to 3 days on non-2xx response;
-- retries also occur for delivery timeouts. Every delivered event has
-- a unique `id` field (e.g. `evt_1Nf...`). We dedup on that id using
-- `INSERT OR IGNORE`:
--
--   - First delivery   → INSERT succeeds → dispatch handlers → ack 200.
--   - Retry delivery   → INSERT IGNOREd  → ack 200 immediately (no
--                        double-mutation; this is the HARD idempotency
--                        guarantee per spec).
--
-- This table is the durable mirror of `WebhookIdempotencyStore` in
-- `apps/server/src/webhook/`. The application performs the dedup
-- INSIDE the same logical transaction as the dispatch (the in-process
-- mirror uses an `Arc<Mutex<HashSet>>`; D1 production wiring uses a
-- single `BEGIN IMMEDIATE TRANSACTION`).
--
-- Idempotent / additive: every statement uses `IF NOT EXISTS`; replays
-- are safe.

-- ============================================================
-- stripe_webhook_events_processed — idempotency dedup table
-- ============================================================
CREATE TABLE IF NOT EXISTS stripe_webhook_events_processed (
    -- Stripe event id (`evt_…`). PRIMARY KEY enforces dedup.
    event_id      TEXT    NOT NULL PRIMARY KEY,
    -- Stripe event type at the time of processing (e.g.
    -- `customer.subscription.created`). Stored for forensics; NOT
    -- used by the dedup path.
    event_type    TEXT    NOT NULL,
    -- Unix epoch milliseconds when the row was inserted.
    processed_at_ms BIGINT NOT NULL,
    -- Outcome bucket: `dispatched` (handler ran, state mutated),
    -- `acknowledged_unknown` (event type not handled by us yet, ack
    -- with 200 for forward-compat), `signature_invalid_pre_insert`
    -- (NEVER stored here — signature-invalid path returns 400 BEFORE
    -- any DB write; documented for completeness).
    outcome       TEXT    NOT NULL
        CHECK (outcome IN ('dispatched', 'acknowledged_unknown')),
    -- Correlation id from the audit chain (PAT-CORRELATION-ID-001).
    correlation_id TEXT   NOT NULL
);

-- Look-ups by processed_at_ms (forensics + GC of >180-day-old rows
-- once retention policy is finalized).
CREATE INDEX IF NOT EXISTS idx_webhook_events_processed_at
    ON stripe_webhook_events_processed (processed_at_ms);

-- Look-ups by event_type (analytics — count of subscription.created vs
-- invoice.paid etc).
CREATE INDEX IF NOT EXISTS idx_webhook_events_processed_type
    ON stripe_webhook_events_processed (event_type);
