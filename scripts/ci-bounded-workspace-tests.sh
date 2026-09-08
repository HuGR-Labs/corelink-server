#!/usr/bin/env bash
# Execute the workspace test suite with an explicit resource and time bound.
#
# This is the nightly/convergence lane's execution path.  It is deliberately
# separate from the bundle CI entrypoint: a bundle invokes its own frozen
# command set, while this scheduled lane provides a bounded workspace smoke.
# In particular, this script must never regress to a compile-only cargo test:
# compiling test binaries is not executing them.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Four compiler jobs is the ceiling for the shared builder fleet.  Refuse an
# invalid or larger override instead of silently turning a nightly run into an
# unbounded Mac load.
jobs="${CARGO_BUILD_JOBS:-4}"
if [[ ! "$jobs" =~ ^[1-9][0-9]*$ ]] || (( jobs > 4 )); then
    echo "CARGO_BUILD_JOBS must be an integer in [1,4] (got: $jobs)" >&2
    exit 2
fi
export CARGO_BUILD_JOBS="$jobs"

# The workflow also has a job timeout, but keeping the deadline here makes the
# command safe when an operator runs it manually.  The supervisor kills the
# complete process group, including children spawned by cargo.
deadline_seconds="${CORELINK_WORKSPACE_TEST_TIMEOUT_SECONDS:-1500}"
if [[ ! "$deadline_seconds" =~ ^[1-9][0-9]*$ ]] || (( deadline_seconds > 1500 )); then
    echo "CORELINK_WORKSPACE_TEST_TIMEOUT_SECONDS must be in [1,1500] (got: $deadline_seconds)" >&2
    exit 2
fi

if command -v cargo-nextest >/dev/null 2>&1; then
    # nextest executes each test binary/test independently and uses the checked
    # in CI profile.  `--all-targets` keeps examples and benches in the
    # population rather than proving only library tests.
    test_command=(cargo nextest run --workspace --all-targets --profile ci)
    echo "workspace execution: cargo nextest run --workspace --all-targets --profile ci"
else
    # The fallback is equally an execution command; --no-fail-fast preserves a
    # complete failure report without changing what is run.
    test_command=(cargo test --workspace --all-targets --no-fail-fast)
    echo "workspace execution: cargo test --workspace --all-targets --no-fail-fast"
fi

python3 - "$deadline_seconds" "${test_command[@]}" <<'PY'
from __future__ import annotations

import os
import signal
import subprocess
import sys

deadline = int(sys.argv[1])
command = sys.argv[2:]
if not command:
    raise SystemExit("workspace test command is empty")

process = subprocess.Popen(command, start_new_session=True)
try:
    status = process.wait(timeout=deadline)
except subprocess.TimeoutExpired:
    print(
        f"workspace test execution exceeded hard deadline ({deadline}s); "
        "terminating the complete process group",
        file=sys.stderr,
    )
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=15)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
    raise SystemExit(124)

raise SystemExit(status)
PY
