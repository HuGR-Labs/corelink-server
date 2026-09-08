#!/usr/bin/env python3
"""Executable B-160 proof gate.

This is deliberately a small, bounded verifier rather than a source grep:
it checks the complete authority path, proves its own teeth with twelve
mutations, and runs the load-bearing Worker focal suite while reading its
machine-readable result.
"""

from __future__ import annotations

import argparse
import ast
import json
import os
import re
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
# The trust-header authority lives in the policy module; index_auth.ts is only
# a re-export shim after the worker source split.
EDGE_FILE = ROOT / "worker/src/index_auth_policy.ts"
AUTH_SHIM_FILE = ROOT / "worker/src/index_auth.ts"
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


def _ts_tokens(source: str) -> list[tuple[str, str]]:
    """Tokenize enough TypeScript to isolate one real initializer.

    Comments (including nested block comments) and all quoted/template
    strings are opaque tokens. This prevents declaration-shaped bait inside
    prose, strings, or templates from becoming verifier input.
    """
    tokens: list[tuple[str, str]] = []
    index = 0
    while index < len(source):
        character = source[index]
        if character.isspace():
            index += 1
            continue
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = len(source) if newline < 0 else newline + 1
            continue
        if source.startswith("/*", index):
            index += 2
            depth = 1
            while index < len(source) and depth:
                # Treat a comment opener at a token boundary as a nested
                # comment. This handles nested bait while leaving prose such
                # as `/v1/privacy/*` inside a documentation comment alone.
                if source.startswith("/*", index) and (
                    index == 0 or source[index - 1].isspace()
                ):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                fail("B-160 CLIENT_TRUST_HEADERS has an unterminated comment")
            continue
        if character in {'"', "'", "`"}:
            quote = character
            start = index
            index += 1
            terminated = False
            while index < len(source):
                if source[index] == "\\":
                    index += 2
                elif source[index] == quote:
                    index += 1
                    terminated = True
                    break
                else:
                    index += 1
            if not terminated:
                fail(f"B-160 CLIENT_TRUST_HEADERS has an unterminated {quote} string")
            kind = "template" if quote == "`" else "string"
            tokens.append((kind, source[start:index]))
            continue
        if character.isalpha() or character in {"_", "$"}:
            end = index + 1
            while end < len(source) and (source[end].isalnum() or source[end] in {"_", "$"}):
                end += 1
            tokens.append(("ident", source[index:end]))
            index = end
            continue
        if character.isdigit():
            end = index + 1
            while end < len(source) and (source[end].isalnum() or source[end] in "._"):
                end += 1
            tokens.append(("number", source[index:end]))
            index = end
            continue
        tokens.append(("punct", character))
        index += 1
    return tokens


def _client_trust_header_literals(source: str) -> tuple[str, ...]:
    """Extract exact literals from exactly one authoritative initializer."""
    tokens = _ts_tokens(source)
    declarations = [
        index for index in range(len(tokens) - 1)
        if tokens[index][0] == "ident"
        and tokens[index][1] in {"const", "let", "var"}
        and tokens[index + 1] == ("ident", "CLIENT_TRUST_HEADERS")
    ]
    if len(declarations) != 1 or tokens[declarations[0]][1] != "const":
        fail("B-160 CLIENT_TRUST_HEADERS initializer is ambiguous; expected exactly one const declaration")

    declaration = declarations[0]
    expected = ["const", "CLIENT_TRUST_HEADERS", ":", "ReadonlyArray", "<", "string", ">", "=", "["]
    actual = [value for _, value in tokens[declaration : declaration + len(expected)]]
    if actual != expected:
        fail("B-160 CLIENT_TRUST_HEADERS initializer is malformed")
    opening = declaration + len(expected) - 1
    depth = 0
    closing: int | None = None
    for index in range(opening, len(tokens)):
        if tokens[index][1] == "[":
            depth += 1
        elif tokens[index][1] == "]":
            depth -= 1
            if depth == 0:
                closing = index
                break
    if closing is None or closing + 1 >= len(tokens) or tokens[closing + 1][1] != ";":
        fail("B-160 CLIENT_TRUST_HEADERS initializer is malformed")

    literals: list[str] = []
    entry: list[tuple[str, str]] = []

    def consume() -> None:
        if not entry:
            return
        if len(entry) != 1:
            fail(f"B-160 CLIENT_TRUST_HEADERS contains non-literal or unknown entry: {entry!r}")
        kind, raw = entry[0]
        if kind == "ident" and raw == "STORAGE_QUOTA_HEADER":
            return
        if kind != "string":
            fail(f"B-160 CLIENT_TRUST_HEADERS contains non-literal or unknown entry: {entry!r}")
        try:
            value = ast.literal_eval(raw)
        except (SyntaxError, ValueError) as error:
            fail(f"B-160 CLIENT_TRUST_HEADERS contains malformed string literal: {error}")
        if not isinstance(value, str):
            fail("B-160 CLIENT_TRUST_HEADERS contains a non-string literal")
        literals.append(value)

    for token in tokens[opening + 1 : closing]:
        if token[1] == ",":
            consume()
            entry = []
        else:
            entry.append(token)
    consume()
    if literals.count("x-corelink-pat-issue-authorized") != 1:
        fail("B-160 authoritative CLIENT_TRUST_HEADERS list lacks exactly one PAT lease header")
    return tuple(literals)


def validate_source(files: dict[str, str]) -> None:
    route = files["route"]
    rate = files["rate"]
    do = files["do"]
    edge = files["edge"]
    shim = files["shim"]
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
        ("edge", "function stripClientTrustHeaders"),
    )
    for name, needle in required:
        if needle not in files[name]:
            fail(f"B-160 runtime proof missing {needle!r} in {name}")

    if "x-corelink-pat-issue-authorized" not in _client_trust_header_literals(edge):
        fail("B-160 authoritative CLIENT_TRUST_HEADERS list lacks exact PAT lease header")

    # index_auth.ts is the production-facing module imported by the worker's
    # request stages. The policy implementation must remain wired through its
    # re-export; checking the policy file alone would allow a dead helper to
    # satisfy this verifier.
    policy_export = re.search(
        r"export\s*\{(?P<exports>.*?)\}\s*from\s*[\"']\./index_auth_policy\.js[\"']",
        shim,
        re.DOTALL,
    )
    if policy_export is None or "stripClientTrustHeaders" not in policy_export.group("exports"):
        fail("B-160 production auth shim does not re-export stripClientTrustHeaders from index_auth_policy.js")

    # Prove behavior, not just the header-list marker: the exported helper
    # must iterate the authoritative list and delete every client value. The
    # compact body match also rejects a commented-out or no-op mutation.
    strip_body = re.search(
        r"export\s+function\s+stripClientTrustHeaders\(h:\s*Headers\):\s*void\s*\{"
        r"\s*for\s*\(const\s+name\s+of\s+CLIENT_TRUST_HEADERS\)\s*\{"
        r"\s*h\.delete\(name\);\s*\}\s*\}",
        edge,
        re.DOTALL,
    )
    if strip_body is None:
        fail("B-160 runtime proof missing semantic header removal in stripClientTrustHeaders")

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
        "shim": read(AUTH_SHIM_FILE),
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
            "authoritative CLIENT_TRUST_HEADERS list lacks exactly one PAT lease header",
        ),
        (
            "comment bait in trust-header list",
            {**files, "edge": files["edge"].replace('  "x-corelink-pat-issue-authorized",\n', '  // "x-corelink-pat-issue-authorized",\n', 1)},
            "authoritative CLIENT_TRUST_HEADERS list lacks exactly one PAT lease header",
        ),
        (
            "single-quoted bait outside initializer",
            {**files, "edge": files["edge"].replace(
                '  "x-corelink-pat-issue-authorized",\n', "", 1
            ) + "\nconst B160_SINGLE_BAIT = 'x-corelink-pat-issue-authorized';\n"},
            "authoritative CLIENT_TRUST_HEADERS list lacks exactly one PAT lease header",
        ),
        (
            "double-quoted bait outside initializer",
            {**files, "edge": files["edge"].replace(
                '  "x-corelink-pat-issue-authorized",\n', "", 1
            ) + '\nconst B160_DOUBLE_BAIT = "x-corelink-pat-issue-authorized";\n'},
            "authoritative CLIENT_TRUST_HEADERS list lacks exactly one PAT lease header",
        ),
        (
            "template bait outside initializer",
            {**files, "edge": files["edge"].replace(
                '  "x-corelink-pat-issue-authorized",\n', "", 1
            ) + "\nconst B160_TEMPLATE_BAIT = `x-corelink-pat-issue-authorized`;\n"},
            "authoritative CLIENT_TRUST_HEADERS list lacks exactly one PAT lease header",
        ),
        (
            "nested comment bait outside initializer",
            {**files, "edge": files["edge"].replace(
                '  "x-corelink-pat-issue-authorized",\n', "", 1
            ) + '\n/* outer /* nested "x-corelink-pat-issue-authorized" */ still hidden */\n'},
            "authoritative CLIENT_TRUST_HEADERS list lacks exactly one PAT lease header",
        ),
        (
            "duplicate trust-header initializer",
            {**files, "edge": files["edge"] + '\nconst CLIENT_TRUST_HEADERS: ReadonlyArray<string> = ["x-corelink-pat-issue-authorized"];\n'},
            "CLIENT_TRUST_HEADERS initializer is ambiguous",
        ),
        (
            "dynamic trust-header entry",
            {**files, "edge": files["edge"].replace(
                '  "x-corelink-pat-issue-authorized",\n', "  makePatIssueHeader(),\n", 1
            )},
            "contains non-literal or unknown entry",
        ),
        (
            "no-op edge strip",
            {**files, "edge": files["edge"].replace("h.delete(name);", "void name;", 1)},
            "semantic header removal",
        ),
        (
            "rewired production auth shim",
            {**files, "shim": files["shim"].replace('} from "./index_auth_policy.js";', '} from "./index_auth_timing.js";', 2)},
            "production auth shim does not re-export",
        ),
    )
    for name, mutant, expected in mutants:
        if files["do"] == mutant["do"] and name == "disable runtime enforcement":
            fail("B-160 mutation fixture did not change DO enforcement")
        if files["edge"] == mutant["edge"] and name == "remove edge lease strip":
            fail("B-160 mutation fixture did not change edge strip")
        if files["edge"] == mutant["edge"] and name == "no-op edge strip":
            fail("B-160 mutation fixture did not change edge behavior")
        if files["edge"] == mutant["edge"] and name == "comment bait in trust-header list":
            fail("B-160 mutation fixture did not change trust-header list")
        if files["edge"] == mutant["edge"] and name in {
            "single-quoted bait outside initializer",
            "double-quoted bait outside initializer",
            "template bait outside initializer",
            "nested comment bait outside initializer",
            "duplicate trust-header initializer",
            "dynamic trust-header entry",
        }:
            fail(f"B-160 mutation fixture did not change trust-header parser input: {name}")
        if files["shim"] == mutant["shim"] and name == "rewired production auth shim":
            fail("B-160 mutation fixture did not change production shim")
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
            print("B-160 verifier mutation teeth: 12/12 rejected")
            return 0
        pnpm = assert_dependencies()
        run_focal(pnpm)
        print("B-160 confirmed: runtime seams, 12/12 mutations, and focal Vitest 9/9 passed")
        return 0
    except VerificationError as error:
        print(f"B-160 DRIFTED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
