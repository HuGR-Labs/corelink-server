-- CoreLink D1 (Cloudflare SQLite) — migration 0058: adapter_cache_map.
--
-- The 2-level "moat" map for the URL-keyed cache adapters (brew / npm / pip).
-- Those adapters key their cache by a hash of the upstream URL, but the
-- content-addressed CAS verifies `key == blake3(content)` and would reject a
-- URL-derived key (HashMismatch). So the bytes are stored content-addressed in
-- CAS (deduped) and this table maps `(namespace, url_hash) -> content_hash`
-- (the CAS digest), enabling a URL lookup to find the deduped bytes.
--
-- The network-effect MOAT:
--   - `namespace = '_public'` for public deterministic deps (Homebrew bottles,
--     public npm/PyPI packages) — identical content is stored ONCE in CAS and
--     served to EVERY authenticated tenant (one tenant warms it, all benefit).
--   - `namespace = <tenant_uuid>` for private/scoped content (isolated).
--   Access is still PAT-gated (Option-B re-verify); only the *public* CONTENT
--   is shared, which is safe because it is already public at the upstream.
--
-- Canonical sources:
--   - specs/_proposals/adapters/WIRING-PLAN.md (the 2-level moat design)
--   - crates/corelink-container/src/adapter_cache.rs (UrlMapStore + MoatCache)
--   - crates/corelink-handler-cas (the content-addressed CAS storing the bytes)
--
-- Design notes:
--   - PRIMARY KEY (namespace, url_hash): O(1) lookup on the cache hot path.
--   - content_hash is the CAS digest (blake3 hex in prod via
--     corelink_hash::CanonicalHash) — the join key into the CAS.
--   - No FK to a tenant table: `namespace` may be the synthetic '_public'.
--   - created_ms enables future TTL/GC of stale map rows (not enforced here).
--
-- Invariants:
--   - INV-TENANT-ISOLATION: private content uses a per-tenant namespace;
--     '_public' holds only public-registry content (safe to share cross-tenant).
--   - Backward compatible: new table; no existing data touched.
--
-- Migration runner: see scripts/migrate_d1.sh. Idempotency: CREATE TABLE IF
-- NOT EXISTS + the runner's sequential numbering (0058 runs exactly once).

CREATE TABLE IF NOT EXISTS adapter_cache_map (
    namespace    TEXT    NOT NULL,
    url_hash     TEXT    NOT NULL,
    content_hash TEXT    NOT NULL,
    content_len  INTEGER NOT NULL,
    created_ms   INTEGER NOT NULL,
    PRIMARY KEY (namespace, url_hash)
);
