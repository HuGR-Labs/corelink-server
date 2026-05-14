-- Migration 0033: Runbook dry-run drill log (WI-S17-003)
--
-- Operationalizes PAT-RUNBOOK-DRILL-001 (resilience_patterns canonical
-- monthly cadence) + FM-202 (runbook desatualizado) drift detection.
--
-- One row per dry-run execution. The CF Cron monthly check scans this table
-- and emits OverdueAlert events for any P0/P1 runbook in the catalog
-- (specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md) whose `MAX(executed_at)`
-- is ≥ 30 days stale or absent.
--
-- Append-only (no UPDATE / DELETE in normal operation). Evidence URL points
-- to R2 `evidence-runbooks/<asciinema-cast>` (1y retention; EVT-017 forensic).
--
-- Privacy: `executor` is an operator id (`op_<alias>`), zero PII per
-- CTRL-PRIV-001. `notes` is sanitized free-text (no secrets, no PII).
--
-- Outcome ENUM (snake_case canonical, matches `corelink_runbook_dry_run_total`
-- Prometheus label):
--   pass           — drill clean, duration_ratio ≤ 2.0
--   fail           — drill failed mid-process; runbook needs urgent update
--   drift_flagged  — duration_ratio > 2.0 → FM-202 post-mortem trigger
--
-- Quality Standard 14.s17.2: ratio > 2× fires post-mortem. The trigger is
-- emitted by the host adapter (CF Worker) at insert time when outcome ==
-- drift_flagged; this migration only persists the record.

CREATE TABLE IF NOT EXISTS runbook_drills (
    drill_id          TEXT    NOT NULL PRIMARY KEY,
    runbook_id        TEXT    NOT NULL,
    executor          TEXT    NOT NULL,
    executed_at       BIGINT  NOT NULL,
    duration_seconds  BIGINT  NOT NULL CHECK (duration_seconds >= 0),
    expected_seconds  BIGINT  NOT NULL CHECK (expected_seconds > 0),
    outcome           TEXT    NOT NULL CHECK (outcome IN ('pass', 'fail', 'drift_flagged')),
    evidence_url      TEXT    NOT NULL,
    notes             TEXT
);

-- Lookup: "last drill for this runbook" (CF Cron scan_overdue path).
CREATE INDEX IF NOT EXISTS idx_runbook_drills_runbook_executed
    ON runbook_drills (runbook_id, executed_at DESC);

-- Lookup: per-operator cadence audit (oncall rotation review).
CREATE INDEX IF NOT EXISTS idx_runbook_drills_executor
    ON runbook_drills (executor);

-- Lookup: drift alerting + post-mortem trigger reconciliation.
CREATE INDEX IF NOT EXISTS idx_runbook_drills_outcome
    ON runbook_drills (outcome);
