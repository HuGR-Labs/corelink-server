-- Migration 0046: Tenant-level offboarding state machine (R-prep)
--
-- Companion to crates/corelink-tenant-offboarding. Additive only.
--
-- Models the tenant-level lifecycle that complements the individual
-- DSR erasure surface (corelink-dsr / corelink-privacy-erasure-worker):
--
--   ACTIVE → CANCEL_REQUESTED (T+0)
--          → GRACE_PERIOD (T+1..T+30; export window)
--          → READ_ONLY (T+30..T+45; restoration window cap)
--          → SUSPENDED (T+45..T+90; admin-revoke only)
--          → ERASED (T+90+; cryptographic erasure committed)
--
-- The D1 row is the single durable source-of-truth for the
-- `TenantOffboardingOrchestrator` trait. Row lifecycle:
--   - At T+0:  INSERT (state = 'cancel_requested', cancel_requested_at_ms = NOW).
--   - At T+1+: UPDATE state via daily-cron tick + audit row first.
--   - At T+90 / AdminCommitErasure: UPDATE state = 'erased'.
--
-- The audit-chain extension carries the per-transition forensic anchor
-- (canonical 6-event taxonomy lives in audit_chain via S-09; this
-- table is the operational state mirror, not the audit anchor).
--
-- Security / privacy:
--   - Anti-fraud verification of the initiator happens at T+0 in
--     the runbook layer (RB-TENANT-OFFBOARDING.md). The initiator id
--     is captured but the verification evidence (RBAC role, MFA
--     attestation, support agent id) lives in audit_chain.
--   - Final erasure: at T+90 the canonical cascade runs BYOK CMK
--     destroy (when applicable) + R2/D1/KV tombstone-and-purge.
--     This row remains as the tombstone (state = 'erased') for
--     7y per privacy_model.md §2 retention.
--   - The tenant_id is reserved (never re-issued) after the row
--     reaches 'erased' (NOT enforced at this layer; enforced by
--     tenant-provisioning at PRR ship gate).
--
-- Concurrency:
--   - State transitions are serialized per tenant by the daily
--     cron + by the orchestrator's per-instance Arc<Mutex<>>
--     closure (F-001 isolation). D1 enforces single-row update
--     atomicity at the storage layer.

CREATE TABLE IF NOT EXISTS tenant_offboarding_state (
    -- Tenant identifier (ULID). PRIMARY KEY: at most one
    -- offboarding record per tenant. Re-cancel after revert is a
    -- new row (the column constraint is enforced at the
    -- orchestrator boundary, not at the table level, so a runbook
    -- DELETE + INSERT is reachable for ops force-revert from
    -- SUSPENDED → ACTIVE).
    tenant_id                   TEXT    NOT NULL PRIMARY KEY,
    -- Current state. CHECK constraint pins the canonical 6-arm
    -- taxonomy from crates/corelink-tenant-offboarding::state.
    state                       TEXT    NOT NULL CHECK (state IN (
        'active',
        'cancel_requested',
        'grace_period',
        'read_only',
        'suspended',
        'erased'
    )),
    -- T+0 anchor: wall-clock ms-since-epoch at which the canonical
    -- CustomerInitiated row was committed. Used by the daily-cron
    -- tick to compute T+30 / T+45 / T+90 deadlines without
    -- re-reading the audit chain.
    cancel_requested_at_ms      BIGINT  NOT NULL,
    -- Initiator user id (optional; captured for audit forensics on
    -- the T+0 click). NULL when the offboarding was opened via an
    -- ops force-advance path.
    initiator_user_id           TEXT,
    -- Free-form survey reason captured at T+0 (customer
    -- cancellation survey). Capped at 4 KB at the request-handler
    -- layer.
    reason                      TEXT,
    -- Row create / update timestamps.
    created_at_ms               BIGINT  NOT NULL,
    updated_at_ms               BIGINT  NOT NULL
);

-- Forward-progress index for the daily-cron tick: select every
-- tenant whose state ∈ {cancel_requested, grace_period, read_only}
-- AND whose canonical timer threshold has elapsed.
CREATE INDEX IF NOT EXISTS idx_tenant_offboarding_state_cron
    ON tenant_offboarding_state (state, cancel_requested_at_ms);

-- Drill / forensic-replay index: list every tenant ever in a given
-- state (e.g. "show me everything currently SUSPENDED awaiting
-- final erasure").
CREATE INDEX IF NOT EXISTS idx_tenant_offboarding_state_updated
    ON tenant_offboarding_state (state, updated_at_ms);
