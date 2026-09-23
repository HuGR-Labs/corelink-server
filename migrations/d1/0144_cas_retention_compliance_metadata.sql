-- 0144: widen the B-009 retention mode through the required SQLite rebuild.
-- Governance rows retain their exact primary key and reversible behavior.
-- Compliance rows are metadata only; no drain may delete or reroute them.
CREATE TABLE cas_retention_next (
  tenant_id TEXT NOT NULL,
  region TEXT NOT NULL,
  object_key TEXT NOT NULL,
  retain_until_ms INTEGER,
  mode TEXT NOT NULL CHECK (mode IN ('governance', 'compliance')),
  pseudonymized_at_ms INTEGER NOT NULL,
  provider_account_ref TEXT,
  archive_target_ref TEXT,
  object_version TEXT,
  evidence_reference TEXT,
  CHECK ((mode = 'governance' AND provider_account_ref IS NULL AND archive_target_ref IS NULL AND object_version IS NULL AND evidence_reference IS NULL) OR (mode = 'compliance' AND retain_until_ms IS NOT NULL AND provider_account_ref IS NOT NULL AND archive_target_ref IS NOT NULL AND object_version IS NOT NULL AND evidence_reference IS NOT NULL)),
  PRIMARY KEY (tenant_id, region, object_key)
);
INSERT INTO cas_retention_next (tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms)
  SELECT tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms FROM cas_retention;
DROP TABLE cas_retention; -- additive-allowed: ADR-0104 SQLite rebuild required to widen the deployed CHECK safely
ALTER TABLE cas_retention_next RENAME TO cas_retention; -- additive-allowed: ADR-0104 SQLite rebuild restores the canonical table name
CREATE INDEX idx_cas_retention_tenant_expiry ON cas_retention (tenant_id, retain_until_ms);
CREATE INDEX idx_cas_retention_tenant_mode ON cas_retention (tenant_id, mode, region);
