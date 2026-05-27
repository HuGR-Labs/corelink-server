-- View 3/7 — Activation rate at D1, D7, D30 (3-window cohort).
--
-- Source: PLG framework §7.2 (3 windows).
-- Definition: for each cohort day in the last 60 days,
--   D1  = signups that produced `first_cache_hit` within 24 h
--   D7  = signups that produced `first_cache_hit` within 7 d
--   D30 = signups that produced `first_cache_hit` within 30 d
--
-- Three-numbers digest pulls D1 from here per PLG §7.5 target ≥ 35 %.

WITH cohort AS (
    SELECT
        tenant_id,
        MIN(created_at) AS signed_up_at
    FROM analytics_events
    WHERE event_name = 'signup_completed'
      AND tenant_id IS NOT NULL
      AND created_at >= datetime('now', '-60 days')
    GROUP BY tenant_id
)
SELECT
    date(c.signed_up_at) AS cohort_day,
    COUNT(*) AS signups,
    SUM(CASE WHEN EXISTS (
            SELECT 1 FROM analytics_events e
            WHERE e.tenant_id = c.tenant_id
              AND e.event_name = 'first_cache_hit'
              AND e.created_at <= datetime(c.signed_up_at, '+1 day')
        ) THEN 1 ELSE 0 END) AS activated_d1,
    SUM(CASE WHEN EXISTS (
            SELECT 1 FROM analytics_events e
            WHERE e.tenant_id = c.tenant_id
              AND e.event_name = 'first_cache_hit'
              AND e.created_at <= datetime(c.signed_up_at, '+7 days')
        ) THEN 1 ELSE 0 END) AS activated_d7,
    SUM(CASE WHEN EXISTS (
            SELECT 1 FROM analytics_events e
            WHERE e.tenant_id = c.tenant_id
              AND e.event_name = 'first_cache_hit'
              AND e.created_at <= datetime(c.signed_up_at, '+30 days')
        ) THEN 1 ELSE 0 END) AS activated_d30
FROM cohort c
GROUP BY cohort_day
ORDER BY cohort_day DESC;
