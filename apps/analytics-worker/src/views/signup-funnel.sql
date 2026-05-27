-- View 1/7 — Signup funnel (5-step) for trailing 30 days.
--
-- Source: PLG framework §7.1 taxonomy.
-- Question answered: "Of N landing visitors, how many reached each step?"
-- Output: 1 row, 5 columns (one count per funnel step).
--
-- Steps:
--   landing_view → signup_started → signup_completed → first_cli_authed → first_cache_hit
--
-- Cohort scope: events created in the last 30 days. The COUNT(DISTINCT) on
-- session_id at the top + tenant_id below stitches anonymous landing visits
-- to the eventual paid tenant via the session-id correlation set in PLG §7.1.

WITH window_30d AS (
    SELECT *
    FROM analytics_events
    WHERE created_at >= datetime('now', '-30 days')
)
SELECT
    (SELECT COUNT(DISTINCT session_id)
       FROM window_30d
       WHERE event_name = 'landing_view' AND session_id IS NOT NULL)
        AS landing_visitors,
    (SELECT COUNT(DISTINCT session_id)
       FROM window_30d
       WHERE event_name = 'signup_started' AND session_id IS NOT NULL)
        AS signups_started,
    (SELECT COUNT(DISTINCT user_id)
       FROM window_30d
       WHERE event_name = 'signup_completed' AND user_id IS NOT NULL)
        AS signups_completed,
    (SELECT COUNT(DISTINCT tenant_id)
       FROM window_30d
       WHERE event_name = 'first_cli_authed' AND tenant_id IS NOT NULL)
        AS first_cli_authed_tenants,
    (SELECT COUNT(DISTINCT tenant_id)
       FROM window_30d
       WHERE event_name = 'first_cache_hit' AND tenant_id IS NOT NULL)
        AS activated_tenants;
