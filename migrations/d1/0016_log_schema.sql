-- CoreLink D1 (Cloudflare SQLite) — migration 0016 for the canonical
-- log schema versioning + redaction pattern config mirror (S-09
-- observability stack; WI-S09-002 Logpush + R2 + Loki + Log Schema +
-- PII Redaction).
--
-- Canonical sources:
--   - specs/04_sprints/S09/work_items/WI-S09-002-logpush-r2-loki-log-schema-pii-redaction.md §6
--   - specs/04_sprints/S09/_spec_contract.md §5.2 (R-S09-4 / R-S09-5 / R-S09-6)
--   - specs/03_architecture/privacy_model.md §6 retention + §11.4 volume budget + CTRL-PRIV-001
--   - specs/03_architecture/observability_model.md §5 logs canonical schema
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - Per WI §6.1.1 the canonical structured log schema (CloudEvents
--     1.0 aligned: id / source / specversion / type / time / data +
--     tenant_id pseudonymous) MUST have a durable schema-version
--     mirror so cold-start log emit verifies the in-memory schema
--     version matches the table-side version BEFORE accepting any emit
--     (fail-closed envelope per Lote 10.6bis pattern adapted for log
--     emit validation).
--   - `log_schema_versions` is the per-region single-row mirror of the
--     canonical schema version pinned by `corelink-logpush::SCHEMA_VERSION`.
--   - `log_redaction_patterns` is the durable per-region redaction
--     pattern config mirror (5 canonical patterns per WI §6.1.1: email
--     / ip / token / pan / cpf_cnpj). The validator reads this at cold
--     start to detect drift between the in-memory pattern set and the
--     durable mirror; SEV-3 alert fires on drift > 5min.
--
-- Invariants enforced at storage layer:
--   - CTRL-PRIV-001 (privacy_model.md canonical; sprint contract §5.2
--     R-S09-4): forbidden raw fields (email / ip / bearer / digest)
--     never reach the durable log archive. The redaction pattern
--     mirror is the source of truth for runtime redactor configuration;
--     drift = SEV-3 + cold-start failure-closed.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+): every log redaction +
--     emit decision emits a `corelink.logpush.{...}` audit_outbox row
--     in the same D1 batch; fail-closed envelope enforced at the
--     InMemoryLogSink layer (audit emit BEFORE state mutation; audit
--     failure aborts the emit; production wiring rolls back the D1
--     batch on emit failure per Lote 10.6bis pattern + S-07 sprint-close
--     P1-1 fix).
--   - INV-TENANT-ISOLATION (CRITICAL, TLA+): tenant_id em log records
--     is stored as the pseudonymous UUID from the auth-context middleware
--     (S-03 WI-S03-003) ONLY; raw tenant identifiers / email / IP never
--     reach this surface (per WI §1 invariant 3 + sprint contract §5.2
--     CTRL-PRIV-001 enforcement).
--
-- Conventions (mirror migrations/d1/0001..0015):
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--   - Region stored as canonical 3-char TEXT matching
--     `corelink_analytics::Region::as_str()` (lowercase IATA-style colocode).
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
-- the logpush emit surface at cold-start hydration + by the
-- `corelink-logpush` LogSink on every emit.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- log_schema_versions — durable per-region canonical schema-version
-- mirror. Single-row-per-region ledger; PK is the canonical region
-- 3-char colocode. Cold-start log emit reads this row + asserts the
-- in-memory `corelink-logpush::SCHEMA_VERSION` const matches the
-- mirror BEFORE accepting any emit (fail-closed envelope adapted for
-- schema-version drift detection).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS log_schema_versions (
  -- Canonical 3-char CF colocode (e.g. `iad`, `gru`, `fra`). TEXT PK;
  -- matches `corelink_analytics::Region::as_str()` slug.
  region                      TEXT     NOT NULL PRIMARY KEY,

  -- Canonical log-schema version (currently 16; mirrors the migration
  -- filename prefix). Cold-start emit asserts the in-memory
  -- `SCHEMA_VERSION` const matches this value. Drift detection: SEV-3
  -- alert fires if the value differs by > 0.
  schema_version              INTEGER  NOT NULL,

  -- Wall-clock instant of the latest schema activation per region
  -- (Unix ms; canonical no `_ms` suffix per Lote 10.7bis P0-3).
  activated_at                INTEGER  NOT NULL,

  -- chk_log_schema_versions_region_3char: region MUST be exactly 3
  -- chars (matches the canonical CF colocode shape).
  CHECK (length(region) = 3),

  -- chk_log_schema_versions_version_positive.
  CHECK (schema_version >= 1),

  -- chk_log_schema_versions_activated_at_non_negative.
  CHECK (activated_at >= 0)
);

-- ---------------------------------------------------------------------------
-- log_redaction_patterns — durable per-region redaction pattern config
-- mirror (5 canonical patterns: email / ip / token / pan / cpf_cnpj).
-- Each pattern has a canonical pattern_id slug + a placeholder string
-- the redactor substitutes when the pattern matches. Cold-start log
-- emit reads this table + asserts the in-memory `InMemoryPiiRedactor`
-- pattern set matches the mirror; drift fires SEV-3 (CTRL-PRIV-001
-- bypass risk).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS log_redaction_patterns (
  -- Canonical region 3-char colocode (composite PK with pattern_id).
  region                      TEXT     NOT NULL,

  -- Canonical redaction pattern id. Bounded canonical list:
  -- `email`, `ip`, `token`, `pan`, `cpf_cnpj`. Mirrors
  -- `corelink_logpush::redaction::PiiPatternKind::as_str()`.
  pattern_id                  TEXT     NOT NULL,

  -- Canonical placeholder substituted when the pattern matches. Per
  -- WI §6.1.1: `<EMAIL_REDACTED>`, `<IP_REDACTED>`, `<TOKEN_REDACTED>`,
  -- `<PAN_REDACTED>`, `<CPF_REDACTED>` / `<CNPJ_REDACTED>` (LGPD scope).
  placeholder                 TEXT     NOT NULL,

  -- Wall-clock instant of the latest pattern activation per region
  -- (Unix ms; canonical no `_ms` suffix).
  activated_at                INTEGER  NOT NULL,

  PRIMARY KEY (region, pattern_id),

  -- chk_log_redaction_patterns_region_3char.
  CHECK (length(region) = 3),

  -- chk_log_redaction_patterns_pattern_id_canonical: pattern_id MUST
  -- be one of the 5 canonical slugs (defense-in-depth; runtime
  -- enforcement via `PiiPatternKind` enum).
  CHECK (pattern_id IN ('email', 'ip', 'token', 'pan', 'cpf_cnpj')),

  -- chk_log_redaction_patterns_placeholder_non_empty.
  CHECK (length(placeholder) >= 1),

  -- chk_log_redaction_patterns_activated_at_non_negative.
  CHECK (activated_at >= 0)
);

-- Index: per-region recent-activation scan — admin forensic surface +
-- cold-start drift detection; ordered DESC for the canonical
-- "latest-N-activations" query.
CREATE INDEX IF NOT EXISTS idx_log_redaction_patterns_recent
  ON log_redaction_patterns(activated_at DESC);

-- Index: per-region pattern_id lookup — cold-start hydration scans
-- this index to load the 5 canonical patterns per region in a single
-- scan.
CREATE INDEX IF NOT EXISTS idx_log_redaction_patterns_region
  ON log_redaction_patterns(region);
