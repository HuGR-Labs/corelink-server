# Mutants triad central dispatch charter

Owner: CoreLink central orchestration. This charter is a review artifact; it
does not authorize a dispatch on its own.

## Reconciled baseline

- Run `36127936753` on `d1149a29406c19a1c7bd710c8fee5d49180a6e35` froze
  24,469 occurrences across 24,295 distinct redacted identities. The former
  24,463 count is six occurrences short and must not be used as an acceptance
  target.
- That run's 27 baseline shards succeeded, while mutation coverage was
  incomplete: shard 0 was incomplete and 15/16 retained no shard artifact.
  Its terminal partial outcomes included misses and timeouts, so it is failure
  evidence only.
- Run `36195446816` exercised the 120-shard rewrite and failed baseline shard
  31 on `binary:corelink-cli:test:release_workflow_contract`; the retained log
  identifies its missing `x86_64-apple-darwin` invariant. It did not reach
  mutation execution.

## Candidate contract

The candidate keeps a 27-way, occurrence-preserving round-robin partition.
Its maximum reserved runner time is `10 + 35 + 27*45 + 10 + 27*255 + 10 =
8,165` minutes, below the 8,200-minute cap. Mutation jobs reserve 255 minutes,
with the mutation command limited to 240 minutes so receipt generation and
upload have 15 minutes. All accepted evidence remains bound to the exact main
SHA, run ID, and attempt; a successful receipt requires all 27 baseline and
mutation receipts, complete outcomes, zero misses, and zero timeouts.

## Central gate before one dispatch

1. Merge the reviewed correction and record the resulting protected `main`
   SHA. Do not use a branch SHA or a stale main SHA.
2. Confirm the focused hosted issue pack for that candidate passed and that the
   current protected-main baseline has no known red receipt. The 120-shard
   shard-31 failure above is a blocker until its underlying release invariant
   is green on the selected SHA.
3. Query `issue-1863-mutants-hosted.yml` runs for the selected SHA. Stop if a
   run is queued or in progress, or if any prior dispatch already used that
   SHA. A failed run is retained as evidence and is never overwritten.
4. Dispatch exactly once from protected `main`. Do not dispatch `nightly.yml`,
   do not use a schedule, and do not re-run a subset as a replacement campaign.
5. On completion, retain the inventory, baseline, every shard artifact, and
   aggregate receipt. Validate the exact run and receipt with
   `scripts/verify_i1863_mutants_hosted.py --receipt ... --run ... --expected-sha ...`.
   A non-successful result remains open evidence for #1683, #1863, #1948, and
   #2238; it authorizes no issue closure.
