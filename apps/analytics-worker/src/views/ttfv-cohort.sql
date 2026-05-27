-- View 2/7 — Time-to-first-value (TTFV) by cohort day.
--
-- Source: PLG framework §3.2 + §7.2.
-- Definition: minutes from `signup_completed` to `first_cache_hit`, computed
-- per tenant, then aggregated as median (p50) + p75 per cohort day (last 60d).
--
-- SQLite has no PERCENTILE_CONT, so we approximate via NTILE-based bucketing:
--   p50 = max(ttfv) of the bottom 50% of the cohort
--   p75 = max(ttfv) of the bottom 75% of the cohort
-- Good enough for the §7.5 three-numbers email (target ≤ 10 min median).

WITH signups AS (
    SELECT
        tenant_id,
        MIN(created_at) AS signed_up_at
    FROM analytics_events
    WHERE event_name = 'signup_completed'
      AND tenant_id IS NOT NULL
      AND created_at >= datetime('now', '-60 days')
    GROUP BY tenant_id
),
first_hits AS (
    SELECT
        tenant_id,
        MIN(created_at) AS first_hit_at
    FROM analytics_events
    WHERE event_name = 'first_cache_hit'
      AND tenant_id IS NOT NULL
    GROUP BY tenant_id
),
joined AS (
    SELECT
        s.tenant_id,
        date(s.signed_up_at) AS cohort_day,
        (julianday(f.first_hit_at) - julianday(s.signed_up_at)) * 24.0 * 60.0 AS ttfv_minutes
    FROM signups s
    JOIN first_hits f USING (tenant_id)
    WHERE f.first_hit_at > s.signed_up_at
),
ranked AS (
    SELECT
        cohort_day,
        ttfv_minutes,
        NTILE(4) OVER (PARTITION BY cohort_day ORDER BY ttfv_minutes) AS q4,
        NTILE(2) OVER (PARTITION BY cohort_day ORDER BY ttfv_minutes) AS q2
    FROM joined
)
SELECT
    cohort_day,
    COUNT(*) AS activated_n,
    ROUND(MAX(CASE WHEN q2 = 1 THEN ttfv_minutes END), 2) AS p50_minutes,
    ROUND(MAX(CASE WHEN q4 <= 3 THEN ttfv_minutes END), 2) AS p75_minutes
FROM ranked
GROUP BY cohort_day
ORDER BY cohort_day DESC;
