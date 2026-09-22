#!/usr/bin/env python3
"""Classify a retained, redacted #1669 aggregate receipt without D1 access."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "probe_i1669_readonly", ROOT / "scripts" / "probe_i1669_readonly.py"
)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("issue-1669 probe schema is unavailable")
PROBE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PROBE
SPEC.loader.exec_module(PROBE)


class ReceiptError(ValueError):
    """The saved aggregate receipt cannot support a trustworthy classification."""


def _canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def _nonnegative_fields(value: object, fields: tuple[str, ...], label: str) -> dict[str, int]:
    if not isinstance(value, dict) or set(value) != set(fields):
        raise ReceiptError(f"{label} fields are missing or unexpected")
    result: dict[str, int] = {}
    for name in fields:
        count = value[name]
        if isinstance(count, bool) or not isinstance(count, int) or count < 0:
            raise ReceiptError(f"{label}.{name} is not a non-negative integer")
        result[name] = count
    return result


def classify(receipt: object) -> dict[str, Any]:
    if not isinstance(receipt, dict):
        raise ReceiptError("receipt is not an object")
    unsigned = dict(receipt)
    digest = unsigned.pop("receipt_sha256", None)
    expected = hashlib.sha256(_canonical(unsigned)).hexdigest()
    if not isinstance(digest, str) or digest != expected:
        raise ReceiptError("receipt digest does not match its contents")
    if (
        receipt.get("schema") != "corelink.issue-1669.read-only-residency.v1"
        or receipt.get("issue") != 1669
        or receipt.get("mode") != "production_read_only"
    ):
        raise ReceiptError("receipt schema, issue, or evidence mode is unexpected")

    queries = receipt.get("queries")
    expected_names = tuple(PROBE.QUERY_ALLOWLIST)
    if not isinstance(queries, list) or len(queries) != len(expected_names):
        raise ReceiptError("receipt does not contain the complete query allowlist")
    for item, name in zip(queries, expected_names, strict=True):
        if not isinstance(item, dict) or item.get("name") != name:
            raise ReceiptError("receipt query order or name is unexpected")
        if item.get("query_sha256") != PROBE._hash(PROBE.QUERY_ALLOWLIST[name]):
            raise ReceiptError(f"receipt query hash does not match {name}")
        if item.get("row_count") != 1:
            raise ReceiptError(f"receipt query {name} is not a single aggregate row")

    counts = receipt.get("counts")
    if not isinstance(counts, dict) or set(counts) != set(expected_names):
        raise ReceiptError("receipt aggregate set is incomplete or unexpected")
    residency = _nonnegative_fields(counts["residency"], PROBE.RESIDENCY.COUNT_FIELDS, "residency")
    population = _nonnegative_fields(counts["population"], PROBE.POPULATION_FIELDS, "population")
    completeness = _nonnegative_fields(
        counts["backfill_completeness"], PROBE.BACKFILL_FIELDS, "backfill_completeness"
    )
    model = PROBE.RESIDENCY.Counts(**residency)
    try:
        state, reason = PROBE.RESIDENCY.assess(model, environment="production")
    except PROBE.RESIDENCY.Indeterminate as exc:
        raise ReceiptError(f"residency partition is indeterminate: {exc}") from exc
    total = model.total_rows
    if population["audit_rows"] != total or population["blank_tenant_rows"]:
        raise ReceiptError("population aggregate does not reconcile with residency")
    if completeness["audit_rows"] != total:
        raise ReceiptError("backfill denominator does not reconcile with residency")
    backfill_partition = (
        completeness["orphan_rows"]
        + completeness["joinable_rows"]
        + completeness["reserved_public_rows"]
        + completeness["invalid_public_rows"]
    )
    if backfill_partition != total:
        raise ReceiptError("backfill completeness partition is partial")
    for key in ("orphan_rows", "reserved_public_rows", "invalid_public_rows", "erased_orphan_rows"):
        if completeness[key] != residency[key]:
            raise ReceiptError(f"backfill {key} does not reconcile with residency")
    if receipt.get("status") != state or receipt.get("reason") != reason:
        raise ReceiptError("receipt verdict does not match its counts")

    classes = [
        {
            "class": "satisfied_customer",
            "rows": model.satisfied_rows,
            "tenants": None,
            "disposition": "PROVEN",
        },
        {
            "class": "violated_customer",
            "rows": model.violated_rows,
            "tenants": None,
            "disposition": "FAIL_CLOSED_INVESTIGATE",
        },
        {
            "class": "erased_orphan_retained_audit",
            "rows": model.erased_orphan_rows,
            "tenants": model.erased_orphan_tenants,
            "disposition": "PRESERVE_AUDIT_EVIDENCE",
        },
        {
            "class": "unexplained_orphan",
            "rows": model.unexplained_orphan_rows,
            "tenants": model.unexplained_orphan_tenants,
            "disposition": "PRESERVE_AND_REQUIRE_RESTRICTED_OWNER_RECONCILIATION",
        },
        {
            "class": "other_unevaluable_customer",
            "rows": model.customer_unevaluable_rows - model.orphan_rows,
            "tenants": None,
            "disposition": "FAIL_CLOSED_INVESTIGATE",
        },
        {
            "class": "reserved_public",
            "rows": model.reserved_public_rows,
            "tenants": None,
            "disposition": "VALID_SYSTEM_NAMESPACE",
        },
        {
            "class": "invalid_public",
            "rows": model.invalid_public_rows,
            "tenants": None,
            "disposition": "FAIL_CLOSED_INVESTIGATE",
        },
    ]
    if sum(item["rows"] for item in classes) != total:
        raise ReceiptError("disposition classes do not conserve the full population")
    return {
        "schema": "corelink.issue-1669.aggregate-classification.v1",
        "issue": 1669,
        "receipt_sha256": digest,
        "source_status": state,
        "source_reason": reason,
        "classification_scope": "aggregate_counts_only",
        "tenant_identity_dispositions_complete": model.unexplained_orphan_rows == 0,
        "classes": classes,
        "overall_disposition": "COMPLIANT" if state == "COMPLIANT" else "KEEP_OPEN",
    }


def verify_sidecar(receipt_path: Path, sidecar_path: Path) -> str:
    if receipt_path.is_symlink() or sidecar_path.is_symlink():
        raise ReceiptError("receipt and checksum sidecar must not be symlinks")
    try:
        line = sidecar_path.read_text(encoding="utf-8").strip()
        expected, separator, artifact_name = line.partition("  ")
        actual = hashlib.sha256(receipt_path.read_bytes()).hexdigest()
    except OSError as exc:
        raise ReceiptError("receipt or checksum sidecar is unavailable") from exc
    if not separator or not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ReceiptError("checksum sidecar is malformed")
    if Path(artifact_name).name != receipt_path.name or actual != expected:
        raise ReceiptError("receipt does not match its artifact checksum")
    return actual


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "receipt", type=Path, help="redacted receipt downloaded from the hosted evidence run"
    )
    parser.add_argument(
        "--sha256", required=True, type=Path, help="matching artifact checksum sidecar"
    )
    args = parser.parse_args()
    try:
        artifact_sha256 = verify_sidecar(args.receipt, args.sha256)
        report = classify(json.loads(args.receipt.read_text(encoding="utf-8")))
    except (OSError, json.JSONDecodeError, ReceiptError) as exc:
        print(f"status=INDETERMINATE reason={exc}", file=sys.stderr)
        return 2
    report["artifact_sha256"] = artifact_sha256
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["overall_disposition"] == "COMPLIANT" else 1


if __name__ == "__main__":
    raise SystemExit(main())
