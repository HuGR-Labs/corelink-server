-- migrations/d1/0106_devenv_monthly_vcpu.sql
-- CoreLink DevEnv: monthly aggregated vCPU-second meter table (WP-07 / INV-05)
-- Keyed by (tenant_id, month_at) where month_at is unix timestamp (ms) of the 1st of the month UTC.

CREATE TABLE IF NOT EXISTS devenv_monthly_vcpu (
    tenant_id    TEXT    NOT NULL,
    month_at     INTEGER NOT NULL,
    vcpu_seconds INTEGER NOT NULL DEFAULT 0,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, month_at)
);

CREATE INDEX IF NOT EXISTS idx_devenv_monthly_vcpu_month
    ON devenv_monthly_vcpu (month_at);
