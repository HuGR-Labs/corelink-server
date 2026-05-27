-- CoreLink D1 (Cloudflare SQLite) — initial migration for `blob_meta` + `audit_outbox`.
--
-- Canonical sources:
--   - specs/03_architecture/data_model.md §4.2 (`blob_meta` DDL — table/columns/index)
--   - specs/04_sprints/_sealed/S01/work_items/WI-S01-004-d1-schema-blob-meta.md §1
--   - specs/03_architecture/invariant_registry.md
--       INV-CAS-IDEMPOTENCY (`(tenant_id, digest)` primary key + `INSERT OR IGNORE`)
--       INV-CAS-IMMUTABILITY (write-once)
--       INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (audit_outbox + blob_meta in same `db.batch([...])`)
--
-- Conventions:
--   - `tenant_id` stored as canonical UUIDv7 TEXT form (data_model.md §2.1 L91-93).
--   - `digest` stored as canonical `'algo:hex'` TEXT form, e.g. `'blake3:a1b2c3...'`
--     (data_model.md §1 L71 + §2.1 L94).
--   - All timestamps stored as `INTEGER` Unix epoch milliseconds (data_model.md §4.2
--     + WI-S01-004 §9.4). SQLite INTEGER is 64-bit; safe to ~year 292 277 026 596.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX
--     IF NOT EXISTS`. WI §6.2 anti-scope forbids ORM (sqlx/diesel); raw SQL only.
--   - Migrations are **additive-only** (WI §9.7): never `DROP COLUMN` / `ALTER TYPE`.
--
-- Migration runner: see scripts/migrate_d1.sh (WI §6.1.4); on Cloudflare D1 this
-- file is fed via `wrangler d1 execute <DB> --file <path>`.

-- ---------------------------------------------------------------------------
-- blob_meta — CAS metadata, single source of truth for size + refcount + tombstone.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS blob_meta (
    tenant_id        TEXT    NOT NULL,                                  -- canonical UUIDv7 text
    digest           TEXT    NOT NULL,                                  -- canonical 'algo:hex'
    size_bytes       INTEGER NOT NULL CHECK (size_bytes > 0),
    refcount         INTEGER NOT NULL DEFAULT 1 CHECK (refcount >= 0),  -- first write yields 1 (sprint.md §1.4)
    created_at       INTEGER NOT NULL,                                  -- unix epoch ms
    last_accessed_at INTEGER NOT NULL,                                  -- unix epoch ms
    deleted_at       INTEGER,                                           -- NULL = alive; non-NULL = tombstoned
    PRIMARY KEY (tenant_id, digest)
);

-- Partial index: alive blobs only. AuthZ checks (S-02 read path) hit this index.
CREATE INDEX IF NOT EXISTS idx_blob_meta_tenant_alive
    ON blob_meta(tenant_id, deleted_at)
    WHERE deleted_at IS NULL;

-- Partial index: GC sweep candidates (S-06).
CREATE INDEX IF NOT EXISTS idx_blob_meta_gc_candidates
    ON blob_meta(deleted_at)
    WHERE deleted_at IS NOT NULL;

-- ---------------------------------------------------------------------------
-- audit_outbox — outbox-pattern (ADR-0027) staging area for audit events.
-- INSERTed in the same `db.batch([...])` as the blob_meta INSERT/UPDATE so the
-- pair is atomic (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER); a separate drain worker
-- (S-09) flips emitted_at when the event reaches the chain.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS audit_outbox (
    id           TEXT    PRIMARY KEY,                                  -- UUIDv7 text
    tenant_id    TEXT    NOT NULL,
    digest       TEXT,                                                  -- nullable (non-blob events)
    request_id   TEXT    NOT NULL,                                     -- client idempotency key
    event_type   TEXT    NOT NULL,                                     -- e.g. 'corelink.cas.put_completed'
    payload_json TEXT    NOT NULL,                                     -- CloudEvents 1.0 envelope
    enqueued_at  INTEGER NOT NULL,                                     -- unix epoch ms
    emitted_at   INTEGER,                                              -- NULL until drained to S-09 chain
    UNIQUE (request_id, event_type)
);

-- Partial index: pending drain queue.
CREATE INDEX IF NOT EXISTS idx_audit_outbox_pending
    ON audit_outbox(enqueued_at)
    WHERE emitted_at IS NULL;
