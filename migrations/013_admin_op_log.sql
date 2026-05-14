-- Migration: 013_admin_op_log.sql (WI-S13-002)
-- Admin op log + collusion-rotation index + nonce replay UNIQUE constraint.
-- Extends WI-S13-001 audit foundation.
--
-- INV-AUTH-MIGRATION-ADDITIVE: additive only — no DROP.

CREATE TABLE IF NOT EXISTS admin_op_log (
    op_id           BLOB(16)  PRIMARY KEY,                            -- UUID (WI primary key)
    op_type         TEXT      NOT NULL,
    tenant_id       BLOB(16)  NOT NULL,                               -- Tenant scope for collusion oracle
    caller_user_id  BLOB(16)  NOT NULL,
    approver_user_id BLOB(16),                                        -- NULL only for non-destructive ops
    op_payload_hash BLOB(32)  NOT NULL,                               -- SHA-256 of op_payload (never stores raw payload)
    prev_state_hash BLOB(32)  NOT NULL,
    mfa_ts_ms       BIGINT    NOT NULL,
    nonce           BLOB(16)  NOT NULL,
    ts_ms           BIGINT    NOT NULL,
    outcome         TEXT      NOT NULL
        CHECK (outcome IN (
            'approved',
            'denied_missing',
            'denied_sig',
            'denied_caller_eq',
            'denied_collusion',
            'denied_mfa_stale',
            'denied_approver_not_admin',
            'denied_nonce_replay',
            'denied_clock_skew'
        )),
    created_at_ms   BIGINT    NOT NULL DEFAULT (unixepoch('subsec') * 1000),

    -- Replay protection: same caller cannot reuse a nonce.
    UNIQUE (caller_user_id, nonce)
);

-- Index for collusion-rotation oracle:
-- SELECT DISTINCT approver_user_id FROM admin_op_log
--   WHERE tenant_id=$t AND op_type IN (destructive) AND outcome='approved'
--     AND created_at_ms > $now - 86400000
-- ORDER BY MAX(created_at_ms) DESC LIMIT 2
CREATE INDEX IF NOT EXISTS idx_admin_op_log_collusion
    ON admin_op_log (tenant_id, op_type, created_at_ms DESC)
    WHERE outcome = 'approved';

-- Caller fast-lookup for nonce replay check.
CREATE INDEX IF NOT EXISTS idx_admin_op_log_caller_nonce
    ON admin_op_log (caller_user_id, nonce);
