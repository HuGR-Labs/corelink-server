#!/usr/bin/env python3
"""Reduce one k6 export to the versioned, identity-bound load-gate input.

k6's summary export is an implementation artifact.  The regression gate only
consumes this small allow-listed envelope, which prevents request labels,
options, and future k6 fields from becoming accidental evidence.  The target
receipt is validated before this command runs and is copied into the envelope
only through its public identity fields.
"""

from __future__ import annotations

import argparse
import json
import math
import re
from pathlib import Path


SUMMARY_SCHEMA = 1
SUITE_VERSION = "r3-prep-v2"
CANONICAL_TARGET = "https://staging.corelink.humangr.com"
SCENARIOS = frozenset({"signup", "webhook", "dsr", "cas", "byok"})
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
TENANT_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)


class SummaryError(ValueError):
    """The k6 export or target identity cannot be trusted."""


def _finite_positive(value: object, label: str) -> float:
    if isinstance(value, bool):
        raise SummaryError(f"{label} must be a numeric measurement, not a boolean")
    try:
        number = float(value)
    except (TypeError, ValueError) as exc:
        raise SummaryError(f"{label} is not numeric") from exc
    if not math.isfinite(number) or number <= 0:
        raise SummaryError(f"{label} must be finite and > 0")
    return number


def _load(path: Path, label: str) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise SummaryError(f"{label} is unreadable: {exc}") from exc


def sanitize(raw: object, receipt: object, scenario: str) -> dict[str, object]:
    if scenario not in SCENARIOS:
        raise SummaryError(f"unsupported scenario: {scenario!r}")
    if not isinstance(receipt, dict):
        raise SummaryError("target receipt must be a JSON object")
    if set(receipt) != {
        "schema",
        "environment",
        "target",
        "tenant_id",
        "deployment_sha",
        "issued_at",
        "expires_at",
    }:
        raise SummaryError("target receipt fields are not the exact public identity set")
    if receipt.get("schema") != 1 or receipt.get("environment") != "staging":
        raise SummaryError("target receipt schema or environment is invalid")
    if receipt.get("target") != CANONICAL_TARGET:
        raise SummaryError("target receipt is not bound to canonical staging")
    tenant_id = receipt.get("tenant_id")
    if not isinstance(tenant_id, str) or not TENANT_RE.fullmatch(tenant_id):
        raise SummaryError("target receipt tenant_id is not a canonical UUID")
    deployment_sha = receipt.get("deployment_sha")
    if not isinstance(deployment_sha, str) or not SHA_RE.fullmatch(deployment_sha):
        raise SummaryError("target receipt deployment_sha is invalid")
    if not isinstance(raw, dict):
        raise SummaryError("k6 summary must be a JSON object")
    metrics = raw.get("metrics")
    if not isinstance(metrics, dict):
        raise SummaryError("k6 summary has no metrics object")
    duration = metrics.get("http_req_duration")
    if not isinstance(duration, dict):
        raise SummaryError("k6 summary has no http_req_duration object")
    median = _finite_positive(duration.get("med"), "http_req_duration.med")
    p99 = _finite_positive(duration.get("p(99)"), "http_req_duration.p(99)")
    return {
        "schema": SUMMARY_SCHEMA,
        "suite_version": SUITE_VERSION,
        "scenario": scenario,
        "target": CANONICAL_TARGET,
        "tenant_id": tenant_id,
        "deployment_sha": deployment_sha,
        "metrics": {"http_req_duration": {"med": median, "p(99)": p99}},
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scenario", required=True)
    args = parser.parse_args(argv)
    try:
        sanitized = sanitize(_load(args.input, "k6 summary"), _load(args.receipt, "target receipt"), args.scenario)
    except SummaryError as exc:
        print(f"::error::sanitized k6 summary rejected: {exc}")
        return 1
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(sanitized, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
