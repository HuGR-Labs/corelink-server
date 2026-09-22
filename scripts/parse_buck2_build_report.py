#!/usr/bin/env python3
"""Read the documented cache counters from a Buck2 build report.

The pinned Buck2 release does not emit the old, guessed top-level
``cache_hits`` and ``total_actions`` fields.  Its documented report schema
places the counters at ``build_metrics.metrics.remote_cache_hits`` and
``build_metrics.metrics.declared_actions`` when
``buck2.detailed_aggregated_metrics`` is enabled.

This parser is deliberately fail-closed.  A missing, renamed, non-numeric,
negative, fractional, or internally inconsistent counter is an error instead
of a zero or a warning.  It uses only Python's standard library so the hosted
contract check does not need a package install.
"""
from __future__ import annotations

import argparse
import copy
import json
import math
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PINNED_FIXTURE = ROOT / "tests/fixtures/buck2-warm-report-2026-08-01.json"


class ReportError(ValueError):
    """The report cannot support a trustworthy cache ratio."""


def _reject_non_finite(value: str) -> Any:
    raise ReportError(f"non-finite JSON number {value!r}")


def load_report(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"), parse_constant=_reject_non_finite)
    except ReportError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ReportError(f"report is unreadable: {exc}") from exc
    if not isinstance(value, dict):
        raise ReportError("report must be a JSON object")
    return value


def _object(value: Any, path: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ReportError(f"{path} must be an object")
    return value


def _counter(metrics: dict[str, Any], name: str) -> int:
    if name not in metrics:
        raise ReportError(f"missing documented counter {name}")
    value = metrics[name]
    # bool is an int subclass in Python, but is not a JSON counter.
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ReportError(f"{name} must be numeric")
    if not math.isfinite(float(value)):
        raise ReportError(f"{name} must be finite")
    if float(value) != math.floor(float(value)):
        raise ReportError(f"{name} must be an integer counter")
    return int(value)


def parse_counters(report: dict[str, Any]) -> dict[str, int]:
    if report.get("success") is not True:
        raise ReportError("report does not describe a successful build")
    build_metrics = _object(report.get("build_metrics"), "build_metrics")
    metrics = _object(build_metrics.get("metrics"), "build_metrics.metrics")
    cache_hits = _counter(metrics, "remote_cache_hits")
    total_actions = _counter(metrics, "declared_actions")

    if cache_hits < 0:
        raise ReportError("remote_cache_hits must be non-negative")
    if total_actions <= 0:
        raise ReportError("declared_actions must be positive")
    if cache_hits > total_actions:
        raise ReportError("remote_cache_hits cannot exceed declared_actions")

    return {"cache_hits": cache_hits, "total_actions": total_actions}


def _fixture() -> dict[str, Any]:
    """A minimal copy of the pinned report shape with documented metrics."""
    return {
        "trace_id": "b113-fixture",
        "success": True,
        "results": {"root//:hello": {"success": "SUCCESS", "configured": {}}},
        "failures": {},
        "project_root": "/runner/examples/buck2-starter",
        "truncated": False,
        "strings": {},
        "build_metrics": {
            "action_graph_size": None,
            "metrics": {
                "full_graph_execution_time_ms": 12.0,
                "full_graph_output_size_bytes": 123.0,
                "local_execution_time_ms": 0.0,
                "remote_execution_time_ms": 0.0,
                "local_executions": 0.0,
                "remote_executions": 0.0,
                "remote_cache_hits": 4.0,
                "analysis_retained_memory": 0.0,
                "declared_actions": 4.0,
            },
        },
    }


def self_test() -> dict[str, Any]:
    good = parse_counters(_fixture())
    pinned = parse_counters(load_report(PINNED_FIXTURE))
    mutations: dict[str, dict[str, Any]] = {}

    mutated = copy.deepcopy(_fixture())
    del mutated["build_metrics"]
    mutations["absent build_metrics"] = mutated

    mutated = copy.deepcopy(_fixture())
    metrics = mutated["build_metrics"]["metrics"]
    metrics["cache_hits"] = metrics.pop("remote_cache_hits")
    mutations["renamed remote_cache_hits"] = mutated

    mutated = copy.deepcopy(_fixture())
    mutated["build_metrics"]["metrics"]["declared_actions"] = "4"
    mutations["string counter"] = mutated

    mutated = copy.deepcopy(_fixture())
    mutated["build_metrics"]["metrics"]["remote_cache_hits"] = -1
    mutations["negative counter"] = mutated

    mutated = copy.deepcopy(_fixture())
    mutated["build_metrics"]["metrics"]["remote_cache_hits"] = 5
    mutations["inconsistent counters"] = mutated

    rejected: list[str] = []
    for name, candidate in mutations.items():
        try:
            parse_counters(candidate)
        except ReportError:
            rejected.append(name)
        else:
            raise ReportError(f"mutation was accepted: {name}")

    if len(rejected) != len(mutations):
        raise ReportError("not every report mutation was rejected")
    return {
        "fixture": good,
        "pinned_fixture": pinned,
        "rejected_mutations": rejected,
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, help="Buck2 build report JSON")
    parser.add_argument("--self-test", action="store_true", help="run fixture and mutation checks")
    parser.add_argument("--json", action="store_true", help="emit counters as JSON")
    args = parser.parse_args(argv)

    try:
        if args.self_test:
            result: dict[str, Any] = self_test()
        elif args.report is not None:
            result = parse_counters(load_report(args.report))
        else:
            parser.error("one of --report or --self-test is required")
    except ReportError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    if args.json or args.self_test:
        print(json.dumps(result, sort_keys=True))
    else:
        print(f"{result['cache_hits']} {result['total_actions']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
