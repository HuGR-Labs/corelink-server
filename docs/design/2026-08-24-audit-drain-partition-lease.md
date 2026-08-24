# Design — serialize audit-chain drains with a partition lease (B-038)

**Status:** design, awaiting review before implementation.
**Owner:** tech lead.
**Addresses:** BACKLOG `B-038`. Related: `B-026` (the historical fork this recurs
from), `B-022` (the per-partition archive-lag page that would now catch a fresh
fork's downstream symptom).

> This is deliberately a design, not a patch. The drain is the audit **integrity**
> path, and its unit tests are pure-function only — no test harness drives D1 — so
> a change here cannot be proven green in CI the way ordinary code can. That makes
> "land it and watch" the wrong default. The fix is specified here in full so it
> can be reviewed *before* it touches prod's audit chain.

## The defect (recurrence, not repair)

`drain_partition` (`crates/corelink-container/src/routes/audit_drain.rs`) does,
per `(tenant_id, region)` partition:

1. `read_checkpoint` → verify head signature → `read_sealed_tail` → `resolve_resume`
   → build the chain from `(head, next_sequence)`.
2. `read_pending_rows` — **whatever is pending at the instant it runs**.
3. `seal_rows` — compute `(sequence_number, prev_hash, chain_hash)` for each row.
4. **Seal the rows FIRST**: a `write_seal` loop, each `UPDATE … WHERE id=? AND
   emitted_at IS NULL`.
5. **Then** `advance_head_cas` — a compare-and-set on `audit_chain_head` against
   the value it resumed from.
6. If the CAS is lost (`Drift`), it does **not** undo the seals, justifying that
   with: *"Our sealed rows are byte-identical to that drain's (deterministic), so
   they are safe."*

The CAS is correct in isolation — a drain that loses it never advances the head.
But by step 6 the loser has **already written** its own `sequence_number` /
`prev_hash` / `chain_hash` onto real rows, and the byte-identity claim holds only
if both drains sealed the **same rows in the same order from the same head**.
Nothing enforces that. `read_pending_rows` returns a snapshot, so two overlapping
drains read **different** sets:

- `write_seal`'s `emitted_at IS NULL` guard prevents re-sealing the **same** row.
  It does **not** stop two **different** rows from receiving the **same**
  `sequence_number` from different resume points.

That is exactly the B-026 prod outcome: partition `…0f0005`/`enam` has TWO rows at
sequence 9234 — a `…write.attempted` sealed 01:43:40Z with `prev_hash` = 9233's
`chain_hash`, and a `…write.committed` sealed 02:00:50Z with a `prev_hash`
(`a4c578…`) that is no row's `chain_hash`. A fork, not a duplicate.

**Trigger:** the seal loop writes one row per D1 round trip, so a large partition
can still be sealing when the next hourly cron tick fires `/_internal/audit/drain`
again. The 17-minute spacing in the B-026 data is that overlap. Nothing serializes
the two invocations.

The eight historical partitions are quarantined and closed under B-026. What is
**unproven** is that it cannot happen again. This design makes it provably cannot.

## Option analysis

### Rejected — B: seal AFTER the CAS

"CAS first (claim the sequence range), then seal; a loser writes nothing" removes
the fork by construction, but **breaks crash-safety**. The current order seals
first precisely so that a crash mid-drain leaves correctly-sealed rows the next
drain resumes from — `read_sealed_tail` is authoritative when the checkpoint lags.
If the CAS moved first, a crash between CAS and seal would leave the head advanced
to `new_head` (a value chained *through* rows that were never sealed). The next
drain resumes from `new_head` and re-seals those still-pending rows starting at
`new_seq` — but `new_head` already incorporates them, so the chain double-counts.
Making B safe means abandoning "sealed-tail authoritative" for a checkpoint-first
recovery model — a rewrite of the crash-recovery invariant, far more than the
defect warrants.

### Chosen — A: a per-partition drain lease

Serialize drains per `(tenant_id, region)` so two never process the same partition
concurrently. Everything in `drain_partition` is already correct for a **single
writer** — the seal→CAS order, the sealed-tail resume, the CAS drift-branch (which
remains correct and useful for the *sequential* crash-then-resume case, where the
resumed rows genuinely ARE byte-identical). The lease removes the *only* precondition
the byte-identity claim needed and never had: single-writer-per-partition.

## The lease

**Migration (additive, new table):**

```sql
-- migrations/d1/0101_audit_drain_lease.sql   (number allocated at push time)
CREATE TABLE IF NOT EXISTS audit_drain_lease (
    tenant_id   TEXT    NOT NULL,
    region      TEXT    NOT NULL,
    holder      TEXT    NOT NULL,   -- unique per drain invocation (uuid v4)
    acquired_ms INTEGER NOT NULL,
    expires_ms  INTEGER NOT NULL,   -- acquired_ms + LEASE_TTL_MS
    PRIMARY KEY (tenant_id, region)
);
```

Additive-only (no ALTER of an existing table, no destructive change), so it needs
no rebuild ADR — but it MUST go through the migration ledger, never an ad-hoc
`execute --file`, or the ledger desyncs.

**Acquire — one atomic statement (SQLite `ON CONFLICT DO UPDATE … WHERE`):**

```sql
INSERT INTO audit_drain_lease (tenant_id, region, holder, acquired_ms, expires_ms)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(tenant_id, region) DO UPDATE
   SET holder=excluded.holder, acquired_ms=excluded.acquired_ms,
       expires_ms=excluded.expires_ms
   WHERE audit_drain_lease.expires_ms < ?4          -- only steal an EXPIRED lease
RETURNING holder;
```

`?4` is `now_ms`. Semantics: fresh row → INSERT succeeds → `RETURNING` yields our
holder → **acquired**. Existing but expired (`expires_ms < now`) → the guarded
UPDATE fires → `RETURNING` yields our holder → **acquired** (we stole a dead
lease). Existing and live → the `WHERE` fails, no row changes → `RETURNING`
yields **zero rows** → **not acquired** (another drain holds it). Atomic under
D1's single-statement execution; no read-then-write race.

**Holder id:** a per-invocation uuid v4 (runtime code — `getrandom`/`uuid` are
fine here, unlike the workflow-script sandbox). Used so `release` only deletes a
lease we still own.

**TTL:** `LEASE_TTL_MS` generously bounds one drain call. One call is already
bounded by `batch_limit` (rows per call) and the edge subrequest timeout; set the
TTL above the worst-case single-call wall time (proposal: 5 min) so a crashed
holder's lease self-expires and the partition is never wedged. Deliberately NOT
tied to the whole backlog drain — each `/drain` call re-acquires.

**Release (best-effort, on success or handled error):**

```sql
DELETE FROM audit_drain_lease
 WHERE tenant_id=?1 AND region=?2 AND holder=?3;   -- only if we still hold it
```

A missed release (crash) is harmless: the next drain steals the lease once it
expires. Releasing on success just returns the partition to availability before
the TTL.

## Code changes (specified)

- `PartitionOutcome`: add `Leased` (a partition another drain is actively holding;
  neither an error nor drift). The sweep counts it into a new `partitions_leased`
  summary field and moves on — it is normal, expected backpressure, logged at
  `debug`, never `warn`.
- `drain_partition`: at the very top (before `read_checkpoint`), `acquire_lease`;
  on not-acquired return `Ok(PartitionOutcome::Leased)`. On every exit path after
  acquisition (success, `Drift`, `Empty`, and the `Err` returns), `release_lease`.
  Cleanest as a small RAII-ish guard or an explicit `release` before each return;
  given the function's size, a `let outcome = async { … }.await; release; outcome`
  wrapper keeps it single-exit.
- The existing seal→CAS→drift logic is UNCHANGED. The CAS drift-branch stays as
  defense-in-depth (a lease TTL expiry under an extraordinarily slow drain could
  in principle still let a second drain in; the CAS then still refuses to double-
  advance, and — with the lease making concurrent *sealing* effectively impossible
  — the byte-identity claim it relies on is now actually true).
- The `"byte-identical to that drain's"` comment is rewritten to state the lease
  guarantee. This is what flips B-038's `verify` (it greps that phrase).

## Why the CAS is kept, not removed

The lease prevents concurrent drains. The CAS still guards the *sequential*
crash-recovery race: drain A seals rows, crashes before advancing the head,
releases nothing; its lease expires; drain B acquires, resumes from the SAME
sealed tail, computes the SAME seals (now genuinely byte-identical — same single
writer, same rows, same head), and the CAS lets exactly one head advance. Belt
and suspenders on the integrity path is correct.

## Validation plan (no D1 test harness exists)

1. **Pure-function unit tests** (the harness supports these): the lease-outcome
   plumbing — `PartitionOutcome::Leased` accounting in the sweep, the summary
   field — and any pure holder-id / TTL arithmetic.
2. **A migration-applied SQLite check** for the acquire/steal/refuse semantics:
   run the three `ON CONFLICT … WHERE` cases against an in-memory SQLite with the
   table created (a standalone `#[test]` using `rusqlite` if the crate can take a
   dev-dependency, or a checked-in `.sql` fixture exercised by `scripts/`), so the
   lease SQL is proven independent of D1-over-HTTP.
3. **Prod concurrency probe (the real proof).** After deploy: fire two
   `/_internal/audit/drain` calls at the same partition within the same second
   (a small loop) and assert, via the B-022 per-partition query plus a
   duplicate-sequence check —
   `SELECT tenant_id, region, sequence_number, COUNT(*) c FROM audit_outbox
    WHERE sequence_number IS NOT NULL GROUP BY 1,2,3 HAVING c > 1` —
   that **zero** duplicate `(tenant, region, sequence_number)` rows appear, and
   that exactly one call reports `Sealed` while the other reports `Leased`.
   Re-run against a seeded synthetic partition, not a live tenant.
4. The migration is additive and reversible (drop the table); rollout follows the
   standard d1-migrations ledger path.

## What this closes

B-038 closes when the drain either serialises partitions or seals after the CAS.
This picks serialisation (the lease), so the drift path's byte-identity assumption
becomes true-by-construction and its justifying comment goes away — which is
exactly what the item's `verify` detects.

## Open questions for review

- **TTL value.** 5 min proposed. Too low risks stealing a lease from a genuinely
  slow (but progressing) drain, re-opening a narrow overlap the CAS still catches;
  too high wedges a partition after a real crash. Pick against the measured
  worst-case single-`/drain` wall time.
- **Lease table residency.** It lives in the same prod D1 as `audit_outbox`
  (`corelink-prod-d1`); it carries no audit evidence, only coordination state, so
  it is not itself under the 7-year retention regime. Confirm that is acceptable.
- **Migration number.** Allocate at push time to avoid the B-`nnn` id-collision
  pattern (two branches picking `0101`).
