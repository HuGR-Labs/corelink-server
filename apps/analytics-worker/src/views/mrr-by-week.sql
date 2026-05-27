-- View 4/7 — MRR by week (Stripe-derived, mirrored via webhooks).
--
-- Source: signup-worker/src/webhooks/stripe.ts emits
--   `paid_subscription_started` with properties.mrr_usd
--   `plan_downgraded` with properties.mrr_delta_usd (negative)
--   `subscription_canceled` with properties.mrr_delta_usd (negative)
-- per PLG framework §7.1 monetization rows.
--
-- The number is approximate — Stripe Sigma is the source of truth for
-- finance — but this view exists so the weekly email has a single column
-- per week without a Stripe API round-trip from the cron worker.

WITH delta AS (
    SELECT
        strftime('%Y-%W', created_at) AS iso_week,
        CASE
            WHEN event_name = 'paid_subscription_started'
                THEN COALESCE(json_extract(properties, '$.mrr_usd'), 0)
            WHEN event_name IN ('plan_downgraded', 'subscription_canceled')
                THEN COALESCE(json_extract(properties, '$.mrr_delta_usd'), 0)
            ELSE 0
        END AS delta_usd
    FROM analytics_events
    WHERE event_name IN ('paid_subscription_started', 'plan_downgraded', 'subscription_canceled')
      AND created_at >= datetime('now', '-90 days')
)
SELECT
    iso_week,
    ROUND(SUM(delta_usd), 2) AS net_delta_usd,
    -- Running total approximation: cumulative sum across visible weeks. The
    -- absolute number is a lower bound (because subscriptions started before
    -- the 90-day window are not visible here); the WoW delta is exact.
    ROUND(SUM(SUM(delta_usd)) OVER (ORDER BY iso_week
                                     ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW), 2) AS cumulative_mrr_usd
FROM delta
GROUP BY iso_week
ORDER BY iso_week DESC;
