#!/usr/bin/env python3
"""Parse one signup-worker workflow's active run steps, not comments/prose."""
from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {ROOT / ".github/workflows/signup-worker-deploy.yml", ROOT / ".github/workflows/signup-worker-vitest.yml"}


def strip_yaml_comment(raw: str) -> str:
    quote = ""
    escaped = False
    for index, char in enumerate(raw):
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = ""
        elif char in ('"', "'"):
            quote = char
        elif char == "#" and (index == 0 or raw[index - 1].isspace()):
            return raw[:index].rstrip()
    return raw.rstrip()


def active_run_lines(text: str) -> list[str]:
    lines = text.splitlines(); result: list[str] = []; in_block = False; indent = 0; block_lines = 0; dead: list[int] = []
    for raw in lines:
        raw = strip_yaml_comment(raw)
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"): continue
        current = len(raw) - len(stripped)
        while dead and current < dead[-1]: dead.pop()
        if re.match(r"^if:\s*(?:false|no|0)\s*$", stripped, re.I):
            dead.append(current)
            continue
        if re.match(r"^\s*run:\s*[|>]\s*$", raw):
            in_block = True; indent = len(raw) - len(raw.lstrip()); block_lines = 0
            continue
        if in_block:
            current = len(raw) - len(raw.lstrip())
            if current <= indent and stripped and not stripped.startswith("#"):
                in_block = False
            else:
                if not stripped.startswith("#"):
                    if dead and re.match(r"pnpm\s+install\b", stripped):
                        raise RuntimeError("pnpm install is inside an inactive if:false scope")
                    result.append(stripped); block_lines += 1
                continue
        if re.match(r"^\s*run:\s*[^|]", raw):
            command = raw.split("run:", 1)[1].strip()
            if dead and re.match(r"pnpm\s+install\b", command):
                raise RuntimeError("pnpm install is inside an inactive if:false scope")
            result.append(command)
    if in_block and block_lines == 0:
        raise RuntimeError("empty run block")
    return result


def check(path: Path, text: str) -> None:
    lines = [strip_yaml_comment(raw) for raw in text.splitlines()]
    lines = [line for line in lines if line.strip() and not line.lstrip().startswith("#")]
    expected_job, expected_trigger = ("deploy", "push") if path.name.endswith("deploy.yml") else ("vitest", "pull_request")
    if not any(re.fullmatch(r"on:\s*", line) for line in lines): raise RuntimeError("workflow has no active on block")
    if not any(re.fullmatch(rf"\s{{2}}{expected_trigger}:.*", line) for line in lines): raise RuntimeError(f"{expected_trigger} trigger missing")
    if not any(re.fullmatch(r"\s{2}workflow_dispatch:\s*", line) for line in lines): raise RuntimeError("workflow_dispatch trigger missing")
    if not any(re.fullmatch(rf"\s{{2}}{expected_job}:\s*", line) for line in lines): raise RuntimeError(f"{expected_job} job missing")
    if not any(re.fullmatch(r"\s{4}steps:\s*", line) for line in lines): raise RuntimeError("workflow steps wiring missing")
    if expected_trigger in {"push", "pull_request"} and not any("apps/signup-worker/**" in line for line in lines):
        raise RuntimeError("signup-worker path trigger missing")
    runs = active_run_lines(text)
    if not runs: raise RuntimeError(f"{path} has no active run steps")
    dep = re.compile(r"^npm\s+(?:install|ci)(?:\s|$)")
    if any(dep.match(line) and not re.match(r"^npm\s+install\s+-g\b", line) for line in runs):
        raise RuntimeError(f"{path} has an active npm dependency install")
    frozen = [line for line in runs if re.match(r"^pnpm\s+install\b", line) and "--frozen-lockfile" in line]
    if len(frozen) != 1: raise RuntimeError(f"{path} has {len(frozen)} active frozen pnpm installs; expected exactly one")
    if not any("--ignore-scripts" in line for line in frozen):
        raise RuntimeError(f"{path} frozen install lacks --ignore-scripts")


def self_test(text: str) -> None:
    try: check(Path("mutation"), text.replace("--frozen-lockfile", "", 1))
    except RuntimeError: pass
    else: raise RuntimeError("frozen-lockfile mutation passed")
    try: check(Path("mutation"), text.replace("        run: pnpm install", "        # run: pnpm install", 1))
    except RuntimeError: pass
    else: raise RuntimeError("commented install mutation passed")
    dead = text.replace(
        "        run: pnpm install --frozen-lockfile",
        "        if: false\n        run: pnpm install --frozen-lockfile",
        1,
    )
    try: check(Path("signup-worker-deploy.yml"), dead)
    except RuntimeError: pass
    else: raise RuntimeError("dead-lane install mutation passed")


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("--workflow", type=Path, required=True)
    args = parser.parse_args()
    path = (ROOT / args.workflow) if not args.workflow.is_absolute() else args.workflow
    if path not in ALLOWED or path.is_symlink() or not path.is_file(): print("B-090 invalid or missing workflow"); return 1
    try:
        text = path.read_text(encoding="utf-8"); check(path, text); self_test(text)
    except (OSError, RuntimeError, UnicodeError) as error: print(f"B-090 semantic check FAILED: {error}"); return 1
    print(f"B-090 semantic check PASS: {path.name}"); return 0


if __name__ == "__main__": raise SystemExit(main())
