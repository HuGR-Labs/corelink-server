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

1. **Run it clean.** Record the exact count and the exact printed string — not
   the exit code. (Exit codes lie: see below.)
2. **Mutate the thing under test.** One mutation, chosen so the gate *must*
   notice it.
3. **Verify the mutation actually applied** — diff the file, or grep for the new
   content. This step is not optional; see "the false pass" below.
4. **Run again.** It must be RED, and the failure message must name the right
   thing. Quote the failing line.
5. **Restore. Run again. Green.**
6. **Report the count before and after each mutation**, not just "it went red".

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

**Always `git add` before mutating**, so `git checkout --` restores exactly what
you touched and nothing else.

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
   it cannot, fail loudly.
2. **Population floor** — refuse to believe "zero violations" unless the scan saw
   a plausible number of items. A truncated file otherwise reports perfect health.
3. **Unparsed input fails HIGH**, with the offending output excerpted — never
   silently counts as clean.

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

## Placement

An instrument installed **after** the point of failure never emits. Before
trusting "the log shows nothing", confirm the probe sits upstream of where the
thing dies.

## Related

`.claude/skills/verify-population/` — the number is right, the set is wrong.
