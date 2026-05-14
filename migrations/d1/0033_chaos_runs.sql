-- Migration 0033: Chaos scheduler tables (WI-S17-001).
--
-- Creates two tables:
--   chaos_runs     — index for cron scheduler + audit query (one row per run)
--   chaos_results  — per-experiment SLO impact + outcome (one row per
--                    completed/aborted run; aborted rows have NULL impact)
--
-- R2 audit bucket `corelink-audit-{region}/chaos_runs/{run_id}.json` is the
-- authoritative store (7y retention via R2 lifecycle policy per WI §6.1.3).
-- D1 is the index; if a D1 entry is missing, quarterly reconcile rebuilds
-- from R2 (procedure documented in specs/_runbooks/RB-CHAOS-CATALOG.md §10).
--
-- Security:
--   * tenant_id is NOT stored (chaos targets infra; tenant-agnostic).
--   * No PII fields anywhere (CTRL-PRIV-001 enforced).
--   * Audit-event lifecycle: corelink.chaos.run.started / .completed / .aborted.

-- ---------------------------------------------------------------------------
-- chaos_runs — one row per scheduler invocation (cron tick OR manual trigger)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS chaos_runs (
    run_id              TEXT    NOT NULL PRIMARY KEY,
    experiment_id       TEXT    NOT NULL,
    -- Canonical FM-ID covered (FM-051 / FM-150 / FM-057 / FM-152 / FM-054 /
    -- FM-202 / FM-059 / FM-105 at GA per RB-CHAOS-CATALOG).
    fm_id               TEXT    NOT NULL,
    -- chaos kind (latency | failure | resource_exhaustion | network_partition |
    -- cpu_pressure | memory_pressure | dns_failure | auth_provider_unavailable)
    kind                TEXT    NOT NULL CHECK (kind IN (
        'latency',
        'failure',
        'resource_exhaustion',
        'network_partition',
        'cpu_pressure',
        'memory_pressure',
        'dns_failure',
        'auth_provider_unavailable'
    )),
    -- Target env — chaos is STAGING-ONLY at GA (HARD code-level check;
    -- 'production' rows are only ever written for prod_target_violation
    -- aborts so the audit trail remains complete).
    target_env          TEXT    NOT NULL CHECK (target_env IN ('staging', 'production')),
    -- Deterministic PRNG seed derived from run_id + experiment_id (FNV-1a 64b).
    seed                BIGINT  NOT NULL,
    -- R2 object key for the 7y archive manifest.
    -- corelink-audit-{region}/chaos_runs/{run_id}.json
    r2_key              TEXT    NOT NULL,
    started_at_ms       BIGINT  NOT NULL,
    completed_at_ms     BIGINT,           -- NULL while in flight
    -- Outcome label (passed | steady_state_breached | aborted).
    outcome             TEXT    CHECK (outcome IS NULL OR outcome IN (
        'passed',
        'steady_state_breached',
        'aborted'
    ))
);

CREATE INDEX IF NOT EXISTS idx_chaos_runs_experiment_id
    ON chaos_runs (experiment_id);

CREATE INDEX IF NOT EXISTS idx_chaos_runs_fm_id
    ON chaos_runs (fm_id);

CREATE INDEX IF NOT EXISTS idx_chaos_runs_target_env
    ON chaos_runs (target_env);

CREATE INDEX IF NOT EXISTS idx_chaos_runs_started_at
    ON chaos_runs (started_at_ms);

CREATE INDEX IF NOT EXISTS idx_chaos_runs_outcome
    ON chaos_runs (outcome);

-- ---------------------------------------------------------------------------
-- chaos_results — per-run SLO-impact detail (1:1 with chaos_runs.run_id)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS chaos_results (
    run_id                  TEXT    NOT NULL PRIMARY KEY,
    -- Measured SLO impact in basis points (100 = 1%).
    -- NULL when outcome = 'aborted' (state capture never happened).
    slo_impact_bps          INTEGER,
    -- Pre/post state digest (64-bit hex; deterministic for replay verification).
    state_pre_digest        TEXT    CHECK (state_pre_digest IS NULL OR length(state_pre_digest) = 16),
    state_post_digest       TEXT    CHECK (state_post_digest IS NULL OR length(state_post_digest) = 16),
    -- Auto-rollback flag — true iff impact > blast_radius_bps.
    auto_rollback_fired     INTEGER NOT NULL DEFAULT 0 CHECK (auto_rollback_fired IN (0, 1)),
    -- Safe-mode abort trigger (NULL when outcome != 'aborted').
    -- Values: prod_target_violation | prod_sev1_active | staging_error_rate_high
    abort_trigger           TEXT    CHECK (abort_trigger IS NULL OR abort_trigger IN (
        'prod_target_violation',
        'prod_sev1_active',
        'staging_error_rate_high'
    )),
    FOREIGN KEY (run_id) REFERENCES chaos_runs (run_id)
);

CREATE INDEX IF NOT EXISTS idx_chaos_results_auto_rollback
    ON chaos_results (auto_rollback_fired);

CREATE INDEX IF NOT EXISTS idx_chaos_results_abort_trigger
    ON chaos_results (abort_trigger);
