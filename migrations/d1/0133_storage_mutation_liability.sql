CREATE TABLE IF NOT EXISTS storage_mutation_liability (
  tenant_id TEXT NOT NULL, region TEXT NOT NULL,
  surface TEXT NOT NULL CHECK (surface = 'cas'), logical_key TEXT NOT NULL,
  bytes_reserved INTEGER NOT NULL CHECK (bytes_reserved >= 0),
  state           TEXT    NOT NULL CHECK (
    state IN ('reserved', 'committed', 'pending', 'unknown', 'released')
  ),
  intent_id TEXT NULL, attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (tenant_id, region, surface, logical_key),
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),
  CHECK (
    (state IN ('pending', 'unknown') AND intent_id IS NOT NULL)
    OR (state NOT IN ('pending', 'unknown') AND intent_id IS NULL)
  ),
  CHECK (updated_at_ms >= created_at_ms)
);

CREATE INDEX IF NOT EXISTS idx_storage_mutation_liability_active
  ON storage_mutation_liability (state, updated_at_ms);
