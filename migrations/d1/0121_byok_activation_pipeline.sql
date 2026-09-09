-- 0121_byok_activation_pipeline.sql
--
-- Durable, multi-replica-safe activation pipeline.  This is intentionally
-- additive: the 0120 backfill ledger remains forensic history while new
-- activation workers use these capability-scoped tables.

-- Rotation decoder authority. The exact prior wrapped secret and provider
-- identity survive target-policy replacement until every source object using
-- that version has been purge-verified.
CREATE TABLE IF NOT EXISTS tenant_byok_secret_history (
    tenant_id          TEXT NOT NULL,
    tcs_version        INTEGER NOT NULL CHECK (tcs_version > 0),
    cmk_provider       TEXT NOT NULL,
    cmk_key_id         TEXT NOT NULL,
    cmk_region         TEXT NOT NULL,
    tcs_wrapped        BLOB,
    wrapped_size       INTEGER NOT NULL CHECK (wrapped_size >= 0),
    archived_at_ms     INTEGER NOT NULL CHECK (archived_at_ms >= 0),
    retired_at_ms      INTEGER,
    PRIMARY KEY (tenant_id, tcs_version),
    CHECK ((tcs_wrapped IS NULL) = (retired_at_ms IS NOT NULL))
);

-- Immutable control-policy snapshot paired with a source generation.  The
-- rotation decoder resolves this exact version rather than consulting the
-- mutable current row.
CREATE TABLE IF NOT EXISTS tenant_byok_config_history (
    tenant_id          TEXT NOT NULL,
    config_version     INTEGER NOT NULL CHECK (config_version > 0),
    mode               TEXT NOT NULL CHECK (mode IN ('managed','byok','hyok')),
    crypto_mode        TEXT NOT NULL CHECK (crypto_mode IN ('convergent','random')),
    cmk_provider       TEXT,
    cmk_key_id         TEXT,
    cmk_region         TEXT,
    state              TEXT NOT NULL CHECK (state IN ('inactive','pending','active','partial','shredded')),
    archived_at_ms     INTEGER NOT NULL CHECK (archived_at_ms >= 0),
    PRIMARY KEY (tenant_id, config_version)
);

CREATE TRIGGER IF NOT EXISTS trg_byok_archive_config_before_rotation
BEFORE UPDATE ON tenant_byok_config
WHEN OLD.config_version IS NOT NEW.config_version
  OR OLD.mode IS NOT NEW.mode
  OR OLD.crypto_mode IS NOT NEW.crypto_mode
  OR OLD.cmk_provider IS NOT NEW.cmk_provider
  OR OLD.cmk_key_id IS NOT NEW.cmk_key_id
  OR OLD.cmk_region IS NOT NEW.cmk_region
  OR OLD.state IS NOT NEW.state
BEGIN
    INSERT OR IGNORE INTO tenant_byok_config_history
        (tenant_id,config_version,mode,crypto_mode,cmk_provider,cmk_key_id,
         cmk_region,state,archived_at_ms)
    VALUES
        (OLD.tenant_id,OLD.config_version,OLD.mode,OLD.crypto_mode,
         OLD.cmk_provider,OLD.cmk_key_id,OLD.cmk_region,OLD.state,
         (CAST(strftime('%s','now') AS INTEGER) * 1000));
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_archive_tcs_before_config_rotation
BEFORE UPDATE OF cmk_provider, cmk_key_id, cmk_region ON tenant_byok_config
WHEN (OLD.cmk_provider IS NOT NEW.cmk_provider
      OR OLD.cmk_key_id IS NOT NEW.cmk_key_id
      OR OLD.cmk_region IS NOT NEW.cmk_region)
 AND OLD.cmk_provider IS NOT NULL AND OLD.cmk_key_id IS NOT NULL
 AND OLD.cmk_region IS NOT NULL
BEGIN
    INSERT OR IGNORE INTO tenant_byok_secret_history
        (tenant_id,tcs_version,cmk_provider,cmk_key_id,cmk_region,tcs_wrapped,
         wrapped_size,archived_at_ms)
    SELECT OLD.tenant_id,s.tcs_version,OLD.cmk_provider,OLD.cmk_key_id,
           OLD.cmk_region,s.tcs_wrapped,length(s.tcs_wrapped),
           (CAST(strftime('%s','now') AS INTEGER) * 1000)
      FROM tenant_byok_secret s
     WHERE s.tenant_id=OLD.tenant_id AND s.tcs_wrapped IS NOT NULL
       AND s.cmk_key_id=OLD.cmk_key_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_archive_tcs_before_secret_rotation
BEFORE UPDATE OF tcs_wrapped, tcs_version, cmk_key_id ON tenant_byok_secret
WHEN OLD.tcs_wrapped IS NOT NULL
 AND (OLD.tcs_wrapped IS NOT NEW.tcs_wrapped
      OR OLD.tcs_version IS NOT NEW.tcs_version
      OR OLD.cmk_key_id IS NOT NEW.cmk_key_id)
BEGIN
    INSERT OR IGNORE INTO tenant_byok_secret_history
        (tenant_id,tcs_version,cmk_provider,cmk_key_id,cmk_region,tcs_wrapped,
         wrapped_size,archived_at_ms)
    SELECT OLD.tenant_id,OLD.tcs_version,
           COALESCE((SELECT h.cmk_provider FROM tenant_byok_config_history h
                     WHERE h.tenant_id=OLD.tenant_id AND h.cmk_key_id=OLD.cmk_key_id
                     ORDER BY h.config_version DESC LIMIT 1),c.cmk_provider),
           OLD.cmk_key_id,
           COALESCE((SELECT h.cmk_region FROM tenant_byok_config_history h
                     WHERE h.tenant_id=OLD.tenant_id AND h.cmk_key_id=OLD.cmk_key_id
                     ORDER BY h.config_version DESC LIMIT 1),c.cmk_region),
           OLD.tcs_wrapped,length(OLD.tcs_wrapped),
           (CAST(strftime('%s','now') AS INTEGER) * 1000)
      FROM tenant_byok_config c
     WHERE c.tenant_id=OLD.tenant_id AND OLD.cmk_key_id IS NOT NULL
       AND (c.cmk_key_id=OLD.cmk_key_id OR EXISTS (
           SELECT 1 FROM tenant_byok_config_history h
            WHERE h.tenant_id=OLD.tenant_id AND h.cmk_key_id=OLD.cmk_key_id));
END;

CREATE TABLE IF NOT EXISTS byok_activation_guard (
    guard_id             TEXT PRIMARY KEY,
    tenant_id            TEXT NOT NULL,
    transition_token     TEXT NOT NULL,
    transition_epoch     INTEGER NOT NULL CHECK (transition_epoch > 0),
    capability_token     TEXT NOT NULL UNIQUE,
    epoch                INTEGER NOT NULL CHECK (epoch > 0),
    action               TEXT NOT NULL
        CHECK (action IN ('activate', 'deactivate', 'shred')),
    priority             INTEGER NOT NULL CHECK (priority IN (10, 20, 30)),
    outcome              TEXT NOT NULL DEFAULT 'active'
        CHECK (outcome IN ('active', 'committed', 'aborted', 'preempted', 'expired')),
    acquired_at_ms       INTEGER NOT NULL CHECK (acquired_at_ms >= 0),
    expires_at_ms        INTEGER NOT NULL CHECK (expires_at_ms > acquired_at_ms),
    completed_at_ms      INTEGER,
    FOREIGN KEY (transition_token) REFERENCES byok_transition_fence(token),
    UNIQUE (tenant_id, epoch),
    CHECK (priority = CASE action
        WHEN 'activate' THEN 10 WHEN 'deactivate' THEN 20 WHEN 'shred' THEN 30 END),
    CHECK ((outcome = 'active') = (completed_at_ms IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_byok_activation_guard_live_tenant
    ON byok_activation_guard (tenant_id) WHERE outcome = 'active';
CREATE INDEX IF NOT EXISTS idx_byok_activation_guard_expiry
    ON byok_activation_guard (outcome, expires_at_ms, tenant_id);

-- Ephemeral, transaction-local handoff of the exact source custody identity.
-- Preparation inserts this from current rows before replacing policy, copies it
-- into the durable intent, proves the resulting history, then deletes it.
CREATE TABLE IF NOT EXISTS byok_activation_source_capture (
    transition_token       TEXT PRIMARY KEY,
    tenant_id              TEXT NOT NULL,
    source_generation      INTEGER NOT NULL CHECK (source_generation >= 0),
    source_config_version  INTEGER,
    source_cmk_provider    TEXT,
    source_cmk_key_id      TEXT,
    source_cmk_region      TEXT,
    source_tcs_version     INTEGER,
    captured_at_ms         INTEGER NOT NULL CHECK (captured_at_ms >= 0),
    FOREIGN KEY (transition_token) REFERENCES byok_transition_fence(token),
    CHECK ((source_generation = 0 AND source_config_version IS NULL
            AND source_cmk_provider IS NULL AND source_cmk_key_id IS NULL
            AND source_cmk_region IS NULL AND source_tcs_version IS NULL)
        OR (source_generation > 0 AND source_config_version IS NOT NULL
            AND source_cmk_provider IS NOT NULL AND source_cmk_key_id IS NOT NULL
            AND source_cmk_region IS NOT NULL AND source_tcs_version IS NOT NULL))
);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_source_capture_exact
BEFORE INSERT ON byok_activation_source_capture
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_transition_fence f
        JOIN byok_tenant_gate g ON g.tenant_id=f.tenant_id
        WHERE f.token=NEW.transition_token AND f.tenant_id=NEW.tenant_id
          AND f.outcome='active'
          AND f.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
          AND f.observed_generation=NEW.source_generation
          AND g.current_generation=NEW.source_generation
          AND CASE WHEN NEW.source_generation=0 THEN 1 ELSE
              f.observed_config_state='active'
              AND f.observed_config_version=NEW.source_config_version
              AND EXISTS (SELECT 1 FROM tenant_byok_config c
                  JOIN tenant_byok_secret s ON s.tenant_id=c.tenant_id
                  WHERE c.tenant_id=NEW.tenant_id AND c.state='active'
                    AND c.config_version=NEW.source_config_version
                    AND c.cmk_provider=NEW.source_cmk_provider
                    AND c.cmk_key_id=NEW.source_cmk_key_id
                    AND c.cmk_region=NEW.source_cmk_region
                    AND s.tcs_version=NEW.source_tcs_version
                    AND s.cmk_key_id=NEW.source_cmk_key_id
                    AND s.tcs_wrapped IS NOT NULL)
              END
    ) THEN RAISE(ABORT, 'invalid activation source capture') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_source_capture_immutable
BEFORE UPDATE ON byok_activation_source_capture
BEGIN
    SELECT RAISE(ABORT, 'activation source capture is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_guard_classify_expired
BEFORE INSERT ON byok_activation_guard
BEGIN
    UPDATE byok_activation_guard
       SET outcome = 'expired', completed_at_ms = expires_at_ms
     WHERE tenant_id = NEW.tenant_id AND outcome = 'active'
       AND expires_at_ms <= (CAST(strftime('%s','now') AS INTEGER) * 1000);
    UPDATE byok_transition_fence
       SET outcome = 'expired', completed_at_ms = expires_at_ms
     WHERE token IN (SELECT transition_token FROM byok_activation_guard
                     WHERE tenant_id = NEW.tenant_id AND outcome = 'expired')
       AND outcome = 'active'
       AND expires_at_ms <= (CAST(strftime('%s','now') AS INTEGER) * 1000);
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_guard_renew_takeover
BEFORE UPDATE OF transition_token, transition_epoch, capability_token, epoch,
                 acquired_at_ms, expires_at_ms, outcome
ON byok_activation_guard
WHEN NOT (
    OLD.outcome = 'active' AND NEW.outcome = 'active'
    AND OLD.transition_token = NEW.transition_token
    AND OLD.transition_epoch = NEW.transition_epoch
    AND OLD.capability_token = NEW.capability_token AND OLD.epoch = NEW.epoch
    AND OLD.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
    AND NEW.expires_at_ms > OLD.expires_at_ms
) AND NOT (
    OLD.outcome IN ('active', 'expired') AND NEW.outcome = 'active'
    AND (OLD.outcome = 'expired'
         OR OLD.expires_at_ms <= (CAST(strftime('%s','now') AS INTEGER) * 1000))
    AND NEW.capability_token <> OLD.capability_token AND NEW.epoch = OLD.epoch + 1
    AND NEW.transition_token <> OLD.transition_token
    AND NEW.transition_epoch > OLD.transition_epoch
    AND NEW.acquired_at_ms = (CAST(strftime('%s','now') AS INTEGER) * 1000)
    AND NEW.expires_at_ms > NEW.acquired_at_ms AND NEW.completed_at_ms IS NULL
) AND NOT (
    OLD.outcome = 'active' AND NEW.outcome IN ('committed','aborted','preempted','expired')
    AND NEW.completed_at_ms IS NOT NULL
) AND NOT (
    OLD.outcome = 'expired' AND NEW.outcome IN ('aborted','preempted')
    AND OLD.transition_token = NEW.transition_token
    AND OLD.transition_epoch = NEW.transition_epoch
    AND OLD.capability_token = NEW.capability_token AND OLD.epoch = NEW.epoch
    AND OLD.acquired_at_ms = NEW.acquired_at_ms
    AND OLD.expires_at_ms = NEW.expires_at_ms
    AND NEW.completed_at_ms IS NOT NULL
) AND NOT (
    OLD.outcome = 'active' AND NEW.outcome = 'active'
    AND NEW.capability_token <> OLD.capability_token AND NEW.epoch = OLD.epoch + 1
    AND NEW.transition_token <> OLD.transition_token
    AND NEW.transition_epoch > OLD.transition_epoch
    AND NEW.acquired_at_ms = (CAST(strftime('%s','now') AS INTEGER) * 1000)
    AND NEW.expires_at_ms > NEW.acquired_at_ms
    AND EXISTS (SELECT 1 FROM byok_activation_intent a
                WHERE a.guard_id = OLD.guard_id AND a.phase = 'published_partial'
                  AND a.publication_gate_epoch IS NOT NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'invalid BYOK activation guard mutation');
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_guard_release_prior_fence
AFTER UPDATE OF transition_token ON byok_activation_guard
WHEN OLD.transition_token IS NOT NEW.transition_token
BEGIN
    UPDATE byok_transition_fence
       SET outcome = CASE
           WHEN expires_at_ms <= (CAST(strftime('%s','now') AS INTEGER) * 1000)
               THEN 'expired' ELSE 'committed' END,
           completed_at_ms = CASE
           WHEN expires_at_ms <= (CAST(strftime('%s','now') AS INTEGER) * 1000)
               THEN expires_at_ms
           ELSE (CAST(strftime('%s','now') AS INTEGER) * 1000) END
     WHERE token = OLD.transition_token AND outcome = 'active';
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_guard_snapshot
BEFORE INSERT ON byok_activation_guard
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_transition_fence f
         WHERE f.token = NEW.transition_token AND f.tenant_id = NEW.tenant_id
           AND f.epoch = NEW.transition_epoch AND f.outcome = 'active'
           AND f.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
    ) OR EXISTS (
        SELECT 1 FROM byok_data_intent i WHERE i.tenant_id = NEW.tenant_id
          AND i.outcome = 'active'
          AND i.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
    ) THEN RAISE(ABORT, 'invalid or non-exclusive BYOK activation transition') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_guard_takeover_snapshot
BEFORE UPDATE OF transition_token, transition_epoch ON byok_activation_guard
WHEN OLD.transition_token IS NOT NEW.transition_token
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_transition_fence f
         WHERE f.token = NEW.transition_token AND f.tenant_id = NEW.tenant_id
           AND f.epoch = NEW.transition_epoch AND f.outcome = 'active'
           AND f.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
    ) THEN RAISE(ABORT, 'invalid BYOK activation transition takeover') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_data_intent_activation_barrier
BEFORE INSERT ON byok_data_intent
WHEN EXISTS (
    SELECT 1 FROM byok_activation_intent a
     WHERE a.tenant_id = NEW.tenant_id AND a.phase = 'copy'
) OR EXISTS (
    SELECT 1 FROM byok_activation_guard f
     WHERE f.tenant_id = NEW.tenant_id AND f.outcome = 'active'
       AND f.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
)
BEGIN
    SELECT RAISE(ABORT, 'BYOK activation transition excludes data intents');
END;

CREATE TABLE IF NOT EXISTS byok_activation_intent (
    intent_id                  TEXT PRIMARY KEY,
    tenant_id                  TEXT NOT NULL,
    guard_id                   TEXT NOT NULL UNIQUE,
    request_blake3             TEXT NOT NULL,
    config_version             INTEGER NOT NULL CHECK (config_version > 0),
    mode                       TEXT NOT NULL CHECK (mode IN ('byok', 'hyok')),
    crypto_mode                TEXT NOT NULL CHECK (crypto_mode IN ('convergent', 'random')),
    cmk_provider               TEXT,
    cmk_key_id                 TEXT,
    cmk_region                 TEXT,
    policy_blake3              TEXT NOT NULL,
    tcs_version                INTEGER NOT NULL CHECK (tcs_version > 0),
    wrapped_tcs_blake3         TEXT,
    source_generation          INTEGER NOT NULL CHECK (source_generation >= 0),
    source_config_version      INTEGER,
    source_cmk_provider        TEXT,
    source_cmk_key_id          TEXT,
    source_cmk_region          TEXT,
    source_tcs_version         INTEGER,
    target_generation          INTEGER NOT NULL
        CHECK (target_generation = source_generation + 1),
    observed_gate_epoch        INTEGER NOT NULL CHECK (observed_gate_epoch > 0),
    publication_gate_epoch     INTEGER,
    published_config_version   INTEGER,
    phase                      TEXT NOT NULL DEFAULT 'copy'
        CHECK (phase IN ('copy', 'published_partial', 'purging',
                         'ready_finalize', 'committed', 'aborted', 'preempted')),
    suspension_state           TEXT NOT NULL DEFAULT 'active'
        CHECK (suspension_state IN ('active', 'degraded_read_only')),
    suspended_at_ms            INTEGER,
    cas_after_logical_key      TEXT,
    ac_after_logical_key       TEXT,
    cas_publish_after_key      TEXT,
    ac_publish_after_key       TEXT,
    purge_after_key            TEXT,
    cas_complete               INTEGER NOT NULL DEFAULT 0 CHECK (cas_complete IN (0, 1)),
    ac_complete                INTEGER NOT NULL DEFAULT 0 CHECK (ac_complete IN (0, 1)),
    claim_owner                TEXT,
    claim_token                TEXT UNIQUE,
    claim_epoch                INTEGER NOT NULL DEFAULT 0 CHECK (claim_epoch >= 0),
    claim_expires_at_ms        INTEGER,
    state_version              INTEGER NOT NULL DEFAULT 1 CHECK (state_version > 0),
    deadline_at_ms             INTEGER NOT NULL,
    created_at_ms              INTEGER NOT NULL CHECK (created_at_ms >= 0),
    checkpointed_at_ms         INTEGER NOT NULL CHECK (checkpointed_at_ms >= created_at_ms),
    completed_at_ms            INTEGER,
    failure_reason             TEXT,
    FOREIGN KEY (guard_id) REFERENCES byok_activation_guard(guard_id),
    CHECK (deadline_at_ms > created_at_ms),
    CHECK (length(request_blake3) = 64 AND request_blake3 NOT GLOB '*[^0-9a-f]*'),
    CHECK (length(policy_blake3) = 64 AND policy_blake3 NOT GLOB '*[^0-9a-f]*'),
    CHECK ((suspension_state = 'degraded_read_only') = (suspended_at_ms IS NOT NULL)),
    CHECK ((publication_gate_epoch IS NULL) = (published_config_version IS NULL)),
    CHECK (phase NOT IN ('published_partial', 'purging', 'ready_finalize', 'committed')
           OR publication_gate_epoch IS NOT NULL),
    CHECK (phase NOT IN ('copy', 'aborted') OR publication_gate_epoch IS NULL),
    CHECK ((claim_owner IS NULL AND claim_token IS NULL AND claim_expires_at_ms IS NULL)
        OR (claim_owner IS NOT NULL AND claim_token IS NOT NULL
            AND claim_expires_at_ms IS NOT NULL)),
    CHECK (phase NOT IN ('committed', 'aborted', 'preempted')
           OR (claim_owner IS NULL AND claim_token IS NULL AND claim_expires_at_ms IS NULL)),
    CHECK ((phase IN ('committed', 'aborted', 'preempted')) =
           (completed_at_ms IS NOT NULL)),
    CHECK ((phase IN ('aborted', 'preempted')) = (failure_reason IS NOT NULL)),
    CHECK (phase IN ('copy', 'aborted', 'preempted')
           OR (cas_complete = 1 AND ac_complete = 1)),
    CHECK (phase = 'preempted' OR (cmk_provider IS NOT NULL
           AND cmk_key_id IS NOT NULL AND cmk_region IS NOT NULL)),
    CHECK ((source_generation = 0 AND source_config_version IS NULL
            AND source_cmk_provider IS NULL AND source_cmk_key_id IS NULL
            AND source_cmk_region IS NULL AND source_tcs_version IS NULL)
        OR (source_generation > 0 AND source_config_version IS NOT NULL
            AND source_cmk_provider IS NOT NULL AND source_cmk_key_id IS NOT NULL
            AND source_cmk_region IS NOT NULL AND source_tcs_version IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_byok_activation_live_tenant
    ON byok_activation_intent (tenant_id)
    WHERE phase IN ('copy', 'published_partial', 'purging', 'ready_finalize');
CREATE UNIQUE INDEX IF NOT EXISTS idx_byok_activation_live_request
    ON byok_activation_intent (tenant_id, request_blake3)
    WHERE phase IN ('copy', 'published_partial', 'purging', 'ready_finalize');
CREATE UNIQUE INDEX IF NOT EXISTS idx_byok_activation_live_generation
    ON byok_activation_intent (tenant_id, target_generation)
    WHERE phase IN ('copy', 'published_partial', 'purging', 'ready_finalize');
CREATE INDEX IF NOT EXISTS idx_byok_activation_claimable
    ON byok_activation_intent (phase, claim_expires_at_ms, checkpointed_at_ms, intent_id);

CREATE TABLE IF NOT EXISTS byok_activation_key_health (
    intent_id       TEXT NOT NULL,
    tenant_id       TEXT NOT NULL,
    dependency_role TEXT NOT NULL CHECK (dependency_role IN ('source','target')),
    cmk_provider    TEXT NOT NULL,
    cmk_key_id      TEXT NOT NULL,
    cmk_region      TEXT NOT NULL,
    access_state    TEXT NOT NULL DEFAULT 'healthy'
        CHECK (access_state IN ('healthy','unavailable')),
    checked_at_ms   INTEGER NOT NULL,
    PRIMARY KEY (intent_id, dependency_role),
    FOREIGN KEY (intent_id) REFERENCES byok_activation_intent(intent_id)
);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_intent_snapshot
BEFORE INSERT ON byok_activation_intent
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
          FROM byok_activation_guard f
          JOIN byok_transition_fence tf ON tf.token = f.transition_token
          JOIN byok_tenant_gate g ON g.tenant_id = f.tenant_id
          JOIN tenant t ON t.tenant_id = f.tenant_id
          JOIN tenant_byok_config c ON c.tenant_id = f.tenant_id
          JOIN tenant_byok_secret s ON s.tenant_id = f.tenant_id
         WHERE f.guard_id = NEW.guard_id AND f.tenant_id = NEW.tenant_id
           AND f.action = 'activate' AND f.outcome = 'active'
           AND tf.tenant_id = NEW.tenant_id AND tf.epoch = f.transition_epoch
           AND tf.outcome = 'active'
           AND tf.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
           AND f.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
           AND g.gate_epoch = NEW.observed_gate_epoch
           AND g.current_generation = NEW.source_generation
           AND c.config_version = NEW.config_version
           AND c.mode = NEW.mode AND c.crypto_mode = NEW.crypto_mode
           AND c.cmk_provider = NEW.cmk_provider
           AND c.cmk_key_id = NEW.cmk_key_id AND c.cmk_region = NEW.cmk_region
           AND c.state = 'pending'
           AND t.byok_status = 'active'
           AND s.tcs_version = NEW.tcs_version AND s.cmk_key_id = NEW.cmk_key_id
           AND CASE WHEN NEW.source_generation=0 THEN 1 ELSE
               tf.observed_config_version=NEW.source_config_version
               AND tf.observed_config_state='active'
               AND EXISTS (SELECT 1 FROM byok_activation_source_capture x
                   WHERE x.transition_token=tf.token AND x.tenant_id=NEW.tenant_id
                     AND x.source_generation=NEW.source_generation
                     AND x.source_config_version=NEW.source_config_version
                     AND x.source_cmk_provider=NEW.source_cmk_provider
                     AND x.source_cmk_key_id=NEW.source_cmk_key_id
                     AND x.source_cmk_region=NEW.source_cmk_region
                     AND x.source_tcs_version=NEW.source_tcs_version)
               AND EXISTS (SELECT 1 FROM tenant_byok_config_history h
                   WHERE h.tenant_id=NEW.tenant_id
                     AND h.config_version=NEW.source_config_version
                     AND h.cmk_provider=NEW.source_cmk_provider
                     AND h.cmk_key_id=NEW.source_cmk_key_id
                     AND h.cmk_region=NEW.source_cmk_region
                     AND h.state='active')
               AND EXISTS (SELECT 1 FROM tenant_byok_secret_history sh
                   WHERE sh.tenant_id=NEW.tenant_id
                     AND sh.tcs_version=NEW.source_tcs_version
                     AND sh.cmk_provider=NEW.source_cmk_provider
                     AND sh.cmk_key_id=NEW.source_cmk_key_id
                     AND sh.cmk_region=NEW.source_cmk_region
                     AND sh.tcs_wrapped IS NOT NULL)
               END
    ) THEN RAISE(ABORT, 'invalid BYOK activation snapshot') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_intent_forward_only
BEFORE UPDATE ON byok_activation_intent
WHEN OLD.intent_id IS NOT NEW.intent_id
  OR OLD.tenant_id IS NOT NEW.tenant_id
  OR OLD.guard_id IS NOT NEW.guard_id
  OR OLD.request_blake3 IS NOT NEW.request_blake3
  OR OLD.config_version IS NOT NEW.config_version
  OR OLD.mode IS NOT NEW.mode
  OR OLD.crypto_mode IS NOT NEW.crypto_mode
  OR (OLD.cmk_provider IS NOT NEW.cmk_provider
      AND NOT (NEW.phase = 'preempted' AND NEW.cmk_provider IS NULL))
  OR (OLD.cmk_key_id IS NOT NEW.cmk_key_id
      AND NOT (NEW.phase = 'preempted' AND NEW.cmk_key_id IS NULL))
  OR (OLD.cmk_region IS NOT NEW.cmk_region
      AND NOT (NEW.phase = 'preempted' AND NEW.cmk_region IS NULL))
  OR OLD.policy_blake3 IS NOT NEW.policy_blake3
  OR OLD.tcs_version IS NOT NEW.tcs_version
  OR OLD.wrapped_tcs_blake3 IS NOT NEW.wrapped_tcs_blake3
  OR OLD.source_generation IS NOT NEW.source_generation
  OR OLD.source_config_version IS NOT NEW.source_config_version
  OR OLD.source_cmk_provider IS NOT NEW.source_cmk_provider
  OR OLD.source_cmk_key_id IS NOT NEW.source_cmk_key_id
  OR OLD.source_cmk_region IS NOT NEW.source_cmk_region
  OR OLD.source_tcs_version IS NOT NEW.source_tcs_version
  OR OLD.target_generation IS NOT NEW.target_generation
  OR (OLD.observed_gate_epoch IS NOT NEW.observed_gate_epoch
      AND NOT (OLD.phase = 'copy'
        AND OLD.suspension_state = 'degraded_read_only'
        AND NEW.suspension_state = 'active'
        AND EXISTS (SELECT 1 FROM byok_activation_guard ag
          JOIN byok_transition_fence tf ON tf.token=ag.transition_token
          JOIN byok_tenant_gate g ON g.tenant_id=NEW.tenant_id
          JOIN byok_transition_commit_guard cg ON cg.tenant_id=NEW.tenant_id
          WHERE ag.guard_id=NEW.guard_id AND ag.outcome='active'
            AND tf.epoch=ag.transition_epoch AND tf.outcome='active'
            AND tf.observed_gate_epoch=NEW.observed_gate_epoch
            AND g.gate_epoch=NEW.observed_gate_epoch
            AND cg.action='restore')))
  OR (OLD.publication_gate_epoch IS NOT NEW.publication_gate_epoch
      AND NOT (OLD.publication_gate_epoch IS NULL
               AND NEW.publication_gate_epoch = OLD.observed_gate_epoch + 1
               AND NEW.phase = 'published_partial')
      AND NOT (OLD.phase IN ('published_partial','purging','ready_finalize')
        AND OLD.suspension_state = 'degraded_read_only'
        AND NEW.suspension_state = 'active'
        AND EXISTS (SELECT 1 FROM byok_activation_guard ag
          JOIN byok_transition_fence tf ON tf.token=ag.transition_token
          JOIN byok_tenant_gate g ON g.tenant_id=NEW.tenant_id
          JOIN byok_transition_commit_guard cg ON cg.tenant_id=NEW.tenant_id
          WHERE ag.guard_id=NEW.guard_id AND ag.outcome='active'
            AND tf.epoch=ag.transition_epoch AND tf.outcome='active'
            AND tf.observed_gate_epoch=NEW.publication_gate_epoch
            AND g.gate_epoch=NEW.publication_gate_epoch
            AND cg.action='restore')))
  OR (OLD.published_config_version IS NOT NEW.published_config_version
      AND NOT (OLD.published_config_version IS NULL
               AND NEW.published_config_version = OLD.config_version + 1
               AND NEW.phase = 'published_partial'))
  OR NEW.cas_complete < OLD.cas_complete
  OR NEW.ac_complete < OLD.ac_complete
  OR (OLD.cas_after_logical_key IS NOT NULL AND
      (NEW.cas_after_logical_key IS NULL OR NEW.cas_after_logical_key < OLD.cas_after_logical_key))
  OR (OLD.ac_after_logical_key IS NOT NULL AND
      (NEW.ac_after_logical_key IS NULL OR NEW.ac_after_logical_key < OLD.ac_after_logical_key))
  OR (OLD.cas_publish_after_key IS NOT NULL AND
      (NEW.cas_publish_after_key IS NULL OR NEW.cas_publish_after_key < OLD.cas_publish_after_key))
  OR (OLD.ac_publish_after_key IS NOT NULL AND
      (NEW.ac_publish_after_key IS NULL OR NEW.ac_publish_after_key < OLD.ac_publish_after_key))
  OR (OLD.purge_after_key IS NOT NULL AND
      (NEW.purge_after_key IS NULL OR NEW.purge_after_key < OLD.purge_after_key))
  OR NEW.claim_epoch < OLD.claim_epoch
  OR ((OLD.claim_owner IS NOT NEW.claim_owner OR OLD.claim_token IS NOT NEW.claim_token)
      AND NEW.phase NOT IN ('committed', 'aborted', 'preempted')
      AND NEW.claim_epoch <= OLD.claim_epoch)
  OR (OLD.claim_token IS NEW.claim_token AND OLD.claim_expires_at_ms IS NOT NULL
      AND NEW.claim_expires_at_ms < OLD.claim_expires_at_ms)
  OR NEW.state_version <= OLD.state_version
  OR (OLD.phase = 'copy' AND NEW.phase NOT IN
      ('copy', 'published_partial', 'aborted', 'preempted'))
  OR (OLD.phase = 'published_partial' AND NEW.phase NOT IN
      ('published_partial', 'purging', 'preempted'))
  OR (OLD.phase = 'purging' AND NEW.phase NOT IN
      ('purging', 'ready_finalize', 'preempted'))
  OR (OLD.phase = 'ready_finalize' AND NEW.phase NOT IN
      ('ready_finalize', 'committed', 'preempted'))
  OR (OLD.phase IN ('committed', 'aborted', 'preempted') AND NEW.phase IS NOT OLD.phase)
BEGIN
    SELECT RAISE(ABORT, 'BYOK activation identity/state is not forward-only');
END;

-- Degrade/restore is authorized by the 0119 control transition. The status
-- row must already reflect that exact transition in the same D1 batch.
CREATE TRIGGER IF NOT EXISTS trg_byok_activation_suspension_guard
BEFORE UPDATE OF suspension_state, suspended_at_ms ON byok_activation_intent
WHEN (OLD.suspension_state IS NOT NEW.suspension_state
      OR OLD.suspended_at_ms IS NOT NEW.suspended_at_ms)
 AND NOT (
    OLD.phase NOT IN ('committed', 'aborted', 'preempted')
    AND ((OLD.suspension_state = 'active'
          AND NEW.suspension_state = 'degraded_read_only'
          AND NEW.suspended_at_ms IS NOT NULL
          AND EXISTS (SELECT 1 FROM tenant t WHERE t.tenant_id = NEW.tenant_id
                      AND t.byok_status = 'degraded_read_only'))
      OR (OLD.suspension_state = 'degraded_read_only'
          AND NEW.suspension_state = 'active'
          AND NEW.suspended_at_ms IS NULL
          AND EXISTS (SELECT 1 FROM tenant t WHERE t.tenant_id = NEW.tenant_id
                      AND t.byok_status = 'active')))
 )
BEGIN
    SELECT RAISE(ABORT, 'invalid BYOK activation suspension transition');
END;

-- Exact source identity is captured before any read.  Generation zero is raw
-- plaintext.  Rotations must name the published allocation and crypto policy
-- needed to decrypt before re-encrypting; ciphertext is never re-encrypted.
CREATE TABLE IF NOT EXISTS byok_activation_source_object (
    intent_id              TEXT NOT NULL,
    tenant_id              TEXT NOT NULL,
    object_kind            TEXT NOT NULL CHECK (object_kind IN ('cas', 'ac')),
    logical_key            TEXT NOT NULL,
    source_generation      INTEGER NOT NULL CHECK (source_generation >= 0),
    source_allocation_id   TEXT,
    source_physical_key    TEXT NOT NULL,
    source_crypto_mode     TEXT NOT NULL
        CHECK (source_crypto_mode IN ('plaintext', 'convergent', 'random')),
    source_config_version  INTEGER,
    source_cmk_provider    TEXT,
    source_cmk_key_id      TEXT,
    source_cmk_region      TEXT,
    source_tcs_version     INTEGER,
    plaintext_size         INTEGER NOT NULL CHECK (plaintext_size >= 0),
    source_stored_size     INTEGER NOT NULL CHECK (source_stored_size >= 0),
    source_blake3          TEXT NOT NULL CHECK (
        length(source_blake3) = 64 AND source_blake3 NOT GLOB '*[^0-9a-f]*'),
    target_allocation_id   TEXT,
    target_physical_key    TEXT,
    target_write_token     TEXT UNIQUE,
    target_write_expires_at_ms INTEGER,
    copy_state             TEXT NOT NULL DEFAULT 'discovered'
        CHECK (copy_state IN ('discovered', 'staged', 'published', 'purge_queued', 'purged')),
    discovered_at_ms       INTEGER NOT NULL CHECK (discovered_at_ms >= 0),
    copied_at_ms           INTEGER,
    PRIMARY KEY (intent_id, object_kind, logical_key),
    FOREIGN KEY (intent_id) REFERENCES byok_activation_intent(intent_id),
    CHECK ((source_generation = 0 AND source_allocation_id IS NULL
            AND source_crypto_mode = 'plaintext'
            AND source_config_version IS NULL AND source_cmk_provider IS NULL
            AND source_cmk_key_id IS NULL AND source_cmk_region IS NULL
            AND source_tcs_version IS NULL)
        OR (source_generation > 0 AND source_allocation_id IS NOT NULL
            AND source_crypto_mode IN ('convergent', 'random')
            AND source_config_version IS NOT NULL AND source_cmk_provider IS NOT NULL
            AND source_cmk_key_id IS NOT NULL AND source_cmk_region IS NOT NULL
            AND source_tcs_version IS NOT NULL)),
    CHECK (source_allocation_id IS NULL OR
           (source_allocation_id <> '' AND instr(source_allocation_id, '/') = 0)),
    CHECK (source_physical_key <> '' AND instr(source_physical_key, char(0)) = 0),
    CHECK ((copy_state = 'discovered' AND target_allocation_id IS NULL
            AND target_physical_key IS NULL)
        OR (copy_state <> 'discovered' AND target_allocation_id IS NOT NULL
            AND target_physical_key IS NOT NULL)),
    CHECK ((target_write_token IS NULL) = (target_write_expires_at_ms IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_byok_activation_source_copy
    ON byok_activation_source_object (intent_id, object_kind, copy_state, logical_key);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_source_insert_guard
BEFORE INSERT ON byok_activation_source_object
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_activation_intent a
         WHERE a.intent_id=NEW.intent_id AND a.tenant_id=NEW.tenant_id
           AND a.phase='copy' AND a.source_generation=NEW.source_generation
           AND NEW.copy_state='discovered'
           AND NEW.target_allocation_id IS NULL AND NEW.target_physical_key IS NULL
           AND NEW.target_write_token IS NULL AND NEW.target_write_expires_at_ms IS NULL
           AND NEW.copied_at_ms IS NULL
           AND EXISTS (SELECT 1 FROM byok_activation_operation_guard og
               WHERE og.intent_id=a.intent_id AND og.action='checkpoint'
                 AND og.expected_state_version=a.state_version)
           AND CASE WHEN NEW.source_generation=0 THEN (
                (NEW.object_kind='cas' AND EXISTS (SELECT 1 FROM blob_meta m
                    WHERE m.tenant_id=NEW.tenant_id AND m.deleted_at IS NULL
                      AND m.digest=substr(NEW.logical_key,instr(NEW.logical_key,':')+1)))
             OR (NEW.object_kind='ac' AND EXISTS (SELECT 1 FROM ac_meta m
                    WHERE m.tenant_id=NEW.tenant_id AND m.action_digest=NEW.logical_key
                      AND (m.expires_at IS NULL OR m.expires_at >
                          (CAST(strftime('%s','now') AS INTEGER)*1000)))))
             ELSE EXISTS (SELECT 1 FROM byok_logical_object_publication p
                 WHERE p.tenant_id=NEW.tenant_id AND p.object_kind=NEW.object_kind
                   AND p.logical_key=NEW.logical_key
                   AND p.generation=NEW.source_generation
                   AND p.allocation_id=NEW.source_allocation_id
                   AND p.physical_key=NEW.source_physical_key
                   AND p.size_bytes=NEW.plaintext_size
                   AND EXISTS (SELECT 1 FROM tenant_byok_config_history h
                       WHERE h.tenant_id=NEW.tenant_id
                         AND h.config_version=NEW.source_config_version
                         AND h.crypto_mode=NEW.source_crypto_mode
                         AND h.cmk_provider=NEW.source_cmk_provider
                         AND h.cmk_key_id=NEW.source_cmk_key_id
                         AND h.cmk_region=NEW.source_cmk_region)
                   AND EXISTS (SELECT 1 FROM tenant_byok_secret_history sh
                       WHERE sh.tenant_id=NEW.tenant_id
                         AND sh.tcs_version=NEW.source_tcs_version
                         AND sh.cmk_provider=NEW.source_cmk_provider
                         AND sh.cmk_key_id=NEW.source_cmk_key_id
                         AND sh.cmk_region=NEW.source_cmk_region))
             END
    ) THEN RAISE(ABORT, 'invalid activation source snapshot identity') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_source_forward_only
BEFORE UPDATE ON byok_activation_source_object
WHEN OLD.intent_id IS NOT NEW.intent_id
  OR OLD.tenant_id IS NOT NEW.tenant_id
  OR OLD.object_kind IS NOT NEW.object_kind
  OR OLD.logical_key IS NOT NEW.logical_key
  OR OLD.source_generation IS NOT NEW.source_generation
  OR OLD.source_allocation_id IS NOT NEW.source_allocation_id
  OR OLD.source_physical_key IS NOT NEW.source_physical_key
  OR OLD.source_crypto_mode IS NOT NEW.source_crypto_mode
  OR OLD.source_config_version IS NOT NEW.source_config_version
  OR OLD.source_cmk_provider IS NOT NEW.source_cmk_provider
  OR OLD.source_cmk_key_id IS NOT NEW.source_cmk_key_id
  OR OLD.source_cmk_region IS NOT NEW.source_cmk_region
  OR OLD.source_tcs_version IS NOT NEW.source_tcs_version
  OR OLD.plaintext_size IS NOT NEW.plaintext_size
  OR OLD.source_stored_size IS NOT NEW.source_stored_size
  OR OLD.source_blake3 IS NOT NEW.source_blake3
  OR (OLD.target_allocation_id IS NOT NEW.target_allocation_id
      AND NOT (OLD.target_allocation_id IS NULL AND NEW.target_allocation_id IS NOT NULL))
  OR (OLD.target_physical_key IS NOT NEW.target_physical_key
      AND NOT (OLD.target_physical_key IS NULL AND NEW.target_physical_key IS NOT NULL))
  OR (OLD.copied_at_ms IS NOT NEW.copied_at_ms
      AND NOT (OLD.copied_at_ms IS NULL AND NEW.copied_at_ms IS NOT NULL))
  OR ((OLD.target_write_token IS NOT NEW.target_write_token
       OR OLD.target_write_expires_at_ms IS NOT NEW.target_write_expires_at_ms)
      AND NOT ((OLD.target_write_token IS NULL AND NEW.target_write_token IS NOT NULL
                AND OLD.target_write_expires_at_ms IS NULL
                AND NEW.target_write_expires_at_ms >
                    (CAST(strftime('%s','now') AS INTEGER)*1000))
               OR (OLD.target_write_token IS NOT NULL AND NEW.target_write_token IS NULL
                   AND OLD.target_write_expires_at_ms IS NOT NULL
                   AND NEW.target_write_expires_at_ms IS NULL)
               OR (OLD.target_write_token IS NOT NULL
                   AND OLD.target_write_expires_at_ms <=
                       (CAST(strftime('%s','now') AS INTEGER)*1000)
                   AND NEW.target_write_token IS NOT OLD.target_write_token
                   AND NEW.target_write_token IS NOT NULL
                   AND NEW.target_write_expires_at_ms>
                       (CAST(strftime('%s','now') AS INTEGER)*1000))))
  OR CASE OLD.copy_state
      WHEN 'discovered' THEN NEW.copy_state NOT IN ('discovered','staged')
      WHEN 'staged' THEN NEW.copy_state NOT IN ('staged','published')
      WHEN 'published' THEN NEW.copy_state NOT IN ('published','purge_queued')
      WHEN 'purge_queued' THEN NEW.copy_state NOT IN ('purge_queued','purged')
      WHEN 'purged' THEN NEW.copy_state<>'purged'
      ELSE 1 END
BEGIN
    SELECT RAISE(ABORT, 'activation source identity/state is not forward-only');
END;

-- A target allocation is reserved before any envelope/R2 I/O.  The exact
-- ciphertext receipt may be filled once, under the same current activation
-- claim; it can never be rewritten or cleared after that checkpoint.
CREATE TRIGGER IF NOT EXISTS trg_byok_activation_generation_ciphertext_once
BEFORE UPDATE OF ciphertext_size, ciphertext_blake3
ON byok_logical_object_generation
WHEN OLD.ciphertext_size IS NOT NEW.ciphertext_size
  OR OLD.ciphertext_blake3 IS NOT NEW.ciphertext_blake3
BEGIN
    SELECT CASE WHEN NOT (
        OLD.ciphertext_size IS NULL AND OLD.ciphertext_blake3 IS NULL
        AND NEW.ciphertext_size IS NOT NULL AND NEW.ciphertext_size >= 0
        AND length(NEW.ciphertext_blake3)=64
        AND NEW.ciphertext_blake3 NOT GLOB '*[^0-9a-f]*'
        AND OLD.outcome='allocated' AND NEW.outcome='allocated'
        AND OLD.tenant_id IS NEW.tenant_id
        AND OLD.object_kind IS NEW.object_kind
        AND OLD.logical_key IS NEW.logical_key
        AND OLD.generation IS NEW.generation
        AND OLD.allocation_id IS NEW.allocation_id
        AND OLD.physical_key IS NEW.physical_key
        AND OLD.intent_token IS NEW.intent_token
        AND OLD.gate_epoch IS NEW.gate_epoch
        AND OLD.size_bytes IS NEW.size_bytes
        AND OLD.allocated_at_ms IS NEW.allocated_at_ms
        AND OLD.completed_at_ms IS NEW.completed_at_ms
        AND OLD.backfill_run_id IS NEW.backfill_run_id
        AND EXISTS (SELECT 1 FROM byok_activation_source_object s
            JOIN byok_activation_intent a ON a.intent_id=s.intent_id
            JOIN byok_activation_operation_guard og ON og.intent_id=a.intent_id
             AND og.action='checkpoint' AND og.expected_state_version=a.state_version
            WHERE s.tenant_id=OLD.tenant_id AND s.object_kind=OLD.object_kind
              AND s.logical_key=OLD.logical_key AND s.target_allocation_id=OLD.allocation_id
              AND s.target_physical_key=OLD.physical_key
              AND a.target_generation=OLD.generation AND a.phase='copy')
    ) THEN RAISE(ABORT, 'activation ciphertext receipt is not append-only') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_generation_forward_only
BEFORE UPDATE ON byok_logical_object_generation
WHEN OLD.backfill_run_id IS NOT NULL
 AND EXISTS (SELECT 1 FROM byok_activation_intent a
             WHERE a.intent_id=OLD.backfill_run_id)
 AND (OLD.tenant_id IS NOT NEW.tenant_id
   OR OLD.object_kind IS NOT NEW.object_kind
   OR OLD.logical_key IS NOT NEW.logical_key
   OR OLD.generation IS NOT NEW.generation
   OR OLD.allocation_id IS NOT NEW.allocation_id
   OR OLD.physical_key IS NOT NEW.physical_key
   OR OLD.intent_token IS NOT NEW.intent_token
   OR OLD.size_bytes IS NOT NEW.size_bytes
   OR OLD.allocated_at_ms IS NOT NEW.allocated_at_ms
   OR OLD.backfill_run_id IS NOT NEW.backfill_run_id
   OR OLD.gate_epoch IS NOT NEW.gate_epoch
   OR OLD.outcome IS NOT NEW.outcome
   OR OLD.completed_at_ms IS NOT NEW.completed_at_ms)
 AND NOT EXISTS (
    SELECT 1 FROM byok_activation_intent a
    JOIN byok_activation_operation_guard og ON og.intent_id=a.intent_id
     AND og.expected_state_version=a.state_version
    WHERE a.intent_id=OLD.backfill_run_id
      AND OLD.tenant_id IS NEW.tenant_id
      AND OLD.object_kind IS NEW.object_kind
      AND OLD.logical_key IS NEW.logical_key
      AND OLD.generation IS NEW.generation
      AND OLD.allocation_id IS NEW.allocation_id
      AND OLD.physical_key IS NEW.physical_key
      AND OLD.intent_token IS NEW.intent_token
      AND OLD.size_bytes IS NEW.size_bytes
      AND OLD.allocated_at_ms IS NEW.allocated_at_ms
      AND OLD.backfill_run_id IS NEW.backfill_run_id
      AND ((og.action='publish' AND a.phase='copy'
            AND OLD.outcome='allocated' AND NEW.outcome='published'
            AND OLD.gate_epoch=a.observed_gate_epoch
            AND NEW.gate_epoch=a.observed_gate_epoch+1
            AND OLD.completed_at_ms IS NULL AND NEW.completed_at_ms IS NOT NULL)
        OR (og.action IN ('abort','preempt')
            AND OLD.outcome IN ('allocated','published') AND NEW.outcome='abandoned'
            AND OLD.gate_epoch IS NEW.gate_epoch
            AND NEW.completed_at_ms IS NOT NULL))
 )
BEGIN
    SELECT RAISE(ABORT, 'activation generation identity/state is not forward-only');
END;

-- Shared exact deletion ledger.  Activation source purge, a live delete and a
-- losing publication all retain the immutable physical identity until DELETE
-- plus HEAD-not-found has been verified.  Mode-B envelope cleanup uses the
-- exact allocation id rather than the legacy digest-only reconciliation key.
CREATE TABLE IF NOT EXISTS byok_object_purge_item (
    purge_id              TEXT PRIMARY KEY,
    tenant_id             TEXT NOT NULL,
    intent_id             TEXT,
    object_kind           TEXT NOT NULL CHECK (object_kind IN ('cas', 'ac')),
    logical_key           TEXT NOT NULL,
    generation            INTEGER NOT NULL CHECK (generation >= 0),
    allocation_id         TEXT,
    physical_key          TEXT NOT NULL,
    object_size           INTEGER CHECK (object_size IS NULL OR object_size >= 0),
    object_blake3         TEXT CHECK (object_blake3 IS NULL OR
        (length(object_blake3) = 64 AND object_blake3 NOT GLOB '*[^0-9a-f]*')),
    crypto_mode           TEXT NOT NULL
        CHECK (crypto_mode IN ('plaintext', 'convergent', 'random')),
    reason                TEXT NOT NULL
        CHECK (reason IN ('activation_source', 'live_delete', 'publish_loser')),
    state                 TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'deleting', 'retry', 'r2_absent',
                         'verified', 'quarantined')),
    claim_owner           TEXT,
    claim_token           TEXT UNIQUE,
    claim_epoch           INTEGER NOT NULL DEFAULT 0 CHECK (claim_epoch >= 0),
    claim_expires_at_ms   INTEGER,
    attempts              INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at_ms    INTEGER NOT NULL,
    created_at_ms         INTEGER NOT NULL CHECK (created_at_ms >= 0),
    verified_at_ms        INTEGER,
    last_error            TEXT,
    FOREIGN KEY (intent_id) REFERENCES byok_activation_intent(intent_id),
    UNIQUE (tenant_id, physical_key),
    CHECK (logical_key <> '' AND physical_key <> '' AND instr(physical_key, char(0)) = 0),
    CHECK (allocation_id IS NULL OR
           (allocation_id <> '' AND instr(allocation_id, '/') = 0)),
    CHECK ((generation = 0 AND allocation_id IS NULL AND crypto_mode = 'plaintext')
        OR (generation > 0 AND allocation_id IS NOT NULL
            AND crypto_mode IN ('convergent', 'random'))),
    CHECK ((object_size IS NULL) = (object_blake3 IS NULL)),
    CHECK (reason = 'publish_loser' OR object_size IS NOT NULL),
    CHECK ((claim_owner IS NULL AND claim_token IS NULL AND claim_expires_at_ms IS NULL)
        OR (claim_owner IS NOT NULL AND claim_token IS NOT NULL
            AND claim_expires_at_ms IS NOT NULL)),
    CHECK (state NOT IN ('verified', 'quarantined')
           OR (claim_owner IS NULL AND claim_token IS NULL AND claim_expires_at_ms IS NULL)),
    CHECK ((state IN ('verified', 'quarantined')) = (verified_at_ms IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_byok_object_purge_claimable
    ON byok_object_purge_item (state, next_attempt_at_ms, claim_expires_at_ms, purge_id);

-- One physical purge may be required by several races. Preserve every cause
-- without duplicating DELETE work or losing provenance to the unique key.
CREATE TABLE IF NOT EXISTS byok_object_purge_cause (
    purge_id       TEXT NOT NULL,
    cause_kind     TEXT NOT NULL
        CHECK (cause_kind IN ('activation_source', 'live_delete', 'publish_loser')),
    cause_id       TEXT NOT NULL,
    created_at_ms  INTEGER NOT NULL CHECK (created_at_ms >= 0),
    PRIMARY KEY (purge_id, cause_kind, cause_id),
    FOREIGN KEY (purge_id) REFERENCES byok_object_purge_item(purge_id)
);

-- A physical-key collision with a different immutable generation identity is
-- never auto-resolved. Keep a durable alert while allowing later valid rows
-- in the bounded census to progress.
CREATE TABLE IF NOT EXISTS byok_purge_identity_quarantine (
    tenant_id             TEXT NOT NULL,
    object_kind           TEXT NOT NULL CHECK (object_kind IN ('cas', 'ac')),
    logical_key           TEXT NOT NULL,
    generation            INTEGER NOT NULL CHECK (generation > 0),
    allocation_id         TEXT NOT NULL,
    physical_key          TEXT NOT NULL,
    conflicting_purge_id  TEXT NOT NULL,
    detected_at_ms        INTEGER NOT NULL CHECK (detected_at_ms >= 0),
    PRIMARY KEY (tenant_id, object_kind, logical_key, generation, allocation_id),
    FOREIGN KEY (conflicting_purge_id) REFERENCES byok_object_purge_item(purge_id)
);

CREATE TRIGGER IF NOT EXISTS trg_byok_object_purge_forward_only
BEFORE UPDATE ON byok_object_purge_item
WHEN OLD.purge_id IS NOT NEW.purge_id
  OR OLD.tenant_id IS NOT NEW.tenant_id
  OR OLD.intent_id IS NOT NEW.intent_id
  OR OLD.object_kind IS NOT NEW.object_kind
  OR OLD.logical_key IS NOT NEW.logical_key
  OR OLD.generation IS NOT NEW.generation
  OR OLD.allocation_id IS NOT NEW.allocation_id
  OR OLD.physical_key IS NOT NEW.physical_key
  OR OLD.object_size IS NOT NEW.object_size
  OR OLD.object_blake3 IS NOT NEW.object_blake3
  OR OLD.crypto_mode IS NOT NEW.crypto_mode
  OR OLD.reason IS NOT NEW.reason
  OR NEW.claim_epoch < OLD.claim_epoch
  OR NEW.attempts < OLD.attempts
  OR ((OLD.claim_owner IS NOT NEW.claim_owner OR OLD.claim_token IS NOT NEW.claim_token)
      AND NEW.state NOT IN ('verified', 'quarantined')
      AND NEW.claim_epoch <= OLD.claim_epoch)
  OR (OLD.claim_token IS NEW.claim_token AND OLD.claim_expires_at_ms IS NOT NULL
      AND NEW.claim_expires_at_ms < OLD.claim_expires_at_ms)
  OR (OLD.state = 'pending' AND NEW.state NOT IN ('pending', 'deleting'))
  OR (OLD.state = 'deleting' AND NEW.state NOT IN
      ('deleting', 'retry', 'r2_absent', 'quarantined'))
  OR (OLD.state = 'retry' AND NEW.state NOT IN ('retry', 'deleting', 'quarantined'))
  OR (OLD.state = 'r2_absent' AND NEW.state NOT IN ('r2_absent', 'verified'))
  OR (OLD.state IN ('verified', 'quarantined') AND NEW.state IS NOT OLD.state)
BEGIN
    SELECT RAISE(ABORT, 'BYOK purge identity/state is not forward-only');
END;

-- A preempting publish-loser purge cannot declare absence while a worker that
-- revalidated immediately before PUT may still complete its bounded request.
CREATE TRIGGER IF NOT EXISTS trg_byok_purge_waits_for_target_write_drain
BEFORE UPDATE OF state ON byok_object_purge_item
WHEN NEW.state IN ('deleting','r2_absent','verified') AND OLD.state IS NOT NEW.state
 AND NEW.reason='publish_loser'
 AND EXISTS (SELECT 1 FROM byok_activation_source_object s
      WHERE s.tenant_id=NEW.tenant_id AND s.target_physical_key=NEW.physical_key
        AND s.target_write_token IS NOT NULL
        AND s.target_write_expires_at_ms>
            (CAST(strftime('%s','now') AS INTEGER)*1000))
BEGIN
    SELECT RAISE(ABORT, 'activation target write has not drained');
END;

-- R2 cannot enforce a D1 lease. A common-worker loser claim must therefore
-- remain impossible until the exact writer/backfill authority has drained;
-- this trigger is the backstop even if a future claimant query regresses.
CREATE TRIGGER IF NOT EXISTS trg_byok_publish_loser_requires_drained_owner
BEFORE UPDATE OF state ON byok_object_purge_item
WHEN EXISTS (SELECT 1 FROM byok_object_purge_cause pc
              WHERE pc.purge_id = NEW.purge_id
                AND pc.cause_kind = 'publish_loser'
                AND pc.cause_id = NEW.allocation_id)
 AND NEW.state IN ('deleting', 'r2_absent', 'verified')
 AND NOT (
    EXISTS (
    SELECT 1 FROM byok_logical_object_generation g
     WHERE g.tenant_id = NEW.tenant_id
       AND g.object_kind = NEW.object_kind
       AND g.logical_key = NEW.logical_key
       AND g.generation = NEW.generation
       AND g.allocation_id = NEW.allocation_id
       AND g.physical_key = NEW.physical_key
       AND (
          EXISTS (SELECT 1 FROM byok_data_intent i
                   WHERE i.tenant_id = g.tenant_id
                     AND i.token = g.intent_token
                     AND i.operation = 'write'
                     AND i.expires_at_ms + 5000 <=
                         (CAST(strftime('%s','now') AS INTEGER) * 1000))
          OR EXISTS (SELECT 1 FROM byok_backfill_run r
                      JOIN byok_transition_fence f
                        ON f.tenant_id = r.tenant_id
                       AND f.token = r.intent_token
                       AND f.epoch = r.transition_epoch
                     WHERE r.tenant_id = g.tenant_id
                       AND r.run_id = g.backfill_run_id
                       AND f.expires_at_ms + 5000 <=
                           (CAST(strftime('%s','now') AS INTEGER) * 1000))
          OR EXISTS (SELECT 1 FROM byok_activation_source_object s
                      JOIN byok_activation_intent a
                        ON a.intent_id = s.intent_id
                       AND a.tenant_id = s.tenant_id
                     WHERE s.intent_id = g.backfill_run_id
                       AND s.tenant_id = g.tenant_id
                       AND s.object_kind = g.object_kind
                       AND s.logical_key = g.logical_key
                       AND s.target_allocation_id = g.allocation_id
                       AND s.target_physical_key = g.physical_key
                       AND a.phase = 'preempted'
                       AND s.target_write_expires_at_ms + 5000 <=
                           (CAST(strftime('%s','now') AS INTEGER) * 1000))
       )
    )
 )
BEGIN
    SELECT RAISE(ABORT, 'BYOK publish-loser owner lease has not drained');
END;

-- Assertion row consumed by every activation mutation batch. It converts a
-- zero-row stale claim/config/deadline predicate into a SQLite error, ensuring
-- D1 rolls the entire batch back rather than partially checkpointing it.
CREATE TABLE IF NOT EXISTS byok_activation_operation_guard (
    operation_token       TEXT PRIMARY KEY,
    intent_id             TEXT NOT NULL,
    claim_token           TEXT,
    control_token         TEXT,
    expected_state_version INTEGER NOT NULL CHECK (expected_state_version > 0),
    action                TEXT NOT NULL
        CHECK (action IN ('checkpoint', 'publish', 'begin_purge', 'purge',
                          'finalize', 'abort', 'cancel', 'preempt')),
    checked_at_ms         INTEGER NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES byok_activation_intent(intent_id),
    CHECK ((action IN ('cancel','preempt') AND claim_token IS NULL AND control_token IS NOT NULL)
        OR (action NOT IN ('cancel','preempt') AND claim_token IS NOT NULL AND control_token IS NULL))
);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_operation_guard
BEFORE INSERT ON byok_activation_operation_guard
WHEN NEW.action NOT IN ('cancel','preempt')
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
          FROM byok_activation_intent a
          JOIN byok_activation_guard f ON f.guard_id = a.guard_id
          JOIN byok_transition_fence tf ON tf.token = f.transition_token
          JOIN byok_tenant_gate g ON g.tenant_id = a.tenant_id
          JOIN tenant t ON t.tenant_id = a.tenant_id
          JOIN tenant_byok_config c ON c.tenant_id = a.tenant_id
          JOIN tenant_byok_secret s ON s.tenant_id = a.tenant_id
         WHERE a.intent_id = NEW.intent_id
           AND a.claim_token = NEW.claim_token
           AND a.claim_expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
           AND a.state_version = NEW.expected_state_version
           AND f.outcome = 'active'
           AND f.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
           AND tf.tenant_id = a.tenant_id AND tf.epoch = f.transition_epoch
           AND tf.outcome = 'active'
           AND tf.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
           AND c.mode = a.mode AND c.crypto_mode = a.crypto_mode
           AND c.cmk_provider = a.cmk_provider
           AND c.cmk_key_id = a.cmk_key_id AND c.cmk_region = a.cmk_region
           AND s.tcs_version = a.tcs_version AND s.cmk_key_id = a.cmk_key_id
           AND a.suspension_state = 'active' AND t.byok_status = 'active'
           AND ((a.phase = 'copy'
                 AND g.gate_epoch = a.observed_gate_epoch
                 AND g.current_generation = a.source_generation
                 AND tf.observed_gate_epoch = a.observed_gate_epoch
                 AND tf.observed_generation = a.source_generation
                 AND c.config_version = a.config_version AND c.state = 'pending')
             OR (a.phase IN ('published_partial', 'purging', 'ready_finalize')
                 AND g.gate_epoch = a.publication_gate_epoch
                 AND g.current_generation = a.target_generation
                 AND tf.observed_gate_epoch = a.publication_gate_epoch
                 AND tf.observed_generation = a.target_generation
                 AND c.config_version = a.published_config_version
                 AND c.state = 'partial'))
           AND (NEW.action NOT IN ('checkpoint', 'publish')
                OR a.deadline_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000))
           AND CASE NEW.action
               WHEN 'checkpoint' THEN a.phase = 'copy'
               WHEN 'publish' THEN a.phase = 'copy'
                   AND a.cas_complete = 1 AND a.ac_complete = 1
               WHEN 'begin_purge' THEN a.phase = 'published_partial'
               WHEN 'purge' THEN a.phase = 'purging'
               WHEN 'finalize' THEN a.phase = 'ready_finalize'
                   AND NOT EXISTS (SELECT 1 FROM byok_object_purge_cause pc
                       JOIN byok_object_purge_item p ON p.purge_id=pc.purge_id
                       WHERE pc.cause_kind='activation_source'
                         AND pc.cause_id=a.intent_id AND p.state<>'verified')
               WHEN 'abort' THEN a.phase = 'copy'
               WHEN 'preempt' THEN 0
               ELSE 0 END
    ) THEN RAISE(ABORT, 'invalid or stale BYOK activation capability') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_preempt_guard
BEFORE INSERT ON byok_activation_operation_guard
WHEN NEW.action IN ('cancel','preempt')
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_activation_intent a
        JOIN byok_transition_commit_guard cg
          ON cg.tenant_id = a.tenant_id AND cg.token = NEW.control_token
        JOIN byok_transition_fence tf ON tf.token = cg.token
        WHERE a.intent_id = NEW.intent_id
          AND a.state_version = NEW.expected_state_version
          AND a.phase NOT IN ('committed', 'aborted', 'preempted')
          AND cg.action = CASE NEW.action WHEN 'cancel' THEN 'deactivate' ELSE 'shred' END
          AND (NEW.action <> 'cancel' OR a.phase = 'copy')
          AND tf.tenant_id = a.tenant_id
          AND tf.outcome = 'active'
          AND tf.expires_at_ms > (CAST(strftime('%s','now') AS INTEGER) * 1000)
    ) THEN RAISE(ABORT, 'invalid BYOK destructive preemption capability') END;
END;

-- Final statement of every object-worker mutation batch. The trigger turns a
-- zero-row/stale intermediate write into an in-transaction abort.
CREATE TABLE IF NOT EXISTS byok_activation_worker_assertion (
    assertion_token TEXT PRIMARY KEY,
    operation_token TEXT NOT NULL,
    intent_id TEXT NOT NULL,
    expected_state_version INTEGER NOT NULL,
    assertion_kind TEXT NOT NULL CHECK (assertion_kind IN
        ('reserve','receipt','surface_complete','purge_failed','purge_verified')),
    object_kind TEXT CHECK (object_kind IN ('cas','ac')),
    logical_key TEXT,
    checkpoint_key TEXT,
    purge_id TEXT,
    checked_at_ms INTEGER NOT NULL,
    FOREIGN KEY (operation_token) REFERENCES byok_activation_operation_guard(operation_token),
    FOREIGN KEY (intent_id) REFERENCES byok_activation_intent(intent_id)
);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_worker_assertion
BEFORE INSERT ON byok_activation_worker_assertion
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_activation_intent a
        JOIN byok_activation_operation_guard og ON og.operation_token=NEW.operation_token
         AND og.intent_id=a.intent_id
         AND og.expected_state_version=NEW.expected_state_version
        WHERE a.intent_id=NEW.intent_id
          AND a.state_version=NEW.expected_state_version+1
          AND CASE NEW.assertion_kind
            WHEN 'reserve' THEN EXISTS (
                SELECT 1 FROM byok_activation_source_object s
                JOIN byok_logical_object_generation g ON g.tenant_id=s.tenant_id
                 AND g.object_kind=s.object_kind AND g.logical_key=s.logical_key
                 AND g.generation=a.target_generation
                 AND g.allocation_id=s.target_allocation_id
                 AND g.physical_key=s.target_physical_key
                 AND g.backfill_run_id=s.intent_id
                WHERE s.intent_id=a.intent_id AND s.object_kind=NEW.object_kind
                 AND s.logical_key=NEW.logical_key AND s.copy_state='staged'
                 AND s.target_write_token IS NOT NULL
                 AND s.target_write_expires_at_ms>
                     (CAST(strftime('%s','now') AS INTEGER)*1000))
            WHEN 'receipt' THEN EXISTS (
                SELECT 1 FROM byok_activation_source_object s
                JOIN byok_logical_object_generation g ON g.tenant_id=s.tenant_id
                 AND g.object_kind=s.object_kind AND g.logical_key=s.logical_key
                 AND g.generation=a.target_generation
                 AND g.allocation_id=s.target_allocation_id
                 AND g.physical_key=s.target_physical_key
                 AND g.backfill_run_id=s.intent_id
                WHERE s.intent_id=a.intent_id AND s.object_kind=NEW.object_kind
                 AND s.logical_key=NEW.logical_key AND s.copied_at_ms IS NOT NULL
                 AND s.target_write_token IS NULL AND g.ciphertext_size IS NOT NULL
                 AND g.ciphertext_blake3 IS NOT NULL
                 AND CASE NEW.object_kind WHEN 'cas' THEN a.cas_after_logical_key
                      ELSE a.ac_after_logical_key END=NEW.checkpoint_key)
            WHEN 'surface_complete' THEN
                ((NEW.object_kind='cas' AND a.cas_complete=1)
                 OR (NEW.object_kind='ac' AND a.ac_complete=1))
            WHEN 'purge_failed' THEN EXISTS (SELECT 1 FROM byok_object_purge_item p
                 WHERE p.purge_id=NEW.purge_id AND p.state IN ('retry','quarantined')
                  AND p.claim_token IS NULL)
            WHEN 'purge_verified' THEN EXISTS (SELECT 1 FROM byok_object_purge_item p
                 WHERE p.purge_id=NEW.purge_id AND p.state='verified'
                  AND p.claim_token IS NULL)
            ELSE 0 END
    ) THEN RAISE(ABORT, 'activation worker postcondition failed') END;
END;

-- Terminalization clears capabilities and truthfully classifies the owning
-- transition guard in the same SQLite statement.
CREATE TRIGGER IF NOT EXISTS trg_byok_activation_terminal_guard
AFTER UPDATE OF phase ON byok_activation_intent
WHEN NEW.phase IN ('committed', 'aborted', 'preempted') AND OLD.phase IS NOT NEW.phase
BEGIN
    UPDATE byok_activation_guard
       SET outcome = CASE NEW.phase
           WHEN 'committed' THEN 'committed'
           WHEN 'aborted' THEN 'aborted'
           ELSE 'preempted' END,
           completed_at_ms = NEW.completed_at_ms
     WHERE guard_id = NEW.guard_id
       AND (outcome = 'active'
            OR (NEW.phase IN ('aborted','preempted') AND outcome = 'expired'));
    SELECT CASE WHEN changes() <> 1
        THEN RAISE(ABORT, 'BYOK activation guard terminalization failed') END;
    UPDATE byok_transition_fence
       SET outcome = CASE WHEN outcome = 'expired' THEN 'expired'
           ELSE CASE NEW.phase
               WHEN 'committed' THEN 'committed'
               WHEN 'aborted' THEN 'aborted'
               ELSE 'aborted' END END,
           completed_at_ms = CASE WHEN outcome = 'expired' THEN completed_at_ms
               ELSE NEW.completed_at_ms END
     WHERE token = (SELECT transition_token FROM byok_activation_guard
                    WHERE guard_id = NEW.guard_id)
       AND (outcome = 'active'
            OR (NEW.phase IN ('aborted','preempted') AND outcome = 'expired'));
    SELECT CASE WHEN changes() <> 1
        THEN RAISE(ABORT, 'BYOK transition fence terminalization failed') END;
END;

-- Last statement in every lifecycle batch. A missing/mismatched durable result
-- raises a real SQLite error so D1 rolls every preceding statement back.
CREATE TABLE IF NOT EXISTS byok_activation_postcondition (
    operation_token        TEXT PRIMARY KEY,
    intent_id              TEXT NOT NULL,
    expected_state_version INTEGER NOT NULL CHECK (expected_state_version > 0),
    expected_phase         TEXT NOT NULL CHECK (expected_phase IN
        ('copy','published_partial','purging','ready_finalize',
         'committed','aborted','preempted')),
    checked_at_ms          INTEGER NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES byok_activation_intent(intent_id)
);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_postcondition
BEFORE INSERT ON byok_activation_postcondition
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_activation_intent a
        JOIN byok_tenant_gate g ON g.tenant_id=a.tenant_id
        JOIN tenant_byok_config c ON c.tenant_id=a.tenant_id
        JOIN tenant t ON t.tenant_id=a.tenant_id
        JOIN byok_activation_guard f ON f.guard_id=a.guard_id
        JOIN byok_transition_fence tf ON tf.token=f.transition_token
        WHERE a.intent_id=NEW.intent_id AND a.state_version=NEW.expected_state_version
          AND a.phase=NEW.expected_phase
          AND CASE NEW.expected_phase
            WHEN 'copy' THEN c.state='pending' AND g.current_generation=a.source_generation
            WHEN 'published_partial' THEN c.state='partial'
                AND g.current_generation=a.target_generation
                AND g.gate_epoch=a.publication_gate_epoch
                AND c.config_version=a.published_config_version
            WHEN 'purging' THEN c.state='partial'
                AND g.current_generation=a.target_generation
                AND g.gate_epoch=a.publication_gate_epoch
            WHEN 'ready_finalize' THEN c.state='partial'
                AND g.current_generation=a.target_generation
                AND NOT EXISTS (SELECT 1 FROM byok_object_purge_cause pc
                    JOIN byok_object_purge_item p ON p.purge_id=pc.purge_id
                    WHERE pc.cause_kind='activation_source'
                     AND pc.cause_id=a.intent_id AND p.state<>'verified')
            WHEN 'committed' THEN c.state='active'
                AND g.current_generation=a.target_generation
                AND g.gate_epoch=a.publication_gate_epoch+1
                AND f.outcome='committed' AND tf.outcome='committed'
            WHEN 'aborted' THEN
                ((a.source_generation=0 AND c.state='inactive'
                  AND NOT EXISTS (SELECT 1 FROM tenant_byok_secret s
                      WHERE s.tenant_id=a.tenant_id AND s.tcs_wrapped IS NOT NULL))
                 OR (a.source_generation>0 AND c.state='active'
                  AND g.current_generation=a.source_generation
                  AND c.cmk_provider=a.source_cmk_provider
                  AND c.cmk_key_id=a.source_cmk_key_id
                  AND c.cmk_region=a.source_cmk_region
                  AND EXISTS (SELECT 1 FROM tenant_byok_config_history h
                      WHERE h.tenant_id=a.tenant_id
                        AND h.config_version=a.source_config_version
                        AND h.mode=c.mode AND h.crypto_mode=c.crypto_mode
                        AND h.cmk_provider=a.source_cmk_provider
                        AND h.cmk_key_id=a.source_cmk_key_id
                        AND h.cmk_region=a.source_cmk_region
                        AND h.state='active')
                  AND EXISTS (SELECT 1 FROM tenant_byok_secret s
                      WHERE s.tenant_id=a.tenant_id AND s.tcs_wrapped IS NOT NULL
                       AND s.cmk_key_id=a.source_cmk_key_id)
                  AND EXISTS (SELECT 1 FROM tenant_byok_secret_history sh
                      WHERE sh.tenant_id=a.tenant_id
                        AND sh.tcs_version=a.source_tcs_version
                        AND sh.cmk_provider=a.source_cmk_provider
                        AND sh.cmk_key_id=a.source_cmk_key_id
                        AND sh.cmk_region=a.source_cmk_region)
                  AND NOT EXISTS (SELECT 1 FROM byok_logical_object_publication p
                      WHERE p.tenant_id=a.tenant_id
                        AND p.generation<>a.source_generation)))
                AND f.outcome='aborted' AND tf.outcome IN ('aborted','expired')
                AND NOT EXISTS (SELECT 1 FROM byok_logical_object_generation x
                    LEFT JOIN byok_object_purge_item p ON p.tenant_id=x.tenant_id
                     AND p.physical_key=x.physical_key AND p.generation=x.generation
                     AND p.allocation_id=x.allocation_id
                    LEFT JOIN byok_object_purge_cause pc ON pc.purge_id=p.purge_id
                     AND pc.cause_kind='publish_loser' AND pc.cause_id=x.allocation_id
                    WHERE x.tenant_id=a.tenant_id AND x.backfill_run_id=a.intent_id
                     AND (p.purge_id IS NULL OR pc.purge_id IS NULL))
                AND NOT EXISTS (SELECT 1 FROM byok_activation_source_object s
                    LEFT JOIN byok_object_purge_item p ON p.tenant_id=s.tenant_id
                     AND p.physical_key=s.target_physical_key
                    LEFT JOIN byok_object_purge_cause pc ON pc.purge_id=p.purge_id
                     AND pc.cause_kind='publish_loser' AND pc.cause_id=s.target_allocation_id
                    WHERE s.intent_id=a.intent_id AND s.target_physical_key IS NOT NULL
                     AND (p.purge_id IS NULL OR pc.purge_id IS NULL))
            WHEN 'preempted' THEN c.state='shredded' AND t.byok_status='revoked'
                AND c.cmk_provider IS NULL AND c.cmk_key_id IS NULL AND c.cmk_region IS NULL
                AND f.outcome='preempted' AND tf.outcome IN ('aborted','expired')
                AND NOT EXISTS (SELECT 1 FROM byok_logical_object_generation x
                    LEFT JOIN byok_object_purge_item p ON p.tenant_id=x.tenant_id
                     AND p.physical_key=x.physical_key AND p.generation=x.generation
                     AND p.allocation_id=x.allocation_id
                    LEFT JOIN byok_object_purge_cause pc ON pc.purge_id=p.purge_id
                     AND pc.cause_kind='publish_loser' AND pc.cause_id=x.allocation_id
                    WHERE x.tenant_id=a.tenant_id AND x.backfill_run_id=a.intent_id
                     AND (p.purge_id IS NULL OR pc.purge_id IS NULL))
                AND NOT EXISTS (SELECT 1 FROM byok_activation_source_object s
                    LEFT JOIN byok_object_purge_item p ON p.tenant_id=s.tenant_id
                     AND p.physical_key=s.target_physical_key
                    LEFT JOIN byok_object_purge_cause pc ON pc.purge_id=p.purge_id
                     AND pc.cause_kind='publish_loser' AND pc.cause_id=s.target_allocation_id
                    WHERE s.intent_id=a.intent_id AND s.target_physical_key IS NOT NULL
                     AND (p.purge_id IS NULL OR pc.purge_id IS NULL))
                AND NOT EXISTS (SELECT 1 FROM tenant_byok_secret s
                    WHERE s.tenant_id=a.tenant_id AND s.tcs_wrapped IS NOT NULL)
                AND NOT EXISTS (SELECT 1 FROM tenant_byok_secret_history h
                    WHERE h.tenant_id=a.tenant_id AND h.tcs_wrapped IS NOT NULL)
                AND (a.publication_gate_epoch IS NULL OR NOT EXISTS (
                    SELECT 1 FROM byok_activation_source_object s
                    LEFT JOIN byok_object_purge_item p ON p.tenant_id=s.tenant_id
                     AND p.physical_key=s.source_physical_key
                    LEFT JOIN byok_object_purge_cause pc ON pc.purge_id=p.purge_id
                     AND pc.cause_kind='activation_source' AND pc.cause_id=a.intent_id
                    WHERE s.intent_id=a.intent_id
                     AND (p.purge_id IS NULL OR pc.purge_id IS NULL)))
            ELSE 0 END
    ) THEN RAISE(ABORT, 'BYOK activation postcondition failed') END;
END;

-- Atomic degrade/restore must either leave the live activation safely paused
-- or rebind it to a fresh fence over the invalidated gate. A zero-row UPDATE
-- is therefore a transaction error, never a falsely successful control event.
CREATE TABLE IF NOT EXISTS byok_activation_suspension_postcondition (
    operation_token       TEXT PRIMARY KEY,
    intent_id             TEXT NOT NULL,
    expected_state_version INTEGER NOT NULL CHECK (expected_state_version > 0),
    action                TEXT NOT NULL CHECK (action IN ('degrade','restore')),
    checked_at_ms         INTEGER NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES byok_activation_intent(intent_id)
);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_suspension_postcondition
BEFORE INSERT ON byok_activation_suspension_postcondition
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_activation_intent a
        JOIN tenant t ON t.tenant_id=a.tenant_id
        JOIN tenant_byok_config c ON c.tenant_id=a.tenant_id
        JOIN byok_tenant_gate g ON g.tenant_id=a.tenant_id
        JOIN byok_activation_guard ag ON ag.guard_id=a.guard_id
        JOIN byok_transition_fence tf ON tf.token=ag.transition_token
        WHERE a.intent_id=NEW.intent_id
          AND a.state_version=NEW.expected_state_version
          AND a.phase IN ('copy','published_partial','purging','ready_finalize')
          AND a.claim_token IS NULL AND a.claim_owner IS NULL
          AND c.cmk_provider=a.cmk_provider AND c.cmk_key_id=a.cmk_key_id
          AND c.cmk_region=a.cmk_region
          AND EXISTS (SELECT 1 FROM byok_control_outcome o
            WHERE o.tenant_id=a.tenant_id AND o.action=NEW.action
              AND o.outcome='completed' AND ((o.cmk_provider=a.cmk_provider
                AND o.cmk_key_id=a.cmk_key_id AND o.cmk_region=a.cmk_region)
               OR (a.source_generation>0
                AND o.cmk_provider=a.source_cmk_provider
                AND o.cmk_key_id=a.source_cmk_key_id
                AND o.cmk_region=a.source_cmk_region)))
          AND CASE NEW.action
            WHEN 'degrade' THEN t.byok_status='degraded_read_only'
              AND a.suspension_state='degraded_read_only'
              AND ag.outcome='expired' AND tf.outcome IN ('aborted','expired')
              AND EXISTS (SELECT 1 FROM byok_activation_key_health kh
                JOIN byok_control_outcome o ON o.tenant_id=kh.tenant_id
                 AND o.cmk_provider=kh.cmk_provider AND o.cmk_key_id=kh.cmk_key_id
                 AND o.cmk_region=kh.cmk_region AND o.action='degrade'
                WHERE kh.intent_id=a.intent_id AND kh.access_state='unavailable')
            WHEN 'restore' THEN t.byok_status='active'
              AND a.suspension_state='active' AND ag.outcome='active'
              AND NOT EXISTS (SELECT 1 FROM byok_activation_key_health kh
                WHERE kh.intent_id=a.intent_id AND kh.access_state<>'healthy')
              AND (SELECT COUNT(*) FROM byok_activation_key_health kh
                WHERE kh.intent_id=a.intent_id) = CASE WHEN a.source_generation=0 THEN 1 ELSE 2 END
              AND tf.outcome='active' AND tf.epoch=ag.transition_epoch
              AND tf.observed_gate_epoch=g.gate_epoch
              AND tf.observed_generation=g.current_generation
              AND ((a.phase='copy' AND a.observed_gate_epoch=g.gate_epoch)
                OR (a.phase IN ('published_partial','purging','ready_finalize')
                  AND a.publication_gate_epoch=g.gate_epoch))
            ELSE 0 END
    ) THEN RAISE(ABORT, 'BYOK activation suspension postcondition failed') END;
END;

CREATE TABLE IF NOT EXISTS byok_activation_transition_assertion (
    assertion_token TEXT PRIMARY KEY,
    guard_id        TEXT NOT NULL,
    checked_at_ms   INTEGER NOT NULL,
    FOREIGN KEY (guard_id) REFERENCES byok_activation_guard(guard_id)
);

CREATE TRIGGER IF NOT EXISTS trg_byok_activation_transition_assertion
BEFORE INSERT ON byok_activation_transition_assertion
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM byok_activation_guard f
        JOIN byok_transition_fence tf ON tf.token=f.transition_token
        WHERE f.guard_id=NEW.guard_id AND f.outcome='active'
          AND f.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000)
          AND tf.tenant_id=f.tenant_id AND tf.epoch=f.transition_epoch
          AND tf.outcome='active'
          AND tf.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000)
    ) THEN RAISE(ABORT, 'BYOK activation transition recovery failed') END;
END;

-- Written last. Runtime adapters require this exact marker before using any
-- 0121 table, so a partial/manual schema copy fails closed.
CREATE TABLE IF NOT EXISTS byok_0121_capability (
    singleton       INTEGER PRIMARY KEY CHECK (singleton = 1),
    schema_version  INTEGER NOT NULL CHECK (schema_version = 2),
    installed_at_ms INTEGER NOT NULL CHECK (installed_at_ms >= 0)
);
INSERT INTO byok_0121_capability (singleton, schema_version, installed_at_ms)
VALUES (1, 2, (CAST(strftime('%s','now') AS INTEGER) * 1000));
