# C5 is exercised by a real git-state harness, NOT a static bundle

C5 freshness ("a cited line-range that changed since `checkpoint_sha` makes the
concept STALE") is a **two-tree diff** (`git diff <checkpoint_sha> HEAD`). A
static bundle would have to hardcode a real host-repo commit SHA as
`checkpoint_sha` AND rely on a specific line of a real file having changed
between that commit and HEAD — which is **not portable to a clean CI checkout**
(a local-only commit SHA simply does not exist there, so C4 would fire instead of
C5). The original static fixture here did exactly that and broke in CI.

The authoritative C5 test lives in `tests/okf/run_fixtures.sh` (`assert_c5`),
which builds a throwaway git repo, commits a file, writes a concept that cites
line 2 with `checkpoint_sha` = that commit, then commits a change to line 2, and
asserts the validator fires `[C5]`. A negative control cites an UNCHANGED line
and asserts `[C5]` does NOT fire (the anti-churn guarantee). This proves the real
two-tree-diff path with zero host-repo-SHA dependence.

(The C5 validator LOGIC — two-tree `git diff`, hunk ∩ cited-range, never
`git log -L`/blame — is unchanged; this was purely a fixture-portability fix.)
