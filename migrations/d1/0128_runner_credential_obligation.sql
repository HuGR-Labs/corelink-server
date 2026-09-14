-- Server-owned runner mint handoff. Raw PAT plaintext never enters this table.
CREATE TABLE IF NOT EXISTS runner_credential_obligation (
  operation_id TEXT PRIMARY KEY NOT NULL,
  tenant_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  repo TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('prepared', 'issued', 'adopted', 'revoking', 'revoked')),
  deadline_ms INTEGER NOT NULL CHECK (deadline_ms > 0),
  pat_id TEXT,
  token_id TEXT,
  CHECK ((state = 'prepared' AND pat_id IS NULL AND token_id IS NULL)
      OR state IN ('revoking', 'revoked')
      OR (state IN ('issued', 'adopted') AND pat_id IS NOT NULL AND token_id IS NOT NULL))
);

CREATE TRIGGER IF NOT EXISTS runner_credential_obligation_binding_insert
BEFORE INSERT ON runner_credential_obligation
WHEN NEW.state IN ('issued','adopted') AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=NEW.pat_id AND pat.tenant_id=NEW.tenant_id AND pat.token_id=NEW.token_id AND pat.revoked_at_ms IS NULL)
BEGIN SELECT RAISE(ABORT,'invalid credential obligation binding'); END;

CREATE TRIGGER IF NOT EXISTS runner_credential_obligation_binding_update
BEFORE UPDATE ON runner_credential_obligation
WHEN NEW.state IN ('issued','adopted') AND NOT EXISTS (SELECT 1 FROM pat WHERE pat.pat_id=NEW.pat_id AND pat.tenant_id=NEW.tenant_id AND pat.token_id=NEW.token_id AND pat.revoked_at_ms IS NULL)
BEGIN SELECT RAISE(ABORT,'invalid credential obligation binding'); END;
