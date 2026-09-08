#!/usr/bin/env python3
"""Semantic, fail-closed verifier for B-067's test-execution contract.

The old backlog probe counted strings.  That allowed a commented command, a
``--no-run`` compile, or an unrelated mutation job to look like coverage.  This
verifier inspects active workflow lines and the bounded runner script, then runs
negative controls against mutated copies so each assertion has a failure mode.
It is stdlib-only because it runs on the CoreLink runner image.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/corelink-auth-pat.yml"
NIGHTLY = ROOT / ".github/workflows/nightly.yml"
WELCOME = ROOT / ".github/workflows/welcome-first-pr.yml"
WORKSPACE_SCRIPT = ROOT / "scripts/ci-bounded-workspace-tests.sh"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def active_lines(text: str) -> list[str]:
    """Return non-comment lines, rejecting tabs in indentation."""
    result: list[str] = []
    for raw in text.splitlines():
        if "\t" in raw[: len(raw) - len(raw.lstrip())]:
            raise ValueError("tabs in workflow indentation are not supported")
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        result.append(raw)
    return result


def fail(message: str) -> None:
    raise AssertionError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def verify_auth_workflow(workflow_text: str) -> None:
    lines = active_lines(workflow_text)
    active = "\n".join(lines)

    require("  pull_request:" in active, "auth lane must have a pull_request trigger")
    require("      - 'crates/**'" in active, "auth trigger must include the crate population")
    for path in (
        "migrations/002_auth_tables.sql",
        "migrations/d1/**",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "scripts/ci-assert-pinned-toolchain.sh",
        ".github/workflows/corelink-auth-pat.yml",
    ):
        require(f"      - '{path}'" in active, f"auth trigger misses {path}")

    require("runs-on: corelink" in active, "auth lane must use the ephemeral corelink runner")
    require("self-hosted" not in active and "ubuntu-" not in active, "auth lane escaped runner boundary")
    timeout_values = re.findall(r"^    timeout-minutes:\s*([0-9]+)\s*$", active, re.MULTILINE)
    require(timeout_values == ["30"], "auth lane must have one explicit 30-minute bound")
    require("persist-credentials: false" in active, "checkout must not persist credentials")

    # Every actual test command must be an active command line.  Require all
    # three contract targets separately; one combined grep cannot prove that
    # both authentication crates execute.
    required_commands = (
        "cargo test --locked --package corelink-auth --all-targets",
        "cargo test --locked --package corelink-pat --all-targets",
        "cargo test --locked --release --package corelink-pat --test constant_time",
    )
    for command in required_commands:
        require(
            any(
                line.strip() == f"run: {command}"
                for line in lines
            ),
            f"missing active test execution command: {command}",
        )
    require(
        not any(
            "cargo test" in line and "--no-run" in line
            for line in lines
        ),
        "auth lane contains a test compile-only command",
    )

    # No shell-only echo or comment can satisfy the command checks above.
    for line in lines:
        if "cargo test --locked --package corelink-auth --all-targets" in line:
            require(line.strip().startswith("run: cargo test") or line.strip().startswith("cargo test"), "auth test is not executable")
        if "cargo test --locked --package corelink-pat --all-targets" in line:
            require(line.strip().startswith("run: cargo test") or line.strip().startswith("cargo test"), "PAT test is not executable")

    # Action pinning is a supply-chain invariant for this newly-added lane.
    for number, line in enumerate(lines, 1):
        match = re.search(r"\buses:\s*[^\s]+@([^\s#]+)", line)
        if match:
            require(SHA_RE.fullmatch(match.group(1)) is not None, f"unpinned action in auth lane line {number}")


def verify_nightly(nightly_text: str) -> None:
    lines = active_lines(nightly_text)
    active = "\n".join(lines)
    require(
        any(
            line.strip() == "run: bash scripts/ci-bounded-workspace-tests.sh"
            for line in lines
        ),
        "nightly lane must invoke the bounded workspace execution script",
    )
    require(
        not any(
            "cargo test" in line and "--workspace" in line and "--no-run" in line
            for line in lines
        ),
        "nightly workspace lane still contains compile-only --no-run",
    )
    require("CARGO_BUILD_JOBS: \"4\"" in active, "nightly workspace lane must cap compiler jobs at four")


def verify_workspace_script(script_text: str) -> None:
    executable_lines = "\n".join(
        line for line in script_text.splitlines()
        if not line.lstrip().startswith("#")
    )
    require("cargo nextest run --workspace --all-targets --profile ci" in executable_lines, "nextest execution command missing")
    require("cargo test --workspace --all-targets --no-fail-fast" in executable_lines, "cargo execution fallback missing")
    require("--no-run" not in executable_lines, "bounded workspace script regressed to --no-run")
    require("start_new_session=True" in script_text, "workspace subprocess is not process-group bounded")
    require(
        'deadline_seconds="${CORELINK_WORKSPACE_TEST_TIMEOUT_SECONDS:-1500}"' in script_text,
        "workspace deadline default must be 1500s",
    )
    require(
        "deadline_seconds > 1500" in executable_lines,
        "workspace deadline ceiling must reject values above 1500s",
    )
    require(
        "CORELINK_WORKSPACE_TEST_TIMEOUT_SECONDS must be in [1,1500]" in executable_lines,
        "workspace deadline error must advertise the 1500s ceiling",
    )
    require("raise SystemExit(124)" in script_text, "workspace timeout must fail nonzero")
    require("raise SystemExit(status)" in script_text, "workspace exit status must be propagated")
    require("SystemExit(0)" not in script_text, "workspace timeout/error must not be masked as success")
    require("CARGO_BUILD_JOBS" in script_text and "jobs > 4" in script_text, "workspace compiler bound missing")


def verify_welcome(welcome_text: str) -> None:
    require("CI runs all three" not in "\n".join(active_lines(welcome_text)), "welcome still claims a workspace PR gate")
    normalized = re.sub(r"\s+", " ", welcome_text)
    require("workspace-wide `cargo test` is not part of the PR gate today" in normalized, "welcome must state the actual PR boundary")


def verify_tree() -> None:
    require(WORKFLOW.is_file(), "auth workflow is missing")
    require(NIGHTLY.is_file(), "nightly workflow is missing")
    require(WELCOME.is_file(), "welcome workflow is missing")
    require(WORKSPACE_SCRIPT.is_file(), "bounded workspace test script is missing")
    verify_auth_workflow(WORKFLOW.read_text())
    verify_nightly(NIGHTLY.read_text())
    verify_workspace_script(WORKSPACE_SCRIPT.read_text())
    verify_welcome(WELCOME.read_text())


def run_self_tests() -> None:
    """Prove that command, path, runner, and no-run mutations are rejected."""
    original_workflow = WORKFLOW.read_text()
    original_nightly = NIGHTLY.read_text()
    original_script = WORKSPACE_SCRIPT.read_text()

    mutations = (
        ("auth command commented", lambda text: text.replace(
            "        run: cargo test --locked --package corelink-auth --all-targets",
            "        # run: cargo test --locked --package corelink-auth --all-targets",
            1,
        ), verify_auth_workflow, original_workflow),
        ("auth command no-run", lambda text: text.replace(
            "cargo test --locked --package corelink-auth --all-targets",
            "cargo test --locked --package corelink-auth --all-targets --no-run",
            1,
        ), verify_auth_workflow, original_workflow),
        ("auth command ignores failure", lambda text: text.replace(
            "run: cargo test --locked --package corelink-auth --all-targets",
            "run: cargo test --locked --package corelink-auth --all-targets || true",
            1,
        ), verify_auth_workflow, original_workflow),
        ("auth runner", lambda text: text.replace("runs-on: corelink", "runs-on: ubuntu-latest", 1), verify_auth_workflow, original_workflow),
        ("auth trigger", lambda text: text.replace("      - 'crates/**'\n", "", 1), verify_auth_workflow, original_workflow),
        ("nightly no-run", lambda text: text.replace(
            "bash scripts/ci-bounded-workspace-tests.sh",
            "cargo test --release --workspace --no-run",
            1,
        ), verify_nightly, original_nightly),
        ("nightly command hidden behind echo", lambda text: text.replace(
            "run: bash scripts/ci-bounded-workspace-tests.sh",
            "run: echo bash scripts/ci-bounded-workspace-tests.sh",
            1,
        ), verify_nightly, original_nightly),
        ("workspace script no-run", lambda text: text.replace(
            "cargo test --workspace --all-targets --no-fail-fast",
            "cargo test --workspace --all-targets --no-run",
            1,
        ), verify_workspace_script, original_script),
        ("workspace timeout default 1800", lambda text: text.replace(
            "CORELINK_WORKSPACE_TEST_TIMEOUT_SECONDS:-1500",
            "CORELINK_WORKSPACE_TEST_TIMEOUT_SECONDS:-1800",
            1,
        ), verify_workspace_script, original_script),
        ("workspace timeout ceiling 3600", lambda text: text.replace(
            "deadline_seconds > 1500",
            "deadline_seconds > 3600",
            1,
        ), verify_workspace_script, original_script),
        ("workspace timeout masks failure", lambda text: text.replace(
            "raise SystemExit(124)",
            "raise SystemExit(0)",
            1,
        ), verify_workspace_script, original_script),
    )
    for name, mutate, checker, source in mutations:
        mutated = mutate(source)
        require(mutated != source, f"self-test mutation did not change input: {name}")
        try:
            checker(mutated)
        except AssertionError:
            continue
        fail(f"negative control was accepted: {name}")
    print(f"B-067 semantic self-test: {len(mutations)} mutations rejected")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run adversarial negative controls")
    args = parser.parse_args(argv)
    try:
        verify_tree()
        if args.self_test:
            run_self_tests()
    except (AssertionError, OSError, ValueError) as error:
        print(f"B-067 verification FAILED: {error}", file=sys.stderr)
        return 1
    print("B-067 verification PASS: bounded workspace execution + explicit auth/PAT execution + truthful welcome")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
