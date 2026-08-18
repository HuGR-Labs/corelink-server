-- Migration 0098: widen the `region` CHECK on `erasure_attestations` and
-- `erasure_public_keys` to admit all 6 CoreLink macro-regions.
--
-- WHY: the GDPR erasure ATTESTATION subsystem
-- (`crates/corelink-erasure-attestation`) gained an `apac` attestation region.
-- The persisted region domain is the INLINE `CHECK (region IN (...))` declared
-- at CREATE TABLE time in migration 0032, which only accepted the original
-- 4-region baseline `('wnam','enam','weur','sam')`. Any attempt to persist an
-- `apac` attestation index row or public key would be rejected by that CHECK,
-- so the subsystem could not actually attest an apac erasure.
--
-- This migration widens BOTH CHECKs to
-- `region IN ('wnam','enam','weur','sam','apac','afr')` — the full set of 6
-- macro-regions (apac AND afr) so a later afr signing key needs no further
-- schema change. Widening is a superset: no previously-accepted value is
-- removed, so no existing row can violate the new CHECK.
--
-- SQLite/D1 limitation forcing a table rebuild (the "12-step" recreate):
--   0032 declares both CHECKs as INLINE COLUMN constraints on CREATE TABLE.
--   SQLite (hence Cloudflare D1) has NO `ALTER TABLE … ALTER COLUMN` /
--   `DROP CONSTRAINT` / `ADD CONSTRAINT` to relax an EXISTING inline CHECK in
--   place (see the SQLite "Making Other Kinds Of Table Schema Changes" 12-step
--   procedure). The only schema-correct way to relax an existing inline CHECK
--   is to rebuild the table, which unavoidably uses DROP TABLE + RENAME TO.
--   This is the exact mechanism ADR-0062 / migration 0062 used for the tier
--   CHECK widening; ADR-0098 is the migration-mechanism record for this one.
--
-- ADDITIVE intent (INV-AUTH-MIGRATION-ADDITIVE, HIGH):
--   Destructive in MECHANISM (rebuild) but PURELY ADDITIVE in EFFECT — the
--   accepted-value set only grows and every row is copied 1:1
--   (`INSERT … SELECT` explicit column list, no WHERE, no transform). The
--   `check_migrations_additive.py` gate's `DROP`/`RENAME` tokens are therefore
--   annotated line-locally with `-- additive-allowed: ADR-0098 <reason>`.
--
-- Idempotency: a rebuild is inherently one-shot; guarded by the migration
-- runner's sequential numbering (`wrangler d1 migrations apply` records 0098
-- exactly once — the same guarantee 0062 relies on).
--
-- Column set replicated EXACTLY from:
--   - migrations/d1/0032_erasure_attestation.sql (base columns + indexes)
--   - migrations/d1/0079_erasure_attestation_signed_columns.sql (the two
--     additive nullable columns signature_ed25519 + canonical_payload_jcs on
--     erasure_attestations).

PRAGMA foreign_keys = OFF;
-- D1 runs each migration file as a single transaction, and SQLite documents
-- `PRAGMA foreign_keys` as a NO-OP inside a transaction — so the line above
-- only helps engines that execute the file statement-by-statement. For D1 the
-- effective mechanism is `defer_foreign_keys` (D1-documented), which IS
-- honoured in-transaction and covers the DROP/RENAME window below.
PRAGMA defer_foreign_keys = true;

-- ============================================================
-- 1. erasure_attestations — rebuild with the widened region CHECK
-- ============================================================
-- Mirrors 0032 EXACTLY plus the two nullable columns 0079 added
-- (signature_ed25519, canonical_payload_jcs); only the `region` CHECK list
-- gains 'apac' + 'afr'.
CREATE TABLE IF NOT EXISTS erasure_attestations_new (
    request_id          TEXT    NOT NULL PRIMARY KEY,
    tenant_id           TEXT    NOT NULL,
    region              TEXT    NOT NULL CHECK (region IN ('wnam', 'enam', 'weur', 'sam', 'apac', 'afr')),
    attestation_key_id  BIGINT  NOT NULL,
    -- R2 object key: corelink-audit-{region}/erasure_attestations/{request_id}.json
    r2_key              TEXT    NOT NULL,
    signed_at_ms        BIGINT  NOT NULL,
    kms_provider        TEXT    NOT NULL,
    kms_key_id          TEXT    NOT NULL,
    -- SHA-256 hex of EvidenceBundle (audit chain segment IDs + KMS destroy ts + key_id + tenant_id)
    evidence_hash       TEXT    NOT NULL CHECK (length(evidence_hash) = 64),
    -- Added by 0079 (additive, nullable): base64 Ed25519 signature over
    -- canonical_payload_jcs, and the exact RFC-8785 JCS bytes that were signed.
    signature_ed25519      TEXT,
    canonical_payload_jcs  TEXT
);

-- Copy every existing row 1:1 (explicit column list; no filter, no transform).
INSERT INTO erasure_attestations_new (
    request_id, tenant_id, region, attestation_key_id, r2_key,
    signed_at_ms, kms_provider, kms_key_id, evidence_hash,
    signature_ed25519, canonical_payload_jcs
)
SELECT
    request_id, tenant_id, region, attestation_key_id, r2_key,
    signed_at_ms, kms_provider, kms_key_id, evidence_hash,
    signature_ed25519, canonical_payload_jcs
FROM erasure_attestations;

DROP TABLE erasure_attestations;  -- additive-allowed: ADR-0098 rebuild to widen inline region CHECK (set grows; rows copied 1:1 above)

ALTER TABLE erasure_attestations_new RENAME TO erasure_attestations;  -- additive-allowed: ADR-0098 finalize rebuild swap (no data loss)

-- Re-create the 0032 indexes verbatim (lost with the dropped table).
CREATE INDEX IF NOT EXISTS idx_erasure_attestations_tenant_id
    ON erasure_attestations (tenant_id);

CREATE INDEX IF NOT EXISTS idx_erasure_attestations_region
    ON erasure_attestations (region);

CREATE INDEX IF NOT EXISTS idx_erasure_attestations_signed_at_ms
    ON erasure_attestations (signed_at_ms);

-- ============================================================
-- 2. erasure_public_keys — rebuild with the widened region CHECK
-- ============================================================
-- Mirrors 0032 EXACTLY; only the `region` CHECK list gains 'apac' + 'afr'.
CREATE TABLE IF NOT EXISTS erasure_public_keys_new (
    key_id              BIGINT  NOT NULL,
    region              TEXT    NOT NULL CHECK (region IN ('wnam', 'enam', 'weur', 'sam', 'apac', 'afr')),
    state               TEXT    NOT NULL CHECK (state IN ('active', 'overlap', 'retired')),
    created_at_ms       BIGINT  NOT NULL,
    -- 30d canonical overlap window end timestamp (ms since epoch)
    overlap_until_ms    BIGINT  NOT NULL,
    -- PEM-encoded SubjectPublicKeyInfo (SPKI) for offline verification
    public_key_pem      TEXT    NOT NULL,
    PRIMARY KEY (key_id, region)
);

INSERT INTO erasure_public_keys_new (
    key_id, region, state, created_at_ms, overlap_until_ms, public_key_pem
)
SELECT
    key_id, region, state, created_at_ms, overlap_until_ms, public_key_pem
FROM erasure_public_keys;

DROP TABLE erasure_public_keys;  -- additive-allowed: ADR-0098 rebuild to widen inline region CHECK (set grows; rows copied 1:1 above)

ALTER TABLE erasure_public_keys_new RENAME TO erasure_public_keys;  -- additive-allowed: ADR-0098 finalize rebuild swap (no data loss)

CREATE INDEX IF NOT EXISTS idx_erasure_public_keys_region_state
    ON erasure_public_keys (region, state);

PRAGMA foreign_keys = ON;
