-- Durable worker receipts for authenticated lifecycle-generation close events.
CREATE TABLE IF NOT EXISTS credential_generation_event_receipts (
  event_id TEXT PRIMARY KEY NOT NULL,
  tenant_id TEXT NOT NULL,
  lifecycle_generation TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('requested', 'complete'))
);

CREATE INDEX IF NOT EXISTS idx_credential_generation_event_receipts_tenant
  ON credential_generation_event_receipts (tenant_id, lifecycle_generation, event_id);
