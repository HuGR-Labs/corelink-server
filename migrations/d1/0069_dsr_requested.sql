-- Migration 0069: dsr_requested — durable "DSR requested at enqueue" record (WI-S11-008 G4).
--
-- ## Why
--
-- The 24h verification sweep (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts`)
-- enumerates candidate DSRs from `dsr_erasure_log`, which only carries a row once
-- a backend has written a tombstone. A DSR that FAILS BEFORE any tombstone
-- (audit-emit fail-closed on backend 1 — ADR-S11-002) therefore has NO row in
-- `dsr_erasure_log`, so its SLA breach goes completely UNDETECTED.
--
-- This table is the missing durable anchor: the Clerk `user.deleted` webhook
-- writes a `requested` row at ENQUEUE time (before the orchestrator runs at all),
-- so the sweep can enumerate requested-but-unverified DSRs and surface a breach
-- even when the erasure never produced a single tombstone.
--
-- ## Idempotency
--
-- `dsr_id` is the deterministic name-based UUID (`deterministicDsrId`), stable
-- across Svix redeliveries — so the enqueue write is `INSERT OR IGNORE` and a
-- redelivered `user.deleted` never creates a duplicate request row.
--
-- ## Privacy
--
-- `tenant_id` is not subject PII; `dsr_id` is a pseudonymous id. No raw subject
-- id / email / salt is stored. This row is part of the RETAIN-set (the erasure
-- *record itself* survives the erasure — proof the obligation was honored), so
-- the D1 erase adapter MUST NOT delete it (ADR-S11-013).

CREATE TABLE IF NOT EXISTS dsr_requested (
    -- Deterministic name-based UUID for the DSR (idempotency key).
    dsr_id        TEXT    NOT NULL PRIMARY KEY,
    -- Tenant being erased (not subject PII).
    tenant_id     TEXT    NOT NULL,
    -- Enqueue instant (Unix epoch ms) — the SLA-clock anchor for the sweep.
    requested_at  INTEGER NOT NULL,
    -- Lifecycle: 'requested' at enqueue; flipped to 'verified' once the 24h
    -- sweep confirms a VerifiedComplete (so the sweep stops re-enumerating it).
    status        TEXT    NOT NULL DEFAULT 'requested'
                          CHECK (status IN ('requested', 'verified'))
);

-- The sweep filters on (status, requested_at): "requested rows past the 24h
-- deadline, within the look-back window".
CREATE INDEX IF NOT EXISTS idx_dsr_requested_status_requested_at
    ON dsr_requested (status, requested_at);
