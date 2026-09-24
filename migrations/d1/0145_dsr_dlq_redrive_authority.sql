CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_envelopes (
  event_id TEXT PRIMARY KEY NOT NULL,
  dsr_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  queued_at_ms INTEGER NOT NULL CHECK (queued_at_ms >= 0),
  legal_hold INTEGER NOT NULL CHECK (legal_hold IN (0, 1)),
  requeue_count INTEGER NOT NULL CHECK (requeue_count >= 0),
  state TEXT NOT NULL CHECK (state IN ('ready', 'claimed', 'submitted', 'ambiguous')),
  actor_ref TEXT NOT NULL,
  approval_ref TEXT NOT NULL,
  expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms >= 0),
  claim_expires_at_ms INTEGER CHECK (claim_expires_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_audit (
  event_id TEXT NOT NULL,
  transition TEXT NOT NULL CHECK (transition IN ('claimed', 'submitted', 'ambiguous')),
  PRIMARY KEY (event_id, transition)
);
