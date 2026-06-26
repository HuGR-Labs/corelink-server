# C5b is exercised by a real git-state harness, NOT a static bundle

C5b ("a PR that advances a concept's `checkpoint_sha` MUST also change the
concept body") is inherently a **two-revision diff** check: it compares the
concept at HEAD against the concept at the PR's merge-base. A static directory
of files cannot express "the same file, two git revisions" — so there is no
static bad fixture here on purpose.

The authoritative C5b test lives in `tests/okf/run_fixtures.sh` (`assert_c5b`),
which builds a throwaway git repo, commits a concept, then makes a second commit
that bumps ONLY the `checkpoint_sha` line (body byte-identical), and asserts the
validator fires `[C5b]` via the real `git show <merge-base>:<path>` path. It also
runs a negative control (a commit that changes the body too) and asserts `[C5b]`
does NOT fire. This proves the git-diff path, not just the comparison helper.

(The validator also accepts `--base-bundle <dir>` to feed the "previous" version
from a directory for ad-hoc local checks, but the acceptance proof uses real git
state so nothing is faked.)
