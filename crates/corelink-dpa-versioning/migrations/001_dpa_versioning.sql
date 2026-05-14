-- WI-S19-003 D1 additive migration.
--
-- 1. New `dpa_versions` append-only catalog table — every DPA version
--    ever published.
-- 2. Three additive columns on `tenants` for the per-tenant
--    re-acceptance lifecycle.
--
-- NOTE: This migration is additive only (no DROP / ALTER on existing
-- non-NULL columns) per CoreLink schema-additive-only discipline
-- (ADR-S11-002). Backfill `current_dpa_version` = the version
-- accepted at onboarding (WI-S19-002); `re_acceptance_pending` and
-- `dpa_grace_expires_at` default to false / NULL.

CREATE TABLE IF NOT EXISTS dpa_versions (
    version          TEXT    NOT NULL PRIMARY KEY, -- canonical "vX.Y.Z"
    major            INTEGER NOT NULL,
    minor            INTEGER NOT NULL,
    patch            INTEGER NOT NULL,
    published_at     INTEGER NOT NULL,             -- UTC seconds
    content_hash     TEXT    NOT NULL,             -- SHA-256 hex
    content_url      TEXT    NOT NULL,             -- immutable R2 URL
    CHECK (major >= 0 AND minor >= 0 AND patch >= 0)
);

CREATE INDEX IF NOT EXISTS idx_dpa_versions_published_at
    ON dpa_versions(published_at);

ALTER TABLE tenants ADD COLUMN current_dpa_version    TEXT;
ALTER TABLE tenants ADD COLUMN dpa_grace_expires_at   INTEGER;
ALTER TABLE tenants ADD COLUMN re_acceptance_pending  INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_tenants_dpa_pending
    ON tenants(re_acceptance_pending, dpa_grace_expires_at)
    WHERE re_acceptance_pending = 1;
