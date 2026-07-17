-- Migration: 0092_tier_select_audit_events.sql
-- Durable audit-chain table backing `TierSelectAuditAdapter::emit` (finding L2).
--
-- ## Why
--
-- `POST /v1/onboarding/tier-select` (`routes/tier_select.rs::orchestrate_tier_select`)
-- calls `TierSelectAudit::emit` BEFORE every state mutation and treats an `Err`
-- as an ABORT (fail-CLOSED; INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER — see
-- `routes/tier_select.rs:794-798`). The production adapter
-- (`routes/tier_select_audit.rs::TierSelectAuditAdapter`) previously only
-- emitted a `tracing::info!` and unconditionally returned `Ok(())`, so the
-- abort-before-mutation wiring — already proven sound against a `SpyAudit`
-- fake in the orchestration unit tests — could never actually fire in prod:
-- a durable-store outage would silently degrade to log-only. This table is
-- the real durable backing so a failed insert propagates as `Err` and aborts
-- the operation, exactly like `customer_d1.rs::insert_audit_event` (migration
-- 0077) already does for the customer-facing audit surface.
--
-- ## Deliberately SEPARATE from `customer_audit_events` (0077)
--
-- `customer_audit_events` feeds the customer-facing `GET /v1/customer/audit`
-- dashboard (best-effort / fail-OPEN writer). Tier-select's audit events
-- (`tier_select_attempted`, `dpa_first_violation_attempt`,
-- `stripe_checkout_session_created`, `tier_activated_free`) are INTERNAL
-- money-path audit-chain entries, fail-CLOSED, and must never leak onto the
-- customer dashboard — hence a dedicated table rather than a shared row shape.
--
-- ## Read / write contract
--
--   write — `tier_select_audit.rs::TierSelectAuditAdapter::emit` inserts one
--           row per call, fail-CLOSED (`Err` propagates and ABORTS the caller
--           BEFORE any state mutation).
--   read  — none yet (operator/ops-console read surface is a future WI); the
--           `idx_tier_select_audit_events_tenant_ts` index is pre-built for
--           the eventual "recent tier-select activity for tenant" query, the
--           same shape as `customer_audit_events`'s dominant query.
--
-- ## Additive policy
--
-- Purely additive (CREATE TABLE / CREATE INDEX IF NOT EXISTS; no DROP, no
-- ALTER of existing tables) — INV-AUTH-MIGRATION-ADDITIVE + INV-AUDIT-APPEND-ONLY.
-- Safe to replay idempotently.

CREATE TABLE IF NOT EXISTS tier_select_audit_events (
    -- Surrogate key for the audit row (D1 INTEGER PRIMARY KEY = rowid alias).
    id              INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Tenant the event belongs to (the edge-verified `x-corelink-tenant-id`
    -- the orchestration passes to `emit` — never re-derived/defaulted here).
    tenant_id       TEXT    NOT NULL,

    -- One of the orchestration's static event labels (`tier_select_attempted`,
    -- `dpa_first_violation_attempt`, `stripe_checkout_session_created`,
    -- `tier_activated_free`).
    event_type      TEXT    NOT NULL,

    -- The orchestration's per-request correlation id (ties this row to the
    -- structured `tracing` log line emitted alongside it).
    correlation_id  TEXT    NOT NULL,

    -- Event instant (Unix epoch ms), wall-clock at emit time.
    ts_ms           BIGINT  NOT NULL
);

-- Pre-built for the eventual "recent tier-select activity for tenant" read
-- surface — same shape as `customer_audit_events`'s dominant query.
CREATE INDEX IF NOT EXISTS idx_tier_select_audit_events_tenant_ts
    ON tier_select_audit_events (tenant_id, ts_ms DESC);
