-- D1 migration N+4: sub_processor_broadcast_log + sub_processor_objection
-- WI-S11-005 — Sub-Processor Register + 30d Email Broadcast + Objection Flow
-- See: specs/04_sprints/S11/work_items/WI-S11-005-sub-processor-register-30d-broadcast-objection-flow.md §6.1.7

-- ─────────────────────────────────────────────────────────────────────────────
-- Table: sub_processor_broadcast_log
-- Tracks per-recipient delivery confirmation for 30d broadcast emails.
-- UNIQUE constraint INV-SUB-PROCESSOR-BROADCAST-IDEMPOTENT.
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE sub_processor_broadcast_log (
  log_id                    TEXT        PRIMARY KEY,                  -- ULID 26 chars
  broadcast_id              TEXT        NOT NULL,                     -- ULID per broadcast event (groups all recipients)
  sub_processors_version    TEXT        NOT NULL,                     -- semver of sub_processors.md
  tenant_id                 TEXT        NOT NULL,
  recipient_email_hash      TEXT        NOT NULL,                     -- sha256 (CTRL-PRIV-014; raw email NEVER in DB)
  locale                    TEXT        NOT NULL CHECK (locale IN ('pt-BR','en-US','es-MX')),
  notification_type         TEXT        NOT NULL CHECK (notification_type IN ('30d_advance_notice','objection_confirmation','final_decision')),
  enqueued_at               TEXT        NOT NULL,                     -- ISO 8601 UTC
  delivered_at              TEXT        NULL,                         -- set via Cloudflare Email webhook
  delivery_status           TEXT        NOT NULL DEFAULT 'enqueued' CHECK (delivery_status IN ('enqueued','sent','delivered','bounced','complained','failed')),
  delivery_error_class      TEXT        NULL,                         -- error class if bounced/failed
  -- idempotency UNIQUE (INV-SUB-PROCESSOR-BROADCAST-IDEMPOTENT)
  UNIQUE (broadcast_id, tenant_id, recipient_email_hash, notification_type)
);

CREATE INDEX idx_sub_processor_broadcast_log_status
  ON sub_processor_broadcast_log(delivery_status, enqueued_at)
  WHERE delivery_status IN ('enqueued', 'sent');

CREATE INDEX idx_sub_processor_broadcast_log_tenant
  ON sub_processor_broadcast_log(tenant_id, enqueued_at DESC);

CREATE INDEX idx_sub_processor_broadcast_log_broadcast
  ON sub_processor_broadcast_log(broadcast_id, delivery_status);

-- ─────────────────────────────────────────────────────────────────────────────
-- Table: sub_processor_objection
-- Tracks customer objections filed via POST /v1/privacy/sub-processor-objection.
-- UNIQUE constraint prevents duplicate objections per (tenant, subject, sp, version).
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE sub_processor_objection (
  objection_id              TEXT        PRIMARY KEY,                  -- ULID 26 chars
  tenant_id                 TEXT        NOT NULL,
  subject_id                TEXT        NOT NULL,
  subject_id_hash           TEXT        NOT NULL,                     -- sha256 (CTRL-PRIV-014)
  sub_processor_id          TEXT        NOT NULL,                     -- e.g., "sentry"
  sub_processors_version    TEXT        NOT NULL,                     -- semver context
  objection_reason          TEXT        NOT NULL,                     -- customer-provided rationale
  proposed_alternative      TEXT        NULL,                         -- customer-proposed workaround
  ticket_status             TEXT        NOT NULL DEFAULT 'pending' CHECK (ticket_status IN ('pending','in_review','accepted','terminated','withdrawn')),
  resolution_decision       TEXT        NULL CHECK (resolution_decision IS NULL OR resolution_decision IN ('workaround_offered','accepted','terminated')),
  resolution_note           TEXT        NULL,                         -- Privacy Officer + Legal rationale
  filed_at                  TEXT        NOT NULL,                     -- ISO 8601 UTC
  resolved_at               TEXT        NULL,
  expected_resolution_at    TEXT        NOT NULL,                     -- filed_at + 14 calendar days
  -- idempotency UNIQUE: one objection per (tenant, subject, sub-processor, version)
  UNIQUE (tenant_id, subject_id, sub_processor_id, sub_processors_version)
);

CREATE INDEX idx_sub_processor_objection_status
  ON sub_processor_objection(ticket_status, expected_resolution_at)
  WHERE ticket_status IN ('pending', 'in_review');

CREATE INDEX idx_sub_processor_objection_tenant
  ON sub_processor_objection(tenant_id, filed_at DESC);

-- ─────────────────────────────────────────────────────────────────────────────
-- Rollback:
-- DROP INDEX IF EXISTS idx_sub_processor_objection_tenant;
-- DROP INDEX IF EXISTS idx_sub_processor_objection_status;
-- DROP TABLE IF EXISTS sub_processor_objection;
-- DROP INDEX IF EXISTS idx_sub_processor_broadcast_log_broadcast;
-- DROP INDEX IF EXISTS idx_sub_processor_broadcast_log_tenant;
-- DROP INDEX IF EXISTS idx_sub_processor_broadcast_log_status;
-- DROP TABLE IF EXISTS sub_processor_broadcast_log;
-- ─────────────────────────────────────────────────────────────────────────────
