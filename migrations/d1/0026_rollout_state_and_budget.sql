-- Migration 0024: Progressive rollout state + budget consumption tables
-- WI-S13-005 — S-13 admin plane, HIGH_RISK lane
-- Invariants: INV-ROLLOUT-SINGLE-ACTIVE (UNIQUE active), INV-ROLLOUT-BUDGET-CAP
-- References: CAP-ADMIN-005, PAT-PROGRESSIVE-ROLLOUT-001, FM-200

-- Rollout session audit log.
-- Per-env singleton enforced via UNIQUE partial index on status='active'.
CREATE TABLE IF NOT EXISTS rollout_state (
    handle_id              BLOB(16) PRIMARY KEY,
    deploy_artifact_sha256 BLOB(32) NOT NULL,
    cosign_signature_url   TEXT NOT NULL,           -- INV-SUPPLY-SIGNED-DEPLOY (S-12)
    rekor_log_index        BIGINT NOT NULL,          -- INV-SUPPLY-PROVENANCE-IN-REKOR (S-12)
    current_stage          TEXT NOT NULL CHECK (current_stage IN (
                               'stage_1pct', 'stage_10pct',
                               'stage_50pct', 'stage_100pct'
                           )),
    status                 TEXT NOT NULL CHECK (status IN (
                               'pending', 'active', 'completed',
                               'auto_rolled_back', 'manually_aborted',
                               'budget_frozen'
                           )),
    started_at_ms          BIGINT NOT NULL,
    stage_entered_at_ms    BIGINT NOT NULL,
    completed_at_ms        BIGINT,
    rollback_trigger       TEXT,                     -- if status = auto_rolled_back
    rollback_at_ms         BIGINT,
    actor_user_id          BLOB(16) NOT NULL
);

-- Enforce: at most 1 active rollout per environment at a time.
-- INV-ROLLOUT-SINGLE-ACTIVE. D1 partial-index syntax (SQLite compat).
CREATE UNIQUE INDEX IF NOT EXISTS idx_rollout_state_unique_active
    ON rollout_state (status)
    WHERE status = 'active';

-- Per-rollback error budget consumption tracking (measured burn, Lote 10.13 P1).
-- Rolling 30d window query: WHERE rollback_started_ms >= (now_ms - 30d_ms).
CREATE TABLE IF NOT EXISTS rollout_budget_consumption (
    record_id                    BLOB(16) PRIMARY KEY,
    handle_id                    BLOB(16) NOT NULL,
    rollback_started_ms          BIGINT NOT NULL,
    rollback_completed_ms        BIGINT NOT NULL,
    error_count_consumed         BIGINT NOT NULL DEFAULT 0,
    monthly_error_budget_target  BIGINT NOT NULL,
    budget_consumed_bps          INTEGER NOT NULL DEFAULT 0, -- basis points [0, 10000]
    created_at_ms                BIGINT NOT NULL
);

-- Index for rolling-window SUM query (consumed_ratio_mtd computation).
CREATE INDEX IF NOT EXISTS idx_rollout_budget_started_ms
    ON rollout_budget_consumption (rollback_started_ms);
