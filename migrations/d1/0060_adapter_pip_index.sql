-- CoreLink D1 (Cloudflare SQLite) — migration 0060: adapter_pip_index.
--
-- The MUTABLE simple-index cache for the pip (PyPI) cache adapter.
--
-- Unlike wheel/sdist BYTES (immutable, content-addressed by sha256 → stored in
-- the 2-level content-dedup moat: adapter_cache_map + the CAS), the PEP 503/691
-- SIMPLE INDEX is re-fetched from upstream on TTL expiry and therefore cannot
-- live in the content-addressed store. It is cached here per-tenant.
--
-- Schema mirrors the `corelink_adapter_host::pip` KvStore port:
--   get(tenant, key) -> Option<(value_bytes, inserted_at_unix_ms)>
--   put(tenant, key, value_bytes, inserted_at_unix_ms)
-- The adapter uses `inserted_ms` for its TTL freshness check
-- (`pip::index::is_fresh`).
--
-- Storage model:
--   - namespace   = the tenant UUID (text). PRIVATE per tenant (isolated) — the
--     simple index is per-tenant by the adapter's port contract, NOT shared
--     under '_public' (only the immutable public wheel BYTES are shared).
--   - kv_key      = the adapter's index key (`pip:idx:<normalised-project>`).
--   - value       = the cached PEP 691 JSON payload, HEX-encoded TEXT. D1's
--     HTTP API has no BLOB type, so the container's PipIndexKvStore round-trips
--     bytes via hex (binary-safe, lint-clean, column-stable).
--   - inserted_ms = unix-millis the row was written (TTL anchor).
--
-- Invariants:
--   - INV-TENANT-ISOLATION: namespace is the per-tenant UUID; no cross-tenant
--     index sharing (a tenant's cached index is private to that tenant).
--   - Backward compatible: new table; no existing data touched.
--
-- Migration runner: see scripts/migrate_d1.sh. Idempotency: CREATE TABLE IF
-- NOT EXISTS + the runner's sequential numbering (0059 runs exactly once).

CREATE TABLE IF NOT EXISTS adapter_pip_index (
    namespace   TEXT    NOT NULL,
    kv_key      TEXT    NOT NULL,
    value       TEXT    NOT NULL,
    inserted_ms INTEGER NOT NULL,
    PRIMARY KEY (namespace, kv_key)
);
