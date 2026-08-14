-- CoreLink D1 (Cloudflare SQLite) — migration 0097: public_blocklist.
--
-- F3.2 BLOCKER-1 fix — the `_public` revocation seam. `adapter_cache_map`
-- rows with `namespace = '_public'` (migration 0058) share their content-
-- addressed CAS bytes across EVERY tenant. Until now a poisoned or
-- misclassified `_public` blob was physically UN-erasable: `cas_erase` erases
-- by a tenant's HMAC prefix and never reaches the shared `_public` principal,
-- and `MoatCache.delete` removes only the map row (leaving the bytes for GC).
-- So a bad `_public` digest kept serving cross-tenant forever.
--
-- `public_blocklist` is the atomic, cross-region revocation guard: an admin
-- revocation INSERTs the offending `content_hash` here FIRST (the linearization
-- point), then removes the map rows, then hard-deletes the R2 CAS bytes across
-- all five regions. The D1 blocklist is durable and atomic where the R2 delete
-- (multi-region, best-effort) is not, so once the row exists NO read path serves
-- the digest even if a few bytes briefly survive.
--
-- Enforcement is layered:
--   1. The `_public` READ lookup joins `NOT EXISTS (public_blocklist)` in one
--      statement (no map-read → blocklist-read TOCTOU).
--   2. The `_public` WRITE / re-mirror path inserts only `WHERE NOT EXISTS
--      (public_blocklist)`.
--   3. The BEFORE INSERT/UPDATE triggers below are the DB-layer backstop: even
--      if application code forgets a check, D1 refuses to (re-)create a
--      `_public` mapping to a blocklisted `content_hash`. This closes the
--      pre-check → insert race (a concurrent mirror put cannot re-populate a
--      digest that was revoked between its check and its write).
--
-- Canonical sources:
--   - docs/design/2026-08-13-f3-cross-tenant-public-layer-cache.md (BLOCKER-1)
--   - crates/corelink-container/src/adapter_cache.rs (MoatCache / UrlMapStore)
--   - crates/corelink-container/src/routes/admin.rs (the revoke endpoint auth)
--
-- Invariants:
--   - A revoked `content_hash` is NEVER served and NEVER re-mirrored into
--     `_public` (read-guard + write-guard + triggers).
--   - Revocation is content-hash scoped: the same public bytes reachable through
--     multiple `url_hash`es are all blocked by one row.
--   - Private tenant blobs are unaffected: CAS keys are per-principal-prefix, so
--     identical bytes under a tenant prefix are a different R2 key.
--   - Backward compatible: new table + additive triggers; existing rows untouched.
--
-- Migration runner: scripts/migrate_d1.sh. Idempotent (IF NOT EXISTS +
-- sequential numbering; 0097 runs exactly once).

CREATE TABLE IF NOT EXISTS public_blocklist (
    content_hash   TEXT    NOT NULL PRIMARY KEY,
    revoked_at_ms  INTEGER NOT NULL,
    reason         TEXT    NOT NULL,
    approver       TEXT    NOT NULL,
    audit_event_id TEXT    NOT NULL
);

-- The read-guard join and the revocation map-delete both filter
-- `adapter_cache_map` by `(namespace, content_hash)`; the base table is keyed
-- (namespace, url_hash), so this index keeps those O(log n).
CREATE INDEX IF NOT EXISTS idx_adapter_cache_map_ns_content
    ON adapter_cache_map (namespace, content_hash);

-- DB-layer backstop: refuse to create/point a `_public` mapping at a blocklisted
-- content_hash, even if an application check is bypassed or races.
CREATE TRIGGER IF NOT EXISTS trg_public_blocklist_block_insert
BEFORE INSERT ON adapter_cache_map
WHEN NEW.namespace = '_public'
  AND EXISTS (SELECT 1 FROM public_blocklist pb WHERE pb.content_hash = NEW.content_hash)
BEGIN
    SELECT RAISE(ABORT, 'public content_hash is revoked (public_blocklist)');
END;

CREATE TRIGGER IF NOT EXISTS trg_public_blocklist_block_update
BEFORE UPDATE OF content_hash ON adapter_cache_map
WHEN NEW.namespace = '_public'
  AND EXISTS (SELECT 1 FROM public_blocklist pb WHERE pb.content_hash = NEW.content_hash)
BEGIN
    SELECT RAISE(ABORT, 'public content_hash is revoked (public_blocklist)');
END;
