#!/usr/bin/env python3
"""Resolve the live CoreLink server repository without relying on redirects."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys

REPOSITORY_ID = "1232040291"
SOURCE_REPOSITORY = "HuGR-Labs/corelink-server"
DESTINATION_REPOSITORY = "HuGR-dev/corelink-server"
SERVER_REPOSITORIES = frozenset({SOURCE_REPOSITORY, DESTINATION_REPOSITORY})


def validate_server_repository(repository: str) -> str:
    """Accept only the exact server source/destination names, never lookalikes."""
    if repository not in SERVER_REPOSITORIES:
        raise ValueError("not an authorized CoreLink server repository")
    return repository


def resolve_server_repository(explicit: str | None = None) -> str:
    """Use an exact caller identity or resolve repo ID through authenticated gh."""
    selected = (
        explicit
        or os.environ.get("CORELINK_SERVER_REPOSITORY")
        or os.environ.get("CORELINK_EXPECTED_REPOSITORY")
    )
    if selected:
        return validate_server_repository(selected)

    # Actions already knows both values. Validate the stable ID before trusting
    # the context name; a matching name alone is not sufficient.
    repository_id = os.environ.get("GITHUB_REPOSITORY_ID")
    context_repository = os.environ.get("GITHUB_REPOSITORY")
    if repository_id is not None or context_repository is not None:
        if repository_id != REPOSITORY_ID or not context_repository:
            raise ValueError("GitHub context is not the CoreLink server repository ID")
        return validate_server_repository(context_repository)

    try:
        result = subprocess.run(
            ["gh", "api", f"repositories/{REPOSITORY_ID}", "--jq", ".full_name"],
            capture_output=True,
            text=True,
            check=False,
            timeout=15,
        )
    except subprocess.TimeoutExpired as exc:
        raise ValueError("repository ID lookup timed out after 15 seconds") from exc
    if result.returncode:
        raise ValueError(f"cannot resolve CoreLink server repository ID (gh exit {result.returncode})")
    return validate_server_repository(result.stdout.strip())


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo",
        help="exact authorized server repo; otherwise resolve repository ID 1232040291",
    )
    args = parser.parse_args(argv)
    try:
        print(resolve_server_repository(args.repo))
    except (OSError, ValueError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
