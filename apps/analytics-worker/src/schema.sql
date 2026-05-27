-- CoreLink analytics — single canonical event table.
-- Source: PLG framework §7.1 (`specs/_audits/2026-05-27-plg-onboarding-framework.md`).
-- Mandate: append-only, no PII, 13-month rolling retention enforced by §7.3
-- and the cleanup job (TODO: separate cron — out of Phase 0.G scope).
--
-- Privacy notes (per PLG §7.3):
--   - `properties` JSON MUST NOT carry email or IP. Use `email_domain` instead.
--   - Client-side events fire only if `cookies.analytics === true` per the
--     existing CookiePolicyPage gate. Server-side events fire regardless
--     under LGPD Art. 7 II / GDPR Art. 6(1)(b) legitimate-interest basis.

CREATE TABLE IF NOT EXISTS analytics_events (
    -- Monotonic ULID generated client-side; uniqueness enforced so retries
    -- of the same event_id are idempotent (POST /v1/event returns 200 on
    -- duplicate INSERT OR IGNORE).
    id            TEXT NOT NULL PRIMARY KEY,
    event_name    TEXT NOT NULL,
    -- Pseudonymous tenant identifier (ULID). NULL for pre-signup events
    -- (`landing_view`, `landing_cta_click`, `signup_started`).
    tenant_id     TEXT,
    -- Pseudonymous Clerk user_id. NULL for pre-signup events.
    user_id       TEXT,
    -- Anonymous browser session correlator (uuid v4 stored in
    -- `sessionStorage`, not a cookie). Lets us stitch landing → signup.
    session_id    TEXT,
    -- Free-form event properties — JSON. Schema per event documented in
    -- the PLG §7.1 taxonomy table; this column is intentionally schemaless
    -- to keep the ingest path one-row-one-write.
    properties    TEXT NOT NULL DEFAULT '{}',
    -- Wall-clock UTC, ISO-8601 (`2026-05-27T08:00:00Z`). String, not INTEGER,
    -- because every saved view in `views/` does `strftime('%Y-%m-%d', created_at)`
    -- which is cheaper than `datetime(created_at, 'unixepoch')`.
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- Index covers the two dominant access patterns:
--   1. Funnel cohort SQL: WHERE event_name = ? AND created_at >= ?
--   2. Per-tenant timeline: WHERE tenant_id = ? ORDER BY created_at
CREATE INDEX IF NOT EXISTS idx_events_name_created
    ON analytics_events (event_name, created_at);

CREATE INDEX IF NOT EXISTS idx_events_tenant_created
    ON analytics_events (tenant_id, created_at);

-- Partial index for the activation cohort hot path — `first_cache_hit` is the
-- §3.2 canonical event and is queried in every saved view.
CREATE INDEX IF NOT EXISTS idx_events_first_cache_hit
    ON analytics_events (tenant_id, created_at)
    WHERE event_name = 'first_cache_hit';
