#!/usr/bin/env python3
"""Verify the regional container vCPU budget without changing infrastructure.

The default check is credentialless and proves only the repository declaration.
Provider mode additionally requires a JSON readback captured by an operator or
CI job.  No mode invokes wrangler, makes an HTTP request, or mutates quotas.
"""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WRANGLER = ROOT / "wrangler.toml"
BUDGET = ROOT / "config/capacity/regional-vcpu-budget.json"
KNOWN_VCPU = {"basic": 0.25}


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
            "runner_reservation_vcpu": runner_reservation, "reservation_vcpu": reservation,
            "headroom_vcpu": account_limit - reservation, "environments": actual}


def verify_provider(path: Path, model: dict[str, object]) -> None:
    evidence = read_json(path)
    if evidence.get("schema_version") != 1 or evidence.get("read_only") is not True:
        raise CapacityError("provider evidence must be schema 1 and read_only=true")
    if evidence.get("total_vcpu") != model["account_limit_vcpu"]:
        raise CapacityError("provider total_vcpu does not match the budget model")
    for key in ("vcpu_per_deployment", "total_memory_mib"):
        if not isinstance(evidence.get(key), (int, float)) or evidence[key] <= 0:
            raise CapacityError(f"provider field missing or invalid: {key}")


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
