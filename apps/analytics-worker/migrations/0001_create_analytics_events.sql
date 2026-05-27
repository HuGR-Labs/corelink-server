-- Migration 0001: bootstrap analytics_events table + indexes.
-- Identical contents to src/schema.sql — `wrangler d1 migrations apply` will
-- treat this as the v0 baseline. The src/schema.sql copy is kept for
-- one-shot `wrangler d1 execute --file=src/schema.sql` bootstrap (acceptance §1).

CREATE TABLE IF NOT EXISTS analytics_events (
    id            TEXT NOT NULL PRIMARY KEY,
    event_name    TEXT NOT NULL,
    tenant_id     TEXT,
    user_id       TEXT,
    session_id    TEXT,
    properties    TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_events_name_created
    ON analytics_events (event_name, created_at);

CREATE INDEX IF NOT EXISTS idx_events_tenant_created
    ON analytics_events (tenant_id, created_at);

CREATE INDEX IF NOT EXISTS idx_events_first_cache_hit
    ON analytics_events (tenant_id, created_at)
    WHERE event_name = 'first_cache_hit';
