# C5 is exercised by a real git-state harness, NOT a static bundle

C5 freshness ("a cited line-range whose CONTENT no longer matches what the author
cited at `checkpoint_sha` makes the concept STALE") is a **content-anchor**
comparison between two git revisions (`checkpoint_sha` and HEAD). A static bundle
would have to hardcode a real host-repo commit SHA as `checkpoint_sha` AND rely on
a specific file having drifted between that commit and HEAD — which is **not
portable to a clean CI checkout** (a local-only commit SHA simply does not exist
there, so C4 would fire instead of C5). The original static fixture here did
exactly that and broke in CI.

The authoritative C5 tests live in `tests/okf/run_fixtures.sh`:

- `assert_c5` — builds a throwaway git repo, commits a file, writes a concept that
  cites line 2 with `checkpoint_sha` = that commit, then commits a change to line
  2, and asserts the validator fires `[C5]`. A negative control cites an UNCHANGED
  line and asserts `[C5]` does NOT fire (the anti-churn guarantee).
- `assert_c5_shift` — the **position-shift hardening** proof. A concept cites a
  byte-identical range (lines 3-4); commit B inserts lines ABOVE it so the
  authored content slides DOWN while the cite still points at lines 3-4. The OLD
  two-tree-diff ∩ cited-range check saw the insertion hunk only at new-lines
  {1,2} (disjoint from [3,4]) and reported **0-stale** — the blind spot. The
  content-anchor check compares the CONTENT at lines 3-4 and MUST fire `[C5]`.

C5 LOGIC (CONTENT-ANCHOR): for each cited `path:Lx-Ly`, compare the content of
lines Lx..Ly of `path` between `checkpoint_sha` and HEAD (each line trailing-
whitespace-stripped, internal whitespace preserved). Drift fires on an in-range
edit OR a pure position-shift. Per-file fast skip when the file blob is identical
at both revs. Never `git log -L`/blame. The predicate lives in
`validate_okf.cited_range_drifted` and is shared with `scripts/okf_reconcile.py`.
