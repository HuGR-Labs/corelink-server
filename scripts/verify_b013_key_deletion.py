#!/usr/bin/env python3
"""Fail-closed verifier for the owner-authorized B-013 evidence artifact.

The private-key deletion happened on the owner's machine, outside this
repository.  This verifier checks only the redacted evidence record; it never
inspects ``~/Downloads``, reads key material, or claims to reproduce the
external deletion.  The artifact is intentionally closed: exactly the three
preflight targets, their observed sizes/dates, and the positive postcondition
are required.  Missing, extra, duplicated, or polarity-flipped fields fail.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "evidence/owner-actions/B-013/downloads-private-key-deletion.json"

REQUIRED_ROOT_FIELDS = {
    "schema_version",
    "captured_at",
    "machine",
    "targets",
    "public_key_preserved",
    "all_targets_absent_after",
    "operator",
}
TARGET_FIELDS = {
    "basename",
    "size_before_bytes",
    "mtime_before",
    "deletion_command",
    "exists_after",
}
EXPECTED_TARGETS = (
    {
        "basename": "corelink-app.pk8.pem",
        "size_before_bytes": 1704,
        "mtime_before": "2026-06-15",
        "deletion_command": "rm -P",
        "exists_after": False,
    },
    {
        "basename": "corelink-runners-fleet.2026-06-15.private-key.pem",
        "size_before_bytes": 1675,
        "mtime_before": "2026-06-15",
        "deletion_command": "rm -P",
        "exists_after": False,
    },
    {
        "basename": "corelink-runners.2026-07-13.private-key.pem",
        "size_before_bytes": 1679,
        "mtime_before": "2026-07-13",
        "deletion_command": "rm -P",
        "exists_after": False,
    },
)
EXPECTED_CAPTURED_AT = "2026-09-08"
EXPECTED_MACHINE = "owner Mac"
EXPECTED_OPERATOR = "Codex under explicit owner authorization"

SECRET_SHAPES = (
    re.compile(r"-----BEGIN [A-Z0-9 ]+ PRIVATE KEY-----"),
    re.compile(r"\b(?:gh[pousr]_|github_pat_|sk_live_|whsec_)\S+", re.IGNORECASE),
    re.compile(r"\b[0-9a-f]{64}\b", re.IGNORECASE),
)


class VerificationError(ValueError):
    """Raised when evidence is missing, ambiguous, or no longer canonical."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise VerificationError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _parse(text: str) -> dict[str, Any]:
    try:
        value = json.loads(text, object_pairs_hook=_reject_duplicate_keys)
    except (json.JSONDecodeError, VerificationError) as exc:
        raise VerificationError(f"invalid B-013 evidence JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise VerificationError("B-013 evidence root must be an object")
    return value


def _read(path: Path = EVIDENCE) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        raise VerificationError(f"missing/non-regular B-013 evidence: {path}")
    try:
        return _parse(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError) as exc:
        raise VerificationError(f"cannot read B-013 evidence: {exc}") from exc


def _check_no_secret_shapes(value: object, label: str = "evidence") -> None:
    if isinstance(value, str):
        if any(pattern.search(value) for pattern in SECRET_SHAPES):
            raise VerificationError(f"{label} contains credential material")
    elif isinstance(value, dict):
        for key, child in value.items():
            _check_no_secret_shapes(child, f"{label}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _check_no_secret_shapes(child, f"{label}[{index}]")


def check_evidence(data: object) -> dict[str, int]:
    if not isinstance(data, dict):
        raise VerificationError("B-013 evidence root must be an object")
    if set(data) != REQUIRED_ROOT_FIELDS:
        missing = sorted(REQUIRED_ROOT_FIELDS - set(data))
        extra = sorted(set(data) - REQUIRED_ROOT_FIELDS)
        raise VerificationError(f"root fields differ: missing={missing}, extra={extra}")
    if data["schema_version"] != 1:
        raise VerificationError("schema_version must be 1")
    if data["captured_at"] != EXPECTED_CAPTURED_AT:
        raise VerificationError("captured_at is not the owner-confirmed closure date")
    if data["machine"] != EXPECTED_MACHINE:
        raise VerificationError("machine must remain the redacted owner Mac label")
    if data["operator"] != EXPECTED_OPERATOR:
        raise VerificationError("operator/authorization wording changed")

    targets = data["targets"]
    if not isinstance(targets, list) or len(targets) != len(EXPECTED_TARGETS):
        raise VerificationError("targets must contain exactly three entries")
    for index, (target, expected) in enumerate(zip(targets, EXPECTED_TARGETS)):
        if not isinstance(target, dict):
            raise VerificationError(f"targets[{index}] must be an object")
        if set(target) != TARGET_FIELDS:
            missing = sorted(TARGET_FIELDS - set(target))
            extra = sorted(set(target) - TARGET_FIELDS)
            raise VerificationError(f"targets[{index}] fields differ: missing={missing}, extra={extra}")
        if target != expected:
            raise VerificationError(f"targets[{index}] does not match the exact preflight record")

    if type(data["public_key_preserved"]) is not bool or data["public_key_preserved"] is not True:
        raise VerificationError("public_key_preserved must be true")
    if type(data["all_targets_absent_after"]) is not bool or data["all_targets_absent_after"] is not True:
        raise VerificationError("all_targets_absent_after must be true")
    _check_no_secret_shapes(data)
    return {"targets": len(targets)}


def mutation_self_test(data: dict[str, Any]) -> int:
    """Prove that load-bearing population, schema, and polarity mutations fail."""
    mutations = 0

    def rejected(mutated: dict[str, Any], label: str) -> None:
        nonlocal mutations
        try:
            check_evidence(mutated)
        except VerificationError:
            mutations += 1
        else:
            raise VerificationError(f"mutation was accepted: {label}")

    for field in REQUIRED_ROOT_FIELDS:
        mutated = copy.deepcopy(data)
        del mutated[field]
        rejected(mutated, f"missing root field {field}")
    extra = copy.deepcopy(data)
    extra["unreviewed_host"] = "host detail"
    rejected(extra, "extra root field")

    for index, target in enumerate(data["targets"]):
        for field in TARGET_FIELDS:
            mutated = copy.deepcopy(data)
            del mutated["targets"][index][field]
            rejected(mutated, f"missing targets[{index}].{field}")
    missing_target = copy.deepcopy(data)
    missing_target["targets"].pop()
    rejected(missing_target, "missing target")
    extra_target = copy.deepcopy(data)
    extra_target["targets"].append(copy.deepcopy(extra_target["targets"][0]))
    rejected(extra_target, "extra target")
    duplicate_target = copy.deepcopy(data)
    duplicate_target["targets"][1] = copy.deepcopy(duplicate_target["targets"][0])
    rejected(duplicate_target, "duplicate target")

    for index, field, value in (
        (0, "basename", "other.pem"),
        (1, "size_before_bytes", 1),
        (2, "mtime_before", "2026-09-08"),
        (0, "deletion_command", "rm -f"),
        (2, "exists_after", True),
    ):
        mutated = copy.deepcopy(data)
        mutated["targets"][index][field] = value
        rejected(mutated, f"polarity/identity target mutation {field}")

    for field, value in (
        ("captured_at", "2026-09-07"),
        ("machine", "host.internal"),
        ("public_key_preserved", False),
        ("all_targets_absent_after", False),
        ("operator", "operator without authorization"),
    ):
        mutated = copy.deepcopy(data)
        mutated[field] = value
        rejected(mutated, f"root mutation {field}")
    secret = copy.deepcopy(data)
    secret["operator"] = "-----BEGIN RSA PRIVATE KEY-----"
    rejected(secret, "secret-shaped value")
    return mutations


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        evidence = _read()
        result = check_evidence(evidence)
        mutations = mutation_self_test(evidence) if args.self_test else 0
    except (OSError, VerificationError) as exc:
        print(f"B013 evidence: FAIL: {exc}", file=sys.stderr)
        return 1
    suffix = f", {mutations} mutation(s) rejected" if args.self_test else ""
    print(f"B013 evidence: PASS: exact {result['targets']}-target closure record verified{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
