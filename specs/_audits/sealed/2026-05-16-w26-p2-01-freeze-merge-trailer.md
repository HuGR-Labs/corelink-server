# W26-P2-01 — `check-ga-freeze-allowed.py --range` Merge-Commit Trailer Closure — 2026-05-16

> **Doc kind:** P2 debt-closure audit (post-GA-deferred finding pulled forward).
> **Doc status:** ACTIVE.
> **Author:** Claude Opus 4.7 (W26-P2-01 closure agent) — branch `wt/r-prep-w26-p2-01-freeze-merge-trailer`.
> **Base:** `main` @ `0f77f48` (parent worktree tip at agent start).
> **Cross-refs:**
> - `specs/_audits/sealed/2026-05-16-wave26-adversarial-review.md §3 P2 finding "--range excludes merge commits"` (source).
> - `specs/_audits/sealed/2026-05-16-p2-absorption-sweep-w25-28.md` row W26-P2-01 (`DEFER-POST-GA` classification).
> - `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md` (gate's canonical declaration).
> - `scripts/check-ga-freeze-allowed.py` (gate implementation).
> - `tests/test_check_ga_freeze_allowed.py` (regression net landed in this commit).

---

## 1. Scope + provenance

The wave-26 adversarial review flagged a P2 in the GA-1 feature-freeze gate (`scripts/check-ga-freeze-allowed.py`): the `--range A..B` mode passed `--no-merges` to `git rev-list`, so any `FREEZE-EXCEPTION:` trailer attached to a merge commit was silently dropped from consideration. The wave-30 P2 absorption sweep classified the finding as **DEFER-POST-GA** because closing it required new code paths plus a test net, not a cosmetic edit. This audit documents the closure of that deferral — the merge-aware code path lands here, with a `pytest` regression net.

The scope is intentionally narrow: only the `--range` mode and only the merge-commit visibility gap. The `--staged` and `--check-empty` modes are unchanged. The frozen-surface pattern table (`FROZEN_PATH_PATTERNS` / `SKIP_PATH_PATTERNS`) is unchanged. The trailer regex and class-classification logic (`FREEZE_TRAILER_RE`, `classify_message`, `ALLOWED_CLASSES`) are unchanged.

## 2. The bug

Pre-fix line citations (file `scripts/check-ga-freeze-allowed.py` at branch `wt/r-prep-w26-p2-01-freeze-merge-trailer` parent commit):

- **L185–187 `commits_in_range(rev_range)`** — issued `git rev-list --no-merges <range>`. Merge commits never appeared in the enumerated set; the loop in `mode_range` (L308–359) therefore never inspected them. Consequence: a `FREEZE-EXCEPTION:` trailer placed exclusively on a merge commit body (e.g. when an integrator records the exception at merge time via `git merge --edit` rather than asking the contributor to amend each branch commit) was invisible to the gate.
- **L190–192 `files_in_commit(sha)`** — issued `git diff-tree --no-commit-id --name-only -r <sha>`. For non-merge commits this is correct (diff vs the single parent). For merge commits, plain `diff-tree` is silent by default — even if the `--no-merges` filter were dropped, merge commits would appear to touch zero files and would always trivially pass the gate. The two issues compound: dropping only `--no-merges` is not sufficient; `files_in_commit` must also be merge-aware.

Severity. Operationally low under the actual wave-30 workflow (source commits on freeze-window branches are authored under the freeze and carry their own trailers), which is why the finding sat at P2 rather than P1. The latent risk is the inverse: if a future operator relies on the merge-commit-only trailer pattern, the gate would silently mis-classify their merge as a non-excepted frozen-surface mutation. The fix removes that footgun.

## 3. Design choice — Option A vs Option B

**Option A (chosen).** Drop `--no-merges` from `commits_in_range` AND make `files_in_commit` merge-aware (use `git diff-tree -m --first-parent --no-commit-id --name-only -r <sha>` for commits with more than one parent). The trailer-classification path (`classify_message`) is unchanged and now naturally handles merge commits because they enter the same per-commit inspection loop. A merge commit with a `FREEZE-EXCEPTION:` trailer passes; a merge commit without one is treated identically to any other frozen-surface commit without an exception (fails).

**Option B (rejected).** Keep `--no-merges` for the main scan; add a separate `git log --merges` pass that enumerates merge commits with `FREEZE-EXCEPTION:` trailers and treats them as additional exceptions.

**Rationale for choosing Option A.**

1. *Behavioural symmetry.* Option A makes merge commits first-class citizens of the gate. Option B builds a parallel, asymmetric scan path that has subtly different semantics (e.g. how would Option B treat a merge with a trailer pointing at an unknown class? With a recognised class but no frozen-surface diff? The asymmetry forces re-deriving `classify_message` semantics twice).
2. *Surface area.* Option A is a ~10-line surgical change to two helpers. Option B is a new code path plus its own enumeration, classification, and merging-of-results logic — strictly more code, strictly more places for a subtle bug to hide.
3. *Common-case neutrality.* For a linear-history range (the empirical common case in wave-30: most PRs are squash-merged or fast-forward-merged, producing zero merge commits in the range), `git rev-list <range>` produces the same set as `git rev-list --no-merges <range>` and `files_in_commit` takes the existing non-merge path. Behavioural change is exactly zero for the common case.
4. *Merge-aware `diff-tree` is the canonical semantic.* `git diff-tree -m --first-parent <merge>` answers "what did this merge add to the first-parent branch?" which is precisely the question the gate is asking ("are there frozen-surface changes that landed via this commit?"). Option B would have had to answer the same question with a less-direct mechanism (probably `git log --merges --grep` or a trailer-only scan ignoring the diff), which would let trailer-less merges that introduce frozen-surface evil-merge content slip through.

## 4. Implementation diff summary

Two helpers in `scripts/check-ga-freeze-allowed.py` modified; one new helper added; nothing else touched.

- **`commits_in_range(rev_range)`** — dropped `--no-merges`. The function now enumerates every commit in `A..B` regardless of merge status. Comment block explains the W26-P2-01 rationale inline.
- **`is_merge_commit(sha)` (new)** — single-line helper using `git rev-list --parents -n 1 <sha>` to count parents. Returns `True` for any commit with > 1 parent.
- **`files_in_commit(sha)`** — branches on `is_merge_commit(sha)`. For non-merge commits, the existing `git diff-tree --no-commit-id --name-only -r <sha>` invocation is preserved verbatim. For merge commits, uses `git diff-tree -m --first-parent --no-commit-id --name-only -r <sha>` so the diff is computed against the first parent (the main-line the merge lands on) and the net-incoming files are surfaced. De-duplicates path strings to absorb the `-m` output's repetition.

Nothing else in the file was reformatted, renamed, or restructured. PEP 8, import ordering, docstring conventions, and the existing `from __future__ import annotations` posture are preserved.

## 5. Test net

Pytest module: `tests/test_check_ga_freeze_allowed.py` — 9 tests, all green. Loaded via `importlib` to handle the hyphenated script filename (same pattern as `tests/beta_feedback_triage_test.py`). Each test spins a throw-away git repo under `tmp_path` and monkey-patches `SCRIPT.REPO_ROOT` so the real `subprocess.run(['git', ...])` paths in the script execute against the fixture.

| # | Test | Intent |
|---|---|---|
| 1 | `test_linear_range_non_frozen_only_passes` | Baseline: non-frozen surface change (`specs/_audits/`) passes the gate without a trailer. |
| 2 | `test_linear_range_frozen_without_trailer_fails` | Baseline: a frozen-surface change (`specs/03_architecture/invariant_registry.md`) with no trailer is rejected. |
| 3 | `test_linear_range_frozen_with_valid_trailer_passes` | Baseline: a frozen-surface change with a recognised `FREEZE-EXCEPTION: cosmetic-doc` trailer passes. |
| 4 | `test_merge_commit_with_trailer_passes` | **Regression net for W26-P2-01.** Constructs an "evil merge" that introduces frozen-surface content (`openapi/corelink.yaml`) at merge time only, with the trailer on the merge body. Pre-fix the merge was invisible to the gate; post-fix the gate sees the trailer and passes. |
| 5 | `test_merge_without_trailer_plus_frozen_source_commit_fails` | **Regression net (other side).** A merge commit with no trailer combined with a source commit that touches a frozen surface (also no trailer) must fail. Proves the fix does not accidentally wave through unprotected feature merges. |
| 6 | `test_empty_range_passes` | `main..main` is vacuously OK. |
| 7 | `test_skip_subtrees_pass` | `specs/_audits/`, `specs/_compliance/`, `reports/` changes are implicit-allow per §3.d and pass without a trailer. |
| 8 | `test_unknown_exception_class_fails` | A `FREEZE-EXCEPTION: unrecognised-class` trailer on a frozen-surface commit fails (gate is conservative — unknown class taints the commit even if otherwise plausible). |
| 9 | `test_help_smoke` | `--help` exits 0 and the description mentions "feature-freeze". CLI parser sanity. |

Pytest output tail (post-fix, all green):

```
tests/test_check_ga_freeze_allowed.py::test_linear_range_non_frozen_only_passes PASSED [ 11%]
tests/test_check_ga_freeze_allowed.py::test_linear_range_frozen_without_trailer_fails PASSED [ 22%]
tests/test_check_ga_freeze_allowed.py::test_linear_range_frozen_with_valid_trailer_passes PASSED [ 33%]
tests/test_check_ga_freeze_allowed.py::test_merge_commit_with_trailer_passes PASSED [ 44%]
tests/test_check_ga_freeze_allowed.py::test_merge_without_trailer_plus_frozen_source_commit_fails PASSED [ 55%]
tests/test_check_ga_freeze_allowed.py::test_empty_range_passes PASSED    [ 66%]
tests/test_check_ga_freeze_allowed.py::test_skip_subtrees_pass PASSED    [ 77%]
tests/test_check_ga_freeze_allowed.py::test_unknown_exception_class_fails PASSED [ 88%]
tests/test_check_ga_freeze_allowed.py::test_help_smoke PASSED            [100%]

============================== 9 passed in 4.41s ===============================
```

## 6. Gates + results

| Gate | Command | Result |
|---|---|---|
| Unit tests | `python3 -m pytest tests/test_check_ga_freeze_allowed.py -v` | **9 / 9 PASS** in 4.41 s |
| CLI smoke `--help` | `python3 scripts/check-ga-freeze-allowed.py --help` | exit 0; usage text intact |
| CLI smoke `--range` (real history) | `python3 scripts/check-ga-freeze-allowed.py --range 0f77f48..HEAD` | exit 0 → `freeze-check OK across range 0f77f48..HEAD.` (range spans the wave-30 merge commits the worktree was branched off of — exercises the new merge-aware path on real data) |
| Spec validator | `python3 scripts/validate_specs.py` | exit 0 → `✅ Todos validados: 449 com schema completo, 9 com YAML only (458 total).` |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 → `✅ Nenhuma dangling reference detectada.` |

## 7. Residual risks

1. **Octopus merges (≥ 3 parents).** `--first-parent` semantics still answer the right question ("what landed on the main-line via this merge?"), but octopus merges introduce content from parents 2..N that the `-m --first-parent` invocation reports against parent 1 only. In practice octopus merges are vanishingly rare in this repo (none in `git log --oneline --merges`) and the freeze workflow does not produce them. Mitigation: a follow-up `git diff-tree -m` (all parents) audit could be added if octopus merges ever land — not in scope here.
2. **Squash merges.** A squash merge is a single non-merge commit and is already correctly handled by the original code path. The trailer must be on the squash commit body — which is the canonical squash-merge convention. No change here.
3. **Rebase-then-fast-forward.** A rebase flattens the source branch onto main; each rebased commit must carry its own trailer (same as pre-fix behavior). The merge-commit code path is not triggered. No change in behavior.
4. **Trailer-only merges with no frozen-surface diff.** Honoured but harmless: the merge passes the gate regardless (no violators to cover). The trailer is informational only in this case.
5. **`--staged` and `--check-empty` modes.** Unchanged. Their semantics (working-tree inspection, no `git log` enumeration) are orthogonal to the bug fixed here.

None of the residuals block GA. Items (1) and (4) are documented here for future-reviewer visibility.

## 8. DCO sign-off

Author + sign-off: **Gustavo Schneiter** `<gustavo@humangr.com>`.

Closure agent: Claude Opus 4.7 — recorded as `Co-Authored-By:` trailer on the closure commit per the repo's DCO convention.

Commit message will carry:

```
Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
```
