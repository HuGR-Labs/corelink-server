-- CoreLink D1 (Cloudflare SQLite) — migration 0015 for the analytics
-- cardinality-budget canonical mirror + per-metric label-set ledger
-- (S-09 observability stack; WI-S09-001 RedMetricsObserver +
-- CardinalityValidator).
--
-- Canonical sources:
--   - specs/04_sprints/S09/work_items/WI-S09-001-worker-analytics-engine-red-metrics-cardinality-validator.md §6
--   - specs/04_sprints/S09/_spec_contract.md §4 CAP-OBS-001 + §5 R-S09-1 + §5 R-S09-2 + §8 INV-OBS-CARDINALITY-BUDGET
--   - specs/03_architecture/invariant_registry.md §3.12 INV-OBS-CARDINALITY-BUDGET
--   - specs/03_architecture/observability_model.md §11.2 cardinality budget
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - Per WI §6.1.2 the canonical 9 RED metrics + 6 USE metrics emit
--     against a per-metric cardinality budget (INV-OBS-CARDINALITY-BUDGET
--     HIGH; per-metric ≤ 20k unique label-tuples + global ≤ 100k).
--   - `analytics_cardinality_budgets` is the durable per-metric budget
--     mirror of the in-memory `AnalyticsConfig::cardinality_budgets`
--     map; cold-start recovery of the validator reads this table and
--     hydrates the budget ledger BEFORE accepting any emit (fail-closed
--     envelope per Lote 10.6bis pattern adapted for budget ingest).
--   - `analytics_cardinality_observed` is the durable per-metric
--     observed-tuple count snapshot; the production CF Workers
--     Analytics Engine binding flushes the in-memory unique-tuple
--     HashSet count here on every cron tick + every transition to
--     `cardinality_rejected` audit. The validator uses this to detect
--     drift between the in-memory ledger and the durable mirror at
--     cold start.
--
-- Invariants enforced at storage layer:
--   - INV-OBS-CARDINALITY-BUDGET (HIGH; invariant_registry §3.12):
--     per-metric ≤ 20k unique label-tuples; global ≤ 100k. Validator
--     rejects emits that would push a metric over budget; durable
--     `observed_unique_tuples` mirror keeps the SEV-2 alert source
--     durable across DO restarts.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+): every cardinality
--     decision (`metric_emitted`, `cardinality_rejected`,
--     `budget_exceeded`) emits a `corelink.analytics.{...}` audit_outbox
--     row in the same D1 batch; fail-closed envelope enforced at the
--     InMemoryRedMetrics layer (audit emit BEFORE state mutation;
--     audit failure aborts the emit; production wiring rolls back the
--     D1 batch on emit failure per Lote 10.6bis pattern + S-07
--     sprint-close P1-1 fix).
--   - INV-TENANT-ISOLATION (CRITICAL, TLA+): the cardinality ledger
--     itself is metric-scoped (NOT tenant-scoped) by design — tenant_id
--     is FORBIDDEN as a label per WI §1 invariant 3 (cardinality
--     explosion + LGPD privacy). Aggregated per `tenant_tier` only.
--
-- Conventions (mirror migrations/d1/0001..0014):
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--   - metric_name stored as canonical TEXT matching the
--     `RedMetricKind::as_str()` slug (snake_case underscore-prefixed
--     `corelink_*_total` per OpenMetrics 1.0).
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT
--     support `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at
--     CREATE TABLE per ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses
--     an implicit transaction).
--   - No `_ms` column-name suffix per Lote 10.7bis P0-3 column-drift
--     lesson; instead `*_at` Unix epoch ms.
--
-- Backfill plan: NONE. The tables start empty; rows are inserted by
-- the analytics validator at cold-start hydration + by the
-- `corelink-analytics` CardinalityValidator on every emit.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- analytics_cardinality_budgets — durable per-metric cardinality budget
-- mirror. Single-row-per-metric ledger; PK is the canonical metric name.
-- Cold-start hydration reads every row + builds the in-memory budget
-- map (canonical default 20k per-metric per WI §1 invariant 1; rows
-- with explicit budget override the canonical default).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS analytics_cardinality_budgets (
  -- Canonical metric name (e.g. `corelink_cas_put_requests_total`).
  -- TEXT PK; matches `RedMetricKind::as_str()` slug.
  metric_name                 TEXT     NOT NULL PRIMARY KEY,

  -- Per-metric cardinality budget (max unique label-tuples allowed
  -- before the validator rejects the next emit). Canonical default
  -- 20_000 per WI §1 invariant 1; explicit override permitted via
  -- this row.
  budget_unique_tuples        INTEGER  NOT NULL,

  -- Wall-clock instant of the latest budget update (Unix ms; canonical
  -- no `_ms` suffix per Lote 10.7bis P0-3). Used by the cold-start
  -- recovery path to detect stale snapshots and SEV-3 alert on budget
  -- drift > 24h.
  updated_at                  INTEGER  NOT NULL,

  -- chk_analytics_budget_positive: budget MUST be at least 1 (zero or
  -- negative budget would make every emit a violation; that's a
  -- mis-configuration not an enforcement signal).
  CHECK (budget_unique_tuples >= 1),

  -- chk_analytics_budget_global_bound: per-metric budget MUST NOT
  -- exceed the global 100_000 ceiling per INV-OBS-CARDINALITY-BUDGET
  -- (defense-in-depth: a single metric can't be configured to consume
  -- the entire global budget).
  CHECK (budget_unique_tuples <= 100000),

  -- chk_analytics_budget_metric_name_non_empty.
  CHECK (length(metric_name) >= 1),

  -- chk_analytics_budget_updated_at_non_negative.
  CHECK (updated_at >= 0)
);

-- ---------------------------------------------------------------------------
-- analytics_cardinality_observed — durable per-metric observed-tuple
-- count snapshot. Single-row-per-metric ledger; PK is the canonical
-- metric name. Production wiring flushes the in-memory unique-tuple
-- HashSet count here on every cron tick + every transition to
-- `cardinality_rejected` audit. Validator uses this to detect drift
-- between the in-memory ledger and the durable mirror at cold start.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS analytics_cardinality_observed (
  -- Canonical metric name (e.g. `corelink_cas_put_requests_total`).
  -- TEXT PK; matches `RedMetricKind::as_str()` slug.
  metric_name                 TEXT     NOT NULL PRIMARY KEY,

  -- Snapshot of the in-memory unique-tuple HashSet count at the most
  -- recent flush instant. Used to detect drift at cold start (if the
  -- in-memory count differs from this row by more than 5%, SEV-3
  -- alert fires).
  observed_unique_tuples      INTEGER  NOT NULL,

  -- Wall-clock instant of the latest snapshot flush (Unix ms; canonical
  -- no `_ms` suffix). Used by the cold-start recovery path to detect
  -- stale snapshots and SEV-3 alert on snapshot drift > 60s.
  snapshotted_at              INTEGER  NOT NULL,

  -- chk_analytics_observed_non_negative.
  CHECK (observed_unique_tuples >= 0),

  -- chk_analytics_observed_metric_name_non_empty.
  CHECK (length(metric_name) >= 1),

  -- chk_analytics_observed_snapshotted_at_non_negative.
  CHECK (snapshotted_at >= 0)
);

-- Index: per-metric observed-tuple recent-update scan — admin forensic
-- surface + DASH-COST widget (WI-S09-005); ordered DESC for the
-- canonical "latest-N-flushes" query.
CREATE INDEX IF NOT EXISTS idx_analytics_observed_recent
  ON analytics_cardinality_observed(snapshotted_at DESC);

-- Index: budget-near-violation scan — every dashboard scan filters on
-- `observed_unique_tuples >= 0.8 * budget` (the 80% proactive warning
-- threshold per WI §6.1.11 cardinality_approaching SEV-3 alert).
-- Production wiring composes the JOIN with `analytics_cardinality_budgets`
-- to compute the threshold; this index keys by metric_name to make the
-- JOIN cheap.
CREATE INDEX IF NOT EXISTS idx_analytics_observed_metric
  ON analytics_cardinality_observed(metric_name);
