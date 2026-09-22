CREATE TABLE IF NOT EXISTS usage_event_staging_conflicts (
  tenant_id TEXT NOT NULL,
  request_id TEXT NOT NULL,
  observed_fingerprint TEXT NOT NULL,
  incoming_fingerprint TEXT NOT NULL,
  reason TEXT NOT NULL CHECK (reason IN ('payload_mismatch', 'existing_fingerprint_unverifiable')),
  first_observed_at INTEGER NOT NULL CHECK (first_observed_at >= 0),
  last_observed_at INTEGER NOT NULL CHECK (last_observed_at >= first_observed_at),
  observation_count INTEGER NOT NULL CHECK (observation_count >= 1),
  PRIMARY KEY (tenant_id, request_id, observed_fingerprint, incoming_fingerprint, reason)
);
CREATE INDEX IF NOT EXISTS idx_usage_event_staging_conflicts_coordinate
  ON usage_event_staging_conflicts (tenant_id, request_id, last_observed_at);
