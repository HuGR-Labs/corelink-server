-- B-216 / #2166: bounded, operator-only recovery authority for DSR DLQ items.
--
-- This intentionally stores an allowlist, never a serialized queue body.  A
-- redrive reconstructs subject_id from tenant_id and derives erasure salt in
-- the Worker, so Clerk identity, salt material, provider data, credentials,
-- and arbitrary request fields cannot enter the recovery lifecycle.

CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_envelopes (
  event_id TEXT PRIMARY KEY NOT NULL CHECK (
    length(event_id) = 80
    AND substr(event_id, 1, 16) = 'dsr-erasure-dlq:'
    AND substr(event_id, 17) NOT GLOB '*[^0-9a-f]*'
  ),
  dsr_id TEXT NOT NULL CHECK (
    length(dsr_id) = 36 AND dsr_id NOT GLOB '*[^0-9a-f-]*'
  ),
  tenant_id TEXT NOT NULL CHECK (
    length(tenant_id) = 36 AND tenant_id NOT GLOB '*[^0-9a-f-]*'
  ),
  queued_at_ms INTEGER NOT NULL CHECK (queued_at_ms >= 0),
  legal_hold INTEGER NOT NULL CHECK (legal_hold IN (0, 1)),
  requeue_count INTEGER NOT NULL CHECK (requeue_count IN (0, 1)),
  redrive_state TEXT NOT NULL CHECK (redrive_state IN (
    'captured', 'ready', 'closed', 'claimed', 'submitted', 'ambiguous'
  )),
  actor_ref TEXT CHECK (
    actor_ref IS NULL OR (
      length(actor_ref) BETWEEN 4 AND 96
      AND substr(actor_ref, 1, 3) = 'op_'
      AND actor_ref NOT GLOB '*[^A-Za-z0-9._:-]*'
    )
  ),
  approval_ref TEXT CHECK (
    approval_ref IS NULL OR (
      length(approval_ref) BETWEEN 5 AND 96
      AND substr(approval_ref, 1, 4) = 'apr_'
      AND approval_ref NOT GLOB '*[^A-Za-z0-9._:-]*'
    )
  ),
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms >= created_at_ms),
  claim_expires_at_ms INTEGER NOT NULL DEFAULT 0 CHECK (claim_expires_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_dsr_dlq_redrive_expiry
  ON dsr_dlq_redrive_envelopes (expires_at_ms, redrive_state, event_id);

CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_audit (
  event_id TEXT NOT NULL,
  transition TEXT NOT NULL CHECK (transition IN ('claimed', 'submitted', 'ambiguous')),
  actor_ref TEXT NOT NULL CHECK (
    length(actor_ref) BETWEEN 4 AND 96
    AND substr(actor_ref, 1, 3) = 'op_'
    AND actor_ref NOT GLOB '*[^A-Za-z0-9._:-]*'
  ),
  approval_ref TEXT NOT NULL CHECK (
    length(approval_ref) BETWEEN 5 AND 96
    AND substr(approval_ref, 1, 4) = 'apr_'
    AND approval_ref NOT GLOB '*[^A-Za-z0-9._:-]*'
  ),
  occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
  PRIMARY KEY (event_id, transition)
);

CREATE INDEX IF NOT EXISTS idx_dsr_dlq_redrive_audit_expiry
  ON dsr_dlq_redrive_audit (occurred_at_ms, event_id);

-- State and the redacted audit event are one SQLite mutation.  The claim
-- update is therefore the durable audit-before-queue boundary.
CREATE TRIGGER IF NOT EXISTS trg_dsr_dlq_redrive_audit_transition
AFTER UPDATE OF redrive_state ON dsr_dlq_redrive_envelopes
WHEN NEW.redrive_state IN ('claimed', 'submitted', 'ambiguous')
  AND NEW.redrive_state <> OLD.redrive_state
BEGIN
  INSERT INTO dsr_dlq_redrive_audit (
    event_id, transition, actor_ref, approval_ref, occurred_at_ms
  ) VALUES (
    NEW.event_id, NEW.redrive_state, NEW.actor_ref, NEW.approval_ref, NEW.updated_at_ms
  );
END;
