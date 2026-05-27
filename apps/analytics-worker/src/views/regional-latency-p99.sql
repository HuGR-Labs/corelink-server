-- View 6/7 — Per-region cache-hit latency p99 (cas-worker emitted).
--
-- Source: cas-worker middleware (apps/cas-worker/src/middleware/analytics.ts)
-- emits `first_cache_hit` plus engagement events with properties
--   { latency_ms: integer, cf_colo: 'GRU'|'IAD'|'LHR'|'NRT'|'SYD'|... }
-- per Phase 0.G mandate + Wave-33 Stream B contract.
--
-- SQLite has no PERCENTILE_CONT — approximate p99 via NTILE(100), pick top
-- bucket. Bucket = COUNT(*) / 100, so the result is rough for low-N regions.

WITH samples AS (
    SELECT
        COALESCE(json_extract(properties, '$.cf_colo'), 'unknown') AS cf_colo,
        CAST(json_extract(properties, '$.latency_ms') AS REAL) AS latency_ms
    FROM analytics_events
    WHERE event_name IN ('first_cache_hit', 'cache_hits_10', 'cache_hits_100', 'cache_hits_1k')
      AND json_extract(properties, '$.latency_ms') IS NOT NULL
      AND created_at >= datetime('now', '-7 days')
),
bucketed AS (
    SELECT
        cf_colo,
        latency_ms,
        NTILE(100) OVER (PARTITION BY cf_colo ORDER BY latency_ms) AS pct_bucket
    FROM samples
)
SELECT
    cf_colo,
    COUNT(*) AS samples,
    ROUND(AVG(latency_ms), 2) AS mean_ms,
    ROUND(MAX(CASE WHEN pct_bucket <= 50 THEN latency_ms END), 2) AS p50_ms,
    ROUND(MAX(CASE WHEN pct_bucket <= 95 THEN latency_ms END), 2) AS p95_ms,
    ROUND(MAX(CASE WHEN pct_bucket <= 99 THEN latency_ms END), 2) AS p99_ms
FROM bucketed
GROUP BY cf_colo
ORDER BY samples DESC;
