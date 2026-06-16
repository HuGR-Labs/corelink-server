-- Migration 0071: per-tenant monthly REQUEST-count backing (`monthly_request_counts`).
--
-- Backs the previously-unenforced monthly request cap from the signed
-- launch rate card (`worker/src/lib/quota.ts` QUOTAS[tier].requestsPerMonthMax:
-- free 500K / solo 2M / starter 6M / pro 20M / max 80M req per calendar
-- month; team/enterprise uncapped). The Worker's `checkRequestQuota` was a
-- hard-coded no-op because no durable per-tenant monthly counter existed
-- (red-team finding #5, docs/security/2026-06-16-redteam-brutal.md). This
-- table is that counter.
--
-- ## Why a dedicated table (not `tenant_quota` / `ratelimit_buckets`)
--
-- Three DISTINCT abuse axes, three backings:
--   * `ratelimit_buckets` (token bucket) bounds VELOCITY (req/s + burst) —
--     enforced in-app by the container's `ratelimit_layer.rs`.
--   * `tenant_quota` (0066) bounds cumulative DOLLARS per cycle (ADR-0068).
--   * THIS table bounds the cumulative REQUEST COUNT per calendar month,
--     the rate-card `requestsPerMonthMax` axis. A slow-but-steady tenant
--     stays under the per-second rate limit and under the $-tripwire yet
--     could still blow past the contracted monthly request allowance;
--     this closes that gap at the Worker edge (the DO/container remain the
--     deeper net on mutations).
--
-- ## Keying — (tenant_id, year_month)
--
-- One row per tenant per calendar month. `year_month` is the UTC calendar
-- month as a fixed `YYYY-MM` string (e.g. `2026-06`) — derived by the
-- Worker from `new Date().toISOString().slice(0, 7)`, matching the
-- `secondsUntilNextMonthStart()` UTC-month boundary used for Retry-After.
-- A new month implicitly resets the count (a fresh row is INSERTed); old
-- rows are inert history (an operator/cron may prune them, out of band).
--
-- ## Atomic increment-and-check
--
-- The Worker enforces with a single atomic UPSERT that increments and
-- returns the post-increment count in one round trip (no read-modify-write
-- race across concurrent isolates):
--
--   INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms)
--   VALUES (?1, ?2, 1, ?3)
--   ON CONFLICT(tenant_id, year_month)
--   DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3
--   RETURNING request_count;
--
-- The returned `request_count` is compared to QUOTAS[tier].requestsPerMonthMax;
-- when it EXCEEDS the cap the Worker returns 429 + Retry-After =
-- secondsUntilNextMonthStart(). On a D1 error the read fails OPEN
-- (availability), consistent with quota.ts's documented posture.
--
-- ## Idempotency / additive-only
--
-- `CREATE TABLE IF NOT EXISTS` lets the migration replay safely. The
-- migration is additive-only (INV-AUTH-MIGRATION-ADDITIVE): it adds a NEW
-- table + index and never alters/drops/renames an existing object, so it
-- needs no ADR waiver and no `-- additive-allowed:` suppression.

CREATE TABLE IF NOT EXISTS monthly_request_counts (
    -- Canonical tenant id (matches `tenant.tenant_id`; UUID/v7 string).
    tenant_id TEXT NOT NULL,

    -- UTC calendar month as `YYYY-MM` (e.g. `2026-06`). The Worker derives
    -- this from `new Date().toISOString().slice(0, 7)` so the bucket
    -- boundary matches `secondsUntilNextMonthStart()`.
    year_month TEXT NOT NULL,

    -- Cumulative request count for this tenant in this calendar month.
    -- Atomically incremented by the UPSERT above; never decremented.
    request_count INTEGER NOT NULL DEFAULT 0
        CHECK (request_count >= 0),

    -- Last-mutation wall-clock (Unix epoch ms), for operator forensics.
    updated_at_ms INTEGER NOT NULL DEFAULT 0,

    -- Exactly one counter row per tenant per calendar month.
    PRIMARY KEY (tenant_id, year_month)
);

-- Operator/forensics slice — sweep a single month's counters (e.g. a prune
-- cron, or "who is near their monthly cap this month") without a full scan.
CREATE INDEX IF NOT EXISTS idx_monthly_request_counts_month
    ON monthly_request_counts (year_month);
