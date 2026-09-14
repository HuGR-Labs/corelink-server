-- Reconcile installations that applied the original 0127-0130 credential
-- migrations before their generation-fencing follow-up landed on main.
--
-- Fresh installs already receive these invariants from 0127-0130.  Every
-- trigger is therefore additive and idempotent, while existing installs gain
-- the same binding, canonical-generation, terminal-state, and identity guards.

-- Do not silently bless a legacy row that the new guards would reject.  Keep
-- one opaque, fixed-cardinality attestation per check so the preflight is
-- additive and repeatable; the table contains no tenant or credential data.
CREATE TABLE IF NOT EXISTS credential_lifecycle_generation_reconciliation_validation (
  check_name TEXT PRIMARY KEY NOT NULL,
  valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO credential_lifecycle_generation_reconciliation_validation (check_name, valid)
VALUES ('devenv_binding', CASE WHEN EXISTS (
  SELECT 1 FROM devenv_credential_obligation o
  WHERE o.state IN ('issued', 'adopted')
    AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=o.pat_id AND pat.tenant_id=o.tenant_id AND pat.token_id=o.token_id AND pat.revoked_at_ms IS NULL)
) THEN 0 ELSE 1 END)
ON CONFLICT(check_name) DO UPDATE SET valid = excluded.valid;

INSERT INTO credential_lifecycle_generation_reconciliation_validation (check_name, valid)
VALUES ('runner_binding', CASE WHEN EXISTS (
  SELECT 1 FROM runner_credential_obligation o
  WHERE o.state IN ('issued', 'adopted')
    AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=o.pat_id AND pat.tenant_id=o.tenant_id AND pat.token_id=o.token_id AND pat.revoked_at_ms IS NULL)
) THEN 0 ELSE 1 END)
ON CONFLICT(check_name) DO UPDATE SET valid = excluded.valid;

INSERT INTO credential_lifecycle_generation_reconciliation_validation (check_name, valid)
VALUES ('revocation_generation', CASE WHEN EXISTS (
  SELECT 1 FROM credential_generation_revocation
  WHERE length(lifecycle_generation) = 0
    OR length(lifecycle_generation) > 19
    OR (length(lifecycle_generation) = 19 AND lifecycle_generation > '9223372036854775807')
    OR lifecycle_generation GLOB '*[^0-9]*'
    OR (length(lifecycle_generation) > 1 AND substr(lifecycle_generation, 1, 1) = '0')
) THEN 0 ELSE 1 END)
ON CONFLICT(check_name) DO UPDATE SET valid = excluded.valid;

INSERT INTO credential_lifecycle_generation_reconciliation_validation (check_name, valid)
VALUES ('receipt_generation', CASE WHEN EXISTS (
  SELECT 1 FROM credential_generation_event_receipts
  WHERE length(lifecycle_generation) = 0
    OR length(lifecycle_generation) > 19
    OR (length(lifecycle_generation) = 19 AND lifecycle_generation > '9223372036854775807')
    OR lifecycle_generation GLOB '*[^0-9]*'
    OR (length(lifecycle_generation) > 1 AND substr(lifecycle_generation, 1, 1) = '0')
) THEN 0 ELSE 1 END)
ON CONFLICT(check_name) DO UPDATE SET valid = excluded.valid;

CREATE TRIGGER IF NOT EXISTS devenv_credential_obligation_binding_insert
BEFORE INSERT ON devenv_credential_obligation
WHEN NEW.state IN ('issued','adopted') AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=NEW.pat_id AND pat.tenant_id=NEW.tenant_id AND pat.token_id=NEW.token_id AND pat.revoked_at_ms IS NULL)
BEGIN SELECT RAISE(ABORT,'invalid credential obligation binding'); END;

CREATE TRIGGER IF NOT EXISTS devenv_credential_obligation_binding_update
BEFORE UPDATE ON devenv_credential_obligation
WHEN NEW.state IN ('issued','adopted') AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=NEW.pat_id AND pat.tenant_id=NEW.tenant_id AND pat.token_id=NEW.token_id AND pat.revoked_at_ms IS NULL)
BEGIN SELECT RAISE(ABORT,'invalid credential obligation binding'); END;

CREATE TRIGGER IF NOT EXISTS runner_credential_obligation_binding_insert
BEFORE INSERT ON runner_credential_obligation
WHEN NEW.state IN ('issued','adopted') AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=NEW.pat_id AND pat.tenant_id=NEW.tenant_id AND pat.token_id=NEW.token_id AND pat.revoked_at_ms IS NULL)
BEGIN SELECT RAISE(ABORT,'invalid credential obligation binding'); END;

CREATE TRIGGER IF NOT EXISTS runner_credential_obligation_binding_update
BEFORE UPDATE ON runner_credential_obligation
WHEN NEW.state IN ('issued','adopted') AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=NEW.pat_id AND pat.tenant_id=NEW.tenant_id AND pat.token_id=NEW.token_id AND pat.revoked_at_ms IS NULL)
BEGIN SELECT RAISE(ABORT,'invalid credential obligation binding'); END;

CREATE TRIGGER IF NOT EXISTS tenant_credential_revocation_floor_tenant_immutable
BEFORE UPDATE OF tenant_id ON tenant_credential_revocation_floor
WHEN NEW.tenant_id <> OLD.tenant_id
BEGIN SELECT RAISE(ABORT, 'tenant identity is immutable'); END;

CREATE TRIGGER IF NOT EXISTS tenant_credential_revocation_floor_monotonic_check
BEFORE UPDATE OF revoked_through ON tenant_credential_revocation_floor
WHEN length(NEW.revoked_through) < length(OLD.revoked_through)
  OR (length(NEW.revoked_through) = length(OLD.revoked_through) AND NEW.revoked_through < OLD.revoked_through)
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_revocation_generation_insert_check
BEFORE INSERT ON credential_generation_revocation
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_revocation_generation_update_check
BEFORE UPDATE OF lifecycle_generation ON credential_generation_revocation
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_revocation_state_terminal_check
BEFORE UPDATE OF state ON credential_generation_revocation
WHEN OLD.state = 'revoked' AND NEW.state <> 'revoked'
BEGIN SELECT RAISE(ABORT, 'invalid credential generation state transition'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_revocation_identity_immutable
BEFORE UPDATE OF pat_id, token_id, tenant_id, lifecycle_generation ON credential_generation_revocation
WHEN NEW.pat_id <> OLD.pat_id
  OR NEW.token_id <> OLD.token_id
  OR NEW.tenant_id <> OLD.tenant_id
  OR NEW.lifecycle_generation <> OLD.lifecycle_generation
BEGIN SELECT RAISE(ABORT, 'credential generation identity is immutable'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_event_receipts_generation_insert_check
BEFORE INSERT ON credential_generation_event_receipts
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_event_receipts_generation_update_check
BEFORE UPDATE OF lifecycle_generation ON credential_generation_event_receipts
WHEN length(NEW.lifecycle_generation) = 0
  OR length(NEW.lifecycle_generation) > 19
  OR (length(NEW.lifecycle_generation) = 19 AND NEW.lifecycle_generation > '9223372036854775807')
  OR NEW.lifecycle_generation GLOB '*[^0-9]*'
  OR (length(NEW.lifecycle_generation) > 1 AND substr(NEW.lifecycle_generation, 1, 1) = '0')
BEGIN SELECT RAISE(ABORT, 'invalid lifecycle generation'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_event_receipts_state_terminal_check
BEFORE UPDATE OF state ON credential_generation_event_receipts
WHEN OLD.state = 'complete' AND NEW.state <> 'complete'
BEGIN SELECT RAISE(ABORT, 'invalid credential generation state transition'); END;

CREATE TRIGGER IF NOT EXISTS credential_generation_event_receipts_identity_immutable
BEFORE UPDATE OF event_id, tenant_id, lifecycle_generation ON credential_generation_event_receipts
WHEN NEW.event_id <> OLD.event_id
  OR NEW.tenant_id <> OLD.tenant_id
  OR NEW.lifecycle_generation <> OLD.lifecycle_generation
BEGIN SELECT RAISE(ABORT, 'credential generation receipt identity is immutable'); END;
