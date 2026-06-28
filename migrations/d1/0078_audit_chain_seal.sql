-- Migration 0078: audit_outbox chain-seal columns + audit_chain_head checkpoint.
--
-- ## Why
--
-- Today `audit_outbox` rows are PLAIN, UNCHAINED CloudEvents envelopes
-- (`emitted_at` left NULL; see `routes/dsr/audit.rs` + `migrations/d1/0001`
-- §audit_outbox). The BLAKE3 `HashChainBuilder`
-- (`crates/corelink-audit-chain/src/chain.rs`) is real + property-tested but
-- has NO live producer, so the live audit trail is not tamper-evident: a D1
-- writer could alter/delete a row undetected.
--
-- The S-09 drain (`routes/audit_drain.rs`, `POST /_internal/audit/drain`) closes
-- that gap. For each pending `(tenant_id, region)` partition it walks the rows in
-- `(enqueued_at, id)` order, computes the RFC-8785 JCS-canonical bytes of the
-- payload, links each event into the BLAKE3 chain
-- (`next_hash = BLAKE3(prev_hash || canonical_jcs)`), and atomically SEALS the
-- row — recording the per-event chain link inline AND advancing the per-partition
-- chain head. This migration adds the durable columns + the head checkpoint the
-- drain needs.
--
-- ## Tamper-evidence contract
--
--   * `canonical_jcs` stores the EXACT bytes the producer hashed — the verifier
--     re-hashes these bytes off the row (it never re-canonicalizes), so producer/
--     consumer canonical-form drift cannot silently corrupt a link.
--   * `prev_hash` / `chain_hash` are 64-char lowercase hex BLAKE3-256 digests.
--   * `sequence_number` is the monotonic per-partition chain position (0 = genesis,
--     `prev_hash` = 64 zeros) — matching `corelink_audit_chain::event` invariants.
--   * `audit_chain_head` is the per-partition checkpoint (head hash + next sequence)
--     the drain resumes from and compare-and-set advances (single-writer anti-fork).
--
-- ## Additive policy
--
-- Purely additive: `ALTER TABLE … ADD COLUMN` (all NULLABLE, no DEFAULT rewrite)
-- + `CREATE TABLE/INDEX IF NOT EXISTS`. No DROP, no column retype, no rewrite of
-- existing rows — INV-AUTH-MIGRATION-ADDITIVE + INV-AUDIT-APPEND-ONLY. Existing
-- unsealed rows keep `emitted_at IS NULL` and are sealed by the first drain. Safe
-- to replay idempotently (SQLite has no `ADD COLUMN IF NOT EXISTS`; the ledger
-- guarantees a migration applies exactly once).

-- ── 1. audit_outbox: per-row chain-seal columns (all nullable until sealed) ───
ALTER TABLE audit_outbox ADD COLUMN sequence_number INTEGER;  -- monotonic per (tenant_id, region) chain position
ALTER TABLE audit_outbox ADD COLUMN prev_hash       TEXT;     -- 64-hex BLAKE3 of the previous link (64 zeros at genesis)
ALTER TABLE audit_outbox ADD COLUMN chain_hash      TEXT;     -- 64-hex BLAKE3 link = BLAKE3(prev_hash || canonical_jcs)
ALTER TABLE audit_outbox ADD COLUMN canonical_jcs   TEXT;     -- EXACT RFC-8785 JCS bytes that were hashed (verifier re-hashes these)
ALTER TABLE audit_outbox ADD COLUMN chained_at      INTEGER;  -- unix epoch ms the row was sealed into the chain

-- ── 2. audit_chain_head: per-partition chain checkpoint (drain resume + CAS) ──
CREATE TABLE IF NOT EXISTS audit_chain_head (
    tenant_id     TEXT    NOT NULL,                 -- canonical UUIDv7 text (chain partition key)
    region        TEXT    NOT NULL,                 -- canonical macro region (mirrors audit_outbox.region)
    head_hash     TEXT    NOT NULL,                 -- 64-hex BLAKE3 of the last sealed event (= its chain_hash)
    next_sequence INTEGER NOT NULL,                 -- sequence_number the next sealed event will carry
    updated_at    INTEGER NOT NULL,                 -- unix epoch ms of the last head advance
    PRIMARY KEY (tenant_id, region)
);

-- ── 3. drain scan index: pending rows in deterministic seal order ────────────
-- The drain selects `WHERE emitted_at IS NULL ORDER BY enqueued_at, id` per
-- partition; this partial index serves both the partition scan and the order.
CREATE INDEX IF NOT EXISTS idx_audit_outbox_chain_order
    ON audit_outbox(tenant_id, region, enqueued_at, id)
    WHERE emitted_at IS NULL;
