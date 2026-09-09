-- 0120_byok_backfill_run.sql
--
-- Durable checkpoint ledger for the B-083 tenant-wide CAS+AC encryption
-- backfill.  Target-generation objects/catalog rows remain unreachable while
-- the transition is copying.  The runtime may set phase='committed' only in
-- the same guarded transaction that advances
-- byok_tenant_gate.current_generation and activates tenant_byok_config.

-- These columns let a retry verify that an existing R2 object is the exact
-- ciphertext it previously staged without carrying body bytes through D1.
-- `allocation_id` is already mandatory in migration 0118.
ALTER TABLE byok_logical_object_generation
    ADD COLUMN backfill_run_id TEXT;
ALTER TABLE byok_logical_object_generation
    ADD COLUMN ciphertext_size INTEGER CHECK (ciphertext_size >= 0);
ALTER TABLE byok_logical_object_generation
    ADD COLUMN ciphertext_blake3 TEXT;

CREATE INDEX IF NOT EXISTS idx_byok_object_generation_backfill_run
    ON byok_logical_object_generation
       (tenant_id, backfill_run_id, object_kind, logical_key)
    WHERE backfill_run_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS byok_backfill_run (
    tenant_id          TEXT NOT NULL,
    run_id             TEXT NOT NULL UNIQUE,
    intent_token       TEXT NOT NULL UNIQUE,
    transition_epoch   INTEGER NOT NULL CHECK (transition_epoch > 0),
    gate_epoch         INTEGER NOT NULL CHECK (gate_epoch > 0),
    source_generation  INTEGER NOT NULL CHECK (source_generation >= 0),
    target_generation  INTEGER NOT NULL CHECK (target_generation = source_generation + 1),
    phase              TEXT NOT NULL DEFAULT 'copying'
        CHECK (phase IN ('copying', 'ready_to_commit', 'committed', 'aborted')),
    cas_cursor         TEXT,
    ac_cursor          TEXT,
    cas_complete       INTEGER NOT NULL DEFAULT 0 CHECK (cas_complete IN (0, 1)),
    ac_complete        INTEGER NOT NULL DEFAULT 0 CHECK (ac_complete IN (0, 1)),
    cas_copied         INTEGER NOT NULL DEFAULT 0 CHECK (cas_copied >= 0),
    ac_copied          INTEGER NOT NULL DEFAULT 0 CHECK (ac_copied >= 0),
    started_at_ms      INTEGER NOT NULL CHECK (started_at_ms >= 0),
    checkpointed_at_ms INTEGER NOT NULL CHECK (checkpointed_at_ms >= started_at_ms),
    completed_at_ms    INTEGER,
    failure_reason     TEXT,
    PRIMARY KEY (tenant_id, target_generation),
    FOREIGN KEY (intent_token) REFERENCES byok_transition_fence(token),
    CHECK (phase NOT IN ('ready_to_commit', 'committed')
           OR (cas_complete = 1 AND ac_complete = 1)),
    CHECK ((phase IN ('committed', 'aborted')) = (completed_at_ms IS NOT NULL)),
    CHECK ((phase = 'aborted') = (failure_reason IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_byok_backfill_live_tenant
    ON byok_backfill_run (tenant_id)
    WHERE phase IN ('copying', 'ready_to_commit');

-- Identity/capability fields and completed surface checkpoints never move
-- backwards. Cursors may advance or stay equal for an idempotent replay.
CREATE TRIGGER IF NOT EXISTS trg_byok_backfill_run_forward_only
BEFORE UPDATE ON byok_backfill_run
WHEN ((OLD.intent_token IS NOT NEW.intent_token
       OR OLD.transition_epoch IS NOT NEW.transition_epoch)
      AND NOT (
        OLD.phase IN ('copying', 'ready_to_commit')
        AND EXISTS (
          SELECT 1 FROM byok_transition_fence old_fence
          WHERE old_fence.tenant_id = OLD.tenant_id
            AND old_fence.token = OLD.intent_token
            AND old_fence.epoch = OLD.transition_epoch
            AND (old_fence.outcome = 'expired'
                 OR (old_fence.outcome = 'active'
                     AND old_fence.expires_at_ms <=
                         (CAST(strftime('%s', 'now') AS INTEGER) * 1000)))
        )
        AND EXISTS (
          SELECT 1 FROM byok_transition_fence new_fence
          WHERE new_fence.tenant_id = NEW.tenant_id
            AND new_fence.token = NEW.intent_token
            AND new_fence.epoch = NEW.transition_epoch
            AND new_fence.outcome = 'active'
            AND new_fence.expires_at_ms >
                (CAST(strftime('%s', 'now') AS INTEGER) * 1000)
            AND new_fence.observed_gate_epoch = NEW.gate_epoch
            AND new_fence.observed_generation = NEW.source_generation
        )
      ))
  OR OLD.run_id IS NOT NEW.run_id
  OR OLD.gate_epoch IS NOT NEW.gate_epoch
  OR OLD.source_generation IS NOT NEW.source_generation
  OR OLD.target_generation IS NOT NEW.target_generation
  OR NEW.cas_complete < OLD.cas_complete
  OR NEW.ac_complete < OLD.ac_complete
  OR NEW.cas_copied < OLD.cas_copied
  OR NEW.ac_copied < OLD.ac_copied
  OR (OLD.phase = 'copying' AND NEW.phase NOT IN ('copying', 'ready_to_commit', 'aborted'))
  OR (OLD.phase = 'ready_to_commit' AND NEW.phase NOT IN ('ready_to_commit', 'committed', 'aborted'))
  OR (OLD.phase IN ('committed', 'aborted') AND NEW.phase IS NOT OLD.phase)
BEGIN
    SELECT RAISE(ABORT, 'byok backfill identity/state is not forward-only');
END;

-- A checkpoint can refer only to target-generation catalog rows owned by the
-- exact transition capability. The production checkpoint transaction first
-- performs an idempotent generation-row upsert, then advances this ledger.
CREATE INDEX IF NOT EXISTS idx_byok_backfill_generation_owner
    ON byok_logical_object_generation
       (tenant_id, generation, intent_token, gate_epoch, object_kind, logical_key);
