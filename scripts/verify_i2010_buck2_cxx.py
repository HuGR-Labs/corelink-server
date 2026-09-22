#!/usr/bin/env python3
"""Credentialless, mutation backed contract for Buck2 issue #2010.

The runtime Buck2 lane remains a manual, post merge dispatch.  This verifier
checks the source contract without downloading Buck2, contacting CoreLink, or
reading a secret.  Its mutations are deliberately close to the load bearing
controls so a weakened static check cannot pass by accident.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
BUCK = ROOT / "examples/buck2-starter/BUCK"
WORKFLOW = ROOT / ".github/workflows/buck2-starter-ci.yml"
CONFIG = ROOT / "examples/buck2-starter/.buckconfig"
TOOLCHAIN = ROOT / "examples/buck2-starter/toolchains/BUCK"

EXPECTED_LOAD = 'load("@prelude//:rules.bzl", "cxx_binary", "cxx_library")'
OBSOLETE_LOAD = '@prelude//cxx:cxx.bzl'
EXPECTED_SHA = "aa304d471a79f69233b09767d4ba9add769049b7a37f78a3a71a72983372f511"


class ContractError(ValueError):
    """Raised when a required issue #2010 control is absent or weakened."""


def _active_lines(text: str) -> list[str]:
    """Return nonblank, noncomment lines for checks of executable YAML text."""

    return [line for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]


def _active_text(text: str) -> str:
    return "\n".join(_active_lines(text))


def _require(text: str, marker: str, label: str) -> None:
    if marker not in text:
        raise ContractError(f"{label}: missing {marker!r}")


def _job_body(workflow: str, job: str) -> str:
    match = re.search(rf"(?ms)^  {re.escape(job)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)", workflow)
    if match is None:
        raise ContractError(f"workflow: missing job {job}")
    return match.group("body")


def _require_once(text: str, marker: str, label: str) -> None:
    count = text.count(marker)
    if count != 1:
        raise ContractError(f"{label}: expected one {marker!r}, found {count}")


def verify(
    buck: str,
    workflow: str,
    config: str,
    toolchain: str,
) -> None:
    """Verify the complete repository contract for the focused correction."""

    if buck.count(EXPECTED_LOAD) != 1:
        raise ContractError("BUCK: the public C++ rule load must appear exactly once")
    if OBSOLETE_LOAD in buck:
        raise ContractError("BUCK: obsolete private cxx.bzl load is present")
    _require(buck, 'cxx_binary(', "BUCK")
    _require(buck, 'cxx_library(', "BUCK")
    if re.search(r"(?m)^\s*load\(.*cxx\.bzl", buck):
        raise ContractError("BUCK: implementation cxx.bzl must not be loaded directly")

    active_workflow = _active_text(workflow)
    _require_once(active_workflow, "workflow_dispatch: {}", "workflow")
    if re.search(r"(?m)^\s+(pull_request|push|schedule):", active_workflow):
        raise ContractError("workflow: Buck2 runtime lane must remain dispatch-only")
    _require(active_workflow, "permissions:\n  contents: read", "workflow")
    for forbidden in ("git push", "gh pr", "wrangler deploy", "gh workflow run"):
        if forbidden in active_workflow:
            raise ContractError(f"workflow: forbidden mutation {forbidden!r}")

    for job, timeout in (("build", "timeout-minutes: 20"), ("negative-scenarios", "timeout-minutes: 10"), ("benchmark", "timeout-minutes: 30")):
        body = _job_body(workflow, job)
        _require(body, "runs-on: ubuntu-latest", f"workflow/{job}")
        _require(body, timeout, f"workflow/{job}")

    for marker in (
        'BUCK2_VERSION: "2026-08-01"',
        'BUCK2_BUILD_VERSION: "2026-07-31"',
        f'BUCK2_SHA256: "{EXPECTED_SHA}"',
        'BUCK2_INSTALL_DIR: "${{ github.workspace }}/.buck2-bin"',
        'CORELINK_PAT: ${{ secrets.CORELINK_CANARY_PAT }}',
        '"${ACTUAL_SHA256}" != "${BUCK2_SHA256}"',
    ):
        _require(active_workflow, marker, "workflow/pinned-runtime")

    build = _active_text(_job_body(workflow, "build"))
    for marker in (
        'name: Install Buck2 latest stable',
        'RELEASE_URL="https://github.com/facebook/buck2/releases/download/${BUCK2_VERSION}/buck2-${ARCH}.zst"',
        '"${BUCK2_INSTALL_DIR}/buck2" --version',
        "time buck2 build :hello \\",
        "test -s /tmp/cold-report.json",
        "test -s /tmp/warm-report.json",
        "scripts/parse_buck2_build_report.py",
        "--report /tmp/warm-report.json",
        "RATIO < 80",
        'CORELINK_PAT: ${{ secrets.CORELINK_CANARY_PAT }}',
    ):
        _require(build, marker, "workflow/build")

    negative = _active_text(_job_body(workflow, "negative-scenarios"))
    for marker in (
        "unset CORELINK_PAT",
        'export CORELINK_PAT="invalid_pat_b113"',
        "192.0.2.1/bad-cache",
        "(( RESULT != 0 ))",
        "authentication",
        'CORELINK_PAT: ${{ secrets.CORELINK_QUOTA_PAT }}',
    ):
        _require(negative, marker, "workflow/negative-scenarios")

    for marker in (
        '[cells]',
        "    root = .",
        "    prelude = prelude",
        "    toolchains = toolchains",
        '[cell_aliases]',
        "    config = prelude",
        "    fbsource = root",
        "[external_cells]",
        "    prelude = bundled",
        "execution_platforms = prelude//platforms:default",
        "root//platforms:corelink-cache",
        "digest_algorithms = SHA256",
        "detailed_aggregated_metrics = true",
        "engine_address = https://corelink-api.humangr.com",
        "action_cache_address = https://corelink-api.humangr.com",
        "cas_address = https://corelink-api.humangr.com",
        "instance_name = replace-with-pat-tenant-id",
        "http_headers = Authorization: Bearer $CORELINK_PAT",
        "hash_algorithm = SHA256",
    ):
        _require(config, marker, "buckconfig")
    _require(toolchain, 'load("@prelude//toolchains:cxx.bzl", "system_cxx_toolchain")', "toolchains/BUCK")
    _require(toolchain, 'system_cxx_toolchain(', "toolchains/BUCK")


def _expect_rejected(label: str, mutate: Callable[[dict[str, str]], None], sources: dict[str, str]) -> None:
    candidate = dict(sources)
    mutate(candidate)
    try:
        verify(candidate["buck"], candidate["workflow"], candidate["config"], candidate["toolchain"])
    except ContractError:
        return
    raise AssertionError(f"mutation unexpectedly passed: {label}")


def mutation_checks(sources: dict[str, str]) -> None:
    """Prove the contract rejects each representative load bearing mutation."""

    verify(sources["buck"], sources["workflow"], sources["config"], sources["toolchain"])
    mutations: tuple[tuple[str, Callable[[dict[str, str]], None]], ...] = (
        (
            "obsolete C++ load path",
            lambda s: s.__setitem__("buck", s["buck"].replace(EXPECTED_LOAD, 'load("@prelude//cxx:cxx.bzl", "cxx_binary", "cxx_library")')),
        ),
        (
            "obsolete C++ symbol set",
            lambda s: s.__setitem__("buck", s["buck"].replace(EXPECTED_LOAD, 'load("@prelude//:rules.bzl", "cxx_binary")')),
        ),
        (
            "Buck2 version drift",
            lambda s: s.__setitem__("workflow", s["workflow"].replace('BUCK2_VERSION: "2026-08-01"', 'BUCK2_VERSION: "2026-07-31"', 1)),
        ),
        (
            "Buck2 checksum drift",
            lambda s: s.__setitem__("workflow", s["workflow"].replace(EXPECTED_SHA, "0" * 64, 1)),
        ),
        (
            "remote cache write policy",
            lambda s: s.__setitem__("config", s["config"].replace("    cas_address = https://corelink-api.humangr.com", "    cas_address = https://example.invalid", 1)),
        ),
        (
            "missing PAT negative probe",
            lambda s: s.__setitem__("workflow", s["workflow"].replace("unset CORELINK_PAT", "# unset CORELINK_PAT", 1)),
        ),
        (
            "hosted runner drift",
            lambda s: s.__setitem__("workflow", s["workflow"].replace("runs-on: ubuntu-latest", "runs-on: self-hosted", 1)),
        ),
        (
            "runtime lane reactivation",
            lambda s: s.__setitem__("workflow", s["workflow"].replace("  workflow_dispatch: {}", "  push:\n    branches: [main]\n  workflow_dispatch: {}", 1)),
        ),
        (
            "cell alias drift",
            lambda s: s.__setitem__("config", s["config"].replace("    fbsource = root", "    fbsource = prelude", 1)),
        ),
    )
    for label, mutate in mutations:
        _expect_rejected(label, mutate, sources)


def _sources() -> dict[str, str]:
    return {
        "buck": BUCK.read_text(encoding="utf-8"),
        "workflow": WORKFLOW.read_text(encoding="utf-8"),
        "config": CONFIG.read_text(encoding="utf-8"),
        "toolchain": TOOLCHAIN.read_text(encoding="utf-8"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run representative mutation checks")
    parser.add_argument("--json", action="store_true", help="emit a redacted machine readable result")
    args = parser.parse_args()
    sources = _sources()
    verify(sources["buck"], sources["workflow"], sources["config"], sources["toolchain"])
    if args.self_test:
        mutation_checks(sources)
    result = {"issue": 2010, "contract": "buck2-public-cxx-rules", "mutation_checks": bool(args.self_test)}
    print(json.dumps(result, sort_keys=True) if args.json else "issue #2010 Buck2 C++ contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
