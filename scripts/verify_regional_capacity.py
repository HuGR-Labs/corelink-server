#!/usr/bin/env python3
"""Verify the regional container vCPU budget without changing infrastructure.

The default check is credentialless and proves only the repository declaration.
Provider mode additionally requires a JSON readback captured by an operator or
CI job.  No mode invokes wrangler, makes an HTTP request, or mutates quotas.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WRANGLER = ROOT / "wrangler.toml"
BUDGET = ROOT / "config/capacity/regional-vcpu-budget.json"
KNOWN_VCPU = {"basic": 0.25}
PROVIDER_RECEIPT_SCHEMA = "corelink.issue-2044.capacity-read-only.v1"
PROVIDER_RECEIPT_ISSUE = 2044
PROVIDER_RECEIPT_ENDPOINT = "GET /accounts/{account}/cloudchamber/me"
REQUIRED_RECEIPT_KEYS = frozenset(
    {
        "schema_version",
        "schema",
        "issue",
        "read_only",
        "endpoint",
        "quota",
        "usage",
        "concurrency",
    }
)
# The #2044 probe emits these metadata fields. They are intentionally optional
# here because regional arithmetic does not consume them, but no other receipt
# member may extend this contract without review.
OPTIONAL_RECEIPT_METADATA_KEYS = frozenset(
    {"captured_at", "account_id_redacted", "provider_api_version"}
)
REQUIRED_QUOTA_KEYS = frozenset(
    {
        "total_vcpu",
        "vcpu_per_deployment",
        "memory_mib_per_deployment",
        "total_memory_mib",
    }
)
UNAVAILABLE_MEASUREMENT = {
    "status": "unavailable",
    "source": PROVIDER_RECEIPT_ENDPOINT,
    "reason": "provider has not established a documented account measurement path",
}


class CapacityError(ValueError):
    pass


def read_json(path: Path) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise CapacityError(f"cannot read evidence: {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise CapacityError(f"evidence must be a JSON object: {path}")
    return value


def declared_budget() -> dict[str, object]:
    budget = read_json(BUDGET)
    if budget.get("schema_version") != 1:
        raise CapacityError("unsupported budget schema")
    try:
        account_limit = int(budget["account_limit_vcpu"])
        cache = budget["cache"]
        runner = budget["runner"]
        expected = int(budget["expected_reservation_vcpu"])
        environments = cache["environments"]  # type: ignore[index]
        cache_type = cache["instance_type"]  # type: ignore[index]
        cache_vcpu = float(cache["vcpu_per_instance"])  # type: ignore[index]
        cache_max = int(cache["max_instances_per_environment"])  # type: ignore[index]
        runner_max = int(runner["max_instances"])  # type: ignore[index]
        runner_vcpu = float(runner["vcpu_per_instance"])  # type: ignore[index]
    except (KeyError, TypeError, ValueError) as exc:
        raise CapacityError(f"malformed budget model: {exc}") from exc
    if not isinstance(environments, list) or not environments or not all(isinstance(x, str) for x in environments):
        raise CapacityError("cache.environments must be a non-empty string list")
    if cache_type not in KNOWN_VCPU or cache_vcpu != KNOWN_VCPU[cache_type]:
        raise CapacityError("cache instance type has no trusted vCPU mapping")
    if min(account_limit, cache_max, runner_max) <= 0 or runner_vcpu <= 0:
        raise CapacityError("budget values must be positive")

    try:
        config = tomllib.loads(WRANGLER.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise CapacityError(f"cannot parse wrangler.toml: {exc}") from exc
    actual: list[dict[str, object]] = []
    envs = config.get("env")
    if not isinstance(envs, dict):
        raise CapacityError("wrangler.toml has no [env] table")
    for name in environments:
        section = envs.get(name)
        containers = section.get("containers") if isinstance(section, dict) else None
        if not isinstance(containers, list) or len(containers) != 1 or not isinstance(containers[0], dict):
            raise CapacityError(f"{name} must declare exactly one container")
        row = containers[0]
        if row.get("class_name") != "CoreLinkServer":
            raise CapacityError(f"{name} container class drifted")
        if row.get("instance_type") != cache_type or row.get("max_instances") != cache_max:
            raise CapacityError(f"{name} container reservation drifted")
        actual.append({"environment": name, "max_instances": row["max_instances"], "instance_type": row["instance_type"]})
    cache_reservation = len(actual) * cache_max * cache_vcpu
    runner_reservation = runner_max * runner_vcpu
    reservation = cache_reservation + runner_reservation
    if reservation != expected or reservation > account_limit:
        raise CapacityError(f"reservation arithmetic drifted: {reservation} vCPU")
    return {"account_limit_vcpu": account_limit, "cache_reservation_vcpu": cache_reservation,
            "runner_vcpu_per_instance": runner_vcpu, "runner_reservation_vcpu": runner_reservation,
            "reservation_vcpu": reservation, "headroom_vcpu": account_limit - reservation,
            "environments": actual}


def positive_finite_number(source: dict[str, object], field: str) -> float:
    value = source.get(field)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise CapacityError(f"provider quota field missing or invalid: {field}")
    if not math.isfinite(value) or value <= 0:
        raise CapacityError(f"provider quota field missing or invalid: {field}")
    return float(value)


def verify_provider(path: Path, model: dict[str, object]) -> None:
    evidence = read_json(path)
    if not REQUIRED_RECEIPT_KEYS.issubset(evidence) or not set(evidence).issubset(
        REQUIRED_RECEIPT_KEYS | OPTIONAL_RECEIPT_METADATA_KEYS
    ):
        raise CapacityError("provider receipt keys do not match the Cloudchamber read-only contract")
    if (
        evidence.get("schema_version") != 1
        or evidence.get("schema") != PROVIDER_RECEIPT_SCHEMA
        or evidence.get("issue") != PROVIDER_RECEIPT_ISSUE
        or evidence.get("read_only") is not True
        or evidence.get("endpoint") != PROVIDER_RECEIPT_ENDPOINT
    ):
        raise CapacityError("provider evidence does not match the Cloudchamber read-only receipt contract")
    quota = evidence.get("quota")
    if not isinstance(quota, dict):
        raise CapacityError("provider quota is missing or invalid")
    if set(quota) != REQUIRED_QUOTA_KEYS:
        raise CapacityError("provider quota keys do not match the Cloudchamber read-only contract")
    if evidence.get("usage") != UNAVAILABLE_MEASUREMENT or evidence.get("concurrency") != UNAVAILABLE_MEASUREMENT:
        raise CapacityError("provider receipt must keep unsupported usage and concurrency unavailable")
    total_vcpu = positive_finite_number(quota, "total_vcpu")
    if total_vcpu != model["account_limit_vcpu"]:
        raise CapacityError("provider total_vcpu does not match the budget model")
    for key in ("vcpu_per_deployment", "total_memory_mib"):
        positive_finite_number(quota, key)
    if positive_finite_number(quota, "vcpu_per_deployment") != model["runner_vcpu_per_instance"]:
        raise CapacityError("provider vcpu_per_deployment does not match the runner contract")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--provider-readback", type=Path, help="operator-captured read-only JSON")
    args = parser.parse_args(argv)
    try:
        model = declared_budget()
        if args.provider_readback is not None:
            verify_provider(args.provider_readback, model)
            provider = "VERIFIED"
        else:
            provider = "UNVERIFIED (repository declaration only)"
        print("regional capacity: PASS")
        print(json.dumps({**model, "provider": provider}, sort_keys=True))
        return 0
    except CapacityError as exc:
        print(f"regional capacity: FAIL-CLOSED: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
