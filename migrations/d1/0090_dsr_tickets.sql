-- 0090_dsr_tickets.sql — customer-facing DSR self-service ticket store.
--
-- Backs the `/v1/privacy/dsr/*` customer portal (crates/corelink-container/
-- src/routes/dsr/portal.rs). One row per submitted data-subject-rights request
-- (Access / Portability / Rectification / Erasure / Restriction / Objection).
-- The DATA operation itself is driven by the LIVE Wave-1 pipeline
-- (routes/dsr/access.rs run_access/portability/rectification + the erase path);
-- this table is the durable, tenant-scoped STATUS record the customer polls
-- (GET /v1/privacy/dsr/{request_id}/status) plus the anti-abuse rate-limit
-- window (10/day per LGPD Art.20 humane cap).
--
-- Tenant isolation is STRUCTURAL: the primary key is tenant-leftmost
-- (tenant_id, request_id), so a cross-tenant status read is impossible by
-- construction — a lookup binds BOTH the Worker-resolved tenant_id AND the
-- request_id (mirrors the corelink-dsr `(tenant_id, request_id)` store key).
--
-- Retention: this is a DSR compliance-evidence record (proof CoreLink received
-- + honoured the request). It carries a lawful retention basis (demonstrate
-- Art.5(2) accountability) exactly like `dsr_requested` / `dsr_erasure_log`, so
-- it is classified into the DSR **RETAIN_SET** (never the erase-set) in
-- crates/corelink-container/src/routes/dsr/adapter_d1.rs — it SURVIVES an Art.17
-- erasure. It stores NO raw PII: the subject is identified by the tenant_id, and
-- any rectified value is hashed by the live pipeline before it touches D1.
--
-- Additive-only: CREATE TABLE IF NOT EXISTS (no destructive change).

CREATE TABLE IF NOT EXISTS dsr_tickets (
  tenant_id          TEXT    NOT NULL,
  request_id         TEXT    NOT NULL,
  -- access | portability | rectification | erasure | restriction | objection
  action             TEXT    NOT NULL,
  -- pending | in_progress | completed | rejected
  status             TEXT    NOT NULL,
  -- lgpd | gdpr | ccpa
  jurisdiction       TEXT    NOT NULL,
  reason             TEXT,
  mfa_required       INTEGER NOT NULL DEFAULT 0,
  submitted_at_ms    INTEGER NOT NULL,
  sla_deadline_ms    INTEGER NOT NULL,
  -- HS256 proof-of-submission receipt (customer-held; anti-replay 90d exp).
  receipt            TEXT    NOT NULL,
  -- Set once an Access/Portability export is durably persisted (R2 key handle).
  data_download_url  TEXT,
  -- JSON array of {at_ms, from, to, note} status-transition events.
  timeline           TEXT    NOT NULL DEFAULT '[]',
  updated_at_ms      INTEGER NOT NULL,
  PRIMARY KEY (tenant_id, request_id)
);

-- List-my-requests (newest first) + the rolling 24h rate-limit COUNT are both
-- tenant-leftmost, submitted_at-ordered scans.
CREATE INDEX IF NOT EXISTS idx_dsr_tickets_tenant_submitted
  ON dsr_tickets (tenant_id, submitted_at_ms DESC);
