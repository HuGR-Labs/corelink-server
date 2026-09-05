#!/usr/bin/env python3
"""Run one argv command with a portable process-group timeout."""

from __future__ import annotations

import os
import signal
import subprocess
import sys


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: exec-with-timeout.py SECONDS COMMAND [ARG ...]", file=sys.stderr)
        return 2
    try:
        timeout = float(sys.argv[1])
    except ValueError:
        print(f"invalid timeout: {sys.argv[1]!r}", file=sys.stderr)
        return 2
    if timeout <= 0:
        print("timeout must be positive", file=sys.stderr)
        return 2

    process = subprocess.Popen(sys.argv[2:], start_new_session=True)
    try:
        return _shell_status(process.wait(timeout=timeout))
    except subprocess.TimeoutExpired:
        print(f"GATE_TIMEOUT exceeded {timeout:g}s", file=sys.stderr)
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        return 124


def _shell_status(returncode: int) -> int:
    """Represent a signal termination with the shell's 128+signal status."""
    return 128 + (-returncode) if returncode < 0 else returncode


if __name__ == "__main__":
    raise SystemExit(main())
