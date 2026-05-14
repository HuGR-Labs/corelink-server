-- Migration 0042: Lighthouse customer tracker (WI-S20-004)
--
-- Operationalizes the 3-lighthouse-customer program (2 team tier — Forge
-- customer-zero + 1 OSS Bazel/Buck2 — + 1 enterprise BYOK ICP) and the 30d
-- SLA observation window per spec contract §6.1 + §10.s20.7 (GA Evidence
-- Gate D+60 criterion).
--
-- Two tables, append-friendly:
--   * lighthouse_customers   — current state per slot (one row per slot).
--   * lighthouse_sla_samples — append-only daily SLA observation samples.
--
-- The Rust state machine (`corelink-lighthouse-tracker`) is fail-CLOSED:
-- illegal transitions are rejected at the library boundary; this migration
-- only enforces the *shape* of legal states via CHECK constraints.
--
-- Privacy: `customer_id` is an INTERNAL lighthouse slot id (`LH-FORGE`,
-- `LH-OSS-01`, `LH-ENT-BYOK-01`) — NOT a tenant_id. Zero PII per
-- CTRL-PRIV-001. The link from slot id → tenant_id lives in the
-- onboarding ledger (S-19) under access control.
--
-- Cardinality budget: at most 3 active rows in `lighthouse_customers`
-- (canonical limited per sprint.md §11 INV-OBS-CARDINALITY-BUDGET).
-- `lighthouse_sla_samples` is bounded by 3 customers × 30 days × 1 sample/day
-- = 90 rows for the full GA window.
--
-- Tier ENUM canonical (snake_case, matches Prometheus `plan` label):
--   team             — Team-tier lighthouse (Forge + OSS).
--   enterprise_byok  — Enterprise BYOK with DPA TBD.
--
-- State ENUM canonical (snake_case, matches
-- `corelink_lighthouse_customer_observation_status_gauge`):
--   recruiting          — outreach + LOI in flight.
--   engaged             — LOI signed + DPA review.
--   migrating           — signup → DPA → tier → first PAT → first CAS PUT.
--   observing           — 30d observation window open.
--   attested            — 30d cleared + attestation form signed.
--   case_study_signed   — Legal-reviewed + customer-approved case study.
--   withdrawn           — customer desistiu mid-sprint (FM mitigation).

CREATE TABLE IF NOT EXISTS lighthouse_customers (
    customer_id              TEXT    NOT NULL PRIMARY KEY,
    tier                     TEXT    NOT NULL CHECK (tier IN ('team', 'enterprise_byok')),
    state                    TEXT    NOT NULL CHECK (state IN (
        'recruiting',
        'engaged',
        'migrating',
        'observing',
        'attested',
        'case_study_signed',
        'withdrawn'
    )),
    loi_signed_at            BIGINT,
    migration_completed_at   BIGINT,
    observation_started_at   BIGINT,
    sla_breach_recorded      INTEGER NOT NULL DEFAULT 0 CHECK (sla_breach_recorded IN (0, 1)),
    attestation_signed_at    BIGINT,
    case_study_signed_at     BIGINT,
    created_at               BIGINT  NOT NULL,
    updated_at               BIGINT  NOT NULL
);

-- Lookup: roster scan by tier (GA Evidence Gate D+60 query).
CREATE INDEX IF NOT EXISTS idx_lighthouse_customers_tier_state
    ON lighthouse_customers (tier, state);

-- Lookup: "who is currently observing?" (CF Cron daily sample emitter).
CREATE INDEX IF NOT EXISTS idx_lighthouse_customers_state
    ON lighthouse_customers (state);

-- Append-only daily SLA observation samples — one row per customer per day.
-- Outcome is computed by the library (`SlaSample::is_met_for`) and emitted
-- to the DASH-LIGHTHOUSE-CUSTOMERS panel; this table is the raw evidence.
CREATE TABLE IF NOT EXISTS lighthouse_sla_samples (
    sample_id              TEXT    NOT NULL PRIMARY KEY,
    customer_id            TEXT    NOT NULL,
    sampled_at             BIGINT  NOT NULL,
    avail_cas_put_met      INTEGER NOT NULL CHECK (avail_cas_put_met IN (0, 1)),
    avail_cas_get_met      INTEGER NOT NULL CHECK (avail_cas_get_met IN (0, 1)),
    lat_cas_get_p99_met    INTEGER NOT NULL CHECK (lat_cas_get_p99_met IN (0, 1)),
    fresh_billing_met      INTEGER NOT NULL CHECK (fresh_billing_met IN (0, 1)),
    byok_key_health_ok     INTEGER NOT NULL CHECK (byok_key_health_ok IN (0, 1)),
    FOREIGN KEY (customer_id) REFERENCES lighthouse_customers (customer_id)
);

-- Lookup: "all samples for this customer, newest first" (attestation
-- evidence pack rendering + DASH panel queries).
CREATE INDEX IF NOT EXISTS idx_lighthouse_sla_samples_customer_sampled
    ON lighthouse_sla_samples (customer_id, sampled_at DESC);

-- Lookup: breach reconciliation (alert > 0 sustained 7d per spec §12).
CREATE INDEX IF NOT EXISTS idx_lighthouse_sla_samples_sampled
    ON lighthouse_sla_samples (sampled_at);
