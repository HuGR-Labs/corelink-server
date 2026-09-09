-- B-089: canonical monthly SLA observations + idempotent credit settlement.
--
-- This is the next free migration ordinal on the integration base (0116 is
-- synthetic-page delivery).  Do not renumber this file: migrations are applied
-- sequentially in production and 0108 is already occupied by another lane.
--
-- `sla_monthly_observations` is the append-only hand-off from the
-- provider-operated Prometheus/Grafana report writer.  The Worker never
-- invents uptime values; it publishes an observation only after the strict UTC
-- month and the ten-business-day report cutoff have elapsed.

CREATE TABLE IF NOT EXISTS sla_monthly_observations (
    tenant_id TEXT NOT NULL,
    service_period TEXT NOT NULL,
    tier TEXT NOT NULL,
    monthly_fee_minor INTEGER NOT NULL CHECK (monthly_fee_minor >= 0),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    availability_percent REAL NOT NULL CHECK (availability_percent >= 0 AND availability_percent <= 100),
    latency_excess_percent REAL,
    latency_sustained_minutes INTEGER,
    dsr_breach_days INTEGER,
    billing_drift_percent REAL,
    billing_drift_sustained_hours REAL,
    force_majeure INTEGER NOT NULL DEFAULT 0 CHECK (force_majeure IN (0, 1)),
    cutoff_at_ms INTEGER NOT NULL,
    observed_at_ms INTEGER NOT NULL,
    published_at_ms INTEGER,
    PRIMARY KEY (tenant_id, service_period),
    CHECK (service_period GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]'),
    CHECK (CAST(substr(service_period, 6, 2) AS INTEGER) BETWEEN 1 AND 12),
    CHECK (cutoff_at_ms >= 0 AND observed_at_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_sla_observations_publish_due
    ON sla_monthly_observations (published_at_ms, cutoff_at_ms, service_period, tenant_id);

CREATE TABLE IF NOT EXISTS sla_monthly_measurements (
    tenant_id TEXT NOT NULL,
    service_period TEXT NOT NULL,
    tier TEXT NOT NULL,
    monthly_fee_minor INTEGER NOT NULL CHECK (monthly_fee_minor >= 0),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    availability_percent REAL NOT NULL CHECK (availability_percent >= 0 AND availability_percent <= 100),
    latency_excess_percent REAL,
    latency_sustained_minutes INTEGER,
    dsr_breach_days INTEGER,
    billing_drift_percent REAL,
    billing_drift_sustained_hours REAL,
    force_majeure INTEGER NOT NULL DEFAULT 0 CHECK (force_majeure IN (0, 1)),
    eligible_at_ms INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'evaluated', 'ineligible')),
    decision_reason TEXT,
    credit_percent INTEGER NOT NULL DEFAULT 0 CHECK (credit_percent BETWEEN 0 AND 100),
    amount_minor INTEGER NOT NULL DEFAULT 0 CHECK (amount_minor >= 0),
    evaluated_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, service_period),
    CHECK (service_period GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]'),
    CHECK (CAST(substr(service_period, 6, 2) AS INTEGER) BETWEEN 1 AND 12)
);

CREATE INDEX IF NOT EXISTS idx_sla_measurements_pending
    ON sla_monthly_measurements (state, eligible_at_ms, tenant_id, service_period);

CREATE TABLE IF NOT EXISTS sla_credit_ledger (
    credit_id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    service_period TEXT NOT NULL,
    credit_percent INTEGER NOT NULL CHECK (credit_percent BETWEEN 1 AND 100),
    amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    stripe_customer_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'failed', 'applied', 'blocked')),
    idempotency_key TEXT NOT NULL UNIQUE,
    catastrophic INTEGER NOT NULL DEFAULT 0 CHECK (catastrophic IN (0, 1)),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    lease_until_ms INTEGER,
    next_attempt_at_ms INTEGER NOT NULL,
    provider_ref TEXT,
    failure_kind TEXT CHECK (failure_kind IN ('transient', 'permanent')),
    failure_reason TEXT,
    applied_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE (tenant_id, service_period)
);

CREATE INDEX IF NOT EXISTS idx_sla_credit_due
    ON sla_credit_ledger (status, next_attempt_at_ms, credit_id);

-- A row is inserted atomically with the processing lease. It survives a
-- Worker death after Stripe accepted the request, so retry uses the same body
-- and Idempotency-Key instead of manufacturing a second credit.
CREATE TABLE IF NOT EXISTS sla_credit_outbox (
    credit_id TEXT NOT NULL PRIMARY KEY,
    idempotency_key TEXT NOT NULL UNIQUE,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'sent', 'failed', 'blocked', 'needs_review')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    provider_ref TEXT,
    last_error TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sla_credit_outbox_pending
    ON sla_credit_outbox (status, updated_at_ms, credit_id);

-- Provider-object verification is recorded separately from the local applied
-- state.  A Stripe success therefore cannot disappear into an untracked gap.
CREATE TABLE IF NOT EXISTS sla_credit_reconciliation (
    credit_id TEXT NOT NULL PRIMARY KEY,
    provider_ref TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'reconciled', 'mismatch')),
    next_attempt_at_ms INTEGER NOT NULL,
    last_error TEXT,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sla_credit_reconciliation_due
    ON sla_credit_reconciliation (status, next_attempt_at_ms, credit_id);

CREATE TABLE IF NOT EXISTS sla_credit_audit_events (
    audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
    credit_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('created', 'applied', 'retry', 'blocked')),
    detail TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sla_credit_audit_credit
    ON sla_credit_audit_events (credit_id, created_at_ms);
