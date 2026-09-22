#!/usr/bin/env python3
"""Credentialless contract for the Buck2 issue #2020 platform correction."""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "examples/buck2-starter/.buckconfig"
BUCK = ROOT / "examples/buck2-starter/BUCK"
RUNTIME_WORKFLOW = ROOT / ".github/workflows/buck2-starter-ci.yml"

EXPECTED_PLATFORM_SPEC = "target:root//...->prelude//platforms:default"
EXPECTED_SHA = "aa304d471a79f69233b09767d4ba9add769049b7a37f78a3a71a72983372f511"
EXPECTED_LOAD = 'load("@prelude//:rules.bzl", "cxx_binary", "cxx_library")'


class ContractError(ValueError):
    """Raised when a load bearing issue #2020 control is absent or weakened."""


def _active_text(text: str) -> str:
    return "\n".join(
        line for line in text.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    )


def _require(text: str, marker: str, label: str) -> None:
    if marker not in text:
        raise ContractError(f"{label}: missing {marker!r}")


def _section(text: str, name: str) -> str:
    match = re.search(
        rf"(?ms)^\[{re.escape(name)}\]\n(?P<body>.*?)(?=^\[|\Z)", text
    )
    if match is None:
        raise ContractError(f"buckconfig: missing [{name}] section")
    return match.group("body")


def _platform_spec(config: str) -> str:
    body = _section(config, "parser")
    values = re.findall(
        r"(?m)^\s*target_platform_detector_spec\s*=\s*(\S+)\s*$", body
    )
    if values != [EXPECTED_PLATFORM_SPEC]:
        raise ContractError(
            "buckconfig/parser: target platform must be the one pinned bundled "
            f"spec {EXPECTED_PLATFORM_SPEC!r}; found {values!r}"
        )
    return values[0]


def verify(config: str, buck: str, runtime_workflow: str) -> None:
    """Verify the platform fix and the surrounding public starter invariants."""

    _platform_spec(config)
    for marker in (
        "[cells]",
        "    root = .",
        "    prelude = prelude",
        "    toolchains = toolchains",
        "[cell_aliases]",
        "    config = prelude",
        "    fbsource = root",
        "[external_cells]",
        "    prelude = bundled",
        "execution_platforms = prelude//platforms:default",
        "url = https://corelink-api.humangr.com/bazel/cache",
        "http_headers = Authorization: Bearer ${CORELINK_PAT}",
        "read = true",
        "write = true",
        "remote_cache_address = https://corelink-api.humangr.com/bazel/cache",
        "hash_algorithm = BLAKE3",
    ):
        _require(config, marker, "buckconfig")

    if config.count(EXPECTED_PLATFORM_SPEC) != 1:
        raise ContractError("buckconfig: platform detector must be pinned exactly once")
    if re.search(r"(?i)(fbcode|fbsource//|config//)", config):
        raise ContractError("buckconfig: Meta-internal graph state is referenced")

    if buck.count(EXPECTED_LOAD) != 1:
        raise ContractError("BUCK: public C++ rule load must appear exactly once")
    if "@prelude//cxx:cxx.bzl" in buck:
        raise ContractError("BUCK: private C++ rule path is present")
    for marker in ("cxx_library(", "cxx_binary(", 'name = "hello"'):
        _require(buck, marker, "BUCK")

    active_workflow = _active_text(runtime_workflow)
    if active_workflow.count("workflow_dispatch: {}") != 1:
        raise ContractError("runtime workflow: dispatch trigger must appear once")
    if re.search(r"(?m)^\s+(pull_request|push|schedule):", active_workflow):
        raise ContractError("runtime workflow: runtime lane must remain dispatch-only")
    for marker in (
        'BUCK2_VERSION: "2026-08-01"',
        'BUCK2_BUILD_VERSION: "2026-07-31"',
        f'BUCK2_SHA256: "{EXPECTED_SHA}"',
        'CORELINK_PAT: ${{ secrets.CORELINK_CANARY_PAT }}',
        '"${ACTUAL_SHA256}" != "${BUCK2_SHA256}"',
    ):
        _require(active_workflow, marker, "runtime workflow")


def _expect_rejected(
    label: str,
    mutate: Callable[[dict[str, str]], None],
    sources: dict[str, str],
) -> None:
    candidate = dict(sources)
    mutate(candidate)
    try:
        verify(candidate["config"], candidate["buck"], candidate["runtime_workflow"])
    except ContractError:
        return
    raise AssertionError(f"mutation unexpectedly passed: {label}")


def mutation_checks(sources: dict[str, str]) -> None:
    """Prove missing, wrong, and unpinned platform mutations fail closed."""

    verify(sources["config"], sources["buck"], sources["runtime_workflow"])
    mutations: tuple[tuple[str, Callable[[dict[str, str]], None]], ...] = (
        (
            "missing default platform",
            lambda s: s.__setitem__(
                "config",
                s["config"].replace(
                    "[parser]\n    # Resolve every root target against the bundled public default platform.\n"
                    "    # This keeps configuration features available without importing a\n"
                    "    # repository-specific or Meta-internal platform graph.\n"
                    f"    target_platform_detector_spec = {EXPECTED_PLATFORM_SPEC}\n\n",
                    "",
                    1,
                ),
            ),
        ),
        (
            "wrong default platform",
            lambda s: s.__setitem__(
                "config",
                s["config"].replace(
                    EXPECTED_PLATFORM_SPEC,
                    "target:root//...->prelude//platforms:linux",
                    1,
                ),
            ),
        ),
        (
            "un-pinned default platform",
            lambda s: s.__setitem__(
                "config",
                s["config"].replace(
                    EXPECTED_PLATFORM_SPEC,
                    "prelude//platforms:default",
                    1,
                ),
            ),
        ),
        (
            "Meta-internal platform graph",
            lambda s: s.__setitem__(
                "config",
                s["config"].replace(
                    EXPECTED_PLATFORM_SPEC,
                    "target:root//...->fbsource//platforms:default",
                    1,
                ),
            ),
        ),
        (
            "runtime lane reactivation",
            lambda s: s.__setitem__(
                "runtime_workflow",
                s["runtime_workflow"].replace(
                    "  workflow_dispatch: {}",
                    "  push:\n    branches: [main]\n  workflow_dispatch: {}",
                    1,
                ),
            ),
        ),
        (
            "public C++ rule drift",
            lambda s: s.__setitem__(
                "buck",
                s["buck"].replace(EXPECTED_LOAD, 'load("@prelude//cxx:cxx.bzl", "cxx_binary", "cxx_library")', 1),
            ),
        ),
    )
    for label, mutate in mutations:
        _expect_rejected(label, mutate, sources)


def _sources() -> dict[str, str]:
    return {
        "config": CONFIG.read_text(encoding="utf-8"),
        "buck": BUCK.read_text(encoding="utf-8"),
        "runtime_workflow": RUNTIME_WORKFLOW.read_text(encoding="utf-8"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    sources = _sources()
    verify(sources["config"], sources["buck"], sources["runtime_workflow"])
    if args.self_test:
        mutation_checks(sources)
    result = {
        "issue": 2020,
        "contract": "buck2-pinned-default-target-platform",
        "mutation_checks": bool(args.self_test),
    }
    print(json.dumps(result, sort_keys=True) if args.json else "issue #2020 Buck2 platform contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
