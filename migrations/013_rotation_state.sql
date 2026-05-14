-- WI-S13-003: rotation_state table + UNIQUE index (per §6.1.3)
--
-- Tracks the lifecycle state of each secret key per (asset_class, region).
-- D1 UNIQUE constraint on (asset_class, region) WHERE state='pending'
-- prevents concurrent rotations (RotationInFlight semantic).
--
-- INV-KEY-AUDIT: every state transition is paired with an audit_outbox
-- INSERT in the same D1 batch (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
-- The audit_outbox table is provisioned by WI-S09-004 (S-09).
--
-- asset_class CHECK values align with AssetClass::as_str() in Rust:
--   tdk | pat_signing | audit_chain | admin_signing | byok
-- state CHECK values align with KeyState::as_str() in Rust:
--   pending | active | overlap | retired | destroyed | rolled_back

CREATE TABLE IF NOT EXISTS rotation_state (
    key_id          INTEGER  NOT NULL,
    asset_class     TEXT     NOT NULL CHECK (
                        asset_class IN (
                            'tdk',
                            'pat_signing',
                            'audit_chain',
                            'admin_signing',
                            'byok'
                        )
                    ),
    state           TEXT     NOT NULL CHECK (
                        state IN (
                            'pending',
                            'active',
                            'overlap',
                            'retired',
                            'destroyed',
                            'rolled_back'
                        )
                    ),
    region          TEXT     NOT NULL,
    created_at_ms   INTEGER  NOT NULL,
    promoted_at_ms  INTEGER,
    overlap_until_ms INTEGER,
    retired_at_ms   INTEGER,
    destroyed_at_ms INTEGER,
    PRIMARY KEY (asset_class, region, key_id)
);

-- Prevents concurrent rotations per (asset_class, region).
-- Only one key may be in 'pending' state at a time per asset+region.
-- Maps to RotationError::RotationInFlight semantic.
CREATE UNIQUE INDEX IF NOT EXISTS idx_rotation_in_progress
    ON rotation_state (asset_class, region)
    WHERE state = 'pending';

-- Speeds up queries for the current Active key per (asset_class, region).
CREATE INDEX IF NOT EXISTS idx_rotation_active
    ON rotation_state (asset_class, region, state)
    WHERE state = 'active';

-- Speeds up queries for keys in Overlap window.
CREATE INDEX IF NOT EXISTS idx_rotation_overlap
    ON rotation_state (asset_class, region, state)
    WHERE state = 'overlap';
