-- CoreLink D1 (Cloudflare SQLite) — migration 0101: audit-drain partition lease.
--
-- WHY
-- ---
-- `POST /_internal/audit/drain` seals the live `audit_outbox` trail into the
-- BLAKE3 hash chain PER `(tenant_id, region)` partition. It seals the pending
-- rows FIRST (each `UPDATE … WHERE emitted_at IS NULL`), THEN advances the
-- `audit_chain_head` checkpoint with a compare-and-set. The CAS is post-seal, so
-- it gates only the head advance — it can NEVER un-seal a loser's rows.
--
-- That order is crash-safe (a crash mid-drain leaves correctly-sealed rows the
-- next drain resumes from) but it is NOT concurrency-safe: two overlapping drains
-- read DIFFERENT `read_pending_rows` snapshots from DIFFERENT resume points, and
-- the `emitted_at IS NULL` guard only stops re-sealing the SAME row — it does
-- nothing about two DIFFERENT rows receiving the same `sequence_number`. That is
-- the B-026 fork (partition `…0f0005`/`enam`: two rows at sequence 9234, two
-- branches) and B-038 records that nothing today prevents its recurrence. The
-- hourly cron can fire a second `/drain` while the first is still sealing a large
-- partition (the 17-minute B-026 overlap); nothing serializes the two.
--
-- This table serializes drains per partition: a drain acquires the lease before
-- it touches a partition and releases it after. A second drain that finds a LIVE
-- lease skips the partition (neither error nor drift). Paired with the seal-loop
-- self-fence in `audit_drain.rs` (a holder stops writing once `now >= expires_ms`,
-- so it cannot still be sealing after a stealer takes an EXPIRED lease), the
-- drift path's byte-identity assumption becomes true-by-construction. See
-- `docs/design/2026-08-24-audit-drain-partition-lease.md`.
--
-- COORDINATION STATE, NOT EVIDENCE. This table holds only who-is-draining-what;
-- it carries no audit evidence and is not under the 7-year retention regime.
-- Dropping it is non-destructive (a drain simply re-acquires from scratch).
--
-- ADDITIVE ONLY: one NEW table. Nothing is dropped, no existing table is altered,
-- no CHECK constraint is touched — so no table rebuild and no ADR is required
-- (see the auth-migrations additive-only rule / `check_migrations_additive.py`).
--
-- FLAG-GATED OFF AT LAUNCH. The consumer (`AUDIT_DRAIN_LEASE_ENABLED`, default
-- OFF) does not read or write this table until an operator flips it on after a
-- prod probe, so applying this migration is inert until then.
--
-- Canonical source:
--   - crates/corelink-container/src/routes/audit_drain.rs  (acquire/release/fence)

CREATE TABLE IF NOT EXISTS audit_drain_lease (
    tenant_id   TEXT    NOT NULL,          -- partition tenant (uuid)
    region      TEXT    NOT NULL,          -- partition region
    holder      TEXT    NOT NULL,          -- unique per drain invocation (uuid v4) — release is holder-scoped
    acquired_ms INTEGER NOT NULL,          -- unix epoch ms the lease was taken
    expires_ms  INTEGER NOT NULL,          -- acquired_ms + LEASE_TTL_MS; a lease is stealable once expires_ms < now
    PRIMARY KEY (tenant_id, region)
);
