#!/usr/bin/env python3
"""
validate_shared_home_cache_guard — CI gate against the shared-$HOME cache race.

WHY THIS GATE EXISTS
--------------------
The self-hosted fleet is FIVE GitHub Actions runners (`~/.gh-runners/runner-1`
… `runner-5`) on ONE Mac sharing ONE `$HOME` — therefore ONE `~/.cargo`.

`Swatinem/rust-cache` is built for GitHub-hosted runners, where `$HOME` is
job-private and ephemeral. Its *save* (post) step prunes the cargo home before
uploading: the pinned bundles walk `path.join(CARGO_HOME, "registry", "src")`
and `rmRF` what they decide is unneeded. On this fleet that means **job A's post
step deletes `~/.cargo/registry/src` out from under job B's running rustc**, on a
different runner, mid-compile.

Cargo launches rustc with its cwd inside the package's `registry/src` directory,
so the spawn itself fails:

    error: could not compile `<crate>` (lib)
    Caused by:
      could not execute process `rustc …` (never executed)
    Caused by:
      No such file or directory (os error 2)

…and every sibling unit still in flight then reports the collateral

    error: could not parse/generate dep info at: …/target/<profile>/deps/<crate>-<hash>.d
    Caused by:
      No such file or directory (os error 2)

The victim job's own YAML is innocent — the destroyer is a *different* job. This
reds real PRs whose diff contains zero Rust files. Observed on PR #974
(2026-08-03): four Rust jobs died inside two windows that each coincide exactly
with another runner's rust-cache save step.

THE RULE
--------
Any step that caches a SHARED cargo home must be gated to GitHub-hosted runners:

    if: runner.environment == 'github-hosted'

On a *persistent* self-hosted runner the action buys nothing anyway — `~/.cargo`
and `target/` already survive between jobs — so the guard costs no cache reuse
there while removing the destruction entirely. The condition is fail-safe: if
`runner.environment` were ever unavailable it evaluates false, which disables
caching rather than re-enabling the race.

WHAT IS CHECKED
---------------
Every step in `.github/workflows/*.yml` that either
  * `uses: Swatinem/rust-cache@…`, or
  * `uses: actions/cache@…` with a `path:` mentioning `.cargo` / `CARGO_HOME`
must carry the guard expression in its `if:`.

stdlib-only on purpose: the self-hosted mac fleet has no reliable PyYAML and
cannot provision one via setup-python (see action-sha-audit.yml).
"""

from __future__ import annotations

import glob
import os
import re
import sys

GUARD = "runner.environment == 'github-hosted'"

WORKFLOW_GLOBS = (".github/workflows/*.yml", ".github/workflows/*.yaml")

CACHE_USES_RE = re.compile(r"uses:\s*(?P<action>Swatinem/rust-cache|actions/cache)[@\s]")
STEP_START_RE = re.compile(r"^(?P<indent>\s*)-\s")
IF_KEY_RE = re.compile(r"^\s*(?:-\s+)?if:\s")
CARGO_PATH_RE = re.compile(r"\.cargo|CARGO_HOME")


def step_block(lines: list[str], idx: int) -> tuple[int, int]:
    """Return [start, end) line indices of the step containing line `idx`."""
    start = idx
    while start >= 0 and not STEP_START_RE.match(lines[start]):
        start -= 1
    if start < 0:
        return idx, idx + 1
    indent = STEP_START_RE.match(lines[start]).group("indent")
    sibling = re.compile(r"^" + re.escape(indent) + r"-\s")
    end = start + 1
    while end < len(lines):
        line = lines[end]
        if sibling.match(line):
            break
        # a dedent out of the step's own body ends the block
        if line.strip() and not line.startswith(indent + " ") and not line.startswith(indent + "-"):
            break
        end += 1
    return start, end


def guards_shared_cargo_home(action: str, block: str) -> bool:
    """True when this step caches a cargo home shared across the fleet."""
    if action == "Swatinem/rust-cache":
        return True
    # actions/cache: only when its `path:` covers ~/.cargo
    return bool(CARGO_PATH_RE.search(block))


def main() -> int:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(root)

    files: list[str] = []
    for pattern in WORKFLOW_GLOBS:
        files.extend(sorted(glob.glob(pattern)))

    checked = 0
    violations: list[str] = []

    for path in files:
        with open(path, encoding="utf-8") as fh:
            lines = fh.read().split("\n")
        idx = 0
        while idx < len(lines):
            match = CACHE_USES_RE.search(lines[idx])
            if not match:
                idx += 1
                continue
            start, end = step_block(lines, idx)
            block = "\n".join(lines[start:end])
            if not guards_shared_cargo_home(match.group("action"), block):
                idx = end
                continue
            checked += 1
            if_lines = [ln for ln in lines[start:end] if IF_KEY_RE.match(ln)]
            if not any(GUARD in ln for ln in if_lines):
                violations.append(
                    f"{path}:{start + 1}: step `uses: {match.group('action')}` "
                    f"is missing the shared-$HOME guard"
                )
            idx = end

    print(f"shared-$HOME cache guard: {checked} cache step(s) checked "
          f"across {len(files)} workflow file(s)")

    if violations:
        print("")
        print("FAIL — cache steps that can destroy the shared ~/.cargo mid-build:")
        for v in violations:
            print(f"  {v}")
        print("")
        print("Fix: add this to each offending step (sibling key of `uses:`), or")
        print("AND it into an existing `if:` —")
        print("")
        print(f"    if: {GUARD}")
        print("")
        print("Rationale: five self-hosted runners share ONE ~/.cargo; the cache")
        print("action's save step prunes $CARGO_HOME/registry/src while another")
        print("runner's rustc is compiling from it (os error 2). Persistent")
        print("runners keep ~/.cargo and target/ between jobs, so the action")
        print("provides no benefit there. See the module docstring for detail.")
        return 1

    if checked == 0:
        print("FAIL — no cache steps found; the gate is not actually scanning "
              "anything (check WORKFLOW_GLOBS / the regexes).")
        return 1

    print("OK — every shared-cargo-home cache step is gated to github-hosted runners.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
