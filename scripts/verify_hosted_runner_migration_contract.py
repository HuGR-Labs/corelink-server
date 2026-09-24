#!/usr/bin/env python3
"""Static, closed-world admission contract for shared hosted-runner bundles.

This verifier reads workflow YAML as inert text.  It never imports candidate
code or starts a build, deployment, provider operation, release, publication,
load test, or other mutation.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path


class ContractError(RuntimeError):
    pass


HOSTED = {"ubuntu-24.04", "macos-15", "macos-15-intel"}
INVENTORIES = {
    "rust-correctness": {
        ".github/workflows/corelink-worker.yml": {
            "pr-gate", "wasm-build", "fuzz-smoke", "fuzz-nightly", "mutants-nightly"
        },
        ".github/workflows/corelink-reapi.yml": {"pr-gate", "wasm-build", "mutants-nightly"},
    },
    "repository-policy": {
        ".github/workflows/dependabot-auto-merge.yml": {"auto-merge"},
        ".github/workflows/dependabot-policy.yml": {"sentinel", "policy-gate"},
        ".github/workflows/dependabot-policy-trust-boundary.yml": {"trust-boundary-teeth"},
    },
}
SHA = re.compile(r"[0-9a-f]{40}")
JOB = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
RUNNER = re.compile(r"^    runs-on:\s*(.*?)\s*(?:#.*)?$")


def fail(message: str) -> None:
    raise ContractError(message)


def job_blocks(text: str, path: str) -> dict[str, list[str]]:
    lines = text.splitlines()
    try:
        start = next(index for index, line in enumerate(lines) if line == "jobs:") + 1
    except StopIteration:
        fail(f"{path}: missing jobs mapping")
    jobs: dict[str, list[str]] = {}
    current: str | None = None
    for line in lines[start:]:
        match = JOB.match(line)
        if match:
            current = match.group(1)
            if current in jobs:
                fail(f"{path}: duplicate job {current}")
            jobs[current] = []
        elif current is not None:
            jobs[current].append(line)
    if not jobs:
        fail(f"{path}: no jobs found")
    return jobs


def require_credentialless_checkout(lines: list[str], path: str, job: str) -> None:
    for index, line in enumerate(lines):
        if "uses: actions/checkout@" not in line:
            continue
        indent = len(line) - len(line.lstrip())
        step_indent = indent - 2
        end = len(lines)
        for cursor in range(index + 1, len(lines)):
            candidate = lines[cursor]
            if candidate.startswith(" " * step_indent + "- "):
                end = cursor
                break
        step = "\n".join(lines[index:end])
        if not re.search(r"(?m)^\s+persist-credentials:\s*false\s*(?:#.*)?$", step):
            fail(f"{path}:{job}: checkout must set persist-credentials: false")


def validate(root: Path, inventory: str) -> None:
    try:
        files = INVENTORIES[inventory]
    except KeyError as error:
        fail(f"unknown inventory {inventory!r}")
        raise AssertionError from error
    for relative, expected_jobs in files.items():
        path = root / relative
        try:
            jobs = job_blocks(path.read_text(encoding="utf-8"), relative)
        except OSError as error:
            fail(f"{relative}: unreadable: {error}")
        if jobs.keys() != expected_jobs:
            fail(
                f"{relative}: closed job inventory drifted; "
                f"expected {sorted(expected_jobs)}, got {sorted(jobs)}"
            )
        for job, lines in jobs.items():
            runner_lines = [match.group(1).strip() for line in lines if (match := RUNNER.match(line))]
            if len(runner_lines) != 1:
                fail(f"{relative}:{job}: expected exactly one runs-on selector")
            runner = runner_lines[0]
            if runner not in HOSTED:
                fail(f"{relative}:{job}: disallowed runner {runner!r}")
            require_credentialless_checkout(lines, relative, job)


def assert_exact_head(root: Path, expected_head: str) -> None:
    if SHA.fullmatch(expected_head) is None:
        fail("expected head must be a full lowercase SHA")
    actual = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"], check=True, capture_output=True, text=True
    ).stdout.strip()
    if actual != expected_head:
        fail(f"candidate checkout is {actual}, expected {expected_head}")


def fixture(root: Path, inventory: str) -> None:
    for relative, jobs in INVENTORIES[inventory].items():
        body = ["name: fixture", "on: pull_request", "permissions:", "  contents: read", "jobs:"]
        for job in sorted(jobs):
            body.extend((f"  {job}:", "    runs-on: ubuntu-24.04", "    steps:",
                         "      - uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
                         "        with:", "          persist-credentials: false"))
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("\n".join(body) + "\n", encoding="utf-8")


def expect_rejected(root: Path, inventory: str, old: str, new: str) -> None:
    target = root / next(iter(INVENTORIES[inventory]))
    original = target.read_text(encoding="utf-8")
    if old not in original:
        fail(f"fixture lost mutation anchor {old!r}")
    target.write_text(original.replace(old, new, 1), encoding="utf-8")
    try:
        validate(root, inventory)
    except ContractError:
        pass
    else:
        fail(f"mutation escaped hosted runner contract: {old!r}")
    target.write_text(original, encoding="utf-8")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for inventory in INVENTORIES:
            fixture(root, inventory)
            validate(root, inventory)
            expect_rejected(root, inventory, "runs-on: ubuntu-24.04", "runs-on: corelink")
            expect_rejected(root, inventory, "runs-on: ubuntu-24.04", "runs-on: self-hosted")
            expect_rejected(root, inventory, "persist-credentials: false", "persist-credentials: true")
            target = root / next(iter(INVENTORIES[inventory]))
            original = target.read_text(encoding="utf-8")
            target.write_text(
                original + "  uncontracted-job:\n    runs-on: ubuntu-24.04\n    steps: []\n",
                encoding="utf-8",
            )
            try:
                validate(root, inventory)
            except ContractError:
                pass
            else:
                fail("uncontracted hosted job escaped the closed inventory")
            target.write_text(original, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    parser.add_argument("--inventory", choices=tuple(INVENTORIES))
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            if args.root or args.inventory or args.expected_head:
                fail("--self-test cannot be combined with candidate arguments")
            self_test()
        elif args.root and args.inventory and args.expected_head:
            assert_exact_head(args.root, args.expected_head)
            validate(args.root, args.inventory)
        else:
            fail("pass --self-test or --root, --inventory, and --expected-head")
    except (ContractError, subprocess.CalledProcessError) as error:
        print(f"hosted-runner contract: FAIL: {error}", file=sys.stderr)
        return 1
    print("hosted-runner contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
