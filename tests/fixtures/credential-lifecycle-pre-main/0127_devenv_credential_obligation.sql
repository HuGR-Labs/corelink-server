-- DevEnv server-side handoff obligation. Raw PAT plaintext never enters this table.
-- Tenant identity is verified before prepare; the canonical pat FK gates activation.
-- Tombstones must not acquire a new FK that blocks the existing tenant erasure flow.
CREATE TABLE IF NOT EXISTS devenv_credential_obligation (
  operation_id TEXT PRIMARY KEY NOT NULL,
  tenant_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('prepared', 'issued', 'revoking', 'revoked', 'adopted')),
  deadline_ms INTEGER NOT NULL CHECK (deadline_ms > 0),
  pat_id TEXT,
  token_id TEXT,
  CHECK ((state = 'prepared' AND pat_id IS NULL AND token_id IS NULL)
      OR state IN ('revoking', 'revoked')
      OR (state IN ('issued', 'adopted') AND pat_id IS NOT NULL AND token_id IS NOT NULL))
);

-- Terminal rows are idempotency tombstones: deleting one could permit a late
-- prepare/activation to recreate an obligation after its alarm was retired.
