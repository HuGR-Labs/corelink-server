-- Migration 0073: seed the `_public` shared-dedup storage-accounting rows.
--
-- The brew/npm/pip cache adapters store PUBLIC, deterministic, cross-tenant-
-- shareable content (Homebrew bottles, public npm/PyPI packages) under the
-- shared `_public` namespace (`crate::adapter_cache::PUBLIC_NAMESPACE`) — the
-- dedup that powers the network-effect moat. The CAS WRITE for that content goes
-- through the same `tenant_storage_state` byte-accounting reservation as a real
-- tenant.
--
-- ## The bug this fixes (brew was 502 in prod, 2026-06-21)
-- `_public` is not a real tenant: nothing ever seeded its `tenant_storage_state`
-- row, and the adapter write threads `storage_quota_bytes = None` (public writes
-- accrue against an EXISTING row, never seed one — only a tenant's first NATIVE
-- write seeds, carrying the Worker cap). So the reservation hit
-- `AccrueOutcome::Indeterminate` (no row + no resolved cap) and failed CLOSED →
-- `cas write: storage cap indeterminate` → HTTP 502 on EVERY public bottle/
-- package fetch. (The companion R2-key fix — a reserved sentinel prefix for
-- `_public` in `storage/r2_s3.rs::tenant_prefix` — lands in the same change.)
--
-- ## Why quota = 0 (unlimited)
-- `bytes_quota = 0` means "no cap" in the accrue WHERE clause (it always admits).
-- The `_public` namespace is intentionally shared + unbilled to any tenant — it
-- is CoreLink's own shared public-dep cache (the moat), bounded by the finite,
-- deterministic public-dep set and reclaimed by the per-region eviction cron, NOT
-- by a per-tenant dollar/byte cap. So it is seeded uncapped, one row per region.
--
-- Additive-only (INSERT OR IGNORE) per scripts/check_migrations_additive.py;
-- idempotent (re-running never duplicates or resets a row).

INSERT OR IGNORE INTO tenant_storage_state (
  tenant_id, region, bytes_used, bytes_quota,
  bytes_used_updated_at_ms, last_synced_at_ms, bytes_reclaimed_lifetime,
  created_at_ms, updated_at_ms
)
SELECT '_public', r.region, 0, 0,
       CAST(unixepoch('now', 'subsec') * 1000 AS INTEGER),
       CAST(unixepoch('now', 'subsec') * 1000 AS INTEGER),
       0,
       CAST(unixepoch('now', 'subsec') * 1000 AS INTEGER),
       CAST(unixepoch('now', 'subsec') * 1000 AS INTEGER)
FROM (
  SELECT 'sam' AS region UNION ALL SELECT 'iad' UNION ALL SELECT 'lhr'
  UNION ALL SELECT 'nrt' UNION ALL SELECT 'syd'
) AS r;
