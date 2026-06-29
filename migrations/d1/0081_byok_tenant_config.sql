-- 0081_byok_tenant_config.sql
--
-- BYOK Wave 2 (SCHEMA): the per-tenant **key-custody + crypto-mode** read
-- model for encryption-at-rest. This lands the control-plane configuration
-- surface that Wave 3 (data-plane wiring) will CONSUME — it does NOT wire any
-- encryption, and NOTHING on the hot path (CAS/AC) reads or writes these
-- tables yet. Purely additive, zero prod risk: two NEW nullable-by-default
-- tables that no live code path touches.
--
-- ## The Tcs / CMK hierarchy (plan §1) these tables describe
--
--   * CMK  — the **Customer Master Key**, held in the customer's KMS
--            (`cmk_provider` + `cmk_key_id` + `cmk_region`). CoreLink never
--            holds the CMK material; it only references the key.
--   * Tcs  — the per-tenant **Tenant Convergence Secret**, CMK-WRAPPED and
--            persisted in `tenant_byok_secret.tcs_wrapped`. The plaintext Tcs
--            is NEVER stored (INV-BYOK-CRYPTO-SOVEREIGNTY, mirrors 0030's
--            wrapped-DEK discipline) — only the CMK-wrapped ciphertext.
--
-- ## The two axes (plan §2)
--
--   * `mode`        — the key-CUSTODY rung: `managed` (CoreLink-held key),
--                     `byok` (customer CMK wraps the Tcs), `hyok` (hold-your-
--                     own-key — strongest custody).
--   * `crypto_mode` — Mode A vs Mode B: `convergent` (dedup-preserving,
--                     convergent encryption keyed by the Tcs) vs `random`
--                     (max-isolation, per-write random keys, no cross-blob
--                     dedup).
--
-- ## State machine (`state`)
--
--   inactive → pending → active            (onboarding completes)
--                      ↘ partial            (active for NEW writes while a
--                                            backfill re-encrypts old blobs)
--   active / partial   → shredded           (crypto-shred: CMK/Tcs destroyed)
--
-- Transitions are **monotonic + audited** and are WRITTEN ONLY by the
-- onboarding / control-plane authority (the signup-worker — audit finding H5,
-- a LATER wave). This migration adds the tables; no writer is wired here.
--
-- ## Additive policy
--
-- `CREATE TABLE IF NOT EXISTS` only — no DROP, no retype, no rewrite of
-- existing rows. INV-AUTH-MIGRATION-ADDITIVE (auth-migrations-additive-only).
-- The d1_migrations ledger guarantees exactly-once application
-- (d1-migrations-ledger-desync).

-- Per-tenant key-custody + crypto-mode configuration (control-plane read model).
CREATE TABLE IF NOT EXISTS tenant_byok_config (
    tenant_id    TEXT    PRIMARY KEY,
    -- Key-custody rung (plan §2). 'managed' = CoreLink-held; 'byok' = customer
    -- CMK wraps the Tcs; 'hyok' = hold-your-own-key (strongest custody).
    mode         TEXT    NOT NULL
        CHECK (mode IN ('managed', 'byok', 'hyok')),
    -- Mode A vs Mode B (plan §2). 'convergent' preserves cross-blob dedup;
    -- 'random' maximises isolation (per-write random keys, no dedup).
    crypto_mode  TEXT    NOT NULL DEFAULT 'convergent'
        CHECK (crypto_mode IN ('convergent', 'random')),
    -- CMK provider — NULL until a BYOK/HYOK tenant is onboarded.
    cmk_provider TEXT
        CHECK (cmk_provider IN ('aws', 'gcp', 'azure', 'vault', 'corelink_managed')),
    -- CMK identity: ARN / GCP resource name / Azure URI / Vault path.
    cmk_key_id   TEXT,
    -- CMK region — bound for the latency SLO (mirrors 0030's kms_region).
    cmk_region   TEXT,
    -- Onboarding state machine (monotonic + audited; signup-worker writes it).
    state        TEXT    NOT NULL DEFAULT 'inactive'
        CHECK (state IN ('inactive', 'pending', 'active', 'partial', 'shredded')),
    -- Unix epoch milliseconds.
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

-- Per-tenant CMK-wrapped Tenant Convergence Secret (Tcs).
-- INV-BYOK-CRYPTO-SOVEREIGNTY: plaintext Tcs is NEVER stored here — only the
-- CMK-wrapped ciphertext. `tcs_wrapped` is NULL until onboarding wraps it.
CREATE TABLE IF NOT EXISTS tenant_byok_secret (
    tenant_id   TEXT    PRIMARY KEY,
    -- CMK-wrapped Tenant Convergence Secret ciphertext (provider-opaque).
    tcs_wrapped BLOB,
    -- CMK identity used to wrap (echoes tenant_byok_config.cmk_key_id at wrap time).
    cmk_key_id  TEXT,
    -- Tcs version — bumped on rotation; starts at 1.
    tcs_version INTEGER NOT NULL DEFAULT 1,
    -- Unix epoch milliseconds the Tcs was wrapped (NULL until wrapped).
    wrapped_at_ms INTEGER
);
