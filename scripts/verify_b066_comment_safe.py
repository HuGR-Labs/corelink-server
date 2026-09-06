#!/usr/bin/env python3
"""Structure-aware, comment-safe semantic guard for B-066."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:  # pragma: no cover - exercised in hermetic/minimal runners
    yaml = None  # type: ignore[assignment]


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/smoke-install.yml"
PAT_MAPPING = "      CORELINK_CANARY_PAT: ${{ secrets.CORELINK_CANARY_PAT }}"
PAT_CONDITION = '          if [ -n "${CORELINK_CANARY_PAT:-}" ]; then'
PREFLIGHT_NAME = "Preflight — Docker-compatible backend (hard requirement)"


class VerificationError(RuntimeError):
    pass


class _NoDuplicateKeysLoader(yaml.SafeLoader if yaml is not None else object):
    """SafeLoader variant that rejects duplicate YAML mapping keys."""


def _construct_no_duplicate(loader: Any, node: Any, deep: bool = False) -> dict[Any, Any]:
    seen: set[Any] = set()
    for key_node, _ in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in seen:
            raise VerificationError(f"B-066 duplicate YAML key: {key!r}")
        seen.add(key)
    return yaml.SafeLoader.construct_mapping(loader, node, deep=deep)


if yaml is not None:
    _NoDuplicateKeysLoader.add_constructor(
        yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
        _construct_no_duplicate,
    )


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise VerificationError(f"B-066 required artifact unreadable: {path}") from error


def _parse_workflow(source: str) -> dict[str, Any]:
    if yaml is None:
        raise VerificationError("B-066 requires PyYAML; refusing unverifiable workflow")
    try:
        document = yaml.load(source, Loader=_NoDuplicateKeysLoader)
    except (yaml.YAMLError, VerificationError) as error:
        raise VerificationError(f"B-066 workflow YAML is not safely parseable: {error}") from error
    if not isinstance(document, dict):
        raise VerificationError("B-066 workflow root is not a mapping")
    return document


def strip_shell_comments(source: str) -> str:
    """Blank shell comments without changing quoted strings or line layout."""
    lines: list[str] = []
    for line in source.splitlines(keepends=True):
        quote = ""
        escaped = False
        result: list[str] = []
        for position, char in enumerate(line):
            if char == "\n":
                result.append(char)
                continue
            if escaped:
                result.append(char)
                escaped = False
                continue
            if char == "\\" and quote == '"':
                result.append(char)
                escaped = True
                continue
            if char in ('"', "'"):
                if not quote:
                    quote = char
                elif quote == char:
                    quote = ""
                result.append(char)
                continue
            if char == "#" and not quote:
                result.extend(" " for _ in line[position:].rstrip("\n"))
                if line.endswith("\n"):
                    result.append("\n")
                break
            result.append(char)
        lines.append("".join(result))
    return "".join(lines)


def _preflight_run(document: dict[str, Any]) -> str:
    jobs = document.get("jobs")
    smoke = jobs.get("smoke") if isinstance(jobs, dict) else None
    if not isinstance(smoke, dict):
        raise VerificationError("B-066 canonical jobs.smoke job is missing")
    env = smoke.get("env")
    if not isinstance(env, dict) or env.get("CORELINK_CANARY_PAT") != "${{ secrets.CORELINK_CANARY_PAT }}":
        raise VerificationError("B-066 jobs.smoke env lacks the canonical PAT secret mapping")
    steps = smoke.get("steps")
    if not isinstance(steps, list):
        raise VerificationError("B-066 jobs.smoke steps are missing or malformed")
    matches = [step for step in steps if isinstance(step, dict) and step.get("name") == PREFLIGHT_NAME]
    if len(matches) != 1:
        raise VerificationError(f"B-066 expected exactly one canonical preflight step, found {len(matches)}")
    step = matches[0]
    run = step.get("run")
    if step.get("shell") != "bash" or not isinstance(run, str) or not run.strip():
        raise VerificationError("B-066 canonical preflight step lacks a bash run block")
    return run


def _reachable_if(lines: list[str], index: int) -> bool:
    """Reject a condition nested under an unclosed ``if false`` shell block."""
    stack: list[bool] = []
    for line in lines[:index]:
        stripped = line.strip()
        if re.match(r"^if\b.*;\s*then\s*$", stripped):
            stack.append(bool(re.match(r"^if\s+false\s*;\s*then$", stripped)))
        elif stripped == "fi" and stack:
            stack.pop()
    return not any(stack)


def _branch(lines: list[str], index: int) -> list[str]:
    depth = 0
    body: list[str] = []
    for line in lines[index + 1 :]:
        stripped = line.strip()
        if re.match(r"^if\b.*;\s*then\s*$", stripped):
            depth += 1
        elif stripped == "fi":
            if depth == 0:
                return body
            depth -= 1
        body.append(line)
    raise VerificationError("B-066 preflight condition has no closing fi")


def verify_text(source: str) -> None:
    document = _parse_workflow(source)
    run = _preflight_run(document)
    code = strip_shell_comments(run)
    lines = code.splitlines()
    backend_re = re.compile(r"^\s*if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then\s*$")
    pat_re = re.compile(r'^\s*if \[ -n "\$\{CORELINK_CANARY_PAT:-\}" \]; then\s*$')
    backend = [i for i, line in enumerate(lines) if backend_re.fullmatch(line)]
    pat = [i for i, line in enumerate(lines) if pat_re.fullmatch(line)]
    if len(backend) != 1:
        raise VerificationError(f"B-066 canonical docker preflight condition count={len(backend)}")
    if len(pat) != 1:
        raise VerificationError(f"B-066 canonical PAT condition count={len(pat)}")
    if backend[0] >= pat[0]:
        raise VerificationError("B-066 PAT guard is not after the protected docker preflight")
    if not _reachable_if(lines, backend[0]) or not _reachable_if(lines, pat[0]):
        raise VerificationError("B-066 preflight condition is unreachable shell code")
    if "set -uo pipefail" not in lines:
        raise VerificationError("B-066 preflight lost set -uo pipefail")
    backend_body = _branch(lines, backend[0])
    if not any("Docker-compatible backend reachable." in line for line in backend_body):
        raise VerificationError("B-066 docker success action is missing from the canonical preflight")
    if not any(line.strip() == "exit 1" for line in backend_body):
        raise VerificationError("B-066 docker failure branch is not fail-closed")
    pat_body = _branch(lines, pat[0])
    if not any("CORELINK_CANARY_PAT" in line for line in pat_body):
        raise VerificationError("B-066 PAT failure branch lost its protected credential evidence")
    if not any(line.strip() == "exit 1" for line in pat_body):
        raise VerificationError("B-066 PAT absence branch is not fail-closed")


def _must_reject(label: str, source: str) -> None:
    try:
        verify_text(source)
    except VerificationError:
        return
    raise VerificationError(f"B-066 mutation survived: {label}")


def mutation_self_test(source: str) -> None:
    if PAT_MAPPING not in source or PAT_CONDITION not in source:
        raise VerificationError("B-066 canonical mutation fixtures are missing")
    _must_reject(
        "missing env with comment bait",
        source.replace(PAT_MAPPING, "      # " + PAT_MAPPING.lstrip(), 1) + f"\n# {PAT_MAPPING}\n",
    )
    _must_reject(
        "false PAT condition in real preflight",
        source.replace(PAT_CONDITION, '          if [ -n "" ]; then', 1),
    )
    _must_reject(
        "PAT condition moved to unrelated run",
        source.replace(PAT_CONDITION, "          echo 'PAT guard moved'", 1).replace(
            "      - name: Build smoke image\n",
            "      - name: unrelated bait\n        run: |\n"
            + PAT_CONDITION
            + "\n          exit 1\n          fi\n\n"
            + "      - name: Build smoke image\n",
            1,
        ),
    )
    _must_reject(
        "string-only PAT condition",
        source.replace(PAT_CONDITION, "          echo '" + PAT_CONDITION.strip() + "'", 1),
    )
    _must_reject(
        "dead PAT condition in real preflight",
        source.replace(PAT_CONDITION, "          if false; then\n" + PAT_CONDITION, 1),
    )
    _must_reject(
        "duplicate canonical preflight step",
        source.replace(
            "      - name: Build smoke image\n",
            source[source.index("      - name: Preflight"):source.index("      - name: Build smoke image")]
            + "      - name: Build smoke image\n",
            1,
        ),
    )


def main() -> int:
    source = read(WORKFLOW)
    verify_text(source)
    mutation_self_test(source)
    print("B-066 structure/comment-safe semantic/mutation guard PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
