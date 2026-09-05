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
SMOKE_DOCKERFILE = "apps/get-corelink-worker/test/smoke-install.Dockerfile"
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


def run_blocks(text: str) -> str:
    """Return only the contents of YAML multiline ``run: |`` blocks.

    Contract checks must inspect executable shell, not comments or prose. The
    workflows intentionally keep their shell in multiline blocks so this
    small indentation-aware extractor is sufficient and dependency-free.
    """

    lines = text.splitlines()
    blocks: list[str] = []
    i = 0
    while i < len(lines):
        match = re.match(r"^(\s*)run:\s*\|\s*(?:#.*)?$", lines[i])
        if not match:
            i += 1
            continue
        base_indent = len(match.group(1).expandtabs(2))
        i += 1
        while i < len(lines):
            line = lines[i]
            if line.strip() and len(line) - len(line.lstrip()) <= base_indent:
                break
            blocks.append(line)
            i += 1
    return "\n".join(blocks)


def require_run_on_corelink(text: str, where: str) -> None:
    runs_on = re.findall(r"(?m)^\s+runs-on:\s*([^#\s]+)", text)
    if not runs_on or any(value != "corelink" for value in runs_on):
        raise ContractError(
            f"{where}: every job must run on corelink for the experiment; got {runs_on!r}"
        )


def check_smoke(text: str) -> None:
    where = WORKFLOW_FILES["smoke-install"]
    script = run_blocks(text)
    require_run_on_corelink(text, where)
    if "HOSTED_ACTIONS_AVAILABLE" in text:
        raise ContractError(f"{where}: hosted availability gate would hide the experiment")
    require(text, "CORELINK_CANARY_PAT: ${{ secrets.CORELINK_CANARY_PAT }}", where)
    require(text, '-e CORELINK_TEST_TOKEN="$CORELINK_CANARY_PAT"', where)
    require(text, "run: docker build -f apps/get-corelink-worker/test/smoke-install.Dockerfile", where)
    require(script, "if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then", where)
    require(script, 'if [ -n "${CORELINK_CANARY_PAT:-}" ]; then', where)
    require(script, 'if [ -z "${CORELINK_TEST_TOKEN:-}" ]; then', where)
    for needle in ("docker info", "docker run", "corelink --version", "corelink doctor"):
        require(script, needle, where)
    if re.search(r"(?i)CORELINK_INSTALL_PROBE_TOKEN|PROBE_TOKEN|openssl rand", script):
        raise ContractError(f"{where}: synthetic installer token would create false evidence")
    if re.search(r"(?i)changeme|placeholder|stub", script):
        raise ContractError(f"{where}: placeholder/stub text remains in the executable contract")


def check_cosign(text: str) -> None:
    where = WORKFLOW_FILES["cosign-sign"]
    script = run_blocks(text)
    require_run_on_corelink(text, where)
    if re.search(r"(?m)^\s+tag:\s*$|\b(?:github\.event\.)?inputs\.tag\b", text):
        raise ContractError(f"{where}: manual tag input is unsupported; dispatch must select a release tag ref")
    require(script, "release signing requires dispatching or pushing a vMAJOR.MINOR.PATCH tag", where)
    require(script, "if ! docker info >/dev/null 2>&1; then", where)
    require(text, "CF_DEPLOY_ZONE_ID: ${{ secrets.CF_DEPLOY_ZONE_ID }}", where)
    require(script, "if ! [[ \"${CF_DEPLOY_ZONE_ID:-}\" =~ ^[0-9a-fA-F]{32}$ ]]; then", where)
    for needle in ("docker info", "docker/build-push-action@", "cosign sign", "cosign verify"):
        require(script if needle.startswith("cosign ") else text, needle, where)
    if re.search(r"(?i)stub|placeholder|TODO|ZONE_ID_PLACEHOLDER", script):
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
    dockerfile = read(root, SMOKE_DOCKERFILE)
    if re.search(r"(?im)^\s*ENV\s+CORELINK_TEST_TOKEN\s*=|CORELINK_INSTALL_PROBE_TOKEN|PROBE_TOKEN", dockerfile):
        raise ContractError(f"{SMOKE_DOCKERFILE}: image must not contain a synthetic token or token default")
    if re.search(r"(?i)ephemeral.*probe token|unauthenticated.*leg", dockerfile):
        raise ContractError(f"{SMOKE_DOCKERFILE}: stale unauthenticated/probe-token claim")
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
