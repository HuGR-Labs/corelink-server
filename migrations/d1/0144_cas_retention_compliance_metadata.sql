-- 0144: widen cas_retention with a SQLite/D1 table rebuild.
--
-- The deployed 0102 inline CHECK only permits Governance. SQLite cannot widen
-- that constraint in place. This one-way migration preserves every legacy
-- primary key and tenant-leftmost index while adding metadata required to bind
-- a Compliance receipt to its provider account, target, exact S3 version, and
-- durable evidence. The migration runner ledger applies a numbered file once;
-- application writers remain replay-safe through INSERT OR IGNORE on the
-- unchanged primary key.
--
-- Governance stays code-reversible and must never carry Compliance metadata.
-- Compliance rows never enter the R2 drain/release path. Rolling back after a
-- Compliance row exists is unsafe: leave this schema in place and forward-fix.

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
  CHECK (
    (mode = 'governance'
      AND provider_account_ref IS NULL
      AND archive_target_ref IS NULL
      AND object_version IS NULL
      AND evidence_reference IS NULL)
    OR
    (mode = 'compliance'
      AND retain_until_ms IS NOT NULL
      AND provider_account_ref IS NOT NULL
      AND archive_target_ref IS NOT NULL
      AND object_version IS NOT NULL
      AND evidence_reference IS NOT NULL)
  ),
  PRIMARY KEY (tenant_id, region, object_key)
);

INSERT INTO cas_retention_next (
  tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms
)
SELECT tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms
FROM cas_retention;

DROP TABLE cas_retention; -- additive-allowed: ADR-0104 SQLite rebuild widens the deployed mode CHECK
ALTER TABLE cas_retention_next RENAME TO cas_retention; -- additive-allowed: ADR-0104 restore canonical table after the audited rebuild

CREATE INDEX idx_cas_retention_tenant_expiry
  ON cas_retention (tenant_id, retain_until_ms);
CREATE INDEX idx_cas_retention_tenant_mode
  ON cas_retention (tenant_id, mode, region);
