SQL_LINE: INSERT INTO tier_selections (tenant_id, tier, subscription_state, subscription_started_at_ms, correlation_id) VALUES (?, ?, 'active', ?, ?) ON CONFLICT(tenant_id) DO UPDATE SET tier = excluded.tier, subscription_state = 'active', subscription_started_at_ms = excluded.subscription_started_at_ms, correlation_id = excluded.correlation_id WHERE NOT EXISTS (SELECT 1 FROM tenant_billing tb WHERE tb.tenant_id = tier_selections.tenant_id AND tb.status = 'canceled')
PLACEHOLDERS: 4
TESTS: upsert_tier_do_update_guarded_against_canceled_resurrection upsert_tier_resurrection_guard_adds_zero_placeholders downgrade_tier_sql_unchanged
UNSURE: None
