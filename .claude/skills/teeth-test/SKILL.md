---
name: teeth-test
version: 1.0.0
description: Prove a test, gate, or verify command can actually FAIL before trusting it green. Invoke whenever you add or change a gate, a `verify:` in BACKLOG.md, a CI check, a selftest, or a test file — and whenever a suite reports green and you are about to rely on that. Triggers — "the gate is green"; adding a check; a suite passes on first try; reviewing a PR that adds tests; a verify command you did not watch fail.
---

# teeth-test — a gate that never failed has not been tested

A green gate proves one of two things and does not tell you which: the code is
correct, or the gate cannot fail. Until you have watched it go red on purpose,
you have no evidence which one you have.

## The procedure

Use a disposable, isolated copy for the entire drill. Record the current
`HEAD` and `git status --short` first. For a normal checkout, create a separate
index and worktree (or an archive copy) and run the gate from there:

```sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/teeth.XXXXXX")
git worktree add --detach "$tmp" HEAD
trap 'git worktree remove --force "$tmp"' EXIT
```

If that setup fails, **HALT**. Do not mutate the user's checkout, index, or
working tree to continue. The `trap` target is the newly-created, exact temp
path; it must not be replaced by a broad directory or by the user's worktree.
When the gate is meant to test an uncommitted change, apply the tracked diff (and
copy explicitly named untracked inputs) into `$tmp` before the clean run; verify
the resulting diff there. Never stage the user's index just to transport a
change into the drill.

1. **Run it clean.** In `$tmp`, record the exact count, population, and printed
   string — not only the exit code. (Exit codes lie: see below.)
2. **Choose the mutation and expected failure first.** Name the invariant it
   violates, the exact diagnostic that must appear, and the non-zero result
   expected. A mutation that cannot be load-bearing for this gate is not a
   teeth test; choose another one or **HALT**.
3. **Mutate by content in `$tmp`**, then prove it applied: assert the exact
   executable predicate (not a comment, label, or broad name match) occurs once,
   the replacement occurs once, and `git -C "$tmp" diff --` shows the intended
   file. If the predicate is not uniquely addressable, or any assertion fails,
   **HALT**; the gate was not tested.
4. **Run again from `$tmp`.** It must be RED, with the expected non-zero result
   and diagnostic naming the right thing. Quote the failing line. A green result,
   an unexpected error, or a failure without the expected diagnostic is a
   failed drill, not evidence of teeth.
5. **Restore only the disposable copy** with
   `git -C "$tmp" restore --source=HEAD --worktree -- <file>`, prove its diff is
   empty, and run again. It must be GREEN with the original count and string.
6. Remove the temp worktree via the trap and verify the original `HEAD` and
   `git status --short` are byte-for-byte unchanged. Report the count and
   population before mutation, after mutation, and after restore.

## Mutate by CONTENT, never by line number

`sed -i '47s/.../.../'` drifts the moment anything above line 47 changes, and a
wrong delimiter fails silently. Match on the distinctive text instead.

**A mutation that did not happen reports a false pass** — the gate runs against
unmutated code, goes green, and you record that as "teeth verified".

### The false pass, measured

A `verify:` predicate was being replaced. Two bugs in the text-splicing helper:

- `blk.index('verify-means: |')` matched a `printf "verify-means: |..."` string
  **inside the old verify block**, splicing into the middle of it;
- `rindex('```backlog\n', 0, i)` — `i` pointed at the `\n` that is part of the
  fence itself, so the window excluded the intended item's fence and fell back to
  the **previous item**, swallowing that whole block (167 items → 166).

The shell exited **0**. The verification command reported **CONFIRMED**.

It was caught only by comparing the **printed string** against the expected
string. Accepting the green would have shipped the old gate wrapped in new prose,
with a neighbouring item silently deleted.

Never `git add`, `git checkout --`, `git reset`, or `git restore` in the user's
worktree as part of this drill. The isolated worktree has its own index and is
the only place mutation and restoration occur; pre-existing user changes are
therefore neither staged nor overwritten.

## The hole this catches most often: the comparator is uncovered

A selftest that loops over N cases and compares actual to expected is itself
untested. Replacing its comparison with `if true` can leave it green at N/N —
including with the very defect the suite exists to catch reintroduced. A
`pass -lt N` floor does not help, because `pass` still reaches N.

**Fix:** one negative control inside the suite — call the checker once with an
impossible expectation in a captured subshell, assert the failure counter
incremented, reset the counters. Now neutering the comparator turns the suite red.

Ask of every suite: *what single edit to the harness would make every case pass
regardless of the code?* If such an edit exists and nothing catches it, the suite
is decorative.

For a finite case suite, declare the expected cell-ID set before running it and
assert that every ID occurs exactly once: no missing, duplicate, or unknown
cell. Declare the expected RED and GREEN partitions as well, and compare the
observed partition to those exact sets. A total count or population floor can
remain unchanged while a cell disappears or changes polarity, so neither is a
substitute for the set comparison.

For these campaign drills, the contract can be written explicitly as:

```text
expected = {b081.clean, b081.mutated, b081.restored,
            b133.clean, b133.mutated, b133.restored,
            b167.clean, b167.mutated, b167.restored}
expected_red = {b081.mutated, b133.mutated, b167.mutated}
expected_green = expected - expected_red
assert observed_ids == expected
assert observed_red == expected_red
assert observed_green == expected_green
```

Reject duplicate IDs before comparing sets; otherwise two cells can cancel a
missing cell and still produce the same set.

## The discriminating DoD for a test file

> For each test file, there must exist **at least one mutation that turns EVERY
> test in that file red.**

If no such mutation exists, the file is not testing one property — it is a dump of
unrelated assertions wearing a filename. A 674-line file covering auth + dedup +
region + backend fails this: no single mutation reaches all four, because that
shared property does not exist.

Exemption is by **written reason**, never by a numeric floor.

## Anti-vacuity clauses for a counting gate

A gate that counts violations and reports zero must prove it was looking at
something:

1. **Positive control, by name** — the parser must find a specific known item. If
   it cannot, **HALT** and report the source and expected control.
2. **Population floor** — refuse to believe "zero violations" unless the scan saw
   a plausible number of items. A zero-item result, empty source, or truncated
   input is **HALT**, never a clean result.
3. **Unparsed input fails HIGH** — malformed records, parser warnings, pagination
   markers, and incomplete output are **HALT**, with the offending output excerpted.
   Never silently count them as clean.

Reuse the **same parser** the thing under test uses. Two parsers can disagree
about what a block is; one cannot.

## Instruments in this toolbox that report absence when the answer is something else

- `gh run view --log-failed` — zero lines **and** exit 0 on a run that failed
- `gh pr view --json files` — silently truncates at 100
- `gh pr checks` — folds `cancelled` into "fail"
- `gh api rate_limit` — blind to the **secondary** limit; showed 4935/5000 while
  the next call took a 403
- `wrangler tail --search` — finds nothing with the line right there
- `cmd | grep -q` under `pipefail` — SIGPIPE kills the producer; **lies when it
  finds**
- `cmd | grep` — hides the producer's exit code entirely

**An empty result from a filtered command is not evidence of absence.** Re-run
unfiltered and read `$?`.

## Three case drills from this campaign

The mutation must exercise the shipped check, not merely edit a nearby comment:

- **B-081 (Worker → container):** target the exact executable rotation-key
  predicate in the container adapter (the `for name in ["PAT_SIGNING_KEY_PREV",
  "PAT_SIGNING_KEY_NEW"]` array), not a comment or a repository-wide name grep.
  Assert that predicate occurs once. Until a real executable behavior test or
  assertion covers that predicate, **HALT**. The B-081 `grep` over names cannot
  prove behavior, so do not invent a RED diagnostic or claim a teeth result. Once
  the test path exists, mutate `PAT_SIGNING_KEY_PREV` in that array and require
  the exact failure emitted by that test, captured from its clean run; then
  restore it.
- **B-133 (CI/workflow):** target the exact executable `working-directory: _base`
  field attached to the trusted-tree script step, remove only that field while
  retaining the executable `run:` and PR merge checkout. Require emitted output
  containing both of these exact emitted phrases: "MAS sem \`working-directory: _base\`" and "rust-toolchain.toml do PR". The output must identify the trusted-tree failure;
  do not invent a diagnostic from the mutation description. If that uniquely
  attached field is absent, **HALT**; changing a script path, comment, or any
  broad `working-directory` is not this mutation.
- **B-167 (backlog parser):** use the production parser in
  `scripts/backlog_verify.py` to enumerate blocks and run `--id B-167`; do not
  recreate its block/ID regex in the drill. Remove the real `B-142` control that
  the production verify requires. Expected RED: require the exact non-zero
  diagnostic actually emitted by the production parser (for example, a known
  residue or instrument-broken line); never invent a message from the mutation.
  Restore and require the original parser-reported population. Any malformed or
  unparsed block, zero population, or parser output that cannot report the
  population is **HALT**.

For each case, capture the clean, mutated, and restored count plus the exact
diagnostic. Do not report only “it went red”.

## Placement

An instrument installed **after** the point of failure never emits. Before
trusting "the log shows nothing", confirm the probe sits upstream of where the
thing dies.

## Related

`.claude/skills/verify-population/` — the number is right, the set is wrong.
