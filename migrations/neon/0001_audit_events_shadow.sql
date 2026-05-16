-- CoreLink Neon Postgres — audit-events analytics shadow table (Wave 18).
--
-- Canonical sources:
--   - specs/_audits/2026-05-15-neon-analytics-shadow.md (this wave audit doc)
--   - specs/_audits/2026-05-15-audit-chain-retention.md (R2 = canonical chain)
--   - crates/corelink-audit-chain/src/neon_shadow.rs (NeonShadowSink trait)
--   - specs/03_architecture/invariant_registry.md
--       INV-OBS-AUDIT-CHAIN-INTEGRITY    (HIGH; R2 = canonical; Neon = analytics)
--       INV-DATA-RESIDENCY               (CRITICAL; per-region Neon writes)
--       INV-AUTH-SCHEMA-RLS-DEFAULT-ON   (CRITICAL; tenant isolation at SQL layer)
--       INV-AUTH-MIGRATION-ADDITIVE      (HIGH; this migration is additive-only)
--
-- ## Architecture
--
-- The R2 NDJSON archive (Wave 15, see `archive_producer.rs`) remains the
-- canonical chain-integrity store. Neon Postgres receives a **shadow copy**
-- of every flushed chunk's events for customer-facing analytics queries
-- (multi-event aggregation, time-range filtering, tenant-cross-referencing).
-- Customers SQL-query the shadow via `GET /v1/audit/analytics/*` endpoints.
--
-- ## Source-of-truth discipline
--
-- The shadow is **analytics-only** — never authoritative for chain integrity.
-- The daily-verify cron (`audit-chain-daily-verify.yml`) walks R2; Neon
-- divergence is treated as a SEV-2 analytics lag, NOT a SEV-0 chain break.
-- On any analytics anomaly the operator pivots to R2 + the canonical verifier
-- via `corelink audit verify` (CLI shipped Wave-15.3).
--
-- ## Lag SLO
--
-- Shadow MAY lag R2 archive by ≤ 5 min nominal (analytics use case). Breach
-- ≥ 60 min surfaces SEV-2 PagerDuty alert per
-- `dashboards/alerts/dash-neon-shadow-lag.yml` (follow-on Lote).
--
-- ## Residency
--
-- The shadow table lives in the tenant's pinned Neon region (INV-DATA-RESIDENCY).
-- Production wiring binds one Neon project per region (`enam` / `weur` / `sam`)
-- and the `RealNeonShadowSink` selects the project via the
-- `corelink-region::region::Region` of the chunk's events.

-- ---------------------------------------------------------------------------
-- audit_events_shadow — analytics shadow of audit-chain events
-- ---------------------------------------------------------------------------
-- Per the WI shape spec:
--   (seq BIGINT PK, tenant_id UUID INDEX, event_time TIMESTAMPTZ INDEX,
--    event_type TEXT INDEX, prev_hash BYTEA, link_hash BYTEA,
--    payload_jsonb JSONB)
--
-- The PRIMARY KEY is composite (tenant_id, seq) so per-tenant chains
-- partition cleanly and INSERT (idempotent on tenant_id+seq) is the only
-- write path. INV-AUDIT-APPEND-ONLY: no UPDATE / no DELETE; INSERT only.
CREATE TABLE IF NOT EXISTS audit_events_shadow (
    tenant_id    UUID        NOT NULL,
    seq          BIGINT      NOT NULL CHECK (seq >= 0),
    event_time   TIMESTAMPTZ NOT NULL,
    event_type   TEXT        NOT NULL,
    prev_hash    BYTEA       NOT NULL,
    link_hash    BYTEA       NOT NULL,
    payload_jsonb JSONB      NOT NULL,
    synced_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, seq)
);

-- Per-tenant secondary indexes for analytics range scans.
CREATE INDEX IF NOT EXISTS idx_audit_events_shadow_tenant_time
    ON audit_events_shadow(tenant_id, event_time);

CREATE INDEX IF NOT EXISTS idx_audit_events_shadow_tenant_event_type
    ON audit_events_shadow(tenant_id, event_type);

CREATE INDEX IF NOT EXISTS idx_audit_events_shadow_event_type_time
    ON audit_events_shadow(event_type, event_time);

-- ---------------------------------------------------------------------------
-- Row-Level Security (INV-AUTH-SCHEMA-RLS-DEFAULT-ON; INV-TENANT-ISOLATION)
-- ---------------------------------------------------------------------------
-- Mirrors the membership / pat / revocation_log policy from migration 002.
-- The application MUST `SET LOCAL app.current_tenant = '<uuid>'` inside the
-- transaction; SELECTs without the GUC return zero rows.
ALTER TABLE audit_events_shadow ENABLE ROW LEVEL SECURITY;

DO $$
BEGIN
    CREATE POLICY tenant_isolation_audit_events_shadow ON audit_events_shadow
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ---------------------------------------------------------------------------
-- audit_shadow_lag — running lag observation per (region, tenant) pair
-- ---------------------------------------------------------------------------
-- The shadow-sync pipeline records the LAG at the time of every successful
-- chunk sync. The lag dashboard queries `max(last_observed_lag_ms)` per
-- region; the SEV-2 alert fires when any tenant's last_observed_lag_ms
-- exceeds 60 min for 5 consecutive samples.
CREATE TABLE IF NOT EXISTS audit_shadow_lag (
    tenant_id              UUID        NOT NULL,
    region                 TEXT        NOT NULL,
    last_synced_r2_seq     BIGINT      NOT NULL CHECK (last_synced_r2_seq >= 0),
    last_observed_lag_ms   BIGINT      NOT NULL CHECK (last_observed_lag_ms >= 0),
    sample_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, region)
);

ALTER TABLE audit_shadow_lag ENABLE ROW LEVEL SECURITY;

DO $$
BEGIN
    CREATE POLICY tenant_isolation_audit_shadow_lag ON audit_shadow_lag
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
