#!/usr/bin/env python3
"""Fail-closed static contract for the B-134 Docker-shim experiment.

This checker deliberately does not manufacture runtime evidence. It proves
that both candidate workflows exercise Docker-compatible commands on the
``corelink`` fleet, that no placeholder/stub can report success, and that the
evidence ledger remains ``UNMEASURED`` until a real run is recorded.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

WORKFLOW_FILES = {
    "smoke-install": ".github/workflows/smoke-install.yml",
    "cosign-sign": ".github/workflows/cosign-sign.yml",
}
LEDGER = "docs/campaigns/remediation/B-134-docker-shim-experiment.md"


class ContractError(RuntimeError):
    """The candidate or ledger is missing a load-bearing invariant."""


def read(root: Path, relative: str) -> str:
    try:
        return (root / relative).read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise ContractError(f"cannot read {relative}: {exc}") from exc


def require(text: str, needle: str, where: str) -> None:
    if needle not in text:
        raise ContractError(f"{where}: missing required anchor {needle!r}")


def require_run_on_corelink(text: str, where: str) -> None:
    runs_on = re.findall(r"(?m)^\s+runs-on:\s*([^#\s]+)", text)
    if not runs_on or any(value != "corelink" for value in runs_on):
        raise ContractError(
            f"{where}: every job must run on corelink for the experiment; got {runs_on!r}"
        )


def check_smoke(text: str) -> None:
    where = WORKFLOW_FILES["smoke-install"]
    require_run_on_corelink(text, where)
    if "HOSTED_ACTIONS_AVAILABLE" in text:
        raise ContractError(f"{where}: hosted availability gate would hide the experiment")
    require(text, "CORELINK_CANARY_PAT: ${{ secrets.CORELINK_CANARY_PAT }}", where)
    require(text, '-e CORELINK_TEST_TOKEN="$CORELINK_CANARY_PAT"', where)
    if re.search(r"(?i)CORELINK_INSTALL_PROBE_TOKEN|PROBE_TOKEN|openssl rand", text):
        raise ContractError(f"{where}: synthetic installer token would create false evidence")
    for needle in ("docker info", "docker build", "docker run", "corelink --version", "corelink doctor"):
        require(text, needle, where)
    if re.search(r"(?i)changeme|placeholder|stub", text):
        raise ContractError(f"{where}: placeholder/stub text remains in the executable contract")


def check_cosign(text: str) -> None:
    where = WORKFLOW_FILES["cosign-sign"]
    require_run_on_corelink(text, where)
    for needle in ("docker info", "docker/build-push-action@", "cosign sign", "cosign verify"):
        require(text, needle, where)
    if re.search(r"(?i)stub|placeholder|TODO|ZONE_ID_PLACEHOLDER", text):
        raise ContractError(f"{where}: placeholder/stub/TODO text remains in the cosign contract")


def check_ledger(text: str) -> None:
    require(text, "B-134", LEDGER)
    require(text, "status: open", LEDGER)
    for workflow in WORKFLOW_FILES:
        if not re.search(rf"(?m)^\|\s*{re.escape(workflow)}\s*\|.*\|\s*UNMEASURED\s*\|", text):
            raise ContractError(f"{LEDGER}: {workflow} must remain UNMEASURED until a real run is recorded")
    if re.search(r"(?im)^\|\s*(smoke-install|cosign-sign)\s*\|.*\|\s*(PASS|GREEN)\s*\|", text):
        raise ContractError(f"{LEDGER}: runtime success cannot be claimed without observed run evidence")


def check(root: Path) -> None:
    check_smoke(read(root, WORKFLOW_FILES["smoke-install"]))
    check_cosign(read(root, WORKFLOW_FILES["cosign-sign"]))
    check_ledger(read(root, LEDGER))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args(argv)
    try:
        check(args.root.resolve())
    except ContractError as exc:
        print(f"B-134 contract FAILED: {exc}", file=sys.stderr)
        return 1
    print("B-134 contract OK: runtime evidence remains UNMEASURED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
