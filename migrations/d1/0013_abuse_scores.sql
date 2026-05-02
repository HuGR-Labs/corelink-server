-- CoreLink D1 (Cloudflare SQLite) — migration 0013 for abuse scoring
-- (S-08 Rate Limiting multi-camada: heuristic abuse detection +
-- automated response gradient; WI-S08-004 AbuseScorer +
-- InMemoryAbuseScorer).
--
-- Canonical sources:
--   - specs/04_sprints/S08/work_items/WI-S08-004-abuse-detection-heuristica-scoring.md §6.1
--   - specs/04_sprints/S08/_spec_contract.md §4 CAP-ABUSE-001 + CAP-ABUSE-002 + §5 R-S08-7
--   - specs/03_architecture/invariant_registry.md INV-TENANT-ISOLATION + INV-AUDIT-APPEND-ONLY
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - Per WI §6.1.7, four NEW tables back the abuse-detection plane.
--     This migration ships the canonical 1-of-4 (durable score history)
--     to keep the migration scope tight; companion tables
--     (abuse_appeals, abuse_response_actions, abuse_calibration_weights,
--     tenant_rate_override) ride along the production-wiring WI-S08-006
--     PRR ship gate when the cron DO + admin S-13 endpoints land.
--   - abuse_score_history is the durable per-tenant score time series
--     emitted by the AbuseScorer cron-tick (5min canonical aggregation
--     window per WI §1 invariant 4 + sprint contract §10.s08.5). Each
--     row captures the 4 raw features + the computed score + the
--     gradient response tier rendered.
--   - 4 features pinned per WI §6.1.3 + sprint contract §5 R-S08-7:
--       (a) cpu_wallclock_ratio    REAL  [0.0, ~clamped 1.0]
--       (b) egress_bytes_per_min   INTEGER (bytes/min observed via S-09 metrics)
--       (c) action_digest_entropy_bits REAL (Shannon entropy ≥ 0.0)
--       (d) concurrent_exec_count  INTEGER (parallel exec sessions)
--   - response_tier captures the 4-tier gradient per WI §6.1.4:
--       Benign / Suspicious / Malicious — collapsed canonical ladder
--       (the full 4-tier names SilentDowngrade50pct1h /
--       AdminReviewTriggerSev2 / SuspendCandidateHumanReviewOnly are
--       the production wiring labels — this WI ships the trait + fake
--       per `trait-abstraction-defer`; storage tier names mirror the
--       crate's AbuseDecision `#[non_exhaustive]` 3-arm enum which the
--       production wiring expands additively).
--
-- Invariants enforced at storage layer:
--   - INV-TENANT-ISOLATION (CRITICAL, TLA+): per-tenant scope; PK
--     tenant-leftmost; cross-tenant comparison architecturally
--     impossible (sprint contract §7.10.s08.4 100k property test).
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+): every score computation
--     emits an audit_outbox row in the same D1 batch as this INSERT;
--     fail-closed envelope enforced at the InMemoryAbuseScorer layer
--     (audit emit BEFORE state mutation; audit failure aborts the
--     decision; production wiring rolls back the D1 batch on emit
--     failure per Lote 10.6bis pattern + S-07 sprint-close P1-1 fix).
--   - LGPD Art. 20 + GDPR Art. 22 humane response (sprint contract
--     §7.10.s08.3): NEVER auto-suspend; AutoSuspendForbidden enforced
--     at the trait layer; storage row records the score + tier but
--     suspend execution always requires admin manual review.
--
-- Conventions (mirror migrations/d1/0001..0012):
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--   - tenant_id stored as canonical UUIDv7 TEXT form.
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--
-- Backfill plan: NONE. The table starts empty; rows are inserted by the
-- AbuseScorer orchestrator on every 5min cron-tick.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- abuse_score_history — durable per-tenant score time series + 4-feature
-- breakdown + rendered decision tier (canonical heuristic abuse-detection
-- audit ring; mirrors `corelink-abuse::AbuseFeatures` + `AbuseScore` +
-- `AbuseDecision` byte-for-byte).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS abuse_score_history (
  -- Tenant scope (canonical UUIDv7 TEXT form). Tenant-leftmost composite
  -- PK per CTRL-ISO-005 + INV-TENANT-ISOLATION.
  tenant_id                   TEXT     NOT NULL,

  -- Producer-side wall-clock instant of the 5min cron-tick (Unix ms).
  -- Composite PK with tenant_id; canonical no `_ms` suffix per Lote
  -- 10.7bis P0-3 column-drift lesson.
  computed_at                 INTEGER  NOT NULL,

  -- 5min aggregation window bounds (Unix ms; canonical no `_ms` suffix).
  window_start                INTEGER  NOT NULL,
  window_end                  INTEGER  NOT NULL,

  -- Computed abuse score [0.0, 1.0] (weighted sum of 4 normalised
  -- features; sprint contract §5 R-S08-7 + WI §6.1.3). Newtype
  -- `AbuseScore` clamps in code; this CHECK pins the storage envelope.
  score                       REAL     NOT NULL,

  -- Feature 1: cpu_wallclock_ratio [0.0, ~clamped 1.0]; abusive ≥ 0.95
  -- sustained = compute farming.
  cpu_wallclock_ratio         REAL     NOT NULL,

  -- Feature 2: egress_bytes_per_min (bytes/min observed via S-09);
  -- abusive ≥ 95th percentile WoW + 5σ.
  egress_bytes_per_min        INTEGER  NOT NULL,

  -- Feature 3: action_digest_entropy_bits (Shannon entropy ≥ 0);
  -- abusive ≤ 1 bit = single-action spam.
  action_digest_entropy_bits  REAL     NOT NULL,

  -- Feature 4: concurrent_exec_count (parallel exec sessions);
  -- abusive ≥ 100× plan tier baseline.
  concurrent_exec_count       INTEGER  NOT NULL,

  -- Rendered decision tier (canonical 3-arm `AbuseDecision`
  -- `#[non_exhaustive]` enum: Benign / Suspicious / Malicious).
  -- Production wiring at WI-S08-006 expands additively (e.g. the full
  -- 4-tier SilentDowngrade50pct1h / AdminReviewTriggerSev2 /
  -- SuspendCandidateHumanReviewOnly canonical ladder).
  decision                    TEXT     NOT NULL,

  -- Source attribution: the request id (`x-request-id` header) of the
  -- cron-tick batch that emitted this row.
  created_by_request_id       TEXT     NOT NULL,

  PRIMARY KEY (tenant_id, computed_at),

  -- chk_abuse_score_envelope: canonical bounds [0.0, 1.0] per WI §1
  -- invariant 5 + R-S08-7 weighted-sum domain.
  CHECK (score >= 0.0 AND score <= 1.0),

  -- chk_abuse_cpu_wallclock_ratio_non_negative.
  CHECK (cpu_wallclock_ratio >= 0.0),

  -- chk_abuse_egress_bytes_per_min_non_negative.
  CHECK (egress_bytes_per_min >= 0),

  -- chk_abuse_action_digest_entropy_non_negative (Shannon entropy ≥ 0).
  CHECK (action_digest_entropy_bits >= 0.0),

  -- chk_abuse_concurrent_exec_count_non_negative.
  CHECK (concurrent_exec_count >= 0),

  -- chk_abuse_window_bounds (window_end > window_start; both Unix ms).
  CHECK (window_end > window_start),

  -- chk_abuse_decision_canonical: closed canonical 3-literal list per
  -- `AbuseDecision` `#[non_exhaustive]` enum (production wiring at
  -- WI-S08-006 may add literals additively — the CHECK is updated in
  -- a follow-on additive migration).
  CHECK (decision IN ('Benign', 'Suspicious', 'Malicious')),

  -- chk_abuse_computed_at_non_negative.
  CHECK (computed_at >= 0),

  -- chk_abuse_window_start_non_negative.
  CHECK (window_start >= 0),

  -- chk_abuse_tenant_id_non_empty.
  CHECK (length(tenant_id) >= 1),

  -- chk_abuse_request_id_non_empty.
  CHECK (length(created_by_request_id) >= 1)
);

-- Index: per-tenant recent-window scan — admin forensic surface +
-- customer self-service /v1/admin/abuse_score endpoint (WI §6.1.5).
CREATE INDEX IF NOT EXISTS idx_abuse_score_history_tenant_recent
  ON abuse_score_history(tenant_id, computed_at DESC);

-- Index: malicious decision surface — every dashboard scan filters on
-- decision = 'Malicious' for the SEV-1 alarm (CAP-ABUSE-002 absorbed).
CREATE INDEX IF NOT EXISTS idx_abuse_score_history_malicious
  ON abuse_score_history(tenant_id, computed_at)
  WHERE decision = 'Malicious';

-- Index: suspicious decision surface — silent-downgrade observability
-- (DASH-RATE widget at WI-S08-006).
CREATE INDEX IF NOT EXISTS idx_abuse_score_history_suspicious
  ON abuse_score_history(tenant_id, computed_at)
  WHERE decision = 'Suspicious';
