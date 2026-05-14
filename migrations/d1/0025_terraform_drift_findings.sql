-- WI-S13-004 — D1 migration: terraform_drift_findings audit table.
-- Additive only (INV-AUTH-MIGRATION-ADDITIVE herdada).
-- Per-run row inserted even when drift_count = 0 (cron health check).
-- INV-AUDIT-APPEND-ONLY (CRITICAL): no UPDATE/DELETE on this table
--   except status + remediation fields (append-semantics via explicit allowlist).

CREATE TABLE IF NOT EXISTS terraform_drift_findings (
    -- Primary key: UUIDv7 (sortable by time; 16 bytes BLOB)
    finding_id BLOB(16) PRIMARY KEY,

    -- Which region this finding covers
    region TEXT NOT NULL CHECK (region IN ('us-east', 'us-west', 'eu-west', 'ap-southeast', 'sa-east', 'global')),

    -- When the cron run detected this (epoch ms)
    detected_at_ms BIGINT NOT NULL,

    -- Number of resources with diff (0 = clean run, > 0 = drift)
    plan_diff_count INTEGER NOT NULL DEFAULT 0,

    -- Human-readable summary from terraform plan output (top 5 resources)
    plan_summary TEXT NOT NULL DEFAULT '',

    -- GitHub Actions artifact URL for full plan output
    plan_full_artifact_url TEXT,

    -- Severity classification
    -- none: diff_count = 0 (clean run)
    -- low:  diff_count = 1–2 (minor; e.g. tag update)
    -- medium: diff_count = 3–10 (moderate; structural changes)
    -- high:  diff_count > 10 or critical resource changed
    severity TEXT NOT NULL CHECK (severity IN ('none', 'low', 'medium', 'high')),

    -- Lifecycle status
    -- open:         drift detected; no remediation decision yet
    -- investigating: SRE actively investigating root cause
    -- remediated:   drift reconciled (apply / import / revert)
    -- wontfix:      accepted drift (documented acceptable pattern)
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'investigating', 'remediated', 'wontfix')),

    -- Remediation decision per RB-FM-206 decision tree
    -- apply:       terraform apply to reconcile IaC to actual state
    -- investigate: root cause investigation in progress
    -- revert:      manual change reverted; re-drift expected
    remediation_decision TEXT CHECK (remediation_decision IN ('apply', 'investigate', 'revert')),

    -- When remediation completed (epoch ms); NULL if not yet remediated
    remediated_at_ms BIGINT,

    -- Admin user_id who executed remediation (dual-approval gated via WI-S13-002)
    remediated_by_user_id BLOB(16),

    -- GitHub Actions run ID for traceability
    github_run_id TEXT,

    -- Runbook reference (always RB-FM-206 for this table)
    runbook_ref TEXT NOT NULL DEFAULT 'RB-FM-206',

    -- Audit chain: link to preceding finding_id for append-only integrity
    prev_finding_id BLOB(16)
);

-- Fast query of open findings (monitoring + age gauge)
CREATE INDEX IF NOT EXISTS idx_terraform_drift_open
    ON terraform_drift_findings (status, detected_at_ms)
    WHERE status = 'open';

-- Fast query by region (per-region trend reports)
CREATE INDEX IF NOT EXISTS idx_terraform_drift_region_time
    ON terraform_drift_findings (region, detected_at_ms);

-- Fast query by severity (metric cardinality)
CREATE INDEX IF NOT EXISTS idx_terraform_drift_severity
    ON terraform_drift_findings (severity, detected_at_ms);
