-- Migration: 0027_hot_blobs
-- WI-S14-003 — Hot blob replica worker D1 table (PAT-REGION-FAILOVER-001)
-- Additive-only: no DROP TABLE / ALTER COLUMN / DROP COLUMN.
--
-- CRITICAL: This table is populated by the OFFLINE daily aggregation job
-- (cron 02:00 UTC) — NOT by live Prometheus metrics labeled by tenant_id.
-- Per INV-OBS-CARDINALITY-BUDGET S-09: live metric labeled by tenant_id is
-- FORBIDDEN (10k+ tenants = 1M+ unique series = cardinality budget exceeded).
--
-- Residency restriction (INV-REGION-NO-CROSS-LEAK; Schrems II + LGPD Art. 33):
-- - WNAM ↔ ENAM (US sibling pair)
-- - WEUR ↔ SAM  (EU ↔ LGPD sibling; WEUR→ENAM/WNAM = FORBIDDEN)
-- Enforced by replication worker + property tests (see crates/corelink-replica-worker).

CREATE TABLE IF NOT EXISTS hot_blobs (
    -- Tenant identifier (plain; hashed in observability spans).
    tenant_id           TEXT        NOT NULL,
    -- BLAKE3 content-addressable hash of the blob (INV-CAS-INTEGRITY).
    blob_hash           TEXT        NOT NULL,
    -- Tenant's primary pinned region (source of truth; source of replication).
    primary_region      TEXT        NOT NULL
        CHECK (primary_region IN ('wnam', 'enam', 'weur', 'sam')),
    -- Allowed sibling region for replication (residency-restricted).
    -- WNAM→ENAM, ENAM→WNAM, WEUR→SAM, SAM→WEUR only.
    -- Cross-jurisdiction (WEUR→ENAM etc.) = FORBIDDEN by worker logic.
    replica_region      TEXT        NOT NULL
        CHECK (replica_region IN ('wnam', 'enam', 'weur', 'sam')),
    -- Total bytes for this blob (from 30d offline aggregation SUM).
    bytes               BIGINT      NOT NULL    CHECK (bytes >= 0),
    -- Access count over last 30d window.
    access_count_30d    BIGINT      NOT NULL    CHECK (access_count_30d >= 0),
    -- Last access timestamp (ms since epoch).
    last_access_ms      BIGINT      NOT NULL    CHECK (last_access_ms >= 0),
    -- Timestamp when replication completed (ms since epoch); NULL if not yet replicated.
    replicated_at_ms    BIGINT                  CHECK (replicated_at_ms IS NULL OR replicated_at_ms >= 0),
    -- Replication lifecycle status.
    replication_status  TEXT        NOT NULL
        CHECK (replication_status IN ('pending', 'in_progress', 'replicated', 'failed', 'evicted')),

    PRIMARY KEY (tenant_id, blob_hash)
);

-- Index for replication worker: reads WHERE replication_status = 'pending'.
CREATE INDEX IF NOT EXISTS idx_hot_blobs_replication_status
    ON hot_blobs (replication_status);

-- Index for failover router: reads WHERE primary_region = ? AND replication_status = 'replicated'.
CREATE INDEX IF NOT EXISTS idx_hot_blobs_primary_region
    ON hot_blobs (primary_region);

-- Index for offline aggregation daily eviction pass: tenant_id lookups.
CREATE INDEX IF NOT EXISTS idx_hot_blobs_tenant_id
    ON hot_blobs (tenant_id);
