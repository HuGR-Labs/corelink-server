#!/usr/bin/env python3
"""Credentialless contract and mutation gate for Buck2 issue #2047.

The runtime Buck2 dispatch remains manual and authenticated.  This verifier
checks the workflow's source contract without downloading Buck2, reading a
secret, contacting CoreLink, or dispatching the long lane.
"""
from __future__ import annotations

import argparse
import copy
import json
import re
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/buck2-starter-ci.yml"
CONTRACT_WORKFLOW = ROOT / ".github/workflows/issue-2047-buck2-cache-contract.yml"
CONFIG = ROOT / "examples/buck2-starter/.buckconfig"
BUCK = ROOT / "examples/buck2-starter/BUCK"
TOOLCHAIN = ROOT / "examples/buck2-starter/toolchains/BUCK"
BENCHMARK = ROOT / "examples/buck2-starter/scripts/benchmark.sh"

EXPECTED_CACHE_KEY = "buck2-build-${{ github.repository }}-${{ github.sha }}"
EXPECTED_CACHE_PATH = "examples/buck2-starter/buck-out/v2/cache"
EXPECTED_SHA = "aa304d471a79f69233b09767d4ba9add769049b7a37f78a3a71a72983372f511"
ACTION_CACHE = "actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9"
ACTION_CACHE_SAVE = "actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9"
ACTION_ARTIFACT = "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"
ACTION_DOWNLOAD = "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c"


class ContractError(ValueError):
    """Raised when a load-bearing #2047 control is absent or weakened."""


def _require(text: str, marker: str, label: str) -> None:
    if marker not in text:
        raise ContractError(f"{label}: missing {marker!r}")


def _require_once(text: str, marker: str, label: str) -> None:
    count = text.count(marker)
    if count != 1:
        raise ContractError(f"{label}: expected one {marker!r}, found {count}")


def _require_before(text: str, earlier: str, later: str, label: str) -> None:
    try:
        earlier_index = text.index(earlier)
        later_index = text.index(later)
    except ValueError as exc:
        raise ContractError(f"{label}: missing ordering marker") from exc
    if earlier_index >= later_index:
        raise ContractError(f"{label}: {earlier!r} must precede {later!r}")


def _job(workflow: str, name: str) -> str:
    match = re.search(
        rf"(?ms)^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        workflow,
    )
    if match is None:
        raise ContractError(f"workflow: missing job {name}")
    return match.group("body")


def _active(text: str) -> str:
    return "\n".join(
        line for line in text.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    )


def _require_cache_pair(job: str, label: str, *, restore: bool, save: bool = False) -> None:
    action = ACTION_CACHE_SAVE if save else ACTION_CACHE
    _require_once(job, action, f"{label}/cache")
    start = job.index(f"uses: {action}")
    end = job.find("\n      - ", start)
    step = job[start:] if end == -1 else job[start:end]
    _require(step, f"path: {EXPECTED_CACHE_PATH}", f"{label}/cache")
    _require(step, f"key: {EXPECTED_CACHE_KEY}", f"{label}/cache")
    if "restore-keys:" in step:
        raise ContractError(f"{label}/cache: broad restore-keys fallback is forbidden")
    if restore and save:
        raise ContractError(f"{label}/cache: restore and save boundaries must be separate")


def verify(
    workflow: str,
    config: str,
    buck: str,
    toolchain: str,
    benchmark: str,
) -> None:
    """Verify the complete authenticated warm-cache contract."""

    active = _active(workflow)
    _require_once(active, "workflow_dispatch: {}", "runtime workflow")
    if re.search(r"(?m)^\s+(push|pull_request|schedule):", active):
        raise ContractError("runtime workflow: Buck2 lane must remain dispatch-only")
    _require(active, "permissions:\n  contents: read", "runtime workflow")
    if "permissions:" in _job(active, "benchmark"):
        raise ContractError("runtime workflow/benchmark: job-level permissions are forbidden")

    for marker in (
        "BUCK2_VERSION: \"2026-08-01\"",
        "BUCK2_BUILD_VERSION: \"2026-07-31\"",
        f'BUCK2_SHA256: "{EXPECTED_SHA}"',
        'CORELINK_PAT: ${{ secrets.CORELINK_CANARY_PAT }}',
        '"${ACTUAL_SHA256}" != "${BUCK2_SHA256}"',
    ):
        _require(active, marker, "runtime workflow/pins")

    build = _active(_job(active, "build"))
    negative = _active(_job(active, "negative-scenarios"))
    benchmark_job = _active(_job(active, "benchmark"))
    for label, job, timeout in (
        ("build", build, "timeout-minutes: 20"),
        ("negative-scenarios", negative, "timeout-minutes: 10"),
        ("benchmark", benchmark_job, "timeout-minutes: 30"),
    ):
        _require(job, "runs-on: ubuntu-latest", f"runtime workflow/{label}")
        _require(job, timeout, f"runtime workflow/{label}")

    _require_cache_pair(build, "runtime workflow/build", restore=True)
    _require_cache_pair(build, "runtime workflow/build", restore=False, save=True)
    _require_cache_pair(benchmark_job, "runtime workflow/benchmark", restore=True)
    _require_before(
        build,
        "id: validate-pat",
        "name: Install Buck2 latest stable",
        "runtime workflow/build/authentication order",
    )
    _require_before(
        benchmark_job,
        "name: Validate CORELINK_PAT before benchmark",
        "name: Install Buck2",
        "runtime workflow/benchmark/authentication order",
    )

    for marker in (
        "id: validate-pat",
        'echo "present=true" >> "${GITHUB_OUTPUT}"',
        "id: warm-build",
        "scripts/parse_buck2_build_report.py",
        "--report /tmp/warm-report.json",
        "RATIO < 80",
        "buck2-build-${GITHUB_REPOSITORY}-${GITHUB_SHA}",
        "authenticated_pat=true",
    ):
        _require(build, marker, "runtime workflow/build")
    _require(build, "steps.validate-pat.outputs.present == 'true'", "runtime workflow/build/cache-save")
    _require(build, "steps.warm-build.outcome == 'success'", "runtime workflow/build/cache-save")
    _require(build, "/tmp/warm-report.json", "runtime workflow/build/receipt")

    for marker in (
        "unset CORELINK_PAT",
        "refusing unauthenticated remote-cache build",
        "exit 78",
        "(( RESULT != 0 ))",
    ):
        _require(negative, marker, "runtime workflow/negative-scenarios")

    for marker in (
        "needs.build.result == 'success'",
        ACTION_DOWNLOAD,
        "name: buck2-build-reports",
        "Require numeric warm receipt before benchmark",
        "parse_buck2_build_report.py",
        "warm_cache_ratio",
        "RATIO < 80",
        "name: buck2-benchmark-report",
        "BENCHMARK.json",
    ):
        _require(benchmark_job, marker, "runtime workflow/benchmark admission")

    for marker in (
        "[cells]",
        "    root = .",
        "    prelude = prelude",
        "    toolchains = toolchains",
        "[cell_aliases]",
        "    fbsource = root",
        "[external_cells]",
        "    prelude = bundled",
        "target_platform_detector_spec = target:root//...->prelude//platforms:default",
        "execution_platforms = prelude//platforms:default",
        "root//platforms:corelink-cache",
        "detailed_aggregated_metrics = true",
        "engine_address = https://corelink-api.humangr.com",
        "action_cache_address = https://corelink-api.humangr.com",
        "cas_address = https://corelink-api.humangr.com",
        "instance_name = replace-with-pat-tenant-id",
        "http_headers = Authorization: Bearer $CORELINK_PAT",
        "hash_algorithm = SHA256",
    ):
        _require(config, marker, "buckconfig")
    _require(buck, 'load("@prelude//:rules.bzl", "cxx_binary", "cxx_library")', "BUCK")
    _require(toolchain, 'load("@prelude//toolchains:cxx.bzl", "system_cxx_toolchain")', "toolchains/BUCK")
    _require(benchmark, "build_metrics.metrics.remote_cache_hits", "benchmark parser")
    _require(benchmark, "declared_actions", "benchmark parser")
    _require(benchmark, "buck2-benchmark-receipt-v1", "benchmark receipt")
    if "git push" in active or "git commit" in active:
        raise ContractError("runtime workflow: repository mutation is forbidden")


def _expect_rejected(
    label: str,
    mutate: Callable[[dict[str, str]], None],
    sources: dict[str, str],
) -> None:
    candidate = copy.deepcopy(sources)
    mutate(candidate)
    try:
        verify(**candidate)
    except ContractError:
        return
    raise AssertionError(f"mutation unexpectedly passed: {label}")


def mutation_checks(sources: dict[str, str]) -> None:
    """Reject representative auth, namespace, admission, and pin mutations."""

    verify(**sources)
    mutations: tuple[tuple[str, Callable[[dict[str, str]], None]], ...] = (
        (
            "broad cache restore fallback",
            lambda s: s.__setitem__(
                "workflow",
                s["workflow"].replace(
                    "          key: buck2-build-${{ github.repository }}-${{ github.sha }}\n",
                    "          key: buck2-build-${{ github.repository }}-${{ github.sha }}\n          restore-keys: buck2-build-\n",
                    1,
                ),
            ),
        ),
        (
            "cross-repository cache namespace",
            lambda s: s.__setitem__(
                "workflow",
                s["workflow"].replace(
                    f"key: {EXPECTED_CACHE_KEY}",
                    "key: buck2-build-${{ github.repository }}",
                    1,
                ),
            ),
        ),
        (
            "cache save without authenticated success gate",
            lambda s: s.__setitem__(
                "workflow",
                s["workflow"].replace(
                    "steps.validate-pat.outputs.present == 'true' && steps.warm-build.outcome == 'success'",
                    "steps.warm-build.outcome == 'success'",
                    1,
                ),
            ),
        ),
        (
            "missing PAT fallback",
            lambda s: s.__setitem__(
                "workflow",
                s["workflow"].replace(
                    "refusing unauthenticated remote-cache build",
                    "attempting local fallback",
                    1,
                ),
            ),
        ),
        (
            "benchmark without warm admission",
            lambda s: s.__setitem__(
                "workflow",
                s["workflow"].replace(
                    "needs.build.result == 'success'",
                    "needs.build.result == 'always'",
                    1,
                ),
            ),
        ),
        (
            "report threshold weakening",
            lambda s: s.__setitem__(
                "workflow",
                s["workflow"].replace("RATIO < 80", "RATIO < 50", 1),
            ),
        ),
        (
            "Buck2 checksum drift",
            lambda s: s.__setitem__("workflow", s["workflow"].replace(EXPECTED_SHA, "0" * 64, 1)),
        ),
        (
            "remote cache write policy drift",
            lambda s: s.__setitem__("config", s["config"].replace("    cas_address = https://corelink-api.humangr.com", "    cas_address = https://example.invalid", 1)),
        ),
        (
            "prelude platform drift",
            lambda s: s.__setitem__(
                "config",
                s["config"].replace(
                    "target:root//...->prelude//platforms:default",
                    "target:root//...->prelude//platforms:linux",
                    1,
                ),
            ),
        ),
        (
            "benchmark receipt removal",
            lambda s: s.__setitem__(
                "benchmark",
                s["benchmark"].replace("buck2-benchmark-receipt-v1", "untyped-receipt", 1),
            ),
        ),
    )
    for label, mutate in mutations:
        _expect_rejected(label, mutate, sources)


def _sources() -> dict[str, str]:
    return {
        "workflow": WORKFLOW.read_text(encoding="utf-8"),
        "config": CONFIG.read_text(encoding="utf-8"),
        "buck": BUCK.read_text(encoding="utf-8"),
        "toolchain": TOOLCHAIN.read_text(encoding="utf-8"),
        "benchmark": BENCHMARK.read_text(encoding="utf-8"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    sources = _sources()
    verify(**sources)
    if args.self_test:
        mutation_checks(sources)
    result = {
        "issue": 2047,
        "contract": "buck2-authenticated-warm-cache",
        "mutation_checks": bool(args.self_test),
    }
    print(json.dumps(result, sort_keys=True) if args.json else "issue #2047 Buck2 cache contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
