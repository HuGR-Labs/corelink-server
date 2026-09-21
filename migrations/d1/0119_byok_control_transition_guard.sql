-- 0119_byok_control_transition_guard.sql
--
-- A D1 REST batch rolls back on a SQL error, but a conditional UPDATE which
-- matches zero rows is still successful SQL.  This assertion row turns a
-- stale/expired/mismatched BYOK transition capability into RAISE(ABORT), so
-- config/status/generation/outcome changes in the same batch are all-or-none.

CREATE TABLE IF NOT EXISTS byok_transition_commit_guard (
    token        TEXT PRIMARY KEY,
    tenant_id    TEXT NOT NULL,
    epoch        INTEGER NOT NULL CHECK (epoch > 0),
    action       TEXT NOT NULL CHECK (action IN ('prepare', 'activate', 'deactivate', 'shred', 'degrade', 'restore')),
    cmk_provider TEXT,
    cmk_key_id   TEXT,
    cmk_region   TEXT
);

CREATE TRIGGER IF NOT EXISTS trg_byok_transition_commit_guard
BEFORE INSERT ON byok_transition_commit_guard
WHEN NOT EXISTS (
    SELECT 1
      FROM byok_transition_fence f
      JOIN byok_tenant_gate g ON g.tenant_id = f.tenant_id
      JOIN tenant t ON t.tenant_id = f.tenant_id
      LEFT JOIN tenant_byok_config c ON c.tenant_id = f.tenant_id
     WHERE f.tenant_id = NEW.tenant_id
       AND f.token = NEW.token
       AND f.epoch = NEW.epoch
       AND f.outcome = 'active'
       AND f.expires_at_ms > (CAST(strftime('%s', 'now') AS INTEGER) * 1000)
       AND f.observed_gate_epoch = g.gate_epoch
       AND f.observed_generation = g.current_generation
       AND ((f.observed_config_version IS NULL AND c.config_version IS NULL)
            OR f.observed_config_version = c.config_version)
       AND f.observed_config_state = COALESCE(c.state, 'absent')
       AND f.observed_byok_status = t.byok_status
       AND COALESCE(c.state, 'absent') <> 'shredded'
       AND t.byok_status <> 'revoked'
       AND CASE NEW.action
           WHEN 'prepare' THEN
               t.byok_status = 'active'
               AND COALESCE(c.state, 'absent') IN ('absent', 'inactive', 'pending', 'active')
           WHEN 'activate' THEN 0
           WHEN 'deactivate' THEN
               c.state = 'pending' AND t.byok_status = 'active'
           WHEN 'shred' THEN
               c.state IN ('pending', 'active', 'partial')
           WHEN 'degrade' THEN
               c.state IN ('pending', 'active', 'partial') AND t.byok_status = 'active'
               AND ((c.cmk_provider = NEW.cmk_provider
                 AND c.cmk_key_id = NEW.cmk_key_id
                 AND c.cmk_region = NEW.cmk_region) OR EXISTS (
                   SELECT 1 FROM byok_activation_intent src
                    WHERE src.tenant_id=c.tenant_id AND src.source_generation>0
                     AND src.phase IN ('copy','published_partial','purging','ready_finalize')
                     AND src.source_cmk_provider=NEW.cmk_provider
                     AND src.source_cmk_key_id=NEW.cmk_key_id
                     AND src.source_cmk_region=NEW.cmk_region))
               AND (c.state <> 'pending' OR EXISTS (
                   SELECT 1 FROM byok_activation_intent a
                    WHERE a.tenant_id = c.tenant_id
                      AND a.phase IN ('copy','published_partial','purging','ready_finalize')
                      AND ((a.cmk_provider = NEW.cmk_provider
                        AND a.cmk_key_id = NEW.cmk_key_id
                        AND a.cmk_region = NEW.cmk_region)
                       OR (a.source_generation > 0
                        AND a.source_cmk_provider = NEW.cmk_provider
                        AND a.source_cmk_key_id = NEW.cmk_key_id
                        AND a.source_cmk_region = NEW.cmk_region))))
           WHEN 'restore' THEN
               c.state IN ('pending', 'active', 'partial')
               AND t.byok_status = 'degraded_read_only'
               AND ((c.cmk_provider = NEW.cmk_provider
                 AND c.cmk_key_id = NEW.cmk_key_id
                 AND c.cmk_region = NEW.cmk_region) OR EXISTS (
                   SELECT 1 FROM byok_activation_intent src
                    WHERE src.tenant_id=c.tenant_id AND src.source_generation>0
                     AND src.phase IN ('copy','published_partial','purging','ready_finalize')
                     AND src.source_cmk_provider=NEW.cmk_provider
                     AND src.source_cmk_key_id=NEW.cmk_key_id
                     AND src.source_cmk_region=NEW.cmk_region))
               AND (c.state <> 'pending' OR EXISTS (
                   SELECT 1 FROM byok_activation_intent a
                    WHERE a.tenant_id = c.tenant_id
                      AND a.phase IN ('copy','published_partial','purging','ready_finalize')
                      AND a.suspension_state = 'degraded_read_only'
                      AND ((a.cmk_provider = NEW.cmk_provider
                        AND a.cmk_key_id = NEW.cmk_key_id
                        AND a.cmk_region = NEW.cmk_region)
                       OR (a.source_generation > 0
                        AND a.source_cmk_provider = NEW.cmk_provider
                        AND a.source_cmk_key_id = NEW.cmk_key_id
                        AND a.source_cmk_region = NEW.cmk_region))))
           ELSE 0
       END
)
BEGIN
    SELECT RAISE(ABORT, 'invalid or stale BYOK transition capability');
END;

-- Per-tenant durable result and notification recipient.  Revocation delivery
-- is the customer activity row written by migration 0114 in the same status
-- transaction; this ledger makes its exact recipient and outcome retryable.
CREATE TABLE IF NOT EXISTS byok_control_outcome (
    token          TEXT PRIMARY KEY,
    tenant_id      TEXT NOT NULL,
    epoch          INTEGER NOT NULL CHECK (epoch > 0),
    action         TEXT NOT NULL CHECK (action IN ('prepare', 'activate', 'deactivate', 'shred', 'degrade', 'restore')),
    cmk_provider   TEXT,
    cmk_key_id     TEXT,
    cmk_region     TEXT,
    outcome        TEXT NOT NULL CHECK (outcome = 'completed'),
    alert_recipient TEXT NOT NULL,
    alert_outcome  TEXT NOT NULL
        CHECK (alert_outcome IN ('not_applicable', 'customer_activity_recorded')),
    completed_at_ms INTEGER NOT NULL,
    UNIQUE (tenant_id, epoch)
);

CREATE INDEX IF NOT EXISTS idx_byok_control_outcome_key
    ON byok_control_outcome
       (cmk_provider, cmk_key_id, cmk_region, action, alert_outcome, tenant_id);
