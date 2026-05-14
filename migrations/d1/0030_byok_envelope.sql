-- WI-S14-004: BYOK envelope encryption metadata
-- Stores wrapped DEK + KMS identity per blob.
-- Body ciphertext stored separately in R2.
--
-- INV-BYOK-CRYPTO-SOVEREIGNTY: plaintext DEK is NEVER stored here.
-- Only the KMS-wrapped DEK ciphertext is persisted.

CREATE TABLE IF NOT EXISTS byok_envelope (
    tenant_id         TEXT    NOT NULL,
    blob_hash         TEXT    NOT NULL,
    -- KMS-wrapped DEK ciphertext (provider-opaque; typically 150–300 bytes for AWS KMS).
    wrapped_dek       BLOB    NOT NULL,
    -- Provider variant — must match KmsProviderKind snake_case serialisation.
    kms_provider      TEXT    NOT NULL
        CHECK (kms_provider IN ('aws_kms', 'gcp_kms', 'azure_key_vault', 'hashicorp_vault')),
    -- CMK identifier: AWS ARN, GCP resource name, Azure URI, Vault path.
    kms_key_id        TEXT    NOT NULL,
    -- CMK region — must match CoreLink region for latency SLO.
    kms_region        TEXT    NOT NULL,
    -- JSON-encoded AAD: {"tenant_id": "...", "blob_hash": "..."}.
    -- NOT NULL — mandatory AAD binding (cross-blob swap protection).
    encryption_context TEXT   NOT NULL,
    -- AES-256-GCM nonce (12 bytes / 96-bit random per-write).
    aes_gcm_nonce     BLOB    NOT NULL,
    -- Unix epoch milliseconds.
    created_at_ms     INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, blob_hash)
);

-- Index for kill-switch revocation path: find all blobs for a given CMK quickly.
CREATE INDEX IF NOT EXISTS idx_byok_envelope_kms_key
    ON byok_envelope (kms_provider, kms_key_id);

-- Index for tenant-level operations (erasure, audit).
CREATE INDEX IF NOT EXISTS idx_byok_envelope_tenant
    ON byok_envelope (tenant_id);
