-- Migration 0037: DPA acceptances (WI-S19-002).
--
-- Persists the durable record of every successful
-- `POST /v1/dpa/accept` call. The Rust crate
-- `corelink-dpa-acceptance` (`dpa_schema_version() == 1`) owns the
-- canonical schema; this table is the production mirror.
--
-- INVARIANTS encoded here:
--   - PK on `signup_id` enforces PAT-RETRY-IDEMPOTENT-001
--     (idempotent per signup_id).
--   - `accepted_ip_hash` is sha256(ip || per-region salt); raw IPs
--     never persisted (CTRL-PRIV-001).
--   - `locale` constrained to the closed 3-locale GA set; mismatch
--     reject happens at the orchestrator before INSERT, so this
--     CHECK is a defence-in-depth backstop.
--   - `jwt_receipt_jti` is UNIQUE — replay protection sentinel for
--     the verify path (`dpa.replay_detected`).
--   - Additive-only migration (`IF NOT EXISTS`); no destructive ops.

CREATE TABLE IF NOT EXISTS dpa_acceptances (
    -- Idempotency key (PAT-RETRY-IDEMPOTENT-001).
    signup_id          TEXT    NOT NULL PRIMARY KEY,
    -- Owning tenant.
    tenant_id          TEXT    NOT NULL,
    -- Semver of the DPA template accepted (CTRL-PRIV-CONSENT-003).
    dpa_version        TEXT    NOT NULL,
    -- BCP-47 locale; closed enum mirrors the Rust crate.
    locale             TEXT    NOT NULL CHECK (locale IN ('en-US', 'pt-BR', 'es-419')),
    -- SHA-256 hex64 of the canonical rendered notice text
    -- (CTRL-PRIV-CONSENT-001).
    notice_hash        TEXT    NOT NULL CHECK (length(notice_hash) = 64),
    -- Deterministic UUID v7 from notice_version
    -- (CTRL-PRIV-CONSENT-006).
    wording_id         TEXT    NOT NULL,
    -- Client-captured timestamp at click moment (ms since epoch).
    ui_capture_ts      BIGINT  NOT NULL,
    -- Server-stamped submission timestamp (ms since epoch); HuGR
    -- clock authoritative.
    submission_ts      BIGINT  NOT NULL,
    -- JWT `jti` — primary verify sentinel + replay detection.
    jwt_receipt_jti    TEXT    NOT NULL UNIQUE,
    -- SHA-256 hex64 of (client_ip || per-region salt). Salt rotated
    -- via S-13 secret rotation worker.
    accepted_ip_hash   TEXT    NOT NULL CHECK (length(accepted_ip_hash) = 64),
    -- Acceptance time (ms since epoch); equals submission_ts at
    -- insert. Kept distinct so a future replay-window column can be
    -- added without altering submission semantics.
    accepted_at        BIGINT  NOT NULL,
    -- Schema version (bump on additive change).
    schema_version     INTEGER NOT NULL DEFAULT 1,
    -- Sanity: submission_ts must be ≥ ui_capture_ts (server time is
    -- authoritative but the click can't logically follow submission).
    CONSTRAINT submission_after_capture CHECK (submission_ts >= ui_capture_ts)
);

-- Per-tenant lookup for "show me this tenant's accepted DPAs"
-- compliance views (DASH-ONBOARDING).
CREATE INDEX IF NOT EXISTS idx_dpa_acceptances_tenant
    ON dpa_acceptances (tenant_id, accepted_at);

-- Per-version analytics (acceptance rate by DPA version).
CREATE INDEX IF NOT EXISTS idx_dpa_acceptances_version
    ON dpa_acceptances (dpa_version, accepted_at);

-- Per-locale analytics (locale mix dashboard).
CREATE INDEX IF NOT EXISTS idx_dpa_acceptances_locale
    ON dpa_acceptances (locale, accepted_at);
