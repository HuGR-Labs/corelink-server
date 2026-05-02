-- CoreLink D1 (Cloudflare SQLite) — migration 0012 for `quota_cas_attempts`
-- (S-08 Rate Limiting multi-camada: canonical 100% hard-block CAS audit
-- ring; WI-S08-003 AtomicQuotaChecker + InMemoryAtomicQuotaChecker).
--
-- Canonical sources:
--   - specs/04_sprints/S08/work_items/WI-S08-003-quota-checker-middleware-atomic-cas.md §6.1
--   - specs/04_sprints/S08/_spec_contract.md §4 CAP-QUOTA-001 + §8 INV-QUOTA-ENFORCEMENT
--   - specs/03_architecture/invariant_registry.md INV-QUOTA-ENFORCEMENT (§3.11) + INV-AUDIT-APPEND-ONLY
--   - specs/03_architecture/adrs/ADR-0020-eviction-quota-boundary.md (FROZEN)
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - Per WI §6.1.9, every CAS attempt — successful / denied / race-detected
--     — emits a row to this table. The table is the durable source-of-truth
--     for CAS race-detection observability + admin-plane forensics. The
--     in-memory `InMemoryQuotaCasAuditSink` mirrors the table 1:1 for tests;
--     production wiring at WI-S08-006 PRR ship gate flushes via the canonical
--     audit_outbox batch (S-01 fail-closed envelope per Lote 10.6bis).
--   - Per WI §6.1.9 + S-08 sprint contract §4 CAP-QUOTA-001 hard-block boundary
--     ADR-0020 FROZEN, the canonical 6-event taxonomy lives in the `event_type`
--     CHECK list:
--       - CasCheckPassed              — Allow decision; CAS predicate held.
--       - CasDenied429HardBlock       — canonical 100% hard-block 429 fired.
--       - CasRaceDetected             — stale CAS version; orchestrator retried.
--       - CasCommitSucceeded          — successful acquire's bytes_used delta
--                                       committed atomically.
--       - CasReleaseIdempotent        — idempotent release (no-op).
--       - CasRetryAfterEmitted        — informational record of the canonical
--                                       Retry-After value emitted on 429.
--   - The `cas_version` column captures the version observed at decision
--     time (NOT the post-mutation version — for audit lineage, the read
--     watermark is the load-bearing identity).
--   - The `cas_attempt` column is 1-indexed; `>= 1` CHECK envelope.
--   - The `retry_after_secs` column is OPTIONAL (NULL for non-Deny / non-RetryAfterEmitted
--     arms); CHECK envelope `[0, 31 × 86_400]` (canonical bounds per
--     `crates/corelink-quota-cas::retry_after::MAX_SECS_PER_MONTH`).
--
-- Invariants enforced at storage layer:
--   - INV-QUOTA-ENFORCEMENT (HIGH): canonical CAS hard-block at 100%
--     boundary; race-aware strict-< predicate; chaos test 30d zero
--     violations (sprint contract §6 DoD).
--   - INV-AVAIL-ISOLATION (HIGH): per-tenant scope; cross-tenant
--     architecturally impossible.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+): every CAS attempt mutation
--     emits an audit_outbox row in the same D1 batch as the
--     quota_cas_attempts INSERT. The fail-closed envelope is enforced at
--     the InMemoryAtomicQuotaChecker layer (audit emit BEFORE state mutation;
--     audit failure aborts the acquire; production wiring rolls back the
--     D1 batch on emit failure per Lote 10.6bis pattern + S-07 sprint-close
--     P1-1 fix).
--   - ADR-0020 FROZEN boundary: S-07 owns ≤95% eviction trigger; S-08 owns
--     100% hard-block; this table records the 100%-boundary signal.
--
-- Conventions (mirror migrations/d1/0001..0011):
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
-- AtomicQuotaChecker orchestrator on every CAS attempt.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- quota_cas_attempts — durable audit ring of every CAS attempt
-- (canonical 100% hard-block path; mirrors `corelink-quota-cas::audit`
-- 6-event taxonomy 1:1).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS quota_cas_attempts (
  -- Surrogate row id; canonical UUIDv7 TEXT form. Driven by the audit
  -- emitter; never trusted from request body.
  attempt_id           TEXT     NOT NULL,

  -- Tenant scope (canonical UUIDv7 TEXT form). Tenant-leftmost composite
  -- key per CTRL-ISO-005.
  tenant_id            TEXT     NOT NULL,

  -- Region scope (canonical 5-region literal: sam / iad / lhr / nrt / syd).
  region               TEXT     NOT NULL,

  -- Canonical 6-event taxonomy (closed CHECK list).
  event_type           TEXT     NOT NULL,

  -- CAS version observed at decision time (read watermark; load-bearing
  -- audit identity). NULL for `CasRetryAfterEmitted` informational arm.
  cas_version          INTEGER,

  -- CAS attempt count (1-indexed; >= 1). NULL for the
  -- `CasRetryAfterEmitted` informational arm.
  cas_attempt          INTEGER,

  -- Bytes touched by THIS decision. NULL for race-detection arms +
  -- informational `CasRetryAfterEmitted`.
  bytes                INTEGER,

  -- Canonical Retry-After value (seconds; canonical days-until-month-reset
  -- semantic per ADR-0020 FROZEN). Populated for `CasDenied429HardBlock`
  -- + `CasRetryAfterEmitted` only.
  retry_after_secs     INTEGER,

  -- Source attribution: the request id (`x-request-id` header).
  created_by_request_id TEXT     NOT NULL,

  -- Producer-side wall-clock instant (Unix ms; immutable).
  now_ms               INTEGER  NOT NULL,

  PRIMARY KEY (attempt_id),

  -- chk_quota_cas_event_type_canonical: closed canonical 6-literal list.
  CHECK (event_type IN (
    'CasCheckPassed',
    'CasDenied429HardBlock',
    'CasRaceDetected',
    'CasCommitSucceeded',
    'CasReleaseIdempotent',
    'CasRetryAfterEmitted'
  )),

  -- chk_quota_cas_region_canonical: closed canonical 5-literal list
  -- (matches data_model.md §4 EvictionRegion enum).
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),

  -- chk_quota_cas_attempt_positive (1-indexed; only when present).
  CHECK (cas_attempt IS NULL OR cas_attempt >= 1),

  -- chk_quota_cas_version_non_negative (when present).
  CHECK (cas_version IS NULL OR cas_version >= 0),

  -- chk_quota_cas_bytes_non_negative (when present).
  CHECK (bytes IS NULL OR bytes >= 0),

  -- chk_quota_cas_retry_after_envelope: canonical bounds [0, 31 days].
  CHECK (retry_after_secs IS NULL OR (
    retry_after_secs >= 0 AND retry_after_secs <= 2678400
  )),

  -- chk_quota_cas_now_ms_non_negative.
  CHECK (now_ms >= 0),

  -- chk_quota_cas_tenant_id_non_empty.
  CHECK (length(tenant_id) >= 1),

  -- chk_quota_cas_attempt_id_non_empty.
  CHECK (length(attempt_id) >= 1),

  -- chk_quota_cas_request_id_non_empty.
  CHECK (length(created_by_request_id) >= 1)
);

-- Index: per-tenant audit ring scan — admin forensic surface.
CREATE INDEX IF NOT EXISTS idx_quota_cas_attempts_tenant_now_ms
  ON quota_cas_attempts(tenant_id, now_ms);

-- Index: race-detection observability — every alarm scan filters on
-- event_type = 'CasRaceDetected' for the SEV-3 sustained-rate alert.
CREATE INDEX IF NOT EXISTS idx_quota_cas_attempts_race_detected
  ON quota_cas_attempts(event_type, now_ms)
  WHERE event_type = 'CasRaceDetected';

-- Index: 429 hard-block surface — every dashboard scan filters on
-- event_type = 'CasDenied429HardBlock' for the SEV-1 alarm.
CREATE INDEX IF NOT EXISTS idx_quota_cas_attempts_denied_429
  ON quota_cas_attempts(tenant_id, now_ms)
  WHERE event_type = 'CasDenied429HardBlock';
