-- 0102_cas_retention.sql — Governance-mode legal-hold retention index for CAS
-- content-addressed objects (B-009 / ADR-S11-013 §R2-erasure legal-hold arm).
--
-- WHY
-- ---
-- When an Art.17 / LGPD Art.18 erasure lands on a tenant whose DSR ticket is
-- under an ACTIVE legal hold (litigation / retention freeze), the CAS
-- legal-hold backend (BackendKind::R2CasLegalHold, adapter
-- crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs) MUST
-- NOT delete the content-addressed bytes — they are frozen for the duration of
-- the hold. Instead it SEVERS the subject→object PII linkage (the frozen bytes
-- are no longer attributable to the data subject) and records, per surviving
-- object, that the object is being retained under a Governance-mode hold so a
-- later drain can finish the physical erasure once the hold is released.
--
-- This table IS that retention index. One row per (tenant_id, region,
-- object_key) CAS object held under governance. It carries NO subject_id by
-- construction — recording the object here anonymously is HOW the PII linkage
-- is severed (GDPR Recital 26 pseudonymization: the bytes remain, the subject
-- reference does not).
--
-- GOVERNANCE MODE IS CODE-REVERSIBLE — NOT storage-level immutability.
-- ------------------------------------------------------------------
-- `mode = 'governance'` means the retention is enforced by CoreLink's own
-- application/adapter logic, and an authorised hold-release path can delete the
-- retained bytes after the hold ends (the drain reads THIS table). It does NOT
-- assert R2 Object Lock / S3 Object-Lock COMPLIANCE mode or any append-only
-- storage-primitive guarantee — no such immutability is configured on the CAS
-- bucket. Compliance / Object-Lock mode (storage-enforced, irreversible for the
-- retention window) is explicitly DEFERRED; adding it later is a NEW `mode`
-- value, which — because `mode` is an INLINE CHECK constraint — SQLite/D1 cannot
-- widen in place. Per INV-AUTH-MIGRATION-ADDITIVE + the SQLite "12-step" table
-- rebuild (see 0098_widen_erasure_region_check_apac.sql), admitting a
-- 'compliance' mode later is a table-REBUILD migration, not an ALTER — plan for
-- it, do not ALTER this CHECK.
--
-- Columns
-- -------
--   tenant_id           — CoreLink tenant UUID (S-03 inheritance; never a body).
--   region              — CAS storage region prefix ('sam'|'iad'|'lhr'|'nrt'|
--                         'syd'); the object key is region-leftmost.
--   object_key          — full R2 key `<region>/<tenant_prefix>/<digest>`; the
--                         drain deletes exactly this key on hold release.
--   retain_until_ms     — best-known hold expiry (Unix epoch ms), or NULL when
--                         the release date is open-ended (hold released by an
--                         explicit admin action, not a timer).
--   mode                — retention mode; ONLY 'governance' today (see above).
--   pseudonymized_at_ms — when the subject→object linkage was severed and this
--                         retention row written (Unix epoch ms).
--
-- Retention drain: a later hold-release path selects rows by (tenant_id) /
-- (retain_until_ms <= now) and deletes the referenced CAS objects, then removes
-- the row — reversibility is by construction (the object key is preserved here).
--
-- Additive-only: CREATE TABLE IF NOT EXISTS (no destructive change;
-- INV-AUTH-MIGRATION-ADDITIVE clean).

CREATE TABLE IF NOT EXISTS cas_retention (
  tenant_id           TEXT    NOT NULL,
  region              TEXT    NOT NULL,
  object_key          TEXT    NOT NULL,
  retain_until_ms     INTEGER,
  mode                TEXT    NOT NULL CHECK (mode IN ('governance')),
  pseudonymized_at_ms INTEGER NOT NULL,
  PRIMARY KEY (tenant_id, region, object_key)
);

-- Drain scan: "everything this tenant is holding" and the timer-expiry sweep
-- are both tenant-leftmost.
CREATE INDEX IF NOT EXISTS idx_cas_retention_tenant_expiry
  ON cas_retention (tenant_id, retain_until_ms);
