-- WI-S14-001 — D1 migration: region provisioning audit log.
-- Additive only (INV-AUTH-MIGRATION-ADDITIVE).
-- Tracks Terraform apply events + DO jurisdiction + R2 location verification
-- per region. Per-region R2/D1/DO resource IDs tracked for drift detection.
-- INV-AUDIT-APPEND-ONLY: no UPDATE/DELETE on provisioning_audit_log.

-- Region provisioning audit log.
-- One row per Terraform apply or verification event per region.
CREATE TABLE IF NOT EXISTS region_provisioning_audit_log (
    -- Primary key: UUIDv7 (sortable by time)
    event_id BLOB(16) PRIMARY KEY,

    -- Region identifier (wnam/enam/weur/sam)
    region TEXT NOT NULL CHECK (region IN ('wnam', 'enam', 'weur', 'sam')),

    -- CloudEvent type (corelink.region.provisioned, etc.)
    event_type TEXT NOT NULL,

    -- Epoch ms when event occurred
    event_at_ms BIGINT NOT NULL,

    -- R2 bucket name provisioned/verified
    r2_bucket_name TEXT,

    -- D1 instance ID provisioned/verified
    d1_instance_id TEXT,

    -- DO Worker name provisioned/verified
    do_worker_name TEXT,

    -- DO jurisdiction set/verified (none/eu/us)
    do_jurisdiction TEXT CHECK (do_jurisdiction IN ('none', 'eu', 'us') OR do_jurisdiction IS NULL),

    -- KV namespace ID
    kv_namespace_id TEXT,

    -- Terraform run ID (from GH Actions)
    terraform_run_id TEXT,

    -- Result (success/failure/drift_detected)
    result TEXT NOT NULL CHECK (result IN ('success', 'failure', 'drift_detected', 'verification_ok', 'verification_fail')),

    -- JSON details (resource IDs, diff counts, etc.)
    details_json TEXT,

    -- Audit chain: hash of previous row for tamper evidence
    prev_hash BLOB(32)
);

-- Index for per-region + time queries (drift detection, compliance audit)
CREATE INDEX IF NOT EXISTS idx_region_provisioning_audit_region_time
    ON region_provisioning_audit_log (region, event_at_ms DESC);

-- Index for WEUR DO jurisdiction queries (Schrems II compliance audit)
CREATE INDEX IF NOT EXISTS idx_region_provisioning_audit_weur_jurisdiction
    ON region_provisioning_audit_log (region, do_jurisdiction)
    WHERE region = 'weur';

-- ---------------------------------------------------------------------------
-- Region migration progress table.
-- Tracks per-tenant migration state (single-region → multi-region).
-- Used by migration script for idempotent re-run detection.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS region_migration_progress (
    -- Composite key: (tenant_id, migration_run_id)
    tenant_id TEXT NOT NULL,
    migration_run_id BLOB(16) NOT NULL,
    PRIMARY KEY (tenant_id, migration_run_id),

    -- Source region (original single-region)
    source_region TEXT NOT NULL CHECK (source_region IN ('wnam', 'enam', 'weur', 'sam')),

    -- Target region
    target_region TEXT NOT NULL CHECK (target_region IN ('wnam', 'enam', 'weur', 'sam')),

    -- Migration state (pending/d1_migrated/r2_copied/verified/completed/failed/rolled_back)
    state TEXT NOT NULL CHECK (state IN (
        'pending', 'd1_migrated', 'r2_copied', 'verified',
        'completed', 'failed', 'rolled_back'
    )),

    -- D1 row count migrated
    d1_rows_migrated BIGINT NOT NULL DEFAULT 0,

    -- R2 blob count migrated
    r2_blobs_migrated BIGINT NOT NULL DEFAULT 0,

    -- Hash verification result
    hash_verified INTEGER NOT NULL DEFAULT 0 CHECK (hash_verified IN (0, 1)),

    -- Started at
    started_at_ms BIGINT NOT NULL,

    -- Completed at
    completed_at_ms BIGINT,

    -- Error message if failed
    error_message TEXT
);

-- Index for per-run progress queries
CREATE INDEX IF NOT EXISTS idx_region_migration_run
    ON region_migration_progress (migration_run_id, state);

-- Index for per-tenant state queries (idempotent re-run check)
CREATE INDEX IF NOT EXISTS idx_region_migration_tenant_state
    ON region_migration_progress (tenant_id, state);
