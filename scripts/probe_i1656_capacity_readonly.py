#!/usr/bin/env python3
"""Capture a bounded, value-free Cloudflare Containers schema observation.

The request is fixed to one GET against the canonical account endpoint. The
receipt contains only fixed, compile-time path predicates and aggregate shape
counts. Provider keys and values are never copied into an artifact or log.
"""

from __future__ import annotations

import argparse
import json
import os
import tempfile
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


CANONICAL_ACCOUNT_ID = "6a1fc1c626fc2628823e60b9db01f5cd"
WRANGLER_CLOUDCHAMBER_ME_PATH = "/cloudchamber/me"
ENDPOINT = (
    "https://api.cloudflare.com/client/v4/accounts/"
    f"{CANONICAL_ACCOUNT_ID}{WRANGLER_CLOUDCHAMBER_ME_PATH}"
)
ACCOUNT_REDACTED = f"{CANONICAL_ACCOUNT_ID[:4]}...{CANONICAL_ACCOUNT_ID[-4:]}"
MAX_RESPONSE_BYTES = 1_048_576
MAX_SCHEMA_DEPTH = 8
MAX_SCHEMA_NODES = 256

# Each identifier and path below is source-controlled. The identifiers are
# CoreLink labels; the paths are known Cloudflare envelope fields or capacity
# fields already present in the repository's operator contract. No path is
# obtained from the response, and no provider key is included in the receipt.
SCHEMA_PREDICATES: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("api_success", ("success",)),
    ("api_errors", ("errors",)),
    ("api_messages", ("messages",)),
    ("api_result", ("result",)),
    ("account_total_vcpu", ("total_vcpu",)),
    ("account_vcpu_per_deployment", ("vcpu_per_deployment",)),
    ("account_total_memory_mib", ("total_memory_mib",)),
    ("account_usage", ("usage",)),
    ("result_total_vcpu", ("result", "total_vcpu")),
    ("result_vcpu_per_deployment", ("result", "vcpu_per_deployment")),
    ("result_total_memory_mib", ("result", "total_memory_mib")),
    ("result_limits", ("result", "limits")),
    ("result_limits_total_vcpu", ("result", "limits", "total_vcpu")),
    ("result_limits_vcpu_per_deployment", ("result", "limits", "vcpu_per_deployment")),
    ("result_limits_memory_mib_per_deployment", ("result", "limits", "memory_mib_per_deployment")),
    ("result_limits_total_memory_mib", ("result", "limits", "total_memory_mib")),
    ("result_usage", ("result", "usage")),
    ("usage_used_vcpu", ("usage", "used_vcpu")),
    ("usage_vcpu", ("usage", "vcpu")),
    ("usage_used", ("usage", "used")),
    ("usage_current_vcpu", ("usage", "current_vcpu")),
    ("result_usage_used_vcpu", ("result", "usage", "used_vcpu")),
    ("result_usage_vcpu", ("result", "usage", "vcpu")),
    ("result_usage_used", ("result", "usage", "used")),
    ("result_usage_current_vcpu", ("result", "usage", "current_vcpu")),
)

# Wrangler 4.111.0 is locked in pnpm-lock.yaml. Its Cloudchamber
# AccountService.getMe request is GET /accounts/{id}/cloudchamber/me and its
# account client consumes these exact fields as account.limits.*. No
# undocumented usage or concurrency field is accepted here.
CAPACITY_LIMIT_FIELDS = (
    "total_vcpu",
    "vcpu_per_deployment",
    "memory_mib_per_deployment",
    "total_memory_mib",
)
UNAVAILABLE_MEASUREMENT = {
    "status": "unavailable",
    "source": "GET /accounts/{account}/cloudchamber/me",
    "reason": "provider has not established a documented account measurement path",
}


class ProbeError(RuntimeError):
    pass


def _assert_wrangler_endpoint_contract() -> None:
    """Reject a source/configuration drift from Wrangler's exact GET path."""

    expected = (
        "https://api.cloudflare.com/client/v4/accounts/"
        f"{CANONICAL_ACCOUNT_ID}{WRANGLER_CLOUDCHAMBER_ME_PATH}"
    )
    if ENDPOINT != expected:
        raise ProbeError("capacity endpoint contract drifted from Wrangler Cloudchamber getMe")


def _request(token: str) -> dict[str, Any]:
    _assert_wrangler_endpoint_contract()
    if not token:
        raise ProbeError("dedicated Cloudflare capacity read token is absent")
    request = urllib.request.Request(
        ENDPOINT,
        method="GET",
        headers={"Authorization": f"Bearer {token}", "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:  # noqa: S310
            if response.status != 200:
                raise ProbeError(f"capacity endpoint returned HTTP {response.status}")
            raw = response.read(MAX_RESPONSE_BYTES + 1)
    except ProbeError:
        raise
    except (urllib.error.HTTPError, urllib.error.URLError, OSError) as exc:
        raise ProbeError("capacity endpoint read unavailable") from exc
    if len(raw) > MAX_RESPONSE_BYTES:
        raise ProbeError("capacity response exceeded the bounded evidence size")
    try:
        payload = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ProbeError("capacity response was not valid JSON") from exc
    _validate_provider_envelope(payload)
    return payload


def _validate_provider_envelope(payload: object) -> None:
    """Accept only a successful Cloudflare API envelope without echoing it."""

    if not isinstance(payload, dict):
        raise ProbeError("capacity response must be a JSON object")
    # Require the documented Cloudflare API success envelope before inspecting
    # any schema predicates. Never print or retain provider error objects.
    if payload.get("success") is not True:
        raise ProbeError("capacity endpoint must report success=true")
    if payload.get("errors") != []:
        raise ProbeError("capacity endpoint must report an empty errors array")


def _json_type(value: object) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "object"
    return "invalid"


def _resolve_path(payload: dict[str, Any], path: tuple[str, ...]) -> object:
    value: object = payload
    for segment in path:
        if not isinstance(value, dict) or segment not in value:
            return _MISSING
        value = value[segment]
    return value


_MISSING = object()


def _schema_projection(payload: dict[str, Any]) -> dict[str, str]:
    """Project only fixed type predicates; never return response keys."""

    return {
        label: "missing" if (value := _resolve_path(payload, path)) is _MISSING else _json_type(value)
        for label, path in SCHEMA_PREDICATES
    }


def _bounded_shape_counts(payload: dict[str, Any]) -> dict[str, int]:
    """Count topology without emitting paths, keys, values, or array contents."""

    node_count = 0
    max_depth_seen = 0
    unknown_object_key_count = 0

    def visit(value: object, depth: int, active_paths: tuple[tuple[str, ...], ...]) -> None:
        nonlocal node_count, max_depth_seen, unknown_object_key_count
        node_count += 1
        if node_count > MAX_SCHEMA_NODES:
            raise ProbeError("capacity schema exceeds the bounded node limit")
        max_depth_seen = max(max_depth_seen, depth)
        if depth > MAX_SCHEMA_DEPTH:
            raise ProbeError("capacity schema exceeds the bounded depth limit")
        if isinstance(value, dict):
            # The allowlist contains complete paths, so only keys that are a
            # prefix of a fixed path are considered recognized. Keys are
            # compared in memory but never copied, sorted, or serialized.
            for key, child in value.items():
                if not isinstance(key, str):
                    raise ProbeError("capacity response contains an invalid JSON object")
                matching_paths = tuple(path[1:] for path in active_paths if path and path[0] == key)
                if not matching_paths:
                    unknown_object_key_count += 1
                visit(child, depth + 1, matching_paths)
        elif isinstance(value, list):
            for child in value:
                # The schema allowlist contains object member paths, not array
                # positions. Treat object members inside arrays as unknown.
                visit(child, depth + 1, ())
        elif _json_type(value) == "invalid":
            raise ProbeError("capacity response contains a non-JSON value")

    # The implementation below uses only path length and fixed path segments
    # to classify a key. It does not turn a provider key into output data.
    visit(payload, 0, tuple(path for _label, path in SCHEMA_PREDICATES))
    return {
        "node_count": node_count,
        "max_depth": max_depth_seen,
        "unknown_object_key_count": unknown_object_key_count,
    }


def _positive_finite_number(source: dict[str, Any], field: str) -> float:
    value = source.get(field)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ProbeError(f"provider capacity field {field} is missing or nonnumeric")
    if value != value or value in (float("inf"), float("-inf")):
        raise ProbeError(f"provider capacity field {field} is nonfinite")
    if value <= 0:
        raise ProbeError(f"provider capacity field {field} is nonpositive")
    return float(value)


def _parse_live_capacity(payload: dict[str, Any]) -> dict[str, object]:
    """Parse only the Cloudchamber limits contract established by Wrangler.

    The endpoint does not establish account usage or tenant concurrency. Those
    values remain explicitly unavailable rather than inferred from reservation
    arithmetic, application count, or undocumented provider object members.
    """

    _validate_provider_envelope(payload)
    result = payload.get("result")
    if not isinstance(result, dict):
        raise ProbeError("provider capacity result is missing or not an object")
    limits = result.get("limits")
    if not isinstance(limits, dict):
        raise ProbeError("provider capacity limits is missing or not an object")
    parsed = {field: _positive_finite_number(limits, field) for field in CAPACITY_LIMIT_FIELDS}
    if parsed["total_vcpu"] < parsed["vcpu_per_deployment"]:
        raise ProbeError("provider vCPU limits are inconsistent")
    if parsed["total_memory_mib"] < parsed["memory_mib_per_deployment"]:
        raise ProbeError("provider memory limits are inconsistent")
    return {
        "quota": parsed,
        "usage": dict(UNAVAILABLE_MEASUREMENT),
        "concurrency": dict(UNAVAILABLE_MEASUREMENT),
    }


def _write(path: Path, receipt: dict[str, object]) -> None:
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ProbeError("receipt output must be a regular non-symlink file")
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(receipt, handle, indent=2, sort_keys=True)
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


def run_topology(token: str, output: Path) -> int:
    payload = _request(token)
    _validate_provider_envelope(payload)
    receipt: dict[str, object] = {
        "schema_version": 1,
        "schema": "corelink.issue-2044.capacity-schema-predicates.v1",
        "issue": 2044,
        "read_only": True,
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "account_id_redacted": ACCOUNT_REDACTED,
        "endpoint": "GET /accounts/{account}/cloudchamber/me",
        "provider_api_version": "v4",
        "capacity_schema_predicates": _schema_projection(payload),
        "shape_counts": _bounded_shape_counts(payload),
    }
    _write(output, receipt)
    return 0


def run_receipt(token: str, output: Path) -> int:
    """Write approved quota aggregates and explicit unavailable measurements."""

    payload = _request(token)
    receipt: dict[str, object] = {
        "schema_version": 1,
        "schema": "corelink.issue-2044.capacity-read-only.v1",
        "issue": 2044,
        "read_only": True,
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "account_id_redacted": ACCOUNT_REDACTED,
        "endpoint": "GET /accounts/{account}/cloudchamber/me",
        "provider_api_version": "v4",
        **_parse_live_capacity(payload),
    }
    _write(output, receipt)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    outputs = parser.add_mutually_exclusive_group(required=True)
    outputs.add_argument("--topology-output", type=Path)
    outputs.add_argument("--receipt-output", type=Path)
    args = parser.parse_args()
    try:
        token = os.environ.get("CLOUDFLARE_CAPACITY_READ_TOKEN", "")
        if args.topology_output is not None:
            return run_topology(token, args.topology_output)
        return run_receipt(token, args.receipt_output)
    except ProbeError as exc:
        print(f"issue-2044 capacity observation: FAIL-CLOSED: {exc}", flush=True)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
