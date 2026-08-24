# Design — serialize audit-chain drains with a partition lease (B-038)

**Status:** design, awaiting review before implementation. **Revised 2026-08-24
after a cold review found a critical hole in v1** — a bare TTL lease still forks on
its steal path, and the CAS does NOT catch it (the CAS is post-seal). v2 adds a
mandatory seal-loop **fence**; see "Fencing".
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
the byte-identity claim needed and never had: single-writer-per-partition — **but
only when paired with the seal-loop fence** (a bare TTL lease still forks on its
steal path; see "Fencing" below). Lease + fence is the chosen mechanism.

### Considered — C: atomic sequence-range reservation

A third option: reserve the sequence range with one CAS *before* sealing, so a
loser gets a disjoint range and cannot collide, and a crash leaves a benign,
detectable *gap* rather than a fork. This is a real fix for the same defect and is
the natural home for a durable fencing token. It is **not** chosen here because it
is a larger change to the seal/resume model (the resume logic must learn to treat
a reserved-but-unsealed gap as resumable rather than as tampering), whereas
lease + wall-clock fence closes the window with no schema change to the chain
columns and no new resume semantics. Recorded so the option is disposed of
explicitly rather than silently; revisit it if the wall-clock fence proves
insufficient under real clock skew.

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
- **Self-fence the seal loop (load-bearing — see "Fencing" below).** Before each
  `write_seal`, abort if `now_ms() >= my_lease_expires_ms`, returning
  `Leased`/`incomplete` so a caller re-drains. This is NOT optional polish: a bare
  TTL lease still forks (a holder that overruns its TTL keeps writing while a
  stealer writes too). The fence makes a holder provably stop writing at its own
  expiry, regardless of TTL/`batch_limit` tuning.
- The seal→CAS order and the CAS drift-branch are otherwise UNCHANGED. The CAS
  keeps guarding the *sequential* crash-recovery race only (below); it does NOT,
  and cannot, catch a concurrent-seal fork — it runs AFTER the seals are on disk.
- The `"byte-identical to that drain's"` comment is rewritten to state the lease
  + fence guarantee. This is what flips B-038's `verify` (it greps that phrase).

## Fencing — a TTL lease alone is NOT enough

A lease with a TTL but no fence is a lock without a fencing token, and it
reintroduces the exact B-026 fork on its own steal path:

1. Drain A holds the lease and is *slowly* sealing a large partition (the very
   B-026 trigger — a 17-minute-wide drain; `write_seal` is one D1 round trip per
   row).
2. A's TTL expires **while A is still inside the seal loop**. Drain B steals the
   now-expired lease.
3. B's `read_checkpoint` / `read_sealed_tail` are separate, non-atomic
   D1-over-HTTP reads with no read-your-writes session, so B can read a **stale**
   checkpoint that does not yet reflect A's in-flight seals. `resolve_resume`
   keeps that stale head.
4. A (still running) and B now BOTH `write_seal`. `write_seal`'s `emitted_at IS
   NULL` guard stops re-sealing the *same* row; it does nothing about two
   *different* rows getting the same `sequence_number` from different resume
   points — which is precisely the `attempted@9234 (prev=9233)` vs
   `committed@9234 (prev=a4c578-orphan)` fork B-026 recorded.
5. Both drains then run their CAS — but the forked rows are ALREADY on disk. The
   CAS gates only the head advance. It never un-seals a loser's rows. So "the CAS
   catches it" is **false** for the seal-then-CAS order — the same reason B-038
   exists.

With default tuning the window is small (`batch_limit`×per-row-latency ≈ 60s vs a
5-min TTL), but the lease NEVER enforces that inequality and B-026 proves the
per-row latency tail can blow it. The **self-fence** closes the window
deterministically: a holder checks `now_ms() >= my_lease_expires_ms` before every
`write_seal` and stops, so it cannot still be writing after a stealer takes over —
no tuning required. (A true fencing token carried into the `write_seal` WHERE
clause is the heavier alternative; the wall-clock self-check is cheaper and needs
no schema change.)

Clock caveat: `now_ms()` is per-container wall clock, so cross-container skew
shifts both the steal boundary and the fence boundary. The fence still holds as
long as a holder's OWN clock is monotonic within its process (it compares its own
acquire-time-derived expiry against its own `now_ms()`), which it is — the skew
affects only WHEN a steal is allowed, not whether a fenced holder stops.

## Why the CAS is kept, not removed

With the fence in place, the CAS still earns its keep on the *sequential*
crash-recovery race (a genuinely single-writer sequence, where it is correct):
drain A seals rows, crashes before advancing the head, releases nothing; its lease
expires; drain B acquires, resumes from the SAME sealed tail, computes the SAME
seals (now genuinely byte-identical — same single writer, same rows, same head),
and the CAS lets exactly one head advance. The fence handles the
overlap-while-sealing case; the CAS handles the crash-then-resume case; together
they cover both. Note the CAS alone was never sufficient (that is the defect), so
it is kept as a companion to the fence, not as the primary guard.

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
   **This probe only reaches the SIMULTANEOUS-invocation case — the one the lease
   closes outright. It structurally CANNOT provoke a >TTL slow holder, so it does
   NOT exercise the TTL-steal-while-sealing path.** It proves "the easy case is
   fine", not fork-freedom.
4. **Fence unit test (reaches the dangerous path — required).** The self-fence is
   pure logic (`now_ms() >= my_lease_expires_ms` before each `write_seal`), so it
   IS unit-testable without D1: drive the seal loop with a clock whose `now_ms`
   crosses `my_lease_expires_ms` partway through, and assert it aborts to
   `Leased`/`incomplete` having written only the pre-expiry prefix. This is the
   test that actually covers the residual fork window; the prod probe cannot.
5. The migration is additive and reversible (drop the table); rollout follows the
   standard d1-migrations ledger path.

## What this closes

B-038 closes when the drain either serialises partitions or seals after the CAS.
This picks serialisation (the lease **plus the seal-loop fence** — the lease alone
is insufficient), so the drift path's byte-identity assumption becomes
true-by-construction and its justifying comment goes away — which is exactly what
the item's `verify` detects.

## Open questions for review

- **TTL value.** 5 min proposed. With the self-fence in place a too-low TTL no
  longer forks (a stolen holder stops writing at its own expiry) — it only causes
  extra `Leased`/re-drain churn; a too-high TTL wedges a partition after a real
  crash for up to the TTL. Pick against the measured worst-case single-`/drain`
  wall time, now for liveness rather than correctness.
- **`RETURNING` on the `ON CONFLICT DO UPDATE … WHERE` path.** The acquire relies
  on SQLite emitting ZERO rows from `RETURNING` when the DO-UPDATE `WHERE` is
  false (a live lease) — correct in upstream SQLite. Confirm D1's SQLite build
  behaves identically (the planned SQLite unit test proves the semantics but not
  D1's specific engine; verify once against D1 before trusting acquire detection).
- **Fairness / starvation.** `read_pending_partitions` has no `ORDER BY` and the
  sweep breaks on the global `batch_limit`, so a partition consistently last in
  the DISTINCT order can be budget-starved across ticks (pre-existing, not the
  lease's fault). The lease marginally worsens the worst case: while one drain
  holds a slow partition's lease, no OTHER invocation can help it, so its seal
  latency is bounded below by its own single-writer throughput. Acceptable given
  B-022 now pages if a partition genuinely stalls, but acknowledge it.
- **Panic mid-seal.** No async `Drop`, so a panic inside the seal loop skips the
  release and the lease dangles until TTL (a ≤TTL stall for that partition). The
  fence bounds the correctness risk to zero regardless; this is purely a liveness
  cost of a panic, and self-heals on expiry.
- **Lease table residency.** It lives in the same prod D1 as `audit_outbox`
  (`corelink-prod-d1`); it carries no audit evidence, only coordination state, so
  it is not itself under the 7-year retention regime. Confirm that is acceptable.
- **Migration number.** Allocate at push time to avoid the B-`nnn` id-collision
  pattern (two branches picking `0101`).
