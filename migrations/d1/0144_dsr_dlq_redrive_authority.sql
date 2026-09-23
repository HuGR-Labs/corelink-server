CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_envelopes (
  event_id TEXT PRIMARY KEY NOT NULL CHECK(length(event_id) = 80 AND substr(event_id, 1, 16) = 'dsr-erasure-dlq:'),
  dsr_id TEXT NOT NULL, tenant_id TEXT NOT NULL, queued_at_ms INTEGER NOT NULL CHECK(queued_at_ms >= 0),
  legal_hold INTEGER NOT NULL CHECK(legal_hold IN (0, 1)), requeue_count INTEGER NOT NULL CHECK(requeue_count IN (0, 1)),
  state TEXT NOT NULL CHECK(state IN ('ready', 'claimed', 'submitted', 'ambiguous')),
  actor_ref TEXT, approval_ref TEXT, expires_at_ms INTEGER NOT NULL, claim_expires_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_audit (
  event_id TEXT NOT NULL, transition TEXT NOT NULL CHECK(transition IN ('claimed', 'submitted', 'ambiguous')),
  actor_ref TEXT NOT NULL, approval_ref TEXT NOT NULL, occurred_at_ms INTEGER NOT NULL, PRIMARY KEY(event_id, transition)
);
CREATE TRIGGER IF NOT EXISTS dsr_dlq_redrive_audit_transition AFTER UPDATE OF state ON dsr_dlq_redrive_envelopes
WHEN NEW.state IN ('claimed', 'submitted', 'ambiguous') AND NEW.state <> OLD.state
BEGIN INSERT INTO dsr_dlq_redrive_audit VALUES(NEW.event_id, NEW.state, NEW.actor_ref, NEW.approval_ref, NEW.updated_at_ms); END;
