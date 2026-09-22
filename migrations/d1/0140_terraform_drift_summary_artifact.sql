-- 0140 — WI-S13-004 / #1722 — bounded Terraform drift evidence.
-- Additive migration: existing plan_full_artifact_url is retained for
-- historical rows, but new writes MUST use this field and contain only the
-- sanitized summary artifact. Raw .tfplan, plan JSON, and terminal logs are
-- forbidden evidence values.
ALTER TABLE terraform_drift_findings
    ADD COLUMN plan_summary_artifact_url TEXT;
