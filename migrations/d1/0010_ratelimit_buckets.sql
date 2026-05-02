-- CoreLink D1 (Cloudflare SQLite) — migration 0010 for `ratelimit_buckets`
-- (S-08 Rate Limiting multi-camada: per-tenant DO RateLimiter token-bucket
-- durable mirror; WI-S08-001 token-bucket lazy-refill state machine).
--
-- Canonical sources:
--   - specs/04_sprints/S08/work_items/WI-S08-001-do-ratelimiter-token-bucket.md §6.1.2
--   - specs/04_sprints/S08/_spec_contract.md §5 R-S08-1 + §8 INV-RATE-LIMIT-PROPORTIONALITY
--   - specs/03_architecture/invariant_registry.md INV-RATE-LIMIT-PROPORTIONALITY (§3.12) + INV-AVAIL-ISOLATION (§3.8)
--   - specs/03_architecture/adrs/ADR-0020-quota-enforcement-ownership.md (FROZEN)
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - The DO singleton `rate-limiter-<tenant_id>` (production wiring;
--     WI-S08-006 PRR ship gate) holds the token-bucket state IN-MEMORY for
--     hot-path latency (≤ 3ms p99 per spec_contract §10.s08.2). The SQL
--     table here is the durable mirror — DO snapshots every ~5 min via
--     alarm to support cold-start recovery (Worker restart preserves
--     `available_tokens` + `last_refill_at_ms` so the next request does
--     not see a phantom over-refill).
--   - Token bucket math (lazy refill canonical; WI-S08-001 §6.1.3):
--         now_tokens     = min(burst_capacity,
--                              available_tokens
--                              + (now_ms - last_refill_at_ms)
--                                * refill_rate_per_sec / 1000.0)
--         if now_tokens >= cost { available_tokens = now_tokens - cost; Allow }
--         else { retry_after_secs = ceil((cost - now_tokens) / refill_rate_per_sec); Deny429 }
--   - Available tokens stored as REAL (f64) so small refill rates don't
--     accumulate integer rounding error over long quiet windows
--     (WI-S08-001 §1 cripto-driven invariant 5: f64 precision ≥ 9 decimal
--     digits sufficient for 10000 RPS × 1ms granularity).
--   - 5-tier canonical refill_rate ladder (Lote 10.7bis P0-7) populated
--     from the `tenant_plan` cache (ships in WI-S08-001 follow-on; for
--     WI-S08-001 the bucket row carries refill_rate + burst_capacity
--     denormalised so the DO can answer cold-start without a join).
--
-- Invariants enforced at storage layer:
--   - INV-TENANT-ISOLATION (CRITICAL): bucket_key is composed
--     `<dimension>:<scope>:<tenant_id>`; tenant_id is preserved as a
--     dedicated column for tenant-leftmost retrieval + index. Cross-tenant
--     bucket injection FM-300 impossible by design (TenantCtx-only
--     enforcement Lote 10.4bis lesson).
--   - INV-RATE-LIMIT-PROPORTIONALITY (HIGH): refill_rate × window
--     proportional to plan tier; mudança plan reflete ≤ 5min via DO
--     config sync. The bucket row carries the active plan_refill_rate
--     so the DO can short-circuit answer a cold-start request.
--   - INV-AVAIL-ISOLATION (HIGH): one bucket row per (key_dimension,
--     scope_key, tenant_id); the per-tenant DO actor model serialises
--     concurrent `try_acquire` calls (no cross-tenant state contamination).
--   - Token monotonicity: `available_tokens` clamped non-negative AND
--     `<= burst_capacity` (CHECK envelope at SQL layer + Rust domain).
--
-- Conventions (mirror migrations/d1/0001..0009):
--   - tenant_id stored as canonical UUIDv7 TEXT form (data_model.md §2.1).
--   - key_dimension stored as canonical 3-literal (per_tenant /
--     per_ip / per_tenant_per_endpoint) with CHECK constraint.
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - available_tokens stored as REAL (f64); see schema rationale above.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--
-- Backfill plan: NONE. The table starts empty; bucket rows are inserted
-- by the DO singleton on first `try_acquire` post-deploy.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- ratelimit_buckets — per-(tenant, dimension, scope) token-bucket rows.
--
-- ONE row per (tenant_id, key_dimension, scope_key). The tenant_id is the
-- leftmost PK component for tenant-leftmost retrieval + cross-tenant
-- isolation. The DO `rate-limiter-<tenant_id>` owns the in-memory
-- representation; this table is the durable mirror.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ratelimit_buckets (
  -- Tenant scope (PK component 1; tenant-leftmost per data_model.md §2.1).
  tenant_id              TEXT     NOT NULL,

  -- Bucket key dimension (PK component 2; canonical 3-literal):
  --   - `per_tenant`               — global aggregate per tenant (camada 1).
  --   - `per_ip`                   — per-IP scoped (camada 2).
  --   - `per_tenant_per_endpoint`  — per (tenant, endpoint) for hot endpoint
  --                                  isolation (camada 1.5; advanced tier).
  key_dimension          TEXT     NOT NULL,

  -- Bucket scope key (PK component 3; opaque per-dimension string).
  --   - `per_tenant`               — empty string "" (one row per tenant).
  --   - `per_ip`                   — IPv4 / IPv6 textual literal.
  --   - `per_tenant_per_endpoint`  — endpoint route id literal.
  scope_key              TEXT     NOT NULL,

  -- Currently available tokens (REAL / f64; clamp [0, burst_capacity]).
  -- Stored as REAL to avoid integer-rounding write amplification on small
  -- refill rates over long quiet windows.
  available_tokens       REAL     NOT NULL,

  -- Bucket capacity (token ceiling; >= 1).
  burst_capacity         INTEGER  NOT NULL,

  -- Plan refill rate (tokens / second; >= 0). 0 means a canceled tenant
  -- (DenyForever path; Retry-After saturated to canonical 7-day floor per
  -- WI §6.1.3 Lote 10.8bis P1-1 division-by-zero guard).
  refill_rate_per_sec    REAL     NOT NULL,

  -- Last refill watermark (Unix ms; updated on every try_acquire call).
  last_refill_at_ms      INTEGER  NOT NULL,

  -- Creation watermark (Unix ms; immutable).
  created_at_ms          INTEGER  NOT NULL,

  -- Last-update watermark (Unix ms; updated on every state mutation).
  updated_at_ms          INTEGER  NOT NULL,

  -- Composite PK — tenant-leftmost; (tenant, dimension, scope) per
  -- WI-S08-001 §6.1.2 state machine.
  PRIMARY KEY (tenant_id, key_dimension, scope_key),

  -- chk_ratelimit_key_dimension: canonical 3-literal list.
  CHECK (key_dimension IN ('per_tenant', 'per_ip', 'per_tenant_per_endpoint')),
  -- chk_ratelimit_available_tokens_non_negative.
  CHECK (available_tokens >= 0.0),
  -- chk_ratelimit_burst_capacity_positive.
  CHECK (burst_capacity >= 1),
  -- chk_ratelimit_available_within_capacity.
  CHECK (available_tokens <= burst_capacity),
  -- chk_ratelimit_refill_rate_non_negative.
  CHECK (refill_rate_per_sec >= 0.0),
  -- chk_ratelimit_lifecycle_create_non_negative.
  CHECK (created_at_ms >= 0),
  -- chk_ratelimit_updated_at_after_create.
  CHECK (updated_at_ms >= created_at_ms),
  -- chk_ratelimit_last_refill_after_create.
  CHECK (last_refill_at_ms >= created_at_ms)
);

-- Index: per-(tenant, dimension) scan for the DO cold-start reload + the
-- DASH-RATE per-tenant aggregation widget (WI-S08-006 forward).
CREATE INDEX IF NOT EXISTS idx_ratelimit_buckets_tenant_dimension
  ON ratelimit_buckets(tenant_id, key_dimension);

-- Index: updated_at scan — the DO snapshot alarm prunes stale
-- `per_ip` bucket rows (idle > 7d) to keep the table bounded.
CREATE INDEX IF NOT EXISTS idx_ratelimit_buckets_updated_at_ms
  ON ratelimit_buckets(updated_at_ms);
