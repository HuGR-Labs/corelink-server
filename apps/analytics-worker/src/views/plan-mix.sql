-- View 5/7 — Plan mix (free / pro / enterprise) at "now".
--
-- Computes the current plan of each tenant by taking the LAST plan-mutation
-- event observed:
--   `tenant_created`            sets initial plan (always 'free' post-§4 redesign)
--   `paid_subscription_started` sets plan = properties.plan
--   `plan_downgraded`           sets plan = properties.to_plan
--   `subscription_canceled`     sets plan = 'free' (back to free, not gone)
--
-- A tenant that never appears = excluded. A tenant whose last event is
-- canceled is counted as free, not lost — churn is a separate view.

WITH plan_events AS (
    SELECT
        tenant_id,
        created_at,
        CASE event_name
            WHEN 'tenant_created'            THEN COALESCE(json_extract(properties, '$.plan'), 'free')
            WHEN 'paid_subscription_started' THEN COALESCE(json_extract(properties, '$.plan'), 'pro')
            WHEN 'plan_downgraded'           THEN COALESCE(json_extract(properties, '$.to_plan'), 'free')
            WHEN 'subscription_canceled'     THEN 'free'
        END AS plan
    FROM analytics_events
    WHERE tenant_id IS NOT NULL
      AND event_name IN ('tenant_created', 'paid_subscription_started', 'plan_downgraded', 'subscription_canceled')
),
latest AS (
    SELECT tenant_id, plan,
           ROW_NUMBER() OVER (PARTITION BY tenant_id ORDER BY created_at DESC) AS rn
    FROM plan_events
)
SELECT
    plan,
    COUNT(*) AS tenants
FROM latest
WHERE rn = 1
GROUP BY plan
ORDER BY tenants DESC;
