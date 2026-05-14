-- Migration 0034: DR drill runs table (WI-S17-002)
--
-- Tracks every DR drill execution (cycle 1 CF region outage; cycles 2/3 deferred
-- annual at GA via waiver opt). One row per drill_id. Persists pre/post wall-clock,
-- target region, sibling failover target, measured RTO + RPO, SLO impact, terminal
-- status. R2 archive `evidence-dr-drills/cycle-N/{drill_id}/report.json` is the
-- authoritative 7y store (per Quality Standard 14.s17.7); D1 is the index.
--
-- Security: drill is staging-only at GA. `env` column enforces the canonical
-- guard (CHECK constraint accepts only 'staging' / 'test' — Production rejected
-- at code-level FIRST per Lote 10.17 P0; D1 is defense-in-depth).
--
-- Cardinality: NUNCA per-tenant labels in derived Prometheus metrics
-- (INV-OBS-CARDINALITY-BUDGET S-09).

CREATE TABLE IF NOT EXISTS dr_drill_runs (
    drill_id            TEXT    NOT NULL PRIMARY KEY,
    cycle               TEXT    NOT NULL CHECK (cycle IN ('cf_region_outage', 'd1_primary_loss', 'byok_key_compromise')),
    env                 TEXT    NOT NULL CHECK (env IN ('staging', 'test')),
    started_at_ms       BIGINT  NOT NULL,
    simulated_region    TEXT    NOT NULL CHECK (simulated_region IN ('wnam', 'enam', 'weur', 'sam')),
    -- failover_target NULL only if drill aborted before failover engaged.
    failover_target     TEXT             CHECK (failover_target IS NULL OR failover_target IN ('wnam', 'enam', 'weur', 'sam')),
    -- RTO / RPO measured in seconds. 0 until drill enters terminal state.
    rto_seconds         INTEGER NOT NULL DEFAULT 0 CHECK (rto_seconds >= 0),
    rpo_seconds         INTEGER NOT NULL DEFAULT 0 CHECK (rpo_seconds >= 0),
    -- Canonical SLO ceilings (RTO ≤ 1800, RPO ≤ 60) are NOT enforced at DB level;
    -- the application layer raises `DrillError::SloViolation` and updates `status`.
    slo_impact_pct      REAL    NOT NULL DEFAULT 0.0 CHECK (slo_impact_pct >= 0.0 AND slo_impact_pct <= 100.0),
    slo_error_count     BIGINT  NOT NULL DEFAULT 0  CHECK (slo_error_count >= 0),
    slo_latency_p99_ms  BIGINT  NOT NULL DEFAULT 0  CHECK (slo_latency_p99_ms >= 0),
    completed_at_ms     BIGINT,
    status              TEXT    NOT NULL CHECK (status IN ('scheduled', 'in_progress', 'completed', 'failed', 'aborted')),
    -- R2 archive object key. Authoritative 7y store.
    r2_report_key       TEXT,
    -- Free-form. Filled when terminal. NUNCA PII (CTRL-PRIV-001).
    sre_lead            TEXT
);

-- Index for the dashboard `recent drills per cycle` panel.
CREATE INDEX IF NOT EXISTS idx_dr_drill_runs_cycle_started
    ON dr_drill_runs (cycle, started_at_ms DESC);

-- Index for the SRE alert query `dr_drill cadence missed` (most-recent per cycle).
CREATE INDEX IF NOT EXISTS idx_dr_drill_runs_status
    ON dr_drill_runs (status, started_at_ms DESC);
