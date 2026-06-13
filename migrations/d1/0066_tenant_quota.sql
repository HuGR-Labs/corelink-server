-- Migration 0066: per-tenant monthly $-ceiling backing (`tenant_quota`).
--
-- Backs WP-FOUND-2 / G1 of the hugit-P2 wave plan, per RATIFIED
-- ADR-0068 (`specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-
-- dollar-ceiling.md`): a per-tenant monthly cumulative-DOLLAR cap,
-- enforced FAIL-CLOSED at the request boundary by the quota middleware
-- in `crates/corelink-container/src/tenant_quota.rs`.
--
-- ## Why a dedicated table (not `tenant_billing` / `tier_selections`)
--
-- The existing per-tenant **rate** limit (`ratelimit_buckets` + the
-- tier→RPS map) bounds VELOCITY (req/s), not cumulative DOLLARS. A
-- slow-but-steady tenant stays under the rate limit yet can accrue
-- unbounded monthly cost. ADR-0068 closes that gap with a separate
-- cumulative-$ axis. `tenant_billing` (0055) models the Stripe
-- subscription/customer linkage (money the tenant OWES us via Stripe);
-- `tier_selections` (0039/0062) models the chosen product tier. Neither
-- carries a per-cycle ACCRUED-spend counter or an owner-tunable
-- per-tenant ceiling, and overloading either would conflate the abuse
-- tripwire with the billing money-path. This table is the dedicated
-- cost-cap ledger.
--
-- ## Units — integer micro-dollars (no floats)
--
-- All dollar amounts are stored as signed `INTEGER` **micro-dollars**
-- (USD * 1_000_000). D1/SQLite has no exact decimal type and floating
-- point is unsafe for money; micro-dollars give exact arithmetic with
-- ample headroom (i64 spans ~9.2e12 USD). The $5 launch tripwire is
-- therefore `5_000_000`.
--
-- ## Launch default — a symbolic $5/mo tripwire
--
-- ADR-0068 §Decision sets a deliberately conservative $5/mo default
-- (5_000_000 micro-USD). It is a TRIPWIRE that bounds day-1 blast radius
-- while real usage calibrates the number — NOT a product tier. The
-- ceiling is owner-tunable per tenant by UPDATEing `monthly_budget_usd_micros`.
--
-- ## Idempotency / additive-only
--
-- `CREATE TABLE IF NOT EXISTS` lets the migration replay safely. The
-- migration is additive-only (INV-AUTH-MIGRATION-ADDITIVE): it adds a
-- NEW table + index and never alters/drops/renames an existing object,
-- so it needs no ADR waiver and no `-- additive-allowed:` suppression.

CREATE TABLE IF NOT EXISTS tenant_quota (
    -- Canonical tenant id (matches `tenant.tenant_id`; UUID/v7 string).
    -- Primary key — exactly one quota row per tenant.
    tenant_id TEXT PRIMARY KEY,

    -- Owner-tunable monthly ceiling, in micro-dollars (USD * 1e6).
    -- Default = the symbolic $5 tripwire (ADR-0068). The middleware
    -- rejects a billable op when `accrued_usd_micros + cost(op)` would
    -- exceed this value.
    monthly_budget_usd_micros INTEGER NOT NULL DEFAULT 5000000
        CHECK (monthly_budget_usd_micros >= 0),

    -- Cumulative cost ACCRUED in the current cycle, in micro-dollars.
    -- Reset to 0 when the cycle rolls over (`cycle_anchor_ms` advances).
    accrued_usd_micros INTEGER NOT NULL DEFAULT 0
        CHECK (accrued_usd_micros >= 0),

    -- Cycle-start wall-clock (Unix epoch ms). The middleware rolls the
    -- cycle (zeroes `accrued_usd_micros`, advances this anchor) when the
    -- wall clock has crossed a ~30-day boundary from this anchor. Stored
    -- as ms for consistency with the wider D1 `*_at_ms` convention.
    cycle_anchor_ms INTEGER NOT NULL DEFAULT 0,

    -- Last-mutation wall-clock (Unix epoch ms), for operator forensics.
    updated_at_ms INTEGER NOT NULL DEFAULT 0
);

-- Operator/forensics slice — list tenants whose accrued spend is near
-- or over their ceiling without a full-table scan over the cost column.
CREATE INDEX IF NOT EXISTS idx_tenant_quota_accrued
    ON tenant_quota (accrued_usd_micros);
