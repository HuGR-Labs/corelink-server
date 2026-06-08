-- CoreLink D1 (Cloudflare SQLite) — migration 0059: adapter_npm_meta.
--
-- Mutable, TTL'd KV for the npm cache adapter's PACKAGE METADATA surface.
-- This is deliberately SEPARATE from the content-addressed moat
-- (`adapter_cache_map`, migration 0058): npm package metadata is mutable
-- JSON refreshed on a TTL (a package gains versions over time), so it cannot
-- live in a content-addressed store (a new version changes the content hash
-- and orphans the old row; there is no stable content identity to key on).
--
-- The npm adapter's `KvStore` port stores `(value, inserted_at_unix_ms)`
-- with upsert semantics; the freshness/TTL decision is pure-logic in the
-- adapter (`corelink_adapter_host::npm::metadata::is_fresh`) — it reads
-- `inserted_ms` back and compares against its configured TTL. This table
-- therefore stores no expiry column; `inserted_ms` is the only anchor.
--
-- The network-effect MOAT applies to metadata too:
--   - `namespace = '_public'` for UNSCOPED packages (public registry data) —
--     one tenant warms it, every authenticated tenant benefits.
--   - `namespace = <tenant_uuid>` for SCOPED `@org/...` packages (isolated).
--   Access is PAT-gated (Option-B re-verify); only the public metadata is
--   shared, which is safe because it is already public at registry.npmjs.org.
--
-- Storage:
--   - value_hex: the metadata JSON bytes, hex-encoded. D1 over the HTTP API
--     is JSON-only; hex keeps the column binary-safe and escaping-free. The
--     container's `adapter_kv.rs` encodes/decodes.
--   - inserted_ms: unix milliseconds at write time (the TTL anchor).
--
-- Design notes (mirror 0058):
--   - PRIMARY KEY (namespace, meta_key): O(1) lookup on the metadata hot path.
--   - No FK to a tenant table: `namespace` may be the synthetic '_public'.
--
-- Invariants:
--   - INV-TENANT-ISOLATION: scoped metadata uses a per-tenant namespace;
--     '_public' holds only public-registry metadata (safe to share).
--   - Backward compatible: new table; no existing data touched.
--
-- Migration runner: see scripts/migrate_d1.sh. Idempotency: CREATE TABLE IF
-- NOT EXISTS + the runner's sequential numbering (0059 runs exactly once).

CREATE TABLE IF NOT EXISTS adapter_npm_meta (
    namespace   TEXT    NOT NULL,
    meta_key    TEXT    NOT NULL,
    value_hex   TEXT    NOT NULL,
    inserted_ms INTEGER NOT NULL,
    PRIMARY KEY (namespace, meta_key)
);
