-- CoreLink Neon Postgres — auth domain schema (WI-S03-005).
--
-- Canonical sources:
--   - specs/03_architecture/data_model.md §4.1 (Neon control-plane DDL)
--   - specs/04_sprints/S03/work_items/WI-S03-005-neon-schema-auth-tables.md §1
--   - specs/03_architecture/auth_model.md §2 (PAT / principal types)
--   - specs/03_architecture/key_management.md §3.13 (column encryption keys)
--   - specs/03_architecture/privacy_model.md §3 (PII handling)
--   - specs/03_architecture/invariant_registry.md
--       INV-AUTH-SCHEMA-RLS-DEFAULT-ON   (CRITICAL)
--       INV-AUTH-PII-ENCRYPTED            (CRITICAL)
--       INV-AUTH-MIGRATION-ADDITIVE       (HIGH)
--       INV-AUTH-CASCADE-DSR-COMPLETE     (HIGH)
--       INV-AUTH-AUDIT-PSEUDONYMIZATION   (CRITICAL)
--   - specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md
--
-- Conventions:
--   - All identifiers (`account_id`, `tenant_id`, `user_id`, `pat_id`, …) are
--     UUIDv7 minted **app-side** via `uuid::Uuid::now_v7()`. The DDL deliberately
--     omits `DEFAULT gen_random_uuid()` (which would produce v4) so a CI gate
--     that scans for `gen_random_uuid()` cannot regress on the canonical mint
--     path. WI-S03-005 §1 P0 fix Lote 10.3bis.
--   - PII columns (`email`, `webauthn_credentials.public_key`,
--     `webauthn_credentials.attestation_object`, `account.billing_email`) live
--     as `BYTEA` ciphertext written via `pgp_sym_encrypt_bytea(...)` in the
--     application layer. Each encrypted column has a sibling `<col>_key_id
--     INTEGER` so multi-key decrypt works during quarterly key rotation
--     (key_management.md §3.13).
--   - Lookup-by-email is satisfied by `email_hash BYTEA UNIQUE` carrying
--     `hmac(lower(trim(email))::bytea, current_setting('app.email_hash_key',
--     true)::bytea, 'sha256')`. The HMAC key is HKDF-SHA256 derived from the
--     Worker secret `CORELINK_MASTER_KEY` (info bytes
--     `b"corelink-v1-email-hash-key"`; salt `"corelink-email-hash-salt-v1"`).
--   - Migration is **additive-only** (INV-AUTH-MIGRATION-ADDITIVE). The CI gate
--     `scripts/check_migrations_additive.py` rejects DROP TABLE / DROP COLUMN /
--     destructive ALTER TYPE / ALTER COLUMN NOT NULL → NULL.
--   - Migration is idempotent: every `CREATE TABLE` / `CREATE INDEX` is guarded
--     with `IF NOT EXISTS`; every enum `CREATE TYPE` is wrapped in a
--     `DO $$ … EXCEPTION WHEN duplicate_object …` block.
--
-- Migration runner:
--     PGPASSWORD=… psql "$NEON_URL" -v ON_ERROR_STOP=1 -f migrations/002_auth_tables.sql
--
-- =============================================================================

-- -----------------------------------------------------------------------------
-- 0. Pre-flight guards (fail-closed when prerequisites not met).
-- -----------------------------------------------------------------------------

-- pgcrypto provides pgp_sym_encrypt_bytea / pgp_sym_decrypt_bytea / hmac().
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Refuse migration if the deploy operator did not seed the email-hash key.
-- Closes "INSERT-time fail-open with NULL email_hash" defect (WI-S03-005
-- §6.1 (3-bis) Lote 10.3-tris P0-R5-003).
DO $$
BEGIN
    IF current_setting('app.email_hash_key', true) IS NULL
       OR length(current_setting('app.email_hash_key', true)) = 0 THEN
        RAISE EXCEPTION
            'app.email_hash_key not set; refusing migration. '
            'Set via: ALTER DATABASE ... SET app.email_hash_key = ''<HKDF-derived-key-hex>'';'
            ' Derivation: HKDF-SHA256(CORELINK_MASTER_KEY, "corelink-email-hash-salt-v1",'
            ' b"corelink-v1-email-hash-key", 32).';
    END IF;
END $$;

-- -----------------------------------------------------------------------------
-- 1. Enum types.
-- -----------------------------------------------------------------------------

DO $$
BEGIN
    CREATE TYPE account_type AS ENUM ('individual', 'business', 'enterprise');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE TYPE region_t AS ENUM ('wnam', 'enam', 'weur', 'eeur', 'apac');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE TYPE tier_t AS ENUM ('solo', 'team', 'business', 'enterprise');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE TYPE role_t AS ENUM ('owner', 'admin', 'developer', 'viewer');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE TYPE pat_env_t AS ENUM ('pat', 'ci', 'ro');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE TYPE revocation_reason_t AS ENUM (
        'user_initiated',
        'admin_revoked',
        'rotation',
        'compromise_suspected',
        'expired',
        'tenant_offboarded'
    );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- -----------------------------------------------------------------------------
-- 2. account — top-level customer entity.
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS account (
    account_id           UUID PRIMARY KEY,
    name                 TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    type                 account_type NOT NULL,
    billing_email        BYTEA,                                   -- pgp_sym_encrypt_bytea
    billing_email_key_id INTEGER NOT NULL DEFAULT 1 CHECK (billing_email_key_id > 0),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at           TIMESTAMPTZ
);

-- -----------------------------------------------------------------------------
-- 3. tenant — multi-tenancy isolation scope.
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS tenant (
    tenant_id              UUID PRIMARY KEY,
    account_id             UUID NOT NULL REFERENCES account(account_id) ON DELETE CASCADE,
    slug                   TEXT NOT NULL UNIQUE
                              CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$'),
    -- The S-03 implementation set is `slug` + `region` (5-region enum) + `tier`.
    -- The S-19 onboarding pipeline + S-14 region residency add `display_name`
    -- (commercial label), `primary_region` TEXT (6-region canonical incl.
    -- `sam` + `afr` per `privacy_model.md §7.1 L313-320`), `plan_id` (FK to
    -- the `plan` table introduced in WI-S10-001 billing), `locale_default`
    -- (3-locale canonical per WI-S11-004), and `status` (provisioning lifecycle)
    -- as canonical control-plane columns alongside the auth-domain set. These
    -- columns are NULL on the auth-only insert path (WI-S03-005); the S-19
    -- signup orchestration populates them atomically. The `plan_id` FK is
    -- deferred until WI-S10-001 introduces the `plan` table — until then
    -- `plan_id` is plain text, matching the canonical `data_model.md §4.1`
    -- contract additively.
    display_name           TEXT,
    primary_region         TEXT
                              CHECK (
                                primary_region IS NULL
                                OR primary_region IN ('wnam','enam','weur','sam','apac','afr')
                              ),
    plan_id                TEXT,
    status                 TEXT
                              CHECK (
                                status IS NULL
                                OR status IN ('provisioning','active','suspended','deleting')
                              ),
    locale_default         TEXT
                              CHECK (
                                locale_default IS NULL
                                OR locale_default IN ('pt-BR','en-US','es-MX')
                              ),
    region                 region_t NOT NULL,
    tier                   tier_t NOT NULL DEFAULT 'team',
    quota_storage_gb       INTEGER NOT NULL DEFAULT 100
                              CHECK (quota_storage_gb >= 0),
    quota_requests_per_day BIGINT NOT NULL DEFAULT 1000000
                              CHECK (quota_requests_per_day >= 0),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at             TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_tenant_account_alive
    ON tenant(account_id) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_tenant_region_alive
    ON tenant(region) WHERE deleted_at IS NULL;

-- -----------------------------------------------------------------------------
-- 4. user_account — Clerk-managed identity mirror.
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS user_account (
    user_id        UUID PRIMARY KEY,
    clerk_user_id  TEXT NOT NULL UNIQUE
                       CHECK (length(clerk_user_id) BETWEEN 1 AND 200),
    email          BYTEA NOT NULL,                                -- pgp_sym_encrypt_bytea
    email_key_id   INTEGER NOT NULL DEFAULT 1 CHECK (email_key_id > 0),
    email_hash     BYTEA NOT NULL UNIQUE
                       CHECK (length(email_hash) = 32),           -- HMAC-SHA256 → 32 bytes
    full_name      TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at  TIMESTAMPTZ,
    deleted_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_user_email_hash_alive
    ON user_account(email_hash) WHERE deleted_at IS NULL;

-- -----------------------------------------------------------------------------
-- 5. membership — M:N (user_account, tenant) with role.
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS membership (
    user_account_id UUID NOT NULL REFERENCES user_account(user_id) ON DELETE CASCADE,
    tenant_id       UUID NOT NULL REFERENCES tenant(tenant_id)    ON DELETE CASCADE,
    role            role_t NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at      TIMESTAMPTZ,
    PRIMARY KEY (user_account_id, tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_membership_tenant_alive
    ON membership(tenant_id, role) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_membership_user_alive
    ON membership(user_account_id) WHERE deleted_at IS NULL;

-- -----------------------------------------------------------------------------
-- 6. pat — Personal Access Token rows (consumed by WI-S03-002 / -003 / -004).
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS pat (
    pat_id            UUID        PRIMARY KEY,
    token_id          TEXT        NOT NULL UNIQUE
                                  CHECK (length(token_id) = 16),
    signing_key_id    INTEGER     NOT NULL DEFAULT 1 CHECK (signing_key_id > 0),
    tenant_id         UUID        NOT NULL REFERENCES tenant(tenant_id) ON DELETE CASCADE,
    issued_to_user    UUID        REFERENCES user_account(user_id),
    kind              TEXT        NOT NULL
                                  CHECK (kind IN ('user','ci','readonly','executor','service')),
    token_hash        BYTEA       NOT NULL UNIQUE,
    scopes            TEXT[]      NOT NULL CHECK (array_length(scopes, 1) >= 1),
    expires_at        TIMESTAMPTZ,
    last_used_at      TIMESTAMPTZ,
    revoked_at        TIMESTAMPTZ,
    revocation_reason revocation_reason_t,
    revoked_by        UUID        REFERENCES user_account(user_id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- revocation invariant: reason iff revoked_at, never one without the other.
    CHECK ((revoked_at IS NULL AND revocation_reason IS NULL)
        OR (revoked_at IS NOT NULL AND revocation_reason IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_pat_token_id              ON pat(token_id);
CREATE INDEX IF NOT EXISTS idx_pat_tenant_alive          ON pat(tenant_id, issued_to_user)
    WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_pat_user_alive            ON pat(issued_to_user)
    WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_pat_expires_alive         ON pat(expires_at)
    WHERE revoked_at IS NULL AND expires_at IS NOT NULL;

-- -----------------------------------------------------------------------------
-- 7. webauthn_credentials — passkey storage (consumed by WI-S03-006).
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS webauthn_credentials (
    id                 UUID PRIMARY KEY,
    user_account_id    UUID NOT NULL REFERENCES user_account(user_id) ON DELETE CASCADE,
    credential_id      BYTEA NOT NULL UNIQUE,
    public_key         BYTEA NOT NULL,
    public_key_key_id  INTEGER NOT NULL DEFAULT 1 CHECK (public_key_key_id > 0),
    attestation_object BYTEA,
    attestation_key_id INTEGER NOT NULL DEFAULT 1 CHECK (attestation_key_id > 0),
    sign_count         BIGINT NOT NULL DEFAULT 0 CHECK (sign_count >= 0),
    transports         TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    aaguid             UUID,
    backup_eligible    BOOLEAN NOT NULL DEFAULT false,
    backup_state       BOOLEAN NOT NULL DEFAULT false,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at       TIMESTAMPTZ,
    deleted_at         TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_webauthn_user_alive
    ON webauthn_credentials(user_account_id) WHERE deleted_at IS NULL;

-- -----------------------------------------------------------------------------
-- 8. revocation_log — denormalised state (WI-S03-004 SoT cross-region).
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS revocation_log (
    id                       UUID PRIMARY KEY,
    pat_id                   UUID NOT NULL,
    tenant_id                UUID NOT NULL,
    revoked_at               TIMESTAMPTZ NOT NULL,
    revoked_by               UUID,
    reason                   revocation_reason_t NOT NULL,
    propagation_completed_at TIMESTAMPTZ,
    UNIQUE (pat_id, revoked_at)
);

CREATE INDEX IF NOT EXISTS idx_revocation_pending_propagation
    ON revocation_log(revoked_at) WHERE propagation_completed_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_revocation_tenant
    ON revocation_log(tenant_id, revoked_at);

-- -----------------------------------------------------------------------------
-- 9. Row-Level Security policies (defence-in-depth Layer 2).
-- -----------------------------------------------------------------------------
--
-- The application sets the per-request tenant context via:
--     SELECT set_config('app.current_tenant', '<tenant_uuid>', true);
-- inside an explicit transaction (`SET LOCAL` is transaction-scoped). The
-- helper macro lives in `corelink-auth-schema::rls::with_tenant_ctx!`.
--
-- The `auth_admin` role bypasses RLS for back-office operations; admin queries
-- must be wrapped in `SET LOCAL ROLE auth_admin` and are logged by the audit
-- chain (S-09).

ALTER TABLE account              ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant               ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_account         ENABLE ROW LEVEL SECURITY;
ALTER TABLE membership           ENABLE ROW LEVEL SECURITY;
ALTER TABLE pat                  ENABLE ROW LEVEL SECURITY;
ALTER TABLE webauthn_credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE revocation_log       ENABLE ROW LEVEL SECURITY;

-- account / user_account / webauthn are not directly tenant-scoped at the
-- column level; their visibility flows through the membership join. We default
-- those policies to deny-all-then-elevate-via-admin role; tenant / pat /
-- membership / revocation_log carry direct tenant_id columns and use the
-- canonical policy.

DO $$
BEGIN
    CREATE POLICY tenant_isolation_tenant ON tenant
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE POLICY tenant_isolation_membership ON membership
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE POLICY tenant_isolation_pat ON pat
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE POLICY tenant_isolation_revocation ON revocation_log
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- account / user_account / webauthn_credentials default to deny-all; admins
-- elevate via SET LOCAL ROLE auth_admin (see ADR-0031 §5.4).
DO $$
BEGIN
    CREATE POLICY admin_only_account ON account USING (false);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE POLICY admin_only_user_account ON user_account USING (false);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    CREATE POLICY admin_only_webauthn ON webauthn_credentials USING (false);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- -----------------------------------------------------------------------------
-- 10. updated_at trigger for `account`.
-- -----------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION corelink_auth_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DO $$
BEGIN
    CREATE TRIGGER account_set_updated_at
        BEFORE UPDATE ON account
        FOR EACH ROW
        EXECUTE FUNCTION corelink_auth_set_updated_at();
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- -----------------------------------------------------------------------------
-- 11. Schema versioning (canonical compatibility check at app startup).
-- -----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS schema_version (
    version     INTEGER PRIMARY KEY,
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    description TEXT NOT NULL
);

INSERT INTO schema_version (version, description)
VALUES (2, 'Auth tables — account/tenant/user_account/membership/pat/webauthn_credentials/revocation_log + pgcrypto + RLS')
ON CONFLICT (version) DO NOTHING;
