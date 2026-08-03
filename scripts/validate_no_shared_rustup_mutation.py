#!/usr/bin/env python3
"""
validate_no_shared_rustup_mutation — CI gate against provisioning a Rust
toolchain on the shared-$HOME self-hosted fleet.

WHY THIS GATE EXISTS
--------------------
Sibling of scripts/validate_shared_home_cache_guard.py. Same fleet, same shared
`$HOME`, one level up: that gate protects `~/.cargo`; this one protects
`~/.rustup`.

Five GitHub Actions runners share ONE `$HOME`, so ONE `~/.rustup`.
`dtolnay/rust-toolchain` is built for GitHub-hosted runners, where `$HOME` is
job-private and ephemeral. Per its own action.yml it runs:

    rustup toolchain install <tc> --profile minimal --no-self-update
    rustup default <tc>

Both write into that SHARED tree while other jobs execute binaries out of it.
Two failure modes, and the second is the dangerous one:

  1. CRASH. Every `~/.cargo/bin/*` is a symlink to the single `rustup` shim, so a
     concurrent install can make `rustc` unexecutable mid-build:

         Caused by:
           could not execute process `rustc …` (never executed)
         Caused by:
           No such file or directory (os error 2)

     This took the whole host down for a day on 2026-06-15 — see the "ROOT FIX"
     note in .github/workflows/fuzz-nightly.yml.

  2. SILENT REPRODUCIBILITY DRIFT. `rustup default` rewrites the MACHINE-GLOBAL
     default toolchain. rust-toolchain.toml pins the workspace to a specific
     channel precisely because "rustc minor upgrades can introduce LLVM
     non-determinism that breaks reproducibility" (ADR-0015). A job that defaults
     the host to `stable` therefore moves every OTHER concurrent job off that pin
     with no error anywhere. Measured 2026-08-03: the host default had already
     drifted to `stable-x86_64-apple-darwin` while the workspace pinned 1.91.1.

THE RULE
--------
A self-hosted job must NOT provision a toolchain. Use the pre-installed host
toolchain via `bash scripts/ci-use-host-toolchain.sh [targets…]`, which reads the
channel from rust-toolchain.toml (so it cannot drift from ADR-0015) and fails
loudly if the toolchain or a requested target is absent.

Also banned: hardcoding a versioned toolchain path such as
`$HOME/.rustup/toolchains/1.91.1-x86_64-apple-darwin/bin`. It works, but it is a
SECOND copy of the pin that no gate keeps in sync with rust-toolchain.toml — the
next ADR-0015 bump would silently leave CI on the old compiler. (Unversioned
`nightly` paths are allowed: cargo-fuzz needs nightly, and there is no pin to
drift from.)

FAIL-CLOSED
-----------
If this script finds ZERO self-hosted jobs it EXITS NON-ZERO rather than
reporting success. A parser that silently matches nothing is the "green gate that
proves nothing" failure mode this repo has been bitten by repeatedly.
"""
from __future__ import annotations

import pathlib
import re
import sys

WORKFLOWS = pathlib.Path(".github/workflows")

PROVISIONING_ACTIONS = re.compile(
    r"uses:\s*(dtolnay/rust-toolchain|actions-rs/toolchain|actions-rust-lang/setup-rust-toolchain)"
)
# A rustup toolchain path carrying an explicit VERSION (1.91.1, 1.90, …).
# `nightly` / `stable` bare channels are not version pins and are not flagged.
HARDCODED_CHANNEL = re.compile(r"rustup/toolchains/\d[\w.\-]*")
JOB_START = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
RUNS_ON = re.compile(r"^\s+runs-on:\s*(.+?)\s*$")


def is_self_hosted(runs_on: str) -> bool:
    return "self-hosted" in runs_on or runs_on.strip() == "corelink"


def main() -> int:
    violations: list[str] = []
    self_hosted_jobs = 0
    seen_jobs: set[tuple[str, str]] = set()

    for path in sorted(WORKFLOWS.glob("*.yml")):
        job: str | None = None
        runs_on: dict[str, str] = {}
        for lineno, line in enumerate(path.read_text().splitlines(), start=1):
            m = JOB_START.match(line)
            if m:
                job = m.group(1)
                runs_on.setdefault(job, "")
            m = RUNS_ON.match(line)
            if m and job:
                runs_on[job] = m.group(1)
                if is_self_hosted(m.group(1)) and (path.name, job) not in seen_jobs:
                    seen_jobs.add((path.name, job))
                    self_hosted_jobs += 1

            if job is None or line.lstrip().startswith("#"):
                continue
            if not is_self_hosted(runs_on.get(job, "")):
                continue

            if PROVISIONING_ACTIONS.search(line):
                violations.append(
                    f"{path}:{lineno}: job '{job}' provisions a toolchain on the shared-$HOME fleet.\n"
                    f"    {line.strip()}\n"
                    f"    FIX: replace with `run: bash scripts/ci-use-host-toolchain.sh [targets…]`."
                )
            if HARDCODED_CHANNEL.search(line):
                violations.append(
                    f"{path}:{lineno}: job '{job}' hardcodes a versioned toolchain path — a second\n"
                    f"    copy of the ADR-0015 pin that nothing keeps in sync with rust-toolchain.toml.\n"
                    f"    {line.strip()}\n"
                    f"    FIX: `run: bash scripts/ci-use-host-toolchain.sh` (it reads the channel)."
                )

    if self_hosted_jobs == 0:
        print(
            "::error::validate_no_shared_rustup_mutation: found ZERO self-hosted jobs. "
            "Either the fleet is gone or this script's parser broke — refusing to report "
            "success on a check that inspected nothing.",
            file=sys.stderr,
        )
        return 2

    if violations:
        print(
            f"::error::validate_no_shared_rustup_mutation: {len(violations)} violation(s) "
            f"across {self_hosted_jobs} self-hosted job(s).",
            file=sys.stderr,
        )
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        return 1

    print(
        f"OK: {self_hosted_jobs} self-hosted job(s) inspected; none provisions a toolchain "
        f"or hardcodes a versioned toolchain path."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
