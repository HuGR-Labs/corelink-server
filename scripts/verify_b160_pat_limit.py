#!/usr/bin/env python3
"""Executable B-160 proof gate.

This is deliberately a small, bounded verifier rather than a source grep:
it checks the complete authority path, proves its own teeth with three
mutations, and runs the load-bearing Worker focal suite while reading its
machine-readable result.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import NoReturn

ROOT = Path(__file__).resolve().parent.parent
PNPM_VERSION = "10.32.1"
TEST_FILE = ROOT / "worker/tests/pat_issue_rate_limit.test.ts"
RATE_FILE = ROOT / "worker/src/pat_issue_rate_limit.ts"
DO_FILE = ROOT / "worker/src/durable_object.ts"
EDGE_FILE = ROOT / "worker/src/index_auth.ts"
ROUTE_FILES = (
    ROOT / "crates/corelink-container/src/routes/customer/part-00.rs",
    ROOT / "crates/corelink-container/src/routes/customer/part-01.rs",
)
MAX_TEST_SECONDS = 90

REQUIRED_TEST_MARKERS = (
    "blocks the real DO request before container startup and ignores a forged lease",
    "allows exactly the burst and serializes concurrent aliases",
    "persists the exhausted bucket across a simulated restart",
    "refills at 360 seconds, but not one millisecond before",
    "rejects tenant switching and keeps the original bucket binding",
    "fails closed for storage errors and malformed persisted state",
)


class VerificationError(RuntimeError):
    pass


def fail(message: str) -> NoReturn:
    raise VerificationError(message)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"missing B-160 proof object: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def validate_source(files: dict[str, str]) -> None:
    route = files["route"]
    rate = files["rate"]
    do = files["do"]
    edge = files["edge"]
    test = files["test"]

    required = (
        ("route", 'create_pat_response(state, headers, body.label, body.scopes)'),
        ("route", 'create_pat_response(state, headers, body.name, body.scopes)'),
        ("route", 'PAT_ISSUE_ENDPOINT_ID: &str = "pat-issue"'),
        ("route", "PAT_ISSUE_AUTHORIZED_HEADER"),
        ("route", '#[cfg(not(test))]'),
        ("route", "PAT issuance rate-limit lease unavailable"),
        ("rate", 'export const PAT_ISSUE_BUCKET_KEY = "ratelimit:pat-issue:v1"'),
        ("rate", "state.blockConcurrencyWhile(async () =>"),
        ("rate", "await storage.put(PAT_ISSUE_BUCKET_KEY"),
        ("rate", 'response.headers.set("Retry-After"'),
        ("do", "await this.enforcePatIssueRateLimit("),
        ("do", 'headers.set(PAT_ISSUE_AUTHORIZED_HEADER, "1")'),
        ("edge", '"x-corelink-pat-issue-authorized"'),
        ("edge", "function stripClientTrustHeaders"),
    )
    for name, needle in required:
        if needle not in files[name]:
            fail(f"B-160 runtime proof missing {needle!r} in {name}")

    for marker in REQUIRED_TEST_MARKERS:
        if marker not in test:
            fail(f"B-160 focal suite lost required case: {marker}")
    if "Promise.all" not in test or "360_000" not in test or "503" not in test:
        fail("B-160 focal suite lost concurrency, boundary, or fail-closed assertions")


def source_files() -> dict[str, str]:
    return {
        "route": "\n".join(read(path) for path in ROUTE_FILES),
        "rate": read(RATE_FILE),
        "do": read(DO_FILE),
        "edge": read(EDGE_FILE),
        "test": read(TEST_FILE),
    }


def mutation_checks(files: dict[str, str]) -> None:
    """Ensure a weakened proof object cannot make this verifier pass."""

    mutants = (
        (
            "empty focal test",
            {**files, "test": ""},
            "focal suite lost required case",
        ),
        (
            "disable runtime enforcement",
            {**files, "do": files["do"].replace("await this.enforcePatIssueRateLimit(", "await this.removedPatIssueRateLimit(", 1)},
            "runtime proof missing 'await this.enforcePatIssueRateLimit('",
        ),
        (
            "remove edge lease strip",
            {**files, "edge": files["edge"].replace('  "x-corelink-pat-issue-authorized",\n', "", 1)},
            "runtime proof missing '\"x-corelink-pat-issue-authorized\"'",
        ),
    )
    for name, mutant, expected in mutants:
        if files["do"] == mutant["do"] and name == "disable runtime enforcement":
            fail("B-160 mutation fixture did not change DO enforcement")
        if files["edge"] == mutant["edge"] and name == "remove edge lease strip":
            fail("B-160 mutation fixture did not change edge strip")
        try:
            validate_source(mutant)
        except VerificationError as error:
            if expected not in str(error):
                fail(f"B-160 mutation {name} failed for the wrong reason: {error}")
        else:
            fail(f"B-160 mutation unexpectedly passed: {name}")


def assert_dependencies() -> str:
    package_path = ROOT / "package.json"
    try:
        package = json.loads(read(package_path))
    except json.JSONDecodeError as error:
        fail(f"package.json is not valid JSON: {error}")
    if package.get("packageManager") != f"pnpm@{PNPM_VERSION}":
        fail(f"package.json must pin pnpm@{PNPM_VERSION}")
    for path in (
        ROOT / "pnpm-lock.yaml",
        ROOT / "worker/package.json",
        ROOT / "worker/vitest.config.mts",
        ROOT / "worker/tsconfig.test.json",
        ROOT / "worker/tests/setup.ts",
    ):
        read(path)
    pnpm = os.environ.get("B160_PNPM") or "pnpm"
    try:
        version = subprocess.run(
            [pnpm, "--version"], cwd=ROOT, check=True, capture_output=True,
            text=True, timeout=30,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError) as error:
        fail(f"pnpm@{PNPM_VERSION} is unavailable: {error}")
    if version != PNPM_VERSION:
        fail(f"pnpm {version!r} detected; expected {PNPM_VERSION}")
    vitest = ROOT / "worker/node_modules/.bin/vitest"
    if not vitest.is_file() or not os.access(vitest, os.X_OK):
        fail("worker Vitest executable is missing; deterministic dependencies were not prepared")
    return pnpm


def run_focal(pnpm: str) -> None:
    with tempfile.NamedTemporaryFile(prefix="b160-vitest-", suffix=".json") as report:
        command = [
            pnpm, "--dir", "worker", "exec", "vitest", "run",
            "tests/pat_issue_rate_limit.test.ts", "--reporter=json",
            f"--outputFile={report.name}",
        ]
        try:
            result = subprocess.run(
                command, cwd=ROOT, capture_output=True, text=True,
                timeout=MAX_TEST_SECONDS,
            )
        except subprocess.TimeoutExpired:
            fail(f"B-160 focal Vitest exceeded bounded timeout ({MAX_TEST_SECONDS}s)")
        if result.returncode != 0:
            detail = (result.stdout + "\n" + result.stderr).strip()[-4000:]
            fail(f"B-160 focal Vitest failed (exit {result.returncode}):\n{detail}")
        try:
            report_data = json.loads(Path(report.name).read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            fail(f"B-160 focal Vitest produced no readable JSON report: {error}")
    passed = report_data.get("numPassedTests")
    failed = report_data.get("numFailedTests")
    total = report_data.get("numTotalTests")
    if (passed, failed, total) != (9, 0, 9):
        fail(f"B-160 focal Vitest expected 9/9, got {passed}/{total} passed, {failed} failed")
    assertion_names = {
        assertion.get("fullName", "")
        for suite in report_data.get("testResults", [])
        for assertion in suite.get("assertionResults", [])
    }
    for marker in REQUIRED_TEST_MARKERS:
        if not any(marker in name for name in assertion_names):
            fail(f"B-160 Vitest JSON omitted required executed case: {marker}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run only the verifier mutation teeth")
    args = parser.parse_args()
    try:
        files = source_files()
        validate_source(files)
        mutation_checks(files)
        if args.self_test:
            print("B-160 verifier mutation teeth: 3/3 rejected")
            return 0
        pnpm = assert_dependencies()
        run_focal(pnpm)
        print("B-160 confirmed: runtime seams, 3/3 mutations, and focal Vitest 9/9 passed")
        return 0
    except VerificationError as error:
        print(f"B-160 DRIFTED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
