-- Migration 0067: per-hash CAS erase 410-Gone tombstone table.
--
-- hugit-P2 seam B (WP-B). Backs the operator/internal write-side endpoint
-- `POST /_internal/cas/:tenant/:hash/erase` (container `routes::cas_erase`,
-- pure logic in crate `corelink-handler-cas-erase`). When a single
-- content-addressed blob is erased from R2, a row is written here; the CAS
-- READ path (`routes::cas`) consults this table BEFORE the R2 GET and answers
-- HTTP 410 Gone for any `(tenant_id, digest)` present — never 404 (which would
-- imply "never existed") and never 200 (which would resurrect erased bytes).
--
-- ADDITIVE intent (INV-AUTH-MIGRATION-ADDITIVE, HIGH):
--   - Pure `CREATE TABLE IF NOT EXISTS` — no existing table is altered,
--     dropped, or rebuilt; no row is mutated. Re-runnable / idempotent.
--   - The erase write is an UPSERT keyed on (tenant_id, digest), so a replayed
--     erasure is a safe no-op (matches the route's idempotent 410 semantics
--     and the DSR R2 `DeleteObject` idempotency).
--
-- Composition with DSR Wave 1 (PR #254): the R2 byte-deletion reuses the DSR
-- increment-3 R2 CAS primitives (`R2S3Client::{delete, list_objects_v2}`);
-- this table is the read-side tombstone index those bytes leave behind. The
-- table is independent of the DSR tenant-wide erase tables (chunks /
-- manifest_chunks / multipart_sessions / blob_meta) — it tracks per-hash
-- public-API erases, not tenant-account erasure.

CREATE TABLE IF NOT EXISTS cas_tombstone (
    -- Tenant the erased hash belonged to (the sole isolation key).
    tenant_id   TEXT NOT NULL,
    -- The erased content digest (hash-safe charset, validated by the handler).
    digest      TEXT NOT NULL,
    -- Operator-supplied audit reason (free-form, e.g. 'dsr-erasure',
    -- 'abuse-takedown'). Bounded by the route; never PII.
    reason      TEXT NOT NULL DEFAULT '',
    -- Unix epoch milliseconds the tombstone was written (set by the route).
    erased_at_ms INTEGER NOT NULL DEFAULT 0,
    -- One tombstone per (tenant, hash); the erase UPSERT targets this key so a
    -- re-erase is idempotent.
    PRIMARY KEY (tenant_id, digest)
);

-- Read-path lookups are by (tenant_id, digest) — already covered by the
-- composite PRIMARY KEY above, so no secondary index is needed.
