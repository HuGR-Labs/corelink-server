-- Migration 0077: customer_audit_events — durable customer-facing audit log.
--
-- ## Why
--
-- `GET /v1/customer/audit` (`routes/customer.rs::handle_audit` →
-- `customer_d1.rs::CustomerAuditHandler::query`) shipped as an honest v1 stub
-- returning `{"rows":[]}` because no customer-queryable audit table was
-- deployed: the canonical security chain lives in the R2 NDJSON archive
-- (operator-facing, served by the export route), with no per-tenant surface a
-- customer can read from their dashboard. This table is that backing — a small,
-- customer-facing activity log keyed by `tenant_id`, written best-effort from
-- the control-plane mutations (PAT create, team invite) and read newest-first
-- by the audit-query handler.
--
-- It is deliberately SEPARATE from `export_audit_log` (0049, operator/analyst
-- facing) and the R2 chain: this surface holds only the coarse, customer-safe
-- "what happened on my account" events, never raw PII (e.g. an invite stores
-- the invitation id + role, never the raw invitee email — CTRL-PRIV-001).
--
-- ## Read / write contract
--
--   write — `customer_d1.rs` inserts one row per committed control-plane op
--           (event_type `pat.created` / `team.invited`), best-effort / fail-OPEN
--           (a failed audit insert NEVER blocks the primary op).
--   read  — `SELECT … WHERE tenant_id = ?1 ORDER BY ts_ms DESC LIMIT ?` →
--           `CustomerAuditEventRow { event_id=id, ts=ISO8601(ts_ms),
--           event_type, severity="info", actor, summary=detail }`.
--
-- ## Additive policy
--
-- Purely additive (CREATE TABLE / CREATE INDEX IF NOT EXISTS; no DROP, no ALTER
-- of existing tables) — INV-AUTH-MIGRATION-ADDITIVE + INV-AUDIT-APPEND-ONLY.
-- Safe to replay idempotently.

CREATE TABLE IF NOT EXISTS customer_audit_events (
    -- Surrogate key for the audit row (D1 INTEGER PRIMARY KEY = rowid alias).
    -- Surfaced verbatim as `CustomerAuditEventRow.event_id`.
    id         INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Tenant the event belongs to (matches `tenant.tenant_id`). Every read is
    -- `WHERE tenant_id = ?` — the tenant-isolation boundary (INV-TENANT-ISOLATION).
    tenant_id  TEXT    NOT NULL,

    -- Dotted event type (`pat.created`, `team.invited`, …).
    event_type TEXT    NOT NULL,

    -- Principal that performed the action (the dashboard caller principal).
    actor      TEXT,

    -- Affected resource id (PAT id / invitation id). NEVER raw PII.
    target     TEXT,

    -- Event instant (Unix epoch ms). Ordering key + rendered to ISO-8601 `ts`.
    ts_ms      BIGINT  NOT NULL,

    -- Human-readable, customer-safe summary (surfaced as `summary`). No raw PII.
    detail     TEXT
);

-- The dominant query shape: a tenant's most-recent activity, newest first.
CREATE INDEX IF NOT EXISTS idx_customer_audit_events_tenant_ts
    ON customer_audit_events (tenant_id, ts_ms DESC);
