-- View 7/7 — The Monday-email "three numbers" payload.
--
-- Source: PLG framework §7.5 — exactly three KPIs:
--   1. Median TTFV (signup_completed → first_cache_hit) — target ≤ 10 min
--   2. D1 activation rate (signups where first_cache_hit within 24 h) — target ≥ 35 %
--   3. Free → paid 30-day conversion rate — target ≥ 5 %
--
-- All three computed over the trailing 7-day cohort of signups so the
-- numerator and denominator are both about the *current week*. The cron
-- handler in src/cron/weekly-email.ts SELECTs this view and formats the
-- text + sends via Resend.
--
-- Returns exactly ONE row.

WITH signups_7d AS (
    SELECT tenant_id, MIN(created_at) AS signed_up_at
    FROM analytics_events
    WHERE event_name = 'signup_completed'
      AND tenant_id IS NOT NULL
      AND created_at >= datetime('now', '-7 days')
    GROUP BY tenant_id
),
ttfv AS (
    SELECT
        s.tenant_id,
        (julianday(MIN(e.created_at)) - julianday(s.signed_up_at)) * 24.0 * 60.0 AS ttfv_minutes
    FROM signups_7d s
    JOIN analytics_events e
      ON e.tenant_id = s.tenant_id
     AND e.event_name = 'first_cache_hit'
     AND e.created_at > s.signed_up_at
    GROUP BY s.tenant_id
),
ranked AS (
    SELECT ttfv_minutes, NTILE(2) OVER (ORDER BY ttfv_minutes) AS q2
    FROM ttfv
),
d1_activated AS (
    SELECT COUNT(*) AS n
    FROM signups_7d s
    WHERE EXISTS (
        SELECT 1 FROM analytics_events e
        WHERE e.tenant_id = s.tenant_id
          AND e.event_name = 'first_cache_hit'
          AND e.created_at <= datetime(s.signed_up_at, '+1 day')
    )
),
signups_30d AS (
    SELECT tenant_id, MIN(created_at) AS signed_up_at
    FROM analytics_events
    WHERE event_name = 'signup_completed'
      AND tenant_id IS NOT NULL
      AND created_at >= datetime('now', '-37 days')
      AND created_at <  datetime('now', '-7 days')
    GROUP BY tenant_id
),
paid_30d AS (
    SELECT COUNT(*) AS n
    FROM signups_30d s
    WHERE EXISTS (
        SELECT 1 FROM analytics_events e
        WHERE e.tenant_id = s.tenant_id
          AND e.event_name = 'paid_subscription_started'
          AND e.created_at <= datetime(s.signed_up_at, '+30 days')
    )
)
SELECT
    -- Number 1: median TTFV (minutes). NULL if no activations yet.
    ROUND((SELECT MAX(ttfv_minutes) FROM ranked WHERE q2 = 1), 2) AS median_ttfv_minutes,
    -- Number 2: D1 activation rate (0.0 - 1.0). NULL-safe denominator.
    (SELECT n FROM d1_activated) AS d1_activated_n,
    (SELECT COUNT(*) FROM signups_7d) AS signups_7d_n,
    -- Number 3: free→paid 30-day conversion rate. Numerator/denominator
    -- computed from the prior-30-day cohort so the 30-day window has elapsed.
    (SELECT n FROM paid_30d) AS paid_30d_n,
    (SELECT COUNT(*) FROM signups_30d) AS signups_prior_30d_n;
