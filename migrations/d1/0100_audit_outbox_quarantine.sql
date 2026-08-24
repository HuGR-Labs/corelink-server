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
-- ADDITIVE ONLY: two nullable columns and one NEW partial index. Nothing is
-- dropped, no CHECK constraint is touched, so no table rebuild and no ADR is
-- required (see the auth-migrations additive-only rule). Every existing row
-- starts NULL, i.e. "not quarantined", which is the truth.
--
-- Canonical sources:
--   - crates/corelink-container/src/routes/audit_archive.rs
--   - crates/corelink-audit-chain/src/sealed_archive.rs  (split_verifying_prefix)
--   - .github/workflows/audit-archive-lag.yml            (the absence monitor)
--   - specs/_runbooks/RB-AUDIT-ARCHIVE-ABSENT.md         (how to list them)

ALTER TABLE audit_outbox ADD COLUMN quarantined_at INTEGER;   -- unix epoch ms the row was ruled unarchivable; NULL = not quarantined
ALTER TABLE audit_outbox ADD COLUMN quarantine_reason TEXT;   -- e.g. 'sequence_gap:expected=11,found=10' — grep-able, GROUP BY-able

-- A SECOND work-queue index, narrowed by `quarantined_at IS NULL`.
--
-- The archiver has exactly ONE work-queue predicate and it now carries
-- `quarantined_at IS NULL` in every one of its three reads (the partition scan,
-- the per-partition row read, and the quarantine census — see
-- `audit_archive.rs`). The narrower index is the one that matches it.
--
-- WHY A SECOND INDEX AND NOT A NARROWED 0099. SQLite cannot alter an index's
-- WHERE clause, so narrowing 0099's `idx_audit_outbox_unarchived` in place would
-- mean DROP + CREATE — and `INV-AUTH-MIGRATION-ADDITIVE` rejects a `DROP INDEX`
-- without an ADR (`scripts/check_migrations_additive.py`). Dropping an index
-- destroys no row data, but the rule is the rule and the correct answer here is
-- not to argue with the gate: 0099's index stays, and this one is added
-- alongside it.
--
-- The cost of keeping both is a plan choice, never a correctness one. Both
-- indexes' WHERE clauses are implied by the archiver's query, so the planner may
-- pick either; if it picks 0099's wider one it walks the quarantined rows and
-- discards them against the query's own `quarantined_at IS NULL` filter. That is
-- slower, not wrong, and the population it would walk is bounded and historical
-- (3,010 rows from one 17-minute window, against ~56k sealed). If the quarantine
-- population ever grows enough for that to matter, THAT is the moment to spend
-- an ADR on retiring 0099's index — and a growing quarantine count is already an
-- incident in its own right (see the audit-archive-lag census).

CREATE INDEX IF NOT EXISTS idx_audit_outbox_unarchived_active
    ON audit_outbox(tenant_id, region, sequence_number)
    WHERE emitted_at IS NOT NULL AND archived_at IS NULL AND quarantined_at IS NULL;
