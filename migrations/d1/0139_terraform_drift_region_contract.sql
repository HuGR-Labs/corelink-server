-- WI-S13-004 / #1720 / #1721 / #1722 — align drift findings with the
-- four regional Terraform roots and keep old rows readable.
--
-- The original 0025 migration used the pre-S14 names (us-east, us-west,
-- eu-west, ap-southeast, sa-east, and global). The production workflow now
-- emits only wnam, enam, weur, and sam. D1/SQLite cannot alter an inline CHECK
-- constraint, so rebuild the table and add an insert guard for new events.
-- Legacy literals stay in the CHECK solely so already persisted audit rows are
-- copied without data loss; the trigger makes the new write contract strict.

PRAGMA foreign_keys = OFF;
PRAGMA defer_foreign_keys = true;

CREATE TABLE terraform_drift_findings_new (
    finding_id BLOB(16) PRIMARY KEY,
    region TEXT NOT NULL CHECK (
        region IN (
            'wnam', 'enam', 'weur', 'sam', 'global',
            'us-east', 'us-west', 'eu-west', 'ap-southeast', 'sa-east'
        )
    ),
    detected_at_ms BIGINT NOT NULL,
    plan_diff_count INTEGER NOT NULL DEFAULT 0,
    plan_summary TEXT NOT NULL DEFAULT '',
    plan_full_artifact_url TEXT,
    plan_summary_artifact_url TEXT,
    severity TEXT NOT NULL CHECK (severity IN ('none', 'low', 'medium', 'high')),
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'investigating', 'remediated', 'wontfix')),
    remediation_decision TEXT
        CHECK (remediation_decision IN ('apply', 'investigate', 'revert')),
    remediated_at_ms BIGINT,
    remediated_by_user_id BLOB(16),
    github_run_id TEXT,
    runbook_ref TEXT NOT NULL DEFAULT 'RB-FM-206',
    prev_finding_id BLOB(16)
);

INSERT INTO terraform_drift_findings_new (
    finding_id, region, detected_at_ms, plan_diff_count, plan_summary,
    plan_full_artifact_url, plan_summary_artifact_url, severity, status,
    remediation_decision, remediated_at_ms, remediated_by_user_id,
    github_run_id, runbook_ref, prev_finding_id
)
SELECT
    finding_id, region, detected_at_ms, plan_diff_count, plan_summary,
    plan_full_artifact_url, plan_summary_artifact_url, severity, status,
    remediation_decision, remediated_at_ms, remediated_by_user_id,
    github_run_id, runbook_ref, prev_finding_id
FROM terraform_drift_findings;

DROP TABLE terraform_drift_findings; -- additive-allowed: ADR-0102 preserve all rows in the explicit copy while changing the region CHECK
ALTER TABLE terraform_drift_findings_new RENAME TO terraform_drift_findings; -- additive-allowed: ADR-0102 finalize the rebuilt region CHECK after the 1:1 copy

CREATE INDEX IF NOT EXISTS idx_terraform_drift_open
    ON terraform_drift_findings (status, detected_at_ms)
    WHERE status = 'open';
CREATE INDEX IF NOT EXISTS idx_terraform_drift_region_time
    ON terraform_drift_findings (region, detected_at_ms);
CREATE INDEX IF NOT EXISTS idx_terraform_drift_severity
    ON terraform_drift_findings (severity, detected_at_ms);

CREATE TRIGGER terraform_drift_findings_canonical_region_insert
BEFORE INSERT ON terraform_drift_findings
WHEN NEW.region NOT IN ('wnam', 'enam', 'weur', 'sam')
BEGIN
    SELECT RAISE(ABORT, 'terraform drift findings require a canonical region');
END;

CREATE TRIGGER terraform_drift_findings_region_immutable
BEFORE UPDATE OF region ON terraform_drift_findings
WHEN NEW.region <> OLD.region OR NEW.region NOT IN ('wnam', 'enam', 'weur', 'sam')
BEGIN
    SELECT RAISE(ABORT, 'terraform drift finding region is immutable');
END;

PRAGMA foreign_keys = ON;
