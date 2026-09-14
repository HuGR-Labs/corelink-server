-- Durable worker receipts for authenticated lifecycle-generation close events.
CREATE TABLE IF NOT EXISTS credential_generation_event_receipts (
  event_id TEXT PRIMARY KEY NOT NULL,
  tenant_id TEXT NOT NULL,
  lifecycle_generation TEXT NOT NULL CHECK (
    length(lifecycle_generation) > 0
    AND length(lifecycle_generation) <= 19
    AND lifecycle_generation NOT GLOB '*[^0-9]*'
    AND (length(lifecycle_generation) = 1 OR substr(lifecycle_generation, 1, 1) <> '0')
    AND (length(lifecycle_generation) < 19 OR lifecycle_generation <= '9223372036854775807')
  ),
  state TEXT NOT NULL CHECK (state IN ('requested', 'complete'))
);

CREATE INDEX IF NOT EXISTS idx_credential_generation_event_receipts_tenant
  ON credential_generation_event_receipts (tenant_id, lifecycle_generation, event_id);
