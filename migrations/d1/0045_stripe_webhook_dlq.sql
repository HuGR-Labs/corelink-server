-- Migration 0045: Stripe webhook dead-letter quarantine table.
--
-- Closes GAP-P0-1 + GAP-P0-2 + GAP-P0-3 from
-- `specs/_audits/sealed/2026-05-15-webhook-retry-dlq.md`.
--
-- # Why this table exists
--
-- The Stripe webhook handler at `apps/server/src/webhook.rs` relies on
-- Stripe's own 3-day retry window for failed deliveries. A
-- persistently-failing handler (e.g. tier-selection ledger bug that
-- only surfaces on a specific subscription shape) will be retried for
-- 3 days then **silently dropped** by Stripe with NO copy on the
-- CoreLink side.
--
-- This DLQ table captures every event that lands in the
-- `processing_failed` audit arm (handler dispatch returned `Err`) so
-- that:
--   1. An operator can inspect failure shape via D1 query.
--   2. After a fix-forward deploy, the operator can replay the
--      quarantined event with a dual-approval audit trail.
--   3. Prometheus alerts fire on `depth > 0` (warning) +
--      `oldest_age > 6h` (page).
--
-- # Idempotent / additive
--
-- Every statement uses `IF NOT EXISTS`; INV-AUTH-MIGRATION-ADDITIVE
-- holds; `scripts/check_migrations_additive.py` exits 0.
--
-- # Schema versioning
--
-- This is the first slot for webhook-DLQ-related schema. Forward
-- additive evolution (e.g. adding a `replay_outcome` column once the
-- admin API lands) will land as migration 0046+ with `ALTER TABLE …
-- ADD COLUMN` (allowed under INV-AUTH-MIGRATION-ADDITIVE).
--
-- # TTL policy
--
-- Rows are pruned by a scheduled job once `expires_at_ms < now_ms`.
-- Default TTL is 30 days from `first_seen_at_ms` (set by the
-- application layer; the SQL CHECK constraint only enforces
-- non-NULL + monotonic ordering).

-- ============================================================
-- stripe_webhook_events_dlq — quarantined exhausted-retry events
-- ============================================================
CREATE TABLE IF NOT EXISTS stripe_webhook_events_dlq (
    -- Stripe event id (`evt_…`). NOT a primary key here because a
    -- single event_id may produce multiple DLQ rows across
    -- non-overlapping failure-resolve-fail cycles; the canonical
    -- per-row identity is `dlq_row_id`.
    event_id          TEXT    NOT NULL,
    -- Per-row UUIDv7 identifier (application-generated). PRIMARY KEY.
    dlq_row_id        TEXT    NOT NULL PRIMARY KEY,
    -- Stripe event type at quarantine time (e.g.
    -- `customer.subscription.created`).
    event_type        TEXT    NOT NULL,
    -- Raw webhook body bytes (hex-encoded for safe TEXT storage; the
    -- original bytes were already validated by HMAC verify BEFORE
    -- this row was inserted, so the bytes are authentically
    -- Stripe-signed). Bounded by the canonical 64 KiB Stripe
    -- webhook payload limit; CHECK clause bounds storage cost.
    raw_body_hex      TEXT    NOT NULL CHECK (length(raw_body_hex) <= 200000),
    -- Correlation id from the audit chain.
    correlation_id    TEXT    NOT NULL,
    -- Number of dispatch attempts before quarantine (1 on first
    -- quarantine; incremented if the same event is re-DLQ'd after a
    -- replay attempt that also failed). Application layer enforces
    -- the increment.
    attempt_count     INTEGER NOT NULL DEFAULT 1 CHECK (attempt_count >= 1),
    -- First time this `event_id` was quarantined.
    first_seen_at_ms  BIGINT  NOT NULL,
    -- Most recent time this `event_id` was quarantined (== first_seen
    -- on creation).
    last_seen_at_ms   BIGINT  NOT NULL,
    -- Quarantine reason: redacted error string from the
    -- `processing_failed` audit arm. NEVER includes raw body or
    -- secrets per `apps/server/src/webhook.rs` invariants.
    last_error        TEXT    NOT NULL,
    -- Hard expiry. Pruning job removes rows past this point.
    -- Application default: first_seen_at_ms + 30 days.
    expires_at_ms     BIGINT  NOT NULL,
    -- Replay-request slots (forward-compatible with the admin-API
    -- dual-approval surface; NULL until a replay is initiated).
    replay_request_id TEXT,
    replayed_by       TEXT,
    replayed_at_ms    BIGINT,
    -- Replay outcome (NULL until replayed). Bounded enum: when the
    -- value is set it MUST be one of the canonical strings.
    replay_outcome    TEXT
        CHECK (replay_outcome IS NULL OR replay_outcome IN
              ('succeeded', 'failed', 'abandoned')),

    CHECK (last_seen_at_ms >= first_seen_at_ms),
    CHECK (expires_at_ms   >= first_seen_at_ms)
);

-- Look-ups by event_id (operator triage path).
CREATE INDEX IF NOT EXISTS idx_webhook_dlq_event_id
    ON stripe_webhook_events_dlq (event_id);

-- Look-ups by event_type (analytics + targeted-replay paths).
CREATE INDEX IF NOT EXISTS idx_webhook_dlq_event_type
    ON stripe_webhook_events_dlq (event_type);

-- Oldest-row first (pruning + age-alert hot paths).
CREATE INDEX IF NOT EXISTS idx_webhook_dlq_first_seen
    ON stripe_webhook_events_dlq (first_seen_at_ms);

-- Expiry-pruning index (the DELETE pruning job scans by expires_at_ms).
CREATE INDEX IF NOT EXISTS idx_webhook_dlq_expires_at
    ON stripe_webhook_events_dlq (expires_at_ms);

-- Replay-tracking index (admin-API replay listing path).
CREATE INDEX IF NOT EXISTS idx_webhook_dlq_replay_request
    ON stripe_webhook_events_dlq (replay_request_id);
