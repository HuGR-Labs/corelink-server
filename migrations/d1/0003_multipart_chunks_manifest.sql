-- CoreLink D1 (Cloudflare SQLite) — migration 0003 for `chunks` + `manifest_chunks`
-- + `multipart_sessions` (S-05 Multipart + Chunking + Merkle storage backbone).
--
-- Canonical sources:
--   - specs/04_sprints/S05/work_items/WI-S05-004-d1-schema-chunks-manifest-multipart-sessions.md §1
--   - specs/04_sprints/S05/_spec_contract.md §5.3 (R-S05-6)
--   - specs/03_architecture/data_model.md §4.3
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--   - specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md (H-3 tenant_prefix materialization)
--
-- Invariants enforced at storage layer:
--   - INV-TENANT-ISOLATION (CRITICAL): every PK is tenant-leftmost so D1/SQLite uses
--     the tenant prefix for index seek; cross-tenant chunk leak FM-303 impossible at
--     storage layer (Layer 4 of 5-Layer Defense).
--   - INV-MULTIPART-IDEMPOTENT (HIGH): chunks PRIMARY KEY (UNIQUE) supports
--     `INSERT … ON CONFLICT (tenant_id, chunk_digest) DO UPDATE SET refcount = refcount + 1`;
--     multipart_sessions PARTIAL UNIQUE on `(tenant_id, blob_digest_expected)
--     WHERE state = 'in_progress'` — concurrent in-progress sessions for the same
--     blob are coalesced into a single row; completed/aborted records can repeat as
--     audit trail (Lote 10.5bis P0 fix; previously plain UNIQUE wrongly blocked
--     re-upload after completion/abort).
--   - INV-MULTIPART-CHUNK-DETERMINISTIC (CRITICAL): tenant_prefix BLOB(16)
--     materialised at INSERT time per ADR-0035 H-3 — cron worker (WI-S05-006
--     sweeper) reads the column directly without TDK access (avoids cron trust
--     boundary expansion).
--   - INV-MULTIPART-STATE-MONOTONIC (HIGH; registry §3.16): multipart_sessions
--     state graph `in_progress → completed | aborted` is enforced at handler
--     level (CHECK on column would not model the transition). The CHECK below
--     enforces the **enum domain** (`state IN ('in_progress','completed','aborted')`).
--   - INV-MULTIPART-PATH-KEY-MATERIALIZED (HIGH; registry §3.16): tenant_prefix
--     BLOB(16) + path_key_id INTEGER columns; CHECK length(tenant_prefix)=16.
--   - INV-CAS-INTEGRITY (CRITICAL): manifest_chunks references (chunk_digest)
--     are FK *logical* — D1 PRAGMA foreign_keys is per-connection inconsistent
--     in CF Workers pool semantics; reconcile diário in S-06 enforces the
--     reachability tree.
--
-- Conventions (mirror migrations/d1/0001_blob_meta.sql + 0002_ac_meta.sql):
--   - tenant_id stored as canonical UUIDv7 TEXT form (data_model.md §2.1 L91-93).
--   - chunk_digest / blob_digest / r2_object_key stored as 64-character hex
--     (BLAKE3-256 over the chunk bytes / canonical blob root). Handler
--     validates length=64 + lower-hex.
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - tenant_prefix BLOB(16) materialised at INSERT time (ADR-0035 H-3); cron
--     worker reads the column directly without TDK access.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS` / `CREATE UNIQUE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py CI gate
--     (INV-AUTH-MIGRATION-ADDITIVE applied across S-03 + S-04 + S-05 schemas).
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - sqlite3 :memory: < migrations/d1/0003_multipart_chunks_manifest.sql executes
--     without error (caught at parser level antes deploy via
--     crates/corelink-multipart-schema migration_canonical test).
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- chunks — intra-tenant chunk dedup table with refcount.
-- PK composite (tenant_id, chunk_digest) — tenant_id leftmost so D1/SQLite
-- uses the tenant prefix for index seek; cross-tenant chunk leak FM-303
-- impossible at storage layer (Layer 4 enforcement).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS chunks (
  -- Tenant scope (PK component 1; ALL queries filter via tenant_id Layer 4).
  tenant_id           TEXT        NOT NULL,

  -- Chunk identity (PK component 2; BLAKE3-256 hex of chunk bytes; length 64).
  chunk_digest        TEXT        NOT NULL,

  -- Tenant prefix materialised (ADR-0035 H-3; consistent with WI-S04-002 pattern).
  -- Pre-computed at INSERT em handler; cron worker reads sem TDK access; 16 raw
  -- bytes = HMAC(TDK_v<path_key_id>, tenant_id)[:16].
  tenant_prefix       BLOB        NOT NULL,
  path_key_id         INTEGER     NOT NULL DEFAULT 1,

  -- R2 storage location (5 regions per S-05 §5.1: 'sam','iad','lhr','nrt','syd').
  region              TEXT        NOT NULL,
  r2_object_key       TEXT        NOT NULL,             -- chunk-<region>/<tenant_prefix_hex>/<chunk_digest>

  -- Content metadata (FastCDC bounds: min 1 byte (final partial may be < 1 MiB) — max 4 MiB).
  size_bytes          INTEGER     NOT NULL,

  -- Reference counting (intra-tenant dedup; CAP-CAS-010).
  refcount            INTEGER     NOT NULL DEFAULT 1,

  -- Lifecycle timestamps (unix ms).
  created_at          INTEGER     NOT NULL,
  last_referenced_at  INTEGER     NOT NULL,             -- updated em manifest_chunks INSERT

  -- Source attribution (audit cross-check).
  created_by_pat_id     TEXT      NULL,
  created_by_request_id TEXT      NULL,

  PRIMARY KEY (tenant_id, chunk_digest),

  -- CHECK constraints inlined per ADR-0036 Rule 1 (D1/SQLite has no ALTER ADD CONSTRAINT).
  -- Defense-in-depth: handler also validates; schema CHECK catches handler bypass.
  CHECK (length(chunk_digest) = 64),                             -- chk_chunks_digest_len: BLAKE3-256 hex
  CHECK (length(tenant_prefix) = 16),                            -- chk_chunks_tenant_prefix_len: 16-byte HMAC truncation
  CHECK (path_key_id >= 1),                                      -- chk_chunks_path_key_id_positive
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),         -- chk_chunks_region (ADR for new region)
  CHECK (size_bytes >= 1 AND size_bytes <= 4194304),             -- chk_chunks_size_bytes: 1 byte to 4 MiB max (FastCDC max bound; final partial may be < 1 MiB; Lote 10.5bis P0 fix: lower bound 1 byte not 1 MiB)
  CHECK (refcount >= 0),                                         -- chk_chunks_refcount_non_negative: 0 = candidate for S-06 GC
  CHECK (last_referenced_at >= created_at)                       -- chk_chunks_lifecycle: monotonic
);

-- Index: tenant + last_referenced (S-06 GC sweep / LRU analytics).
CREATE INDEX IF NOT EXISTS idx_chunks_tenant_last_ref
  ON chunks(tenant_id, last_referenced_at);

-- Index: tenant + refcount = 0 partial (S-06 GC reachability candidate scan).
-- Most rows have refcount >= 1; partial index keeps it small + cheap to scan.
CREATE INDEX IF NOT EXISTS idx_chunks_tenant_refcount_zero
  ON chunks(tenant_id, last_referenced_at)
  WHERE refcount = 0;

-- ---------------------------------------------------------------------------
-- manifest_chunks — ordered chunk references per chunked blob.
-- PK composite (tenant_id, blob_digest, chunk_index) — tenant_id leftmost;
-- chunk_index 0..MAX_CHUNKS_PER_BLOB-1 sequential per blob.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS manifest_chunks (
  -- Tenant scope (PK component 1).
  tenant_id           TEXT        NOT NULL,

  -- Blob digest (PK component 2; BLAKE3-256 hex of canonical blob root; length 64;
  -- references cas_blobs.blob_digest at logical-FK level).
  blob_digest         TEXT        NOT NULL,

  -- Chunk order (PK component 3; 0-indexed; bounded by MAX_CHUNKS_PER_BLOB
  -- aligned cross-crate to 81920 = 160 GiB / 2 MiB per Lote 10.5-tris P1-SR5-001).
  chunk_index         INTEGER     NOT NULL,

  -- Chunk reference (logical FK to chunks.chunk_digest under same tenant_id).
  chunk_digest        TEXT        NOT NULL,

  PRIMARY KEY (tenant_id, blob_digest, chunk_index),

  -- CHECK constraints inlined.
  CHECK (length(blob_digest) = 64),                              -- chk_manifest_blob_digest_len
  CHECK (length(chunk_digest) = 64),                             -- chk_manifest_chunk_digest_len
  CHECK (chunk_index >= 0 AND chunk_index < 81920)               -- chk_manifest_chunk_index_bounded: 0..81919 = 81920 chunks max (Lote 10.5bis P0 fix: 81920 = 160 GiB / 2 MiB exact, was off-by-one 80000)
);

-- Index: tenant + blob (manifest lookup ordered SCAN by chunk_index).
-- Note: PK already covers (tenant_id, blob_digest, chunk_index) so this lookup
-- uses the PK btree directly. No additional index required for the canonical
-- ordered-scan path.

-- Index: tenant + chunk_digest (reverse lookup for refcount maintenance during
-- manifest delete in S-06 GC; finds every (blob_digest, chunk_index) tuple
-- that references a given chunk).
CREATE INDEX IF NOT EXISTS idx_manifest_chunks_tenant_chunk
  ON manifest_chunks(tenant_id, chunk_digest);

-- ---------------------------------------------------------------------------
-- multipart_sessions — R2 multipart upload tracking (FM-060 detection).
-- PRIMARY KEY (session_id) — host-side cryptographically random ULID/UUIDv7;
-- tenant_id binding column for cross-tenant replay rejection at handler level.
--
-- The plain UNIQUE inline below covers the (tenant_id, blob_digest_expected,
-- state) tuple as a static fallback constraint; the *partial* UNIQUE INDEX
-- below pins the active "single in-progress session per blob" invariant
-- (Lote 10.5bis P0 fix). Handler semantic on collision: retry-on-existing
-- (return existing session_id) matching S3/R2 native multipart idempotency.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS multipart_sessions (
  -- Session identity (PK; opaque host-minted UUIDv7 / ULID; cryptographically random).
  -- This is the local CoreLink session id, NOT R2's upload_id (which is opaque
  -- and stored separately in r2_upload_id).
  session_id          TEXT        PRIMARY KEY,

  -- R2 upload_id (opaque from R2 InitiateMultipartUpload; recorded for the
  -- abort path + audit trail). NULLable until R2 init returns; handler
  -- enforces NOT NULL post-init via UPDATE.
  r2_upload_id        TEXT        NULL,

  -- Tenant scope (every method on the session enforces ctx.tenant_id == row.tenant_id).
  tenant_id           TEXT        NOT NULL,

  -- Tenant prefix materialised (ADR-0035 H-3; cron sweeper reads sem TDK access).
  tenant_prefix       BLOB        NOT NULL,
  path_key_id         INTEGER     NOT NULL DEFAULT 1,

  -- Expected blob digest (computed at SplitBlob start; verified at Complete).
  blob_digest_expected TEXT       NOT NULL,

  -- R2 location (5 regions per S-05 §5.1).
  region              TEXT        NOT NULL,
  bucket              TEXT        NOT NULL,             -- 'corelink-chunk-<region>' or 'corelink-manifest-<region>'
  object_key          TEXT        NOT NULL,             -- composed via tenant_prefix; never client-supplied (INV-MULTIPART-PATH-TENANT-SCOPED)

  -- Lifecycle (unix ms).
  started_at          INTEGER     NOT NULL,
  last_activity_at    INTEGER     NOT NULL,             -- updated per UploadPart
  finalized_at_ms     INTEGER     NULL,                 -- set on completed | aborted transition; NULL while in_progress
  expires_at_ms       INTEGER     NOT NULL,             -- TTL: started_at + 7 days (sweeper aborts orphans past this)

  -- State graph (handler enforces in_progress → completed | aborted; never reverse).
  state               TEXT        NOT NULL DEFAULT 'in_progress',

  -- Source attribution (audit cross-check; reuse WI-S03-007 chain).
  created_by_pat_id     TEXT      NULL,
  created_by_request_id TEXT      NOT NULL,

  -- CHECK constraints inlined.
  CHECK (length(blob_digest_expected) = 64),                                 -- chk_multipart_blob_digest_len
  CHECK (length(tenant_prefix) = 16),                                        -- chk_multipart_tenant_prefix_len
  CHECK (path_key_id >= 1),                                                  -- chk_multipart_path_key_id_positive
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),                     -- chk_multipart_region
  CHECK (state IN ('in_progress', 'completed', 'aborted')),                  -- chk_multipart_state_domain
  CHECK (last_activity_at >= started_at),                                    -- chk_multipart_lifecycle_activity
  CHECK (expires_at_ms >= started_at),                                       -- chk_multipart_lifecycle_expires
  CHECK (finalized_at_ms IS NULL OR finalized_at_ms >= started_at),          -- chk_multipart_lifecycle_finalized
  CHECK ((state = 'in_progress') = (finalized_at_ms IS NULL))                -- chk_multipart_finalized_iff_terminal: in_progress ⇔ NULL; finalized ⇔ NOT NULL
);

-- Partial UNIQUE INDEX (Lote 10.5bis P0 fix; spec contract §5.1):
-- only state='in_progress' enforces uniqueness; completed/aborted records can
-- coexist as audit trail (legitimate re-upload after completion/abort).
CREATE UNIQUE INDEX IF NOT EXISTS uq_multipart_sessions_in_progress
  ON multipart_sessions(tenant_id, blob_digest_expected)
  WHERE state = 'in_progress';

-- Index: orphan sweeper consumes (state='in_progress' + last_activity old).
-- Partial — most rows in steady state are completed/aborted; partial keeps
-- the index small + cheap to scan.
CREATE INDEX IF NOT EXISTS idx_multipart_sessions_orphan_sweep
  ON multipart_sessions(last_activity_at)
  WHERE state = 'in_progress';

-- Index: tenant analytics (state distribution, throughput dashboards).
CREATE INDEX IF NOT EXISTS idx_multipart_sessions_tenant_state
  ON multipart_sessions(tenant_id, state);
