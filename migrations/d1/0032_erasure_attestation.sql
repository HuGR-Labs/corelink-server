-- Migration 0032: Erasure attestation tables (WI-S14-007)
--
-- Creates two tables:
--   erasure_attestations  — index for verify endpoint lookup by request_id
--   erasure_public_keys   — per-region Ed25519 public keys (Active + Overlap)
--
-- R2 audit bucket `corelink-audit-{region}/erasure_attestations/{request_id}.json`
-- is the authoritative store (7y retention via R2 lifecycle policy).
-- D1 is the index; if a D1 entry is missing, quarterly reconcile rebuilds
-- from R2 (procedure: docs/ops/d1-r2-reconcile.md).
--
-- Security: tenant_id stored for audit only; no PII beyond DSR scope.

-- Attestation index for GET /v1/public/attestation/{request_id}
CREATE TABLE IF NOT EXISTS erasure_attestations (
    request_id          TEXT    NOT NULL PRIMARY KEY,
    tenant_id           TEXT    NOT NULL,
    region              TEXT    NOT NULL CHECK (region IN ('wnam', 'enam', 'weur', 'sam')),
    attestation_key_id  BIGINT  NOT NULL,
    -- R2 object key: corelink-audit-{region}/erasure_attestations/{request_id}.json
    r2_key              TEXT    NOT NULL,
    signed_at_ms        BIGINT  NOT NULL,
    kms_provider        TEXT    NOT NULL,
    kms_key_id          TEXT    NOT NULL,
    -- SHA-256 hex of EvidenceBundle (audit chain segment IDs + KMS destroy ts + key_id + tenant_id)
    evidence_hash       TEXT    NOT NULL CHECK (length(evidence_hash) = 64)
);

CREATE INDEX IF NOT EXISTS idx_erasure_attestations_tenant_id
    ON erasure_attestations (tenant_id);

CREATE INDEX IF NOT EXISTS idx_erasure_attestations_region
    ON erasure_attestations (region);

CREATE INDEX IF NOT EXISTS idx_erasure_attestations_signed_at_ms
    ON erasure_attestations (signed_at_ms);

-- Per-region Ed25519 public keys (served by GET /v1/public/keys/erasure/{region}.pub)
-- State = active | overlap | retired; overlap window 30d canonical (key_management.md §3.2.1)
CREATE TABLE IF NOT EXISTS erasure_public_keys (
    key_id              BIGINT  NOT NULL,
    region              TEXT    NOT NULL CHECK (region IN ('wnam', 'enam', 'weur', 'sam')),
    state               TEXT    NOT NULL CHECK (state IN ('active', 'overlap', 'retired')),
    created_at_ms       BIGINT  NOT NULL,
    -- 30d canonical overlap window end timestamp (ms since epoch)
    overlap_until_ms    BIGINT  NOT NULL,
    -- PEM-encoded SubjectPublicKeyInfo (SPKI) for offline verification
    public_key_pem      TEXT    NOT NULL,
    PRIMARY KEY (key_id, region)
);

CREATE INDEX IF NOT EXISTS idx_erasure_public_keys_region_state
    ON erasure_public_keys (region, state);
