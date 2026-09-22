#!/usr/bin/env python3
"""Emit the bounded Terraform drift evidence artifact.

Terraform plan JSON is deliberately treated as an input boundary.  Only the
fixed metadata and action counters below are written; resource addresses,
configuration, variables, prior state, and attribute values never cross it.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import tempfile
from pathlib import Path
from typing import Any


_REGION = re.compile(r"^[a-z0-9][a-z0-9_-]{0,31}$")
_UTC_TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
_ACTION_KEYS = ("create", "update", "delete", "replace", "read")
_ACTIONS = {
    ("create",): "create",
    ("update",): "update",
    ("delete",): "delete",
    ("read",): "read",
    ("create", "delete"): "replace",
    ("delete", "create"): "replace",
}


def _positive_integer(raw: str, name: str) -> int:
    try:
        value = int(raw)
    except ValueError as exc:
        raise ValueError(f"{name} must be a non-negative integer") from exc
    if value < 0:
        raise ValueError(f"{name} must be a non-negative integer")
    return value


def _action_counts(plan_path: Path) -> dict[str, int]:
    try:
        with plan_path.open(encoding="utf-8") as handle:
            document: Any = json.load(handle)
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError("Terraform plan JSON could not be read") from exc

    changes = document.get("resource_changes", []) if isinstance(document, dict) else None
    if not isinstance(changes, list):
        raise ValueError("Terraform plan JSON has an invalid resource_changes list")

    counts = {key: 0 for key in _ACTION_KEYS}
    for resource in changes:
        if not isinstance(resource, dict):
            raise ValueError("Terraform plan JSON has an invalid resource change")
        change = resource.get("change")
        actions = change.get("actions") if isinstance(change, dict) else None
        if not isinstance(actions, list) or not all(isinstance(action, str) for action in actions):
            raise ValueError("Terraform plan JSON has an invalid action list")
        category = _ACTIONS.get(tuple(actions))
        if category is None:
            raise ValueError("Terraform plan JSON has an unsupported action list")
        counts[category] += 1
    return counts


def build_summary(args: argparse.Namespace) -> dict[str, object]:
    region = args.region
    if not _REGION.fullmatch(region):
        raise ValueError("region contains unsupported characters")
    run_id = _positive_integer(args.run_id, "run-id")
    run_attempt = _positive_integer(args.run_attempt, "run-attempt")
    if not _UTC_TIMESTAMP.fullmatch(args.detected_at):
        raise ValueError("detected-at must be an RFC3339 UTC timestamp")
    if args.exit_code not in (0, 1, 2):
        raise ValueError("Terraform exit code must be 0, 1, or 2")

    if args.exit_code in (0, 2):
        if args.plan_json is None:
            raise ValueError("a plan JSON file is required for exit code 0 or 2")
        action_counts = _action_counts(args.plan_json)
    else:
        action_counts = {key: 0 for key in _ACTION_KEYS}

    result = {0: "clean", 1: "error", 2: "drift"}[args.exit_code]
    return {
        "schema_version": 1,
        "workflow_run_id": run_id,
        "workflow_run_attempt": run_attempt,
        "detected_at": args.detected_at,
        "region": region,
        "terraform_exit_code": args.exit_code,
        "result": result,
        "drift_detected": args.exit_code == 2,
        "action_counts": action_counts,
    }


def _write_atomic(path: Path, summary: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(summary, handle, sort_keys=True, separators=(",", ":"))
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--region", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--run-attempt", required=True)
    parser.add_argument("--detected-at", required=True)
    parser.add_argument("--exit-code", required=True, type=int)
    parser.add_argument("--plan-json", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        _write_atomic(args.output, build_summary(args))
    except (OSError, ValueError) as exc:
        # Keep diagnostics metadata-only: never echo input paths or payloads.
        parser.error(str(exc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
