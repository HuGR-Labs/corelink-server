-- Migration 0095: durable billing-audit evidence rows (MED-5 closure).
--
-- # Why this table exists
--
-- The container's Stripe-webhook materializer emits one billing-audit
-- record BEFORE every state mutation (`BillingAuditEmitter`,
-- audit-before-mutation, fail-CLOSED). Until now the only native
-- implementation was `InMemoryBillingAuditEmitter` — the D1 billing
-- STATE rows were durable while their audit EVIDENCE evaporated on
-- container restart. This table gives the native path a durable sink
-- with the same row shape the wasm32 production binder archives to the
-- audit chain.
--
-- One row per `emit_billing` call. Append-only: nothing in this repo
-- UPDATEs or DELETEs from this table (INV-AUDIT-APPEND-ONLY); retention
-- is an operator concern (D1 time-travel + future TTL job).
--
-- # Idempotent / additive
--
-- `IF NOT EXISTS`; INV-AUTH-MIGRATION-ADDITIVE holds;
-- `scripts/check_migrations_additive.py` exits 0.

CREATE TABLE IF NOT EXISTS stripe_billing_audit_events (
    -- Surrogate key (D1 INTEGER PRIMARY KEY = rowid alias).
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Canonical event name
    -- (`corelink.billing.*.materialized.v1` /
    -- `corelink.tenant.tier_changed.v1` / `corelink.billing.echo.v1`).
    event_name         TEXT    NOT NULL,

    -- Stripe event id this audit row was derived from (`evt_…`).
    stripe_event_id    TEXT    NOT NULL,

    -- Canonical Stripe event-type label (e.g. `invoice.paid`).
    stripe_event_type  TEXT    NOT NULL,

    -- Tenant id (mirrors `tier_selections.tenant_id`).
    tenant_id          TEXT    NOT NULL,

    -- Optional Stripe object id (`sub_…`, `in_…`, `cus_…`, …).
    stripe_object_id   TEXT,

    -- Severity bucket: 'info' | 'notice' | 'sev1'
    -- ('sev1' reserved for `charge.dispute.created`).
    severity           TEXT    NOT NULL
        CHECK (severity IN ('info', 'notice', 'sev1')),

    -- Wall-clock ms when the audit row was assembled.
    ts_ms              BIGINT  NOT NULL,

    -- Free-form JSON payload (the canonical column set per event name),
    -- serialized by the application layer.
    payload_json       TEXT    NOT NULL
);

-- Operator triage: all evidence for one webhook delivery.
CREATE INDEX IF NOT EXISTS idx_sbae_stripe_event
    ON stripe_billing_audit_events (stripe_event_id);

-- Tenant-facing surfaces: newest-first per tenant.
CREATE INDEX IF NOT EXISTS idx_sbae_tenant_ts
    ON stripe_billing_audit_events (tenant_id, ts_ms);
