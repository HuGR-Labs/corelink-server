#!/usr/bin/env python3
"""Verify B-142's hosted CodeQL and secrets-drift workflow wiring.

This is the repository-side half of B-142. It proves the jobs that must run
on ephemeral GitHub-hosted Linux runners. It does not query GitHub, assert
runner availability, or claim that GHAS accepted a SARIF upload; those remain
external evidence.

The workflow reader is the repository's stdlib-compatible structural parser,
so comments and shell strings cannot manufacture a job or runner label. The
secrets-drift watchdog has no owner-controlled runner fallback.
"""

from __future__ import annotations

import argparse
import pathlib
import sys
from typing import Any

from validate_no_shared_rustup_mutation import (  # type: ignore[import-not-found]
    WorkflowParseError,
    _load_jobs,
)


ROOT = pathlib.Path(__file__).resolve().parents[1]


class ContractError(ValueError):
    """A B-142 workflow contract is absent or weakened."""


CONTRACTS: tuple[tuple[str, str, str], ...] = (
    (".github/workflows/codeql.yml", "analyze", "ubuntu-latest"),
    (".github/workflows/secrets-drift.yml", "secrets-drift", "ubuntu-latest"),
    (".github/workflows/codeql-evidence-watchdog.yml", "inspect", "ubuntu-latest"),
    (".github/workflows/secrets-drift-evidence-watchdog.yml", "inspect", "ubuntu-latest"),
)


def _read_jobs(root: pathlib.Path, relative: str) -> list[Any]:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise ContractError(f"missing/non-regular workflow: {relative}")
    try:
        return _load_jobs(path)
    except (OSError, UnicodeError, WorkflowParseError) as exc:
        raise ContractError(f"cannot parse {relative}: {exc}") from exc


def _runner_value(value: Any) -> str | None:
    """Return a scalar runner label, rejecting ambiguous collections."""
    if not isinstance(value, str):
        return None
    return value.strip().strip("'\"")


def check_workflow(root: pathlib.Path, relative: str, expected_job: str, runner: str) -> None:
    jobs = _read_jobs(root, relative)
    matches = [job for job in jobs if getattr(job, "name", None) == expected_job]
    if len(matches) != 1:
        names = [getattr(job, "name", "<unknown>") for job in jobs]
        raise ContractError(
            f"{relative}: expected exactly one job {expected_job!r}; observed {names!r}"
        )
    observed = _runner_value(getattr(matches[0], "runs_on", None))
    if observed != runner:
        raise ContractError(
            f"{relative}:{expected_job}: runner must be {runner!r}, got {observed!r}"
        )


def verify(root: pathlib.Path = ROOT) -> dict[str, str]:
    result: dict[str, str] = {}
    for relative, job, runner in CONTRACTS:
        check_workflow(root, relative, job, runner)
        result[f"{relative}:{job}"] = runner
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=pathlib.Path, default=ROOT)
    args = parser.parse_args(argv)
    try:
        verify(args.root)
    except ContractError as exc:
        print(f"B-142 workflow contract: FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        "B-142 workflow contract: PASS: CodeQL and secrets-drift jobs/watchdogs "
        "are structurally present on hosted Linux; GHAS/run evidence remains external"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
