-- CoreLink D1 (Cloudflare SQLite) — migration 0002 for `ac_meta` (Action Cache).
--
-- Canonical sources:
--   - specs/04_sprints/_sealed/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md §1
--   - specs/03_architecture/data_model.md §4.2
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--   - specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md (H-3 tenant_prefix materialization)
--
-- Invariants enforced at storage layer:
--   - INV-AC-TENANT-SCOPED (CRITICAL): PK composite (tenant_id, action_digest) — tenant_id
--     leftmost so D1/SQLite uses the tenant prefix for index seek; impossible cross-tenant
--     via PK alone (Layer 4 enforced at storage).
--   - INV-AC-OUTPUTS-VALID (HIGH): blob_refs JSON references digests in blob_meta;
--     FK is *logical* (D1 PRAGMA foreign_keys is per-connection inconsistent in CF Workers
--     pool semantics; reconcile diário in S-06 enforces).
--   - INV-AC-IDEMPOTENT (HIGH): PRIMARY KEY (UNIQUE constraint) supports
--     `INSERT … ON CONFLICT (tenant_id, action_digest) DO UPDATE …`.
--   - INV-AC-RESULT-HASH-IMMUTABLE (HIGH): handler enforces; schema permits ON CONFLICT
--     UPDATE list to omit result_hash (handler logic; not column-level).
--
-- Conventions (mirror migrations/d1/0001_blob_meta.sql):
--   - tenant_id stored as canonical UUIDv7 TEXT form (data_model.md §2.1 L91-93).
--   - action_digest stored as 64-character hex (BLAKE3-256 OR SHA-256 depending on REAPI
--     digest_function announced; v1 ships BLAKE3-256 only — handler validates length=64).
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - result_hash stored as 64-character hex (BLAKE3 of canonical merkle_root per ADR-0037).
--   - tenant_prefix BLOB(16) materialized at INSERT time (ADR-0035 H-3); cron worker
--     reads the column directly without TDK access (avoids cron trust-boundary expansion).
--   - blob_refs is a JSON array TEXT column (D1 SQLite has JSON1 ext; full-scan acceptable
--     for batch reconcile; queries always filter on tenant_id PK first).
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py CI gate
--     (INV-AUTH-MIGRATION-ADDITIVE applied across S-03 + S-04 schemas).
--
-- D1 SQL correctness gates (Lote 10.4bis P0 fix; pre-deploy CI):
--   - sqlite3 :memory: < migrations/d1/0002_ac_meta.sql executes without error
--     (caught at parser level antes deploy via crates/corelink-ac-schema migration_canonical
--     test).
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an implicit transaction).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- ac_meta — REAPI v2 Action Cache metadata; PK composite (tenant_id, action_digest);
-- result envelope referenced via R2 ac-<region> bucket (envelope path Layer 4 HMAC scoped).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ac_meta (
  -- Tenant scope (PK component 1; ALL queries filter via tenant_id Layer 4).
  tenant_id           TEXT        NOT NULL,

  -- Tenant prefix materialized (ADR-0035 H-3; WI-S04-002 §1).
  -- Pre-computed at INSERT em handler step [7]; cron worker reads sem TDK access
  -- (avoids cron trust boundary expansion); 16 raw bytes = HMAC(TDK_v<path_key_id>, tenant_id)[:16].
  tenant_prefix       BLOB        NOT NULL,
  path_key_id         INTEGER     NOT NULL DEFAULT 1,

  -- Action identity (PK component 2; REAPI v2 SHA-256 / BLAKE3 hex digest, length 64).
  action_digest       TEXT        NOT NULL,

  -- Result metadata (per ADR-0037: result_hash == BLAKE3(merkle_root) hex; eliminates
  -- protobuf-determinism dependency).
  result_hash         TEXT        NOT NULL,             -- 64 hex chars
  blob_refs           TEXT        NOT NULL,             -- JSON array of digests (output_files + output_dirs)
  blob_refs_count     INTEGER     NOT NULL,             -- denormalized; cheap COUNT (D1 has no JSON path index)
  result_size_bytes   INTEGER     NOT NULL,             -- envelope payload size (cost/quota observability)

  -- Lifecycle timestamps (unix ms).
  created_at          INTEGER     NOT NULL,
  last_hit_at         INTEGER     NOT NULL,
  expires_at          INTEGER     NULL,                 -- NULL = no expiry pre-S-07; per-tier override S-07/ADR-0019

  -- Crypto integrity binding (WI-S04-004 HKDF signing).
  sig_key_id          INTEGER     NOT NULL DEFAULT 1,   -- HKDF tenant_key version
  sig_alg             TEXT        NOT NULL DEFAULT 'hkdf-sha256',  -- v2+ via ADR migration

  -- Region scope (R2 envelope location).
  -- 5 regions per WI-S04-002 §1: 'sam', 'iad', 'lhr', 'nrt', 'syd'.
  -- Adding a new region requires a new ADR + new migration updating the CHECK list.
  region              TEXT        NOT NULL,

  -- Source attribution (audit cross-check; reuse WI-S03-007 chain).
  -- NULLable for legacy entries pre-WI-S03-002 PAT migration; modern entries always populated.
  created_by_pat_id        TEXT   NULL,
  created_by_request_id    TEXT   NULL,

  PRIMARY KEY (tenant_id, action_digest),

  -- CHECK constraints inlined per ADR-0036 Rule 1 (D1/SQLite has no ALTER ADD CONSTRAINT).
  -- Defense-in-depth: handler also validates; schema CHECK catches handler bypass.
  CHECK (length(action_digest) = 64),                                -- chk_ac_action_digest_len: BLAKE3/SHA-256 hex
  CHECK (length(result_hash) = 64),                                  -- chk_ac_result_hash_len
  CHECK (length(blob_refs) <= 10240),                                -- chk_ac_blob_refs_size: 10 KiB max JSON
  CHECK (blob_refs_count >= 0 AND blob_refs_count <= 4096),          -- chk_ac_blob_refs_count
  CHECK (result_size_bytes >= 0 AND result_size_bytes <= 1048576),   -- chk_ac_result_size: 1 MiB
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),             -- chk_ac_region (ADR for new region)
  CHECK (sig_alg = 'hkdf-sha256'),                                   -- chk_ac_sig_alg: v1 only; v2+ via ADR migration
  CHECK (last_hit_at >= created_at AND (expires_at IS NULL OR expires_at >= created_at)),  -- chk_ac_lifecycle
  CHECK (length(tenant_prefix) = 16),                                -- chk_ac_tenant_prefix_len: 16-byte HMAC truncation
  CHECK (path_key_id >= 1),                                          -- chk_ac_path_key_id_positive
  CHECK (sig_key_id >= 1)                                            -- chk_ac_sig_key_id_positive (0 reserved sentinel per ADR-0021)
);

-- Index: tenant + TTL queries (eviction worker WI-S04-005); partial — most rows have NULL.
CREATE INDEX IF NOT EXISTS idx_ac_meta_tenant_expires
  ON ac_meta(tenant_id, expires_at)
  WHERE expires_at IS NOT NULL;

-- Index: tenant + last_hit (cache analytics + LRU support); not partial (all rows have last_hit_at).
CREATE INDEX IF NOT EXISTS idx_ac_meta_tenant_last_hit
  ON ac_meta(tenant_id, last_hit_at);

-- Index: region (cross-region migration support S-14; analytics queries).
CREATE INDEX IF NOT EXISTS idx_ac_meta_region
  ON ac_meta(region);
