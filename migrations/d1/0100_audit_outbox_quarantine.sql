-- CoreLink D1 (Cloudflare SQLite) — migration 0100: audit_outbox quarantine.
--
-- WHY
-- ---
-- `POST /_internal/audit/archive` copies sealed `audit_outbox` rows into NDJSON
-- chunks in R2 (migration 0099 added the `archived_at` watermark). The archiver
-- re-verifies every link before it emits bytes and refuses a chunk whose chain
-- does not verify. That refusal is CORRECT — the archive is evidence, and
-- writing a chunk that does not verify would either manufacture a chain break
-- offsite or launder a real one.
--
-- What was WRONG is what happened next: the refused rows stayed in the work
-- queue and were retried on every hourly tick, forever, invisibly.
--
-- Measured against prod on 2026-08-23: in a 17-minute window on 2026-08-14
-- (01:43:40–02:00:50 UTC) the seal path FORKED. 1,505 excess rows across 3,010
-- rows sit on DUPLICATED `sequence_number`s inside their `(tenant_id, region)`
-- partition, each duplicate pair carrying a DIFFERENT `prev_hash` — two
-- branches, not a relabelling — and the sequence immediately following each
-- duplicate is missing. 8 of 360 partitions can never satisfy the verifier.
-- (`_public`/`wnam`: 131 rows, sequences 0..130, duplicates at 10, 13, 20, 30,
-- 32, 37; missing 12, 14, 21, 31, 33, 38.) The drain on `main` today has
-- fork-freedom — it aborts a partition when the head drifted rather than
-- forking — so this is HISTORICAL, not recurring.
--
-- RE-SEQUENCING IS FORBIDDEN. The sealed rows ARE the evidence; rewriting
-- `sequence_number` / `prev_hash` / `chain_hash` / `canonical_jcs` to make the
-- verifier happy would destroy the exact thing the chain exists to prove.
-- Nothing in this change or its writer ever UPDATEs a chain column.
--
-- So the rows are QUARANTINED instead: marked permanently unarchivable, with a
-- machine-readable reason naming the break, and removed from the archiver's
-- work queue. The archiver now writes the longest contiguous VERIFYING PREFIX
-- of each partition and quarantines from the break onward, so healthy rows
-- reach R2 instead of being held hostage by one historical fork.
--
-- ADDITIVE ONLY: two nullable columns and one partial-index replacement. No
-- CHECK constraint is touched, so no table rebuild and no ADR is required (see
-- the auth-migrations additive-only rule). Every existing row starts NULL, i.e.
-- "not quarantined", which is the truth.
--
-- Canonical sources:
--   - crates/corelink-container/src/routes/audit_archive.rs
--   - crates/corelink-audit-chain/src/sealed_archive.rs  (split_verifying_prefix)
--   - .github/workflows/audit-archive-lag.yml            (the absence monitor)
--   - specs/_runbooks/RB-AUDIT-ARCHIVE-ABSENT.md         (how to list them)

ALTER TABLE audit_outbox ADD COLUMN quarantined_at INTEGER;   -- unix epoch ms the row was ruled unarchivable; NULL = not quarantined
ALTER TABLE audit_outbox ADD COLUMN quarantine_reason TEXT;   -- e.g. 'sequence_gap:expected=11,found=10' — grep-able, GROUP BY-able

-- The archiver's work queue, narrowed.
--
-- REPLACING 0099's `idx_audit_outbox_unarchived` rather than adding a second
-- index, because the archiver has exactly ONE work-queue predicate and it now
-- carries `quarantined_at IS NULL` in every one of its three queries (the
-- partition scan, the per-partition row read, and the quarantine census — see
-- `audit_archive.rs`). A second index would leave 0099's index still matching
-- quarantined rows, so the planner could pick the wider one and walk a growing
-- tail of permanently-unarchivable rows on every hourly tick — which is the
-- cost this migration exists to remove. SQLite cannot alter an index's WHERE
-- clause, so a drop-and-create is the only way to narrow it; dropping an index
-- touches no row data.
DROP INDEX IF EXISTS idx_audit_outbox_unarchived;

CREATE INDEX IF NOT EXISTS idx_audit_outbox_unarchived
    ON audit_outbox(tenant_id, region, sequence_number)
    WHERE emitted_at IS NOT NULL AND archived_at IS NULL AND quarantined_at IS NULL;
