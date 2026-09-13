-- Credential lifecycle generations and durable D1 revocation projection.
-- Generation 0 is legacy history; only rows with a known runner/DevEnv
-- authority are classified during this migration. Customer PATs remain NULL.

ALTER TABLE runner_credential_obligation ADD COLUMN lifecycle_generation TEXT NOT NULL DEFAULT '0';
ALTER TABLE devenv_credential_obligation ADD COLUMN lifecycle_generation TEXT NOT NULL DEFAULT '0';
ALTER TABLE pat ADD COLUMN lifecycle_generation TEXT;

UPDATE pat
SET lifecycle_generation = '0'
WHERE lifecycle_generation IS NULL
  AND (
    runner_job_ac_key IS NOT NULL
    OR EXISTS (SELECT 1 FROM runner_credential_obligation o WHERE o.pat_id = pat.pat_id)
    OR EXISTS (SELECT 1 FROM devenv_credential_obligation o WHERE o.pat_id = pat.pat_id)
  );

CREATE TABLE IF NOT EXISTS tenant_credential_revocation_floor (
  tenant_id TEXT PRIMARY KEY NOT NULL,
  revoked_through TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS credential_generation_revocation (
  pat_id TEXT PRIMARY KEY NOT NULL,
  token_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  lifecycle_generation TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'revoked'))
);

CREATE INDEX IF NOT EXISTS idx_credential_generation_revocation_tenant_generation
  ON credential_generation_revocation (tenant_id, lifecycle_generation, pat_id);

CREATE TRIGGER IF NOT EXISTS tenant_credential_revocation_floor_generation_check
BEFORE INSERT ON tenant_credential_revocation_floor
WHEN length(NEW.revoked_through) = 0
  OR length(NEW.revoked_through) > 19
  OR (length(NEW.revoked_through) = 19 AND NEW.revoked_through > '9223372036854775807')
  OR NEW.revoked_through GLOB '*[^0-9]*'
  OR (length(NEW.revoked_through) > 1 AND substr(NEW.revoked_through, 1, 1) = '0')
  OR CAST(NEW.revoked_through AS INTEGER) < 0
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS tenant_credential_revocation_floor_generation_update_check
BEFORE UPDATE OF revoked_through ON tenant_credential_revocation_floor
WHEN length(NEW.revoked_through) = 0
  OR length(NEW.revoked_through) > 19
  OR (length(NEW.revoked_through) = 19 AND NEW.revoked_through > '9223372036854775807')
  OR NEW.revoked_through GLOB '*[^0-9]*'
  OR (length(NEW.revoked_through) > 1 AND substr(NEW.revoked_through, 1, 1) = '0')
  OR CAST(NEW.revoked_through AS INTEGER) < 0
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

-- SQLite ALTER TABLE cannot add a CHECK to an existing table. These guards keep
-- all generation-bearing rows canonical after the additive columns land.
CREATE TRIGGER IF NOT EXISTS runner_credential_obligation_generation_check
BEFORE INSERT ON runner_credential_obligation
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
  OR CAST(NEW.lifecycle_generation AS INTEGER) < 0
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS runner_credential_obligation_generation_update_check
BEFORE UPDATE OF lifecycle_generation ON runner_credential_obligation
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
  OR CAST(NEW.lifecycle_generation AS INTEGER) < 0
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS devenv_credential_obligation_generation_check
BEFORE INSERT ON devenv_credential_obligation
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
  OR CAST(NEW.lifecycle_generation AS INTEGER) < 0
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS devenv_credential_obligation_generation_update_check
BEFORE UPDATE OF lifecycle_generation ON devenv_credential_obligation
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
  OR CAST(NEW.lifecycle_generation AS INTEGER) < 0
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS pat_lifecycle_generation_check
BEFORE INSERT ON pat
WHEN NEW.lifecycle_generation IS NOT NULL AND (
  length(NEW.lifecycle_generation) = 0 OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
  OR CAST(NEW.lifecycle_generation AS INTEGER) < 0
)
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS pat_lifecycle_generation_update_check
BEFORE UPDATE OF lifecycle_generation ON pat
WHEN NEW.lifecycle_generation IS NOT NULL AND (
  length(NEW.lifecycle_generation) = 0 OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
  OR CAST(NEW.lifecycle_generation AS INTEGER) < 0
)
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;
