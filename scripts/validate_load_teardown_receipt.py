#!/usr/bin/env python3
"""Validate the exact server receipt before the endurance lane claims cleanup."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


SCHEMA = "corelink.load-test-teardown-receipt.v1"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
RUN_ID_RE = re.compile(r"^\d{1,20}$")
RESOURCE_RE = re.compile(r"^[a-z][a-z0-9_]{0,63}$")
REQUIRED_RESOURCE_CLASSES = frozenset(
    {"cas_objects", "webhook_idempotency_rows", "dsr_jobs", "audit_entries"}
)
REQUIRED_FIELDS = {
    "schema",
    "run_id",
    "scenario",
    "target_deployment_sha",
    "inventory_complete",
    "resources",
    "cross_run_deletions",
}


class TeardownReceiptError(ValueError):
    """The endpoint did not prove complete, exact run-scoped deletion."""


def _object_without_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise TeardownReceiptError("receipt contains a duplicate JSON key")
        result[key] = value
    return result


def parse_receipt(text: str) -> object:
    """Parse JSON while rejecting ambiguous duplicate keys at every depth."""
    return json.loads(text, object_pairs_hook=_object_without_duplicate_keys)


def _count(value: object, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise TeardownReceiptError(f"{field} must be a nonnegative integer")
    return value


def validate(
    receipt: object,
    *,
    run_id: str,
    scenario: str,
    deployment_sha: str,
) -> dict[str, object]:
    """Require an exact schema and reconciled zero-residual inventory."""
    if not RUN_ID_RE.fullmatch(run_id):
        raise TeardownReceiptError("expected run id must be numeric")
    if not SHA_RE.fullmatch(deployment_sha):
        raise TeardownReceiptError("expected deployment SHA must be a full lowercase commit SHA")
    if not isinstance(receipt, dict) or set(receipt) != REQUIRED_FIELDS:
        raise TeardownReceiptError("receipt fields are not the exact required set")
    if receipt["schema"] != SCHEMA:
        raise TeardownReceiptError("unsupported teardown receipt schema")
    if receipt["run_id"] != run_id:
        raise TeardownReceiptError("receipt run id does not match this GitHub run")
    if receipt["scenario"] != scenario:
        raise TeardownReceiptError("receipt scenario does not match this lane")
    if receipt["target_deployment_sha"] != deployment_sha:
        raise TeardownReceiptError("receipt deployment SHA does not match the validated target")
    if receipt["inventory_complete"] is not True:
        raise TeardownReceiptError("receipt does not attest a complete resource inventory")
    if _count(receipt["cross_run_deletions"], "cross_run_deletions") != 0:
        raise TeardownReceiptError("receipt reports a cross-run deletion")

    resources = receipt["resources"]
    if not isinstance(resources, dict) or set(resources) != REQUIRED_RESOURCE_CLASSES:
        raise TeardownReceiptError("receipt must enumerate the exact persistent resource classes")
    normalized_resources: dict[str, dict[str, int]] = {}
    for resource, counts in resources.items():
        if not isinstance(resource, str) or not RESOURCE_RE.fullmatch(resource):
            raise TeardownReceiptError("resource names must be lowercase identifiers")
        if not isinstance(counts, dict) or set(counts) != {"inventory", "attempted", "deleted", "remaining"}:
            raise TeardownReceiptError(f"{resource} counts have an invalid shape")
        inventory = _count(counts["inventory"], f"{resource}.inventory")
        attempted = _count(counts["attempted"], f"{resource}.attempted")
        deleted = _count(counts["deleted"], f"{resource}.deleted")
        remaining = _count(counts["remaining"], f"{resource}.remaining")
        if attempted != inventory or deleted != inventory or remaining != 0:
            raise TeardownReceiptError(f"{resource} inventory and deletion counts do not reconcile to zero")
        normalized_resources[resource] = {
            "inventory": inventory,
            "attempted": attempted,
            "deleted": deleted,
            "remaining": remaining,
        }

    return {
        "schema": SCHEMA,
        "run_id": run_id,
        "scenario": scenario,
        "target_deployment_sha": deployment_sha,
        "inventory_complete": True,
        "resources": normalized_resources,
        "cross_run_deletions": 0,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--deployment-sha", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        receipt = parse_receipt(args.receipt.read_text(encoding="utf-8"))
        validated = validate(
            receipt,
            run_id=args.run_id,
            scenario=args.scenario,
            deployment_sha=args.deployment_sha,
        )
    except (OSError, json.JSONDecodeError, TeardownReceiptError) as exc:
        print(f"::error::teardown receipt rejected: {exc}")
        return 1
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(validated, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("teardown receipt accepted: exact run, scenario, deployment, and deletion inventory")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
