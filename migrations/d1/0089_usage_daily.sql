-- 0089_usage_daily.sql — per-tenant, per-UTC-day usage rollup for the customer
-- dashboard ROI surface (BE-1 reads/writes/daily + BE-2 cache hit-rate / $-saved).
--
-- DISPLAY telemetry, NOT billing. Billing stays authoritative on the synchronous
-- paths (`monthly_request_counts` for request caps, `tenant_quota`/
-- `tenant_storage_state` for bytes + spend). This table is fed by an in-process
-- batched aggregator (`crate::usage_meter`) that flushes accumulated deltas every
-- ~30s with an additive `+=` UPSERT — so it is eventually-consistent and may
-- under-count by at most one un-flushed window per container instance on an
-- eviction. That approximation is acceptable for a displayed hit-rate / savings
-- estimate and is NEVER used to bill or to gate a request (no hot-path read of it).
--
-- Counters (all monotonic non-negative, additive across container instances):
--   reads  — cache READ ops   (GET/download on cas/ac/bazel/turbo/oci/…)
--   writes — cache WRITE ops   (PUT/upload)
--   hits   — cache read that HIT  (200/found)
--   misses — cache read that MISSED (404/not-found)  → hit_rate = hits/(hits+misses)
--
-- Tenant-keyed → CLASSIFIED ERASE in the DSR adapter (ADR-S11-013): the tenant's
-- own operational usage state, no retention basis, removed on GDPR Art.17 erase.
CREATE TABLE IF NOT EXISTS usage_daily (
    tenant_id     TEXT NOT NULL,        -- canonical tenant_id
    day           TEXT NOT NULL,        -- 'YYYY-MM-DD' UTC bucket
    reads         INTEGER NOT NULL DEFAULT 0 CHECK (reads  >= 0),
    writes        INTEGER NOT NULL DEFAULT 0 CHECK (writes >= 0),
    hits          INTEGER NOT NULL DEFAULT 0 CHECK (hits   >= 0),
    misses        INTEGER NOT NULL DEFAULT 0 CHECK (misses >= 0),
    updated_at_ms INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, day)
);

-- Per-tenant time-window scan (the customer usage handler reads the current
-- period's rows for one tenant, newest-first).
CREATE INDEX IF NOT EXISTS idx_usage_daily_tenant_day
    ON usage_daily (tenant_id, day);
