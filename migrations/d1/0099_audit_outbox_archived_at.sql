-- CoreLink D1 (Cloudflare SQLite) — migration 0099: audit_outbox.archived_at.
--
-- The S-09 control calls for an IMMUTABLE OFFSITE copy of the sealed audit
-- chain: NDJSON chunks in an R2 bucket under a 7-year Object Lock. Migration
-- 0078 added the seal columns and `POST /_internal/audit/drain` fills them, but
-- the seal lands in D1 — which is mutable. Nothing ever copied a sealed row
-- anywhere. `POST /_internal/audit/archive` is that copier and this column is
-- its watermark.
--
-- Why a per-ROW watermark and not a per-partition cursor: a cursor records how
-- far the archiver BELIEVES it got, which is precisely the kind of bookkeeping
-- that drifts from what R2 actually holds. A per-row NULL is a claim about one
-- row that a listing of the bucket can contradict, and the archiver only ever
-- sets it AFTER the chunk containing that row is durable in R2.
--
-- ADDITIVE ONLY: a nullable column plus a partial index. No CHECK constraint is
-- touched, so no table rebuild and no ADR is required (see the auth-migrations
-- additive-only rule). Every existing row starts NULL, i.e. "not yet archived",
-- which is the truth: at the time of writing, 56,026 sealed rows exist and ZERO
-- of them have ever been archived. Backfill is therefore not a separate script
-- — the archiver's ordinary sweep walks the backlog to zero.
--
-- Canonical sources:
--   - crates/corelink-container/src/routes/audit_archive.rs
--   - crates/corelink-audit-chain/src/sealed_archive.rs
--   - docs/knowledge/compliance/audit-chain.md

ALTER TABLE audit_outbox ADD COLUMN archived_at INTEGER;  -- unix epoch ms the row's chunk landed in R2; NULL = not archived

-- Partial index: the archiver's work queue. Sealed rows that still need a
-- chunk, in the chain order the archiver reads them.
CREATE INDEX IF NOT EXISTS idx_audit_outbox_unarchived
    ON audit_outbox(tenant_id, region, sequence_number)
    WHERE emitted_at IS NOT NULL AND archived_at IS NULL;
