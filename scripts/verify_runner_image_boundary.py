#!/usr/bin/env python3
"""Verify the corelink-server boundary for the B-114/B-135/B-138 image hold.

Those three contracts belong to the sibling ``corelink-runners`` repository.
This repository cannot build, pin, or inspect that image in CI: its token is
repo-scoped.  The boundary is therefore deliberately fail-closed: keep the
three backlog items open/manual, avoid inventing a local image or OCI-label
consumer, and ensure the one local container-build lane checks its baked tools
before invoking a build.  ``--self-test`` mutates each control in memory and
proves the verifier rejects the weakened contract.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BACKLOG = ROOT / "BACKLOG.md"
HOLDS = ROOT / "docs/operator/dependency-holds.md"
BUILD_WORKFLOW = ROOT / ".github/workflows/container-build-push-prod.yml"
WORKFLOWS = ROOT / ".github/workflows"
PREFLIGHT_TITLE = "Preflight — BuildKit + runc must be baked in the runner image"
REQUIRED_TOOL_LOOP = re.compile(r"^for\s+t\s+in\s+buildkitd\s+buildctl\s+runc\s*$")
REQUIRED_MISSING_IF = re.compile(r'^if\s+\[\s*"\$missing"\s*=\s*"1"\s*\]\s*$')
BUILD_COMMAND = re.compile(r"^(?:sudo\s+)?buildctl\s+--addr\s+[^\n]*\sbuild(?:\s|$)")


class ContractError(RuntimeError):
    pass


def _backlog_block(backlog: str, item: str) -> str:
    match = re.search(rf"(?ms)^### {re.escape(item)} .*?(?=^### B-\d+ |\Z)", backlog)
    if match is None:
        raise ContractError(f"missing backlog block {item}")
    return match.group(0)


def _active_commands(step_body: str) -> list[str]:
    """Return shell command fragments, excluding YAML/shell comments.

    This is intentionally a small shell lexer, not a shell evaluator.  It only
    needs to distinguish executable control/command fragments from prose,
    comments, and ``echo``/quoted bait that happens to contain guard literals.
    Separators are recognized outside quotes, and ``#`` starts a shell comment
    only at the beginning of a word (the shell rule), so URLs and quoted text
    remain intact.
    """

    commands: list[str] = []
    for raw_line in step_body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or line.startswith("run:"):
            continue
        quote: str | None = None
        escaped = False
        comment_at: int | None = None
        for index, char in enumerate(line):
            if escaped:
                escaped = False
                continue
            if char == "\\" and quote != "'":
                escaped = True
                continue
            if quote:
                if char == quote:
                    quote = None
                continue
            if char in ("'", '"'):
                quote = char
                continue
            if char == "#" and (index == 0 or line[index - 1].isspace() or line[index - 1] in ";|&"):
                comment_at = index
                break
        if comment_at is not None:
            line = line[:comment_at]
        line = line.strip()
        if not line:
            continue

        quote = None
        escaped = False
        segment_start = 0
        index = 0
        while index < len(line):
            char = line[index]
            if escaped:
                escaped = False
            elif char == "\\" and quote != "'":
                escaped = True
            elif quote:
                if char == quote:
                    quote = None
            elif char in ("'", '"'):
                quote = char
            elif char in ";|&":
                segment = line[segment_start:index].strip()
                if segment:
                    commands.append(segment)
                if index + 1 < len(line) and line[index : index + 2] in ("||", "&&"):
                    index += 1
                segment_start = index + 1
            index += 1
        segment = line[segment_start:].strip()
        if segment:
            commands.append(segment)
    return commands


def _require_active_preflight(step_body: str, build_workflow: str) -> None:
    commands = _active_commands(step_body)
    loop_index = next((i for i, command in enumerate(commands) if REQUIRED_TOOL_LOOP.fullmatch(command)), None)
    if_index = next((i for i, command in enumerate(commands) if REQUIRED_MISSING_IF.fullmatch(command)), None)
    exit_index = next((i for i, command in enumerate(commands) if re.fullmatch(r"exit\s+1(?:\s+.*)?", command)), None)
    if loop_index is None:
        raise ContractError("local build fail-closed preflight missing active tool loop")
    if if_index is None:
        raise ContractError("local build fail-closed preflight missing active missing-tool if")
    if exit_index is None:
        raise ContractError("local build fail-closed preflight missing active exit 1")
    if not loop_index < if_index < exit_index:
        raise ContractError("local build fail-closed preflight requires active for/if/exit sequence")

    if not any(BUILD_COMMAND.match(command) for command in _active_commands(build_workflow)):
        raise ContractError("local build invocation is missing")


def check_contract(
    backlog: str,
    holds: str,
    build_workflow: str,
    workflow_texts: tuple[str, ...],
) -> None:
    for item in ("B-114", "B-135", "B-138"):
        block = _backlog_block(backlog, item)
        if not re.search(r"(?m)^repo: corelink-runners$", block):
            raise ContractError(f"{item} must remain owned by corelink-runners")
        if not re.search(r"(?m)^status: open$", block):
            raise ContractError(f"{item} cannot be closed from corelink-server")
        if not re.search(r"(?m)^verify: manual$", block):
            raise ContractError(f"{item} must remain manually verified cross-repo")

    hold_heading = "## corelink-runners image family (B-114/B-135/B-138) — HELD"
    if hold_heading not in holds:
        raise ContractError("cross-repo image hold is not documented")
    for phrase in (
        "cannot inspect the sibling repository from this CI token",
        "must not be closed by a corelink-server-only green check",
        "do not re-run the failed image build from this repository",
    ):
        if phrase not in holds:
            raise ContractError(f"image-hold boundary statement missing: {phrase}")

    if not re.search(r"(?m)^    runs-on: corelink$", build_workflow):
        raise ContractError("local container build must use the real corelink label")
    if "  workflow_dispatch:" not in build_workflow:
        raise ContractError("local image build must remain explicitly dispatch-only")
    preflight = re.search(
        rf"(?ms)^      - name: {re.escape(PREFLIGHT_TITLE)}\n(?P<body>.*?)(?=^      - name: |\Z)",
        build_workflow,
    )
    if preflight is None:
        raise ContractError("local build baked-tool preflight step is missing")
    before_preflight = _active_commands(build_workflow[: preflight.start()])
    if any(BUILD_COMMAND.match(command) for command in before_preflight):
        raise ContractError("local build baked-tool preflight must precede the first active build invocation")
    after_preflight = _active_commands(build_workflow[preflight.end() :])
    if not any(BUILD_COMMAND.match(command) for command in after_preflight):
        raise ContractError("local build invocation is missing after the baked-tool preflight")
    _require_active_preflight(preflight.group("body"), build_workflow)

    joined_workflows = "\n".join(workflow_texts)
    if "corelink-runner-devenv" in joined_workflows:
        raise ContractError("server workflows must not invent the absent DevEnv image")
    if re.search(r"corelink\.(rust|gh|node)\.[A-Za-z0-9_.-]+", joined_workflows):
        raise ContractError("server workflows must not pretend to consume external OCI labels")


@dataclass(frozen=True)
class Mutation:
    name: str
    mutate: tuple[str, str]
    expected: str
    target: str


MUTATIONS = (
    Mutation("close-b114", ("status: open", "status: done"), "B-114 cannot be closed", "backlog"),
    Mutation("cross-repo-automation", ("verify: manual", "verify: python3 scripts/fake.py"), "B-114 must remain manually verified", "backlog"),
    Mutation("fake-image-consumer", ("", "corelink-runner-devenv"), "server workflows must not invent", "workflows"),
    Mutation("remove-tool-preflight", ("for t in buildkitd buildctl runc; do", "for t in something-else; do"), "local build fail-closed preflight missing", "build"),
    Mutation("enable-unbounded-build", ("  workflow_dispatch:", "  pull_request:"), "local image build must remain explicitly dispatch-only", "build"),
    Mutation("preflight-after-build", ("__preflight_after_build__", ""), "local build baked-tool preflight must precede", "build-order"),
    Mutation("commented-preflight", ("for t in buildkitd buildctl runc; do", "# for t in buildkitd buildctl runc; do"), "local build fail-closed preflight missing active tool loop", "build-comments"),
    Mutation("echo-preflight-bait", ("for t in buildkitd buildctl runc; do", "echo 'for t in buildkitd buildctl runc; do'"), "local build fail-closed preflight missing active tool loop", "build-echo-bait"),
)


def verify_files(backlog: str, holds: str, build_workflow: str, workflow_texts: tuple[str, ...]) -> None:
    check_contract(backlog, holds, build_workflow, workflow_texts)


def self_test() -> int:
    backlog = BACKLOG.read_text(encoding="utf-8")
    holds = HOLDS.read_text(encoding="utf-8")
    build_workflow = BUILD_WORKFLOW.read_text(encoding="utf-8")
    workflow_texts = tuple(path.read_text(encoding="utf-8") for path in sorted(WORKFLOWS.glob("*.y*ml")))

    for mutation in MUTATIONS:
        old, new = mutation.mutate
        mutated_backlog = backlog
        mutated_build = build_workflow
        mutated_workflows = workflow_texts
        if mutation.target == "backlog":
            block = _backlog_block(backlog, "B-114")
            if block.count(old) != 1:
                raise ContractError(f"{mutation.name}: mutation target count is not one")
            mutated_backlog = backlog.replace(block, block.replace(old, new, 1), 1)
        elif mutation.target == "build":
            if build_workflow.count(old) != 1:
                raise ContractError(f"{mutation.name}: mutation target count is not one")
            mutated_build = build_workflow.replace(old, new, 1)
        elif mutation.target == "build-order":
            preflight = re.search(
                rf"(?ms)^      - name: {re.escape(PREFLIGHT_TITLE)}\n.*?(?=^      - name: |\Z)",
                build_workflow,
            )
            build = re.search(
                r"(?m)^\s*(?:sudo\s+)?buildctl\s+--addr\s+[^\n]*\sbuild(?:\s|$)",
                build_workflow,
            )
            if preflight is None or build is None:
                raise ContractError(f"{mutation.name}: source workflow shape changed")
            preflight_text = preflight.group(0)
            without_preflight = build_workflow[: preflight.start()] + build_workflow[preflight.end() :]
            build_after = re.search(
                r"(?m)^\s*(?:sudo\s+)?buildctl\s+--addr\s+[^\n]*\sbuild(?:\s|$)",
                without_preflight,
            )
            if build_after is None:
                raise ContractError(f"{mutation.name}: build invocation disappeared")
            mutated_build = (
                without_preflight[: build_after.start()]
                + without_preflight[build_after.start() :]
                + "\n"
                + preflight_text
            )
        elif mutation.target == "build-comments":
            old_loop, new_loop = mutation.mutate
            if build_workflow.count(old_loop) != 1:
                raise ContractError(f"{mutation.name}: mutation target count is not one")
            mutated_build = build_workflow.replace(
                f"          {old_loop}\n",
                f"          {new_loop}\n",
                1,
            ).replace(
                '          if [ "$missing" = "1" ]; then\n',
                '          # if [ "$missing" = "1" ]; then\n',
                1,
            ).replace(
                "            exit 1\n",
                "            # exit 1\n",
                1,
            )
        elif mutation.target == "build-echo-bait":
            old_loop, new_loop = mutation.mutate
            if build_workflow.count(old_loop) != 1:
                raise ContractError(f"{mutation.name}: mutation target count is not one")
            mutated_build = build_workflow.replace(
                f"          {old_loop}\n",
                f"          {new_loop}\n",
                1,
            ).replace(
                '          if [ "$missing" = "1" ]; then\n',
                '          echo \'if [ "$missing" = "1" ]; then\'\n',
                1,
            ).replace(
                "            exit 1\n",
                "            echo 'exit 1'\n",
                1,
            )
        elif mutation.target == "workflows":
            mutated_workflows = (workflow_texts[0] + "\ncorelink-runner-devenv",) + workflow_texts[1:]
        try:
            verify_files(mutated_backlog, holds, mutated_build, mutated_workflows)
        except ContractError as error:
            if mutation.expected not in str(error):
                raise ContractError(f"{mutation.name}: wrong rejection reason: {error}") from error
            print(f"B-114/B-135/B-138 MUTATION PASS: {mutation.name} -> {error}")
        else:
            raise ContractError(f"{mutation.name}: weakened contract was accepted")
    return 0


def main() -> int:
    try:
        backlog = BACKLOG.read_text(encoding="utf-8")
        holds = HOLDS.read_text(encoding="utf-8")
        build_workflow = BUILD_WORKFLOW.read_text(encoding="utf-8")
        workflow_texts = tuple(path.read_text(encoding="utf-8") for path in sorted(WORKFLOWS.glob("*.y*ml")))
        verify_files(backlog, holds, build_workflow, workflow_texts)
        if "--self-test" in sys.argv:
            self_test()
    except (OSError, ContractError) as error:
        print(f"RUNNER IMAGE BOUNDARY FAIL: {error}", file=sys.stderr)
        return 1
    print("RUNNER IMAGE BOUNDARY PASS: external image holds remain open and local build wiring is fail-closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
