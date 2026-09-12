-- 0118_byok_transition_fence.sql
--
-- Tenant-wide exclusion between BYOK configuration transitions and CAS/AC
-- data-plane operations.  D1 serializes the conditional INSERT statements
-- used by the runtime adapter, so either the transition fence or a data intent
-- wins; they can never both be live for the same tenant.
--
-- Leases are deliberately bounded by the adapter.  Expired rows are retained
-- as forensic evidence and atomically classified when a successor arrives.
-- This file is a one-shot Wrangler migration: replay safety is provided by the
-- `d1_migrations` ledger and per-file transaction, not by replaying ALTER TABLE.

ALTER TABLE tenant_byok_config
    ADD COLUMN config_version INTEGER NOT NULL DEFAULT 1
        CHECK (typeof(config_version) = 'integer'
               AND config_version > 0
               AND config_version <= 9223372036854775806);

-- Writer-first rollout invariant: no active generation may predate its logical
-- publication catalog. Persist the proof row so operators can audit it.
CREATE TABLE IF NOT EXISTS byok_0118_rollout_guard (
    singleton           INTEGER PRIMARY KEY CHECK (singleton = 1),
    active_config_count INTEGER NOT NULL CHECK (active_config_count = 0)
);
INSERT INTO byok_0118_rollout_guard (singleton, active_config_count)
    VALUES (1, (SELECT COUNT(*) FROM tenant_byok_config
               WHERE state IN ('active', 'partial')));

-- The gate exists independently of BYOK configuration. This is essential:
-- plaintext operations for a tenant with no config/inactive config must still
-- exclude a concurrent activation. Existing tenants are backfilled and the
-- trigger covers every future tenant insert.
CREATE TABLE IF NOT EXISTS byok_tenant_gate (
    tenant_id          TEXT PRIMARY KEY,
    gate_epoch         INTEGER NOT NULL DEFAULT 1
        CHECK (typeof(gate_epoch) = 'integer'
               AND gate_epoch > 0
               AND gate_epoch <= 9223372036854775806),
    current_generation INTEGER NOT NULL DEFAULT 0
        CHECK (typeof(current_generation) = 'integer'
               AND current_generation >= 0
               AND current_generation <= 9223372036854775806)
);

INSERT OR IGNORE INTO byok_tenant_gate (tenant_id)
    SELECT tenant_id FROM tenant;

CREATE TRIGGER IF NOT EXISTS trg_byok_tenant_gate_insert
AFTER INSERT ON tenant
BEGIN
    INSERT OR IGNORE INTO byok_tenant_gate (tenant_id) VALUES (NEW.tenant_id);
END;

CREATE TABLE IF NOT EXISTS byok_transition_fence (
    token                  TEXT    PRIMARY KEY,
    tenant_id              TEXT    NOT NULL,
    epoch                  INTEGER NOT NULL
        CHECK (typeof(epoch) = 'integer'
               AND epoch > 0
               AND epoch <= 9223372036854775806),
    outcome                TEXT    NOT NULL DEFAULT 'active'
        CHECK (outcome IN ('active', 'committed', 'aborted', 'expired')),
    observed_gate_epoch    INTEGER NOT NULL
        CHECK (typeof(observed_gate_epoch) = 'integer'
               AND observed_gate_epoch > 0
               AND observed_gate_epoch <= 9223372036854775806),
    observed_generation    INTEGER NOT NULL
        CHECK (typeof(observed_generation) = 'integer'
               AND observed_generation >= 0
               AND observed_generation <= 9223372036854775806),
    observed_config_version INTEGER
        CHECK (observed_config_version IS NULL OR
               (typeof(observed_config_version) = 'integer'
                AND observed_config_version > 0
                AND observed_config_version <= 9223372036854775806)),
    observed_config_state  TEXT    NOT NULL
        CHECK (observed_config_state IN ('absent', 'inactive', 'pending', 'active', 'partial')),
    observed_byok_status   TEXT    NOT NULL
        CHECK (observed_byok_status IN ('active', 'degraded_read_only')),
    acquired_at_ms         INTEGER NOT NULL CHECK (acquired_at_ms >= 0),
    expires_at_ms          INTEGER NOT NULL CHECK (expires_at_ms > acquired_at_ms),
    completed_at_ms        INTEGER
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_byok_transition_fence_tenant_epoch
    ON byok_transition_fence (tenant_id, epoch);

CREATE INDEX IF NOT EXISTS idx_byok_transition_fence_expiry
    ON byok_transition_fence (expires_at_ms);

CREATE TABLE IF NOT EXISTS byok_data_intent (
    token                   TEXT    PRIMARY KEY,
    tenant_id               TEXT    NOT NULL,
    operation               TEXT    NOT NULL
        CHECK (operation IN ('read', 'write', 'delete')),
    outcome                 TEXT    NOT NULL DEFAULT 'active'
        CHECK (outcome IN ('active', 'completed', 'expired')),
    observed_gate_epoch     INTEGER NOT NULL
        CHECK (typeof(observed_gate_epoch) = 'integer'
               AND observed_gate_epoch > 0
               AND observed_gate_epoch <= 9223372036854775806),
    observed_generation     INTEGER NOT NULL
        CHECK (typeof(observed_generation) = 'integer'
               AND observed_generation >= 0
               AND observed_generation <= 9223372036854775806),
    observed_config_version INTEGER
        CHECK (observed_config_version IS NULL OR
               (typeof(observed_config_version) = 'integer'
                AND observed_config_version > 0
                AND observed_config_version <= 9223372036854775806)),
    observed_config_state   TEXT    NOT NULL
        CHECK (observed_config_state IN
            ('absent', 'inactive', 'pending', 'active', 'partial')),
    observed_byok_status    TEXT    NOT NULL
        CHECK (observed_byok_status = 'active'),
    acquired_at_ms          INTEGER NOT NULL CHECK (acquired_at_ms >= 0),
    expires_at_ms           INTEGER NOT NULL CHECK (expires_at_ms > acquired_at_ms),
    completed_at_ms         INTEGER
);

CREATE INDEX IF NOT EXISTS idx_byok_data_intent_tenant_expiry
    ON byok_data_intent (tenant_id, outcome, expires_at_ms);

-- Classify crashed/expired predecessors in the same SQLite statement that
-- admits a successor. This keeps the persistent outcome truthful without a
-- correctness dependency on an asynchronous sweeper.
CREATE TRIGGER IF NOT EXISTS trg_byok_intent_classify_expired_fence
BEFORE INSERT ON byok_data_intent
BEGIN
    UPDATE byok_transition_fence
       SET outcome = 'expired', completed_at_ms = expires_at_ms
     WHERE tenant_id = NEW.tenant_id AND outcome = 'active'
       AND expires_at_ms <= (CAST(strftime('%s', 'now') AS INTEGER) * 1000);
    UPDATE byok_data_intent
       SET outcome = 'expired', completed_at_ms = expires_at_ms
     WHERE tenant_id = NEW.tenant_id AND outcome = 'active'
       AND expires_at_ms <= (CAST(strftime('%s', 'now') AS INTEGER) * 1000);
END;

CREATE TRIGGER IF NOT EXISTS trg_byok_transition_classify_expired
BEFORE INSERT ON byok_transition_fence
BEGIN
    UPDATE byok_transition_fence
       SET outcome = 'expired', completed_at_ms = expires_at_ms
     WHERE tenant_id = NEW.tenant_id AND outcome = 'active'
       AND expires_at_ms <= (CAST(strftime('%s', 'now') AS INTEGER) * 1000);
    UPDATE byok_data_intent
       SET outcome = 'expired', completed_at_ms = expires_at_ms
     WHERE tenant_id = NEW.tenant_id AND outcome = 'active'
       AND expires_at_ms <= (CAST(strftime('%s', 'now') AS INTEGER) * 1000);
END;

-- Generation catalog. R2 object keys are never addressable merely because a
-- PUT completed: only the publication row makes one exact generation visible.
CREATE TABLE IF NOT EXISTS byok_logical_object_generation (
    tenant_id      TEXT NOT NULL,
    object_kind    TEXT NOT NULL CHECK (object_kind IN ('cas', 'ac')),
    logical_key    TEXT NOT NULL,
    generation     INTEGER NOT NULL
        CHECK (typeof(generation) = 'integer'
               AND generation > 0
               AND generation <= 9223372036854775806),
    allocation_id  TEXT NOT NULL,
    physical_key   TEXT NOT NULL,
    intent_token   TEXT NOT NULL,
    gate_epoch     INTEGER NOT NULL
        CHECK (typeof(gate_epoch) = 'integer'
               AND gate_epoch > 0
               AND gate_epoch <= 9223372036854775806),
    size_bytes     INTEGER NOT NULL CHECK (size_bytes >= 0),
    outcome        TEXT NOT NULL DEFAULT 'allocated'
        CHECK (outcome IN ('allocated', 'published', 'abandoned')),
    allocated_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, object_kind, logical_key, generation, allocation_id),
    UNIQUE (tenant_id, physical_key)
);

CREATE INDEX IF NOT EXISTS idx_byok_object_generation_full_identity
    ON byok_logical_object_generation
       (tenant_id, object_kind, logical_key, generation, allocation_id, outcome);

CREATE TABLE IF NOT EXISTS byok_logical_object_publication (
    tenant_id      TEXT NOT NULL,
    object_kind    TEXT NOT NULL CHECK (object_kind IN ('cas', 'ac')),
    logical_key    TEXT NOT NULL,
    generation     INTEGER NOT NULL
        CHECK (typeof(generation) = 'integer'
               AND generation > 0
               AND generation <= 9223372036854775806),
    allocation_id  TEXT NOT NULL,
    physical_key   TEXT NOT NULL,
    gate_epoch     INTEGER NOT NULL
        CHECK (typeof(gate_epoch) = 'integer'
               AND gate_epoch > 0
               AND gate_epoch <= 9223372036854775806),
    size_bytes     INTEGER NOT NULL CHECK (size_bytes >= 0),
    published_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, object_kind, logical_key),
    UNIQUE (tenant_id, physical_key)
);

CREATE INDEX IF NOT EXISTS idx_byok_object_publication_full_identity
    ON byok_logical_object_publication
       (tenant_id, object_kind, logical_key, generation, allocation_id, physical_key);
