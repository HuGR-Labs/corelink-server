### Security

- **`dependabot-policy.yml` executed code from the PR it was judging (B-133).**
  The workflow runs on `pull_request_target` — base-branch privileges — and
  checks out `refs/pull/N/merge`. Its own SECURITY NOTE said that was safe "ONLY
  because no subsequent step executes code from the checked-out tree", while the
  step below it ran `bash scripts/ci-use-host-toolchain.sh` straight out of that
  tree. The note kept asserting an invariant the file had stopped obeying.

  Fixed by the route the item listed first, not by rewriting the note: a second,
  sparse checkout of the base ref (`github.event.pull_request.base.sha`, which
  comes from the event payload rather than PR content) into `_base/`, placed
  *after* the PR checkout because `actions/checkout` cleans its destination.

- **The prescribed fix alone would not have closed the hole.**
  `ci-use-host-toolchain.sh` does `cd "$(git rev-parse --show-toplevel)"` and
  reads the toolchain channel from `rust-toolchain.toml`, so the *working
  directory* decides whose toml it trusts. Running trusted code with the PR tree
  as cwd still fed it attacker-controlled data — and that data is not inert:

      TC="$HOME/.rustup/toolchains/${CHANNEL}-${HOST_TRIPLE}"
      echo "$TC/bin" >> "$GITHUB_PATH"

  `CHANNEL` reached a filesystem path, unvalidated, that is prepended to `PATH`
  for every later step. Chain executed end to end before the fix: with
  `channel = "../../../<checkout>/evil"` and an `evil-<triple>/bin/{cargo,rustc}`
  inside the PR tree (git preserves the executable bit), the script exits **0**,
  prints `PATH -> …/evil-…/bin`, and runs the attacker's binary as `rustc`.

  Closed at both ends: `working-directory: _base` on the step, and a
  character-class check on `CHANNEL` in the script — which protects all **25**
  workflows that call it, not just this lane. With the guard, the same channel
  exits 2 and `$GITHUB_PATH` stays empty.

### Added

- **`scripts/test_ci_use_host_toolchain.sh`** — 13 cells. Seven refuse traversal
  spellings (including `..`, which the character class alone permits since dots
  are legal in `1.91.1`); six are negative controls asserting every legal channel
  still works, so a guard that refuses everything cannot pass.
- **`scripts/check_dependabot_policy_trusted_tree.py`** — mechanises the SECURITY
  NOTE. Fails on a missing base checkout, on any `run:` invoking a script from
  the PR tree, on the half-fix (`_base/` path without `working-directory`), and —
  the one that matters — on finding **zero** script invocations to check, which
  is a broken detector rather than proof of absence.
