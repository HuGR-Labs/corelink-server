-- Migration 0068: per-Clerk-sub throttle for `/v1/session/exchange`
-- (`session_exchange_throttle`).
--
-- ## Why
--
-- `/v1/session/exchange` mints a short-lived `cas:rw` PAT (Clerk verify →
-- container Argon2id mint → D1 insert) for the verified principal. Before
-- this migration the mint had NO rate limit: a single still-valid Clerk
-- session could loop-mint unbounded PATs (each a durable D1 row + an
-- Argon2id hash on the shared container), an abuse / resource-exhaustion
-- vector. This table backs a lightweight per-principal fixed-window cap on
-- the edge (`worker/src/lib/session_throttle.ts`), mirroring the per-IP
-- rate-limit posture the pilot-signup path already enforces.
--
-- ## Shape — fixed-window counter, one row per principal
--
-- `clerk_sub` is the SHA-256-derived opaque principal UUID (the same value
-- the Worker sends to the container as `principal_id` — NOT the raw Clerk
-- `user_xxx` id, so no upstream identity material lands here). Each mint
-- attempt atomically rolls the window / increments the counter in a single
-- `INSERT … ON CONFLICT … DO UPDATE … RETURNING` statement; the Worker
-- rejects with 429 when the returned count exceeds the per-window cap.
--
-- ## Idempotency / additive-only
--
-- `CREATE TABLE IF NOT EXISTS` replays safely. Additive-only
-- (INV-AUTH-MIGRATION-ADDITIVE): a NEW table + index, no alter/drop/rename
-- of any existing object — no ADR waiver / suppression needed.

CREATE TABLE IF NOT EXISTS session_exchange_throttle (
    -- Opaque, SHA-256-derived principal UUID (== the container's
    -- `principal_id`). One throttle row per principal.
    clerk_sub TEXT PRIMARY KEY,

    -- Start of the current fixed window (Unix epoch ms). The window rolls
    -- (counter resets to 1, this advances to `now`) once `now` crosses
    -- `window_start_ms + window_length_ms`.
    window_start_ms INTEGER NOT NULL DEFAULT 0,

    -- Mint attempts counted in the current window. Reset to 1 on a window
    -- roll; otherwise incremented. The Worker rejects when this exceeds the
    -- per-window cap.
    count INTEGER NOT NULL DEFAULT 0
        CHECK (count >= 0)
);

-- Operator/forensics slice — find principals currently at/over their cap
-- without a full-table scan over the count column.
CREATE INDEX IF NOT EXISTS idx_session_exchange_throttle_count
    ON session_exchange_throttle (count);
