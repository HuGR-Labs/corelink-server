-- Migration 0094: enforce DLQ idempotency on event_id (UNIQUE index).
--
-- # Why this exists
--
-- The `stripe_webhook_events_dlq` table (migration 0045) deliberately
-- kept `event_id` non-unique at the SQL level ("a single event_id may
-- produce multiple DLQ rows across failure-resolve-fail cycles") while
-- the APPLICATION-layer contract (`DLQ_IDEMPOTENT_ON_EVENT_ID`,
-- `crates/corelink-stripe-real/src/dlq.rs`) treats one event_id as ONE
-- row whose `attempt_count` increments on re-quarantine. The only store
-- ever wired was the in-memory one, which enforces the application
-- contract via a HashMap keyed by event_id.
--
-- Wiring the DURABLE D1-backed store requires the SQL layer to enforce
-- the SAME contract (`INSERT … ON CONFLICT(event_id) DO UPDATE`), which
-- needs a UNIQUE index on `event_id`. The table is empty in production
-- (no durable writer has ever been deployed), so backfilling conflicts
-- are impossible; if any environment ever did accumulate duplicate
-- event_ids, this CREATE UNIQUE INDEX would fail LOUDLY at migration
-- time rather than silently splitting audit trails.
--
-- # Idempotent / additive
--
-- `IF NOT EXISTS`; INV-AUTH-MIGRATION-ADDITIVE holds;
-- `scripts/check_migrations_additive.py` exits 0.

CREATE UNIQUE INDEX IF NOT EXISTS idx_webhook_dlq_event_id_unique
    ON stripe_webhook_events_dlq (event_id);
