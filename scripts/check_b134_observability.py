#!/usr/bin/env python3
"""Fail-closed static contract for the B-134 Docker-shim experiment.

This checker deliberately does not manufacture runtime evidence. It proves
that the credential-free ``smoke-install`` observation runs on GitHub-hosted
Ubuntu, records hosted runner and backend provenance, runs the structured
helper, and keeps the evidence ledger ``UNMEASURED`` until a real fleet run is
recorded. The helper observes a local fixture; it does not claim the separate
image build, installer, doctor, publish, or deploy acceptance boundary.
"""

from __future__ import annotations

import argparse
import re
import shlex
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

    The caller removes shell comments before matching commands. The workflows
    intentionally keep their shell in multiline blocks so this small
    indentation-aware extractor is sufficient and dependency-free.
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


def executable_script(text: str) -> str:
    """Remove blank and full-line shell comments from extracted run blocks."""

    return "\n".join(line for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#"))


def reachable_script_lines(script: str) -> list[tuple[str, bool]]:
    """Mark lines hidden by a few obvious, statically dead shell constructs.

    This is intentionally not a shell interpreter. It only rejects the easy
    ways to make a required command look executable while placing it after an
    unconditional exit/return, inside ``if false; then``, or after a continued
    ``false &&``. The real workflow run remains the authority for runtime
    behavior.
    """

    lines: list[tuple[str, bool]] = []
    if_stack: list[bool] = []
    false_and_pending = False
    brace_group_depth = 0
    terminated = False
    for raw in script.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        dead = terminated or any(if_stack) or false_and_pending
        lines.append((raw, dead))

        if false_and_pending:
            false_and_pending = False
        if re.search(r"^false\s*&&\s*(?:\\\s*)?$", line):
            false_and_pending = True
        if re.match(r"^if\b", line):
            if_stack.append(bool(if_stack) or bool(re.search(r"^if\s+false\s*;\s*then(?:\s*#.*)?$", line)))
        if line == "fi" or line.startswith("fi "):
            if if_stack:
                if_stack.pop()
        if re.search(r"(?:\|\||&&)\s*\{\s*$", line) or line == "{":
            brace_group_depth += 1
        if line == "}":
            brace_group_depth = max(0, brace_group_depth - 1)
        if (
            not if_stack
            and brace_group_depth == 0
            and re.match(r"^(?:exit|return)(?:\s|$)", line)
        ):
            terminated = True
    return lines


def require_exec(script: str, pattern: str, where: str) -> None:
    """Require a command/guard at the beginning of an executable shell line."""

    matches = [raw for raw, dead in reachable_script_lines(script) if not dead and re.search(pattern, raw)]
    if not matches:
        raise ContractError(f"{where}: missing executable contract {pattern!r}")


def require_exec_line(script: str, line: str, where: str) -> None:
    """Require one exact non-comment shell line, allowing workflow indentation."""

    require_exec(script, rf"^\s*{re.escape(line)}\s*$", where)


def require_line(text: str, pattern: str, where: str) -> None:
    """Require a non-comment workflow/Dockerfile line, not prose mentioning it."""

    if not re.search(pattern, "\n".join(
        line for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")
    ), re.MULTILINE):
        raise ContractError(f"{where}: missing required line {pattern!r}")


def require_run_on_corelink(text: str, where: str) -> None:
    runs_on = re.findall(r"(?m)^\s+runs-on:\s*([^#\s]+)", text)
    if not runs_on or any(value != "corelink" for value in runs_on):
        raise ContractError(
            f"{where}: every job must run on corelink for the experiment; got {runs_on!r}"
        )


def require_run_on_hosted(text: str, where: str) -> None:
    runs_on = re.findall(r"(?m)^\s+runs-on:\s*([^#\s]+)", text)
    if not runs_on or any(value != "ubuntu-24.04" for value in runs_on):
        raise ContractError(
            f"{where}: every job must run on ubuntu-24.04 for the hosted observation; got {runs_on!r}"
        )


def check_smoke(text: str) -> None:
    where = WORKFLOW_FILES["smoke-install"]
    script = executable_script(run_blocks(text))
    require_run_on_hosted(text, where)
    if "HOSTED_ACTIONS_AVAILABLE" in text:
        raise ContractError(f"{where}: hosted availability gate would hide the experiment")
    require(text, "GitHub-hosted", where)
    if "CoreLink-fleet evidence" in text or "corelink-fleet" in text.lower():
        raise ContractError(f"{where}: a hosted observation cannot claim CoreLink-fleet evidence")
    require_line(text, r"^\s*workflow_dispatch:\s*$", where)
    require_line(text, r"^\s*I1672_FLEET_LABEL:\s*github-hosted\s*$", where)
    require_line(text, r"^\s*I1672_RUNNER_NAME:\s*\$\{\{ runner\.name \}\}\s*$", where)
    require_line(text, r"^\s*I1672_RUNNER_OS:\s*\$\{\{ runner\.os \}\}\s*$", where)
    require_line(text, r"^\s*I1672_RUNNER_ARCH:\s*\$\{\{ runner\.arch \}\}\s*$", where)
    require_exec_line(script, "set -euo pipefail", where)
    require_exec_line(script, "if docker info > artifacts/i1672/docker-info.txt 2>&1; then", where)
    require_exec_line(script, 'exit "${rc}"', where)
    require_exec_line(script, "python3 scripts/smoke_install_observe.py \\", where)
    require_exec_line(script, "--receipt artifacts/i1672/smoke-install-receipt.json", where)
    require(text, "if: ${{ always() }}", where)
    require(text, "upload-artifact@", where)
    if re.search(r"(?i)secrets\.|CORELINK_CANARY_PAT|CORELINK_TEST_TOKEN|docker build|docker run|corelink doctor", text):
        raise ContractError(f"{where}: credentialed install claims do not belong in the contract-free observation")


def check_cosign(text: str) -> None:
    where = WORKFLOW_FILES["cosign-sign"]
    script = executable_script(run_blocks(text))
    require_run_on_corelink(text, where)
    if re.search(r"(?m)^\s+tag:\s*$|\b(?:github\.event\.)?inputs\.tag\b", text):
        raise ContractError(f"{where}: manual tag input is unsupported; dispatch must select a release tag ref")
    require_exec_line(script, 'if ! [[ "${GITHUB_REF:-}" =~ ^refs/tags/v[0-9]+\\.[0-9]+\\.[0-9]+$ ]]; then', where)
    require_exec_line(script, "if ! docker info >/dev/null 2>&1; then", where)
    require_line(text, r"^\s*CF_DEPLOY_ZONE_ID:\s*\$\{\{ secrets\.CF_DEPLOY_ZONE_ID \}\}\s*$", where)
    require_exec_line(script, 'if ! [[ "${CF_DEPLOY_ZONE_ID:-}" =~ ^[0-9a-fA-F]{32}$ ]]; then', where)
    require_line(text, r"^\s*uses:\s*docker/build-push-action@\S+\s*(?:#.*)?$", where)
    require_exec(script, r"^\s*cosign sign\s+", where)
    require_exec(script, r"^\s*cosign verify\s+", where)
    if re.search(r"(?i)stub|placeholder|TODO|ZONE_ID_PLACEHOLDER", script):
        raise ContractError(f"{where}: placeholder/stub/TODO text remains in the cosign contract")


def check_ledger(text: str, *, cosign_exists: bool) -> None:
    require(text, "B-134", LEDGER)
    statuses = re.findall(r"(?im)^\*\*Status:\*\*\s*([a-z]+)\b", text)
    if statuses != ["open"]:
        raise ContractError(f"{LEDGER}: expected exactly one `**Status:** open` field, got {statuses!r}")
    if not re.search(r"(?m)^\|\s*smoke-install\s*\|.*\|\s*UNMEASURED\s*\|", text):
        raise ContractError(f"{LEDGER}: smoke-install must remain UNMEASURED until a real run is recorded")
    cosign_row = re.search(r"(?m)^\|\s*cosign-sign\s*\|.*\|\s*([^|]+?)\s*\|", text)
    if cosign_exists:
        if not cosign_row or cosign_row.group(1).strip() != "UNMEASURED":
            raise ContractError(f"{LEDGER}: cosign-sign must remain UNMEASURED while the workflow exists")
    elif not cosign_row or not cosign_row.group(1).strip().startswith("RETIRED"):
        raise ContractError(f"{LEDGER}: removed cosign-sign lane must be marked RETIRED (B-118)")
    if re.search(r"(?im)^\|\s*(smoke-install|cosign-sign)\s*\|.*\|\s*(PASS|GREEN)\s*\|", text):
        raise ContractError(f"{LEDGER}: runtime success cannot be claimed without observed run evidence")


def check(root: Path) -> None:
    check_smoke(read(root, WORKFLOW_FILES["smoke-install"]))
    cosign = root / WORKFLOW_FILES["cosign-sign"]
    if cosign.exists():
        check_cosign(read(root, WORKFLOW_FILES["cosign-sign"]))
    dockerfile = read(root, SMOKE_DOCKERFILE)
    for raw in dockerfile.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        instruction, _, payload = line.partition(" ")
        if instruction.upper() not in {"ENV", "ARG"}:
            continue
        try:
            assignments = shlex.split(payload)
        except ValueError as exc:
            raise ContractError(f"{SMOKE_DOCKERFILE}: invalid {instruction.upper()} syntax: {exc}") from exc
        if instruction.upper() == "ENV" and assignments and "=" not in assignments[0]:
            assignments = [f"{assignments[0]}={' '.join(assignments[1:])}"]
        for assignment in assignments:
            name, separator, default = assignment.partition("=")
            if (
                separator
                and default
                and re.search(r"(?i)(?:^|_)(?:TOKEN|PAT|SECRET|KEY|CREDENTIALS?)(?:_|$)", name)
            ):
                raise ContractError(f"{SMOKE_DOCKERFILE}: token-shaped {instruction.upper()} default is not allowed ({name})")
    if re.search(r"(?i)ephemeral.*probe token|unauthenticated.*leg", dockerfile):
        raise ContractError(f"{SMOKE_DOCKERFILE}: stale unauthenticated/probe-token claim")
    check_ledger(read(root, LEDGER), cosign_exists=cosign.exists())


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
