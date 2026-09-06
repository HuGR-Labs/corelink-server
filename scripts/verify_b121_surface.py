#!/usr/bin/env python3
"""Fail-closed structural and mutation guard for the B-121 parity gate.

``validate_api_surface.py`` is the production comparator.  This companion
guard keeps the gate from becoming vacuous after a repair: it proves that the
comparator and its positive control are still wired, exercises both
divergence directions on synthetic populations, rejects a MISSING_ROUTE
ledger, and requires non-empty live populations before accepting the live
ledger verdict.
"""

from __future__ import annotations

import argparse
import importlib.util
import inspect
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VALIDATOR_PATH = ROOT / "scripts" / "validate_api_surface.py"
WORKFLOW_PATH = ROOT / ".github" / "workflows" / "api-surface-parity.yml"
MIN_DOCUMENTED_PATHS = 30
MIN_CRATE_ROUTES = 50
MIN_WORKER_ROUTES = 3


def _load_validator():
    spec = importlib.util.spec_from_file_location("b121_validator", VALIDATOR_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {VALIDATOR_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _divergences(
    validator, documented, rust_routes, worker_routes, app_routes=None, *, ledger=None
):
    missing_route, missing_doc = validator.compare(
        documented, rust_routes, worker_routes, app_routes or {}
    )
    missing_doc_paths = {path for path, _ in missing_doc}
    undeclared_route = set(missing_route)
    declared_doc = validator.LEDGER_MISSING_DOC if ledger is None else ledger
    undeclared_doc = missing_doc_paths - set(declared_doc)
    stale_doc = set(declared_doc) - missing_doc_paths
    return undeclared_route, undeclared_doc, stale_doc


def _missing_route_ledger_is_invalid(validator) -> bool:
    """The published-lie direction has no exception mechanism."""
    return bool(validator.LEDGER_MISSING_ROUTE)


def mutation_self_test(validator) -> int:
    """Exercise clean, MISSING_ROUTE, MISSING_DOC, and stale-ledger verdicts."""
    documented = {"/v1/known": {"GET"}}
    served = {"/v1/known": {"fixture:1"}}
    empty = _divergences(validator, documented, served, {}, ledger={})
    if any(empty):
        print(f"FAIL: clean mutation fixture diverged: {empty}")
        return 1

    missing_route = dict(documented)
    missing_route["/v1/not-served"] = {"GET"}
    raw_route, _, _ = _divergences(validator, missing_route, served, {}, ledger={})
    if raw_route != {"/v1/not-served"}:
        print(f"FAIL: comparator missed MISSING_ROUTE mutation: {raw_route}")
        return 1

    missing_doc = dict(served)
    missing_doc["/v1/not-documented"] = {"fixture:2"}
    _, raw_doc, _ = _divergences(validator, documented, missing_doc, {}, ledger={})
    if raw_doc != {"/v1/not-documented"}:
        print(f"FAIL: comparator missed MISSING_DOC mutation: {raw_doc}")
        return 1

    old_ledger = validator.LEDGER_MISSING_DOC
    validator.LEDGER_MISSING_DOC = {"/v1/known": "mutation"}
    try:
        _, _, stale = _divergences(
            validator, documented, served, {}, ledger=validator.LEDGER_MISSING_DOC
        )
    finally:
        validator.LEDGER_MISSING_DOC = old_ledger
    if stale != {"/v1/known"}:
        print(f"FAIL: stale-ledger mutation was not rejected: {stale}")
        return 1

    old_route_ledger = validator.LEDGER_MISSING_ROUTE
    validator.LEDGER_MISSING_ROUTE = {"/v1/not-served": "mutation"}
    try:
        route_ledger_rejected = _missing_route_ledger_is_invalid(validator)
    finally:
        validator.LEDGER_MISSING_ROUTE = old_route_ledger
    if not route_ledger_rejected:
        print("FAIL: MISSING_ROUTE ledger mutation was accepted")
        return 1
    print("B-121 mutation self-test passed (clean, missing, and stale controls).")
    return 0


def _wiring_failures(validator) -> list[str]:
    failures: list[str] = []
    required = (
        "parse_openapi_paths",
        "collect_rust_routes",
        "collect_worker_routes",
        "collect_app_routes",
        "collect_app_unsupported",
        "compare",
        "self_test",
    )
    for name in required:
        if not callable(getattr(validator, name, None)):
            failures.append(f"validator missing callable: {name}")
    source = inspect.getsource(validator.main)
    # The production validator now passes the optional app route populations
    # to both calls.  Match the required leading arguments structurally rather
    # than requiring the old three-argument spelling; otherwise this guard
    # reports the live positive control as absent even though main executes it.
    call_prefix = r"\(\s*documented\s*,\s*rust_routes\s*,\s*worker_routes(?:\s*,|\s*\))"
    if not re.search(rf"\bcompare{call_prefix}", source):
        failures.append("validator main does not call compare")
    if not re.search(rf"\bself_test{call_prefix}", source):
        failures.append("validator main does not run the positive control")
    workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
    required_workflow_paths = (
        "openapi/corelink-v1.yaml",
        '"crates/**/*.rs"',
        '"worker/src/**/*.ts"',
        '"apps/**"',
        '"scripts/validate_api_surface.py"',
        '"scripts/verify_b119_surface.py"',
        '"scripts/verify_b121_surface.py"',
        '".github/workflows/api-surface-parity.yml"',
    )
    for path in required_workflow_paths:
        if path not in workflow:
            failures.append(f"workflow path filter missing: {path}")
    if "python3 scripts/validate_api_surface.py" not in workflow:
        failures.append("workflow does not execute validate_api_surface.py")
    if "python3 scripts/verify_b121_surface.py" not in workflow:
        failures.append("workflow does not execute verify_b121_surface.py")
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run structural and mutation controls without scanning the live tree",
    )
    try:
        args = parser.parse_args(argv)
        validator = _load_validator()
        failures = _wiring_failures(validator)
        if failures:
            for failure in failures:
                print(f"FAIL: {failure}")
            return 1
        if mutation_self_test(validator) != 0:
            return 1
        if args.self_test:
            return 0

        documented = validator.parse_openapi_paths(
            validator.OPENAPI.read_text(encoding="utf-8")
        )
        rust_routes = validator.collect_rust_routes()
        worker_routes = validator.collect_worker_routes()
        app_routes = validator.collect_app_routes()
        app_unsupported = validator.collect_app_unsupported()
        populations = (
            ("documented", len(documented), MIN_DOCUMENTED_PATHS),
            ("rust", len(rust_routes), MIN_CRATE_ROUTES),
            ("worker", len(worker_routes), MIN_WORKER_ROUTES),
        )
        if any(actual < minimum for _, actual, minimum in populations):
            print(
                "FAIL: B-121 populations are vacuous: "
                + ", ".join(
                    f"{name}={actual} (minimum {minimum})"
                    for name, actual, minimum in populations
                )
            )
            return 1
        if validator.self_test(
            documented, rust_routes, worker_routes, app_routes, app_unsupported
        ) != 0:
            print("FAIL: B-121 production positive control failed")
            return 1
        if _missing_route_ledger_is_invalid(validator):
            print("FAIL: MISSING_ROUTE is not ledgerable")
            return 1
        route, doc, stale = _divergences(
            validator, documented, rust_routes, worker_routes, app_routes
        )
        if route or doc or stale:
            print(
                "FAIL: B-121 live verdict has undeclared/stale divergence: "
                f"missing_route={sorted(route)}, missing_doc={sorted(doc)}, "
                f"stale={sorted(stale)}"
            )
            return 1
        print(
            "B-121 guard passed: "
            f"{len(documented)} documented, {len(rust_routes)} crate, "
            f"{len(worker_routes)} Worker paths; no undeclared/stale divergence."
        )
        return 0
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"FAIL: B-121 guard could not establish its preconditions: {exc}")
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
