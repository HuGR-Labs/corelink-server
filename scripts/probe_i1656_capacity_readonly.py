#!/usr/bin/env python3
"""Capture a bounded Cloudflare Containers capacity observation.

This probe deliberately has no account or URL arguments.  It performs exactly
one GET against the canonical account endpoint, requires a dedicated
read-only token, and writes only an aggregate, redacted receipt.  The
``--topology-output`` discovery mode records JSON paths and JSON types only;
it never records a provider value, an HTTP-header value, or a token.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import tempfile
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


CANONICAL_ACCOUNT_ID = "6a1fc1c626fc2628823e60b9db01f5cd"
ENDPOINT = (
    "https://api.cloudflare.com/client/v4/accounts/"
    f"{CANONICAL_ACCOUNT_ID}/containers/me"
)
ACCOUNT_REDACTED = f"{CANONICAL_ACCOUNT_ID[:4]}...{CANONICAL_ACCOUNT_ID[-4:]}"
MAX_RESPONSE_BYTES = 1_048_576
ACCOUNT_ID_RE = re.compile(r"^[0-9a-f]{32}$")
SAFE_SCHEMA_KEY_RE = re.compile(r"^[a-z][a-z0-9_]{0,63}$")
OPAQUE_IDENTIFIER_KEY_RE = re.compile(r"^(?:[0-9a-f]{16,}|[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})$", re.IGNORECASE)
SENSITIVE_KEY_RE = re.compile(r"(?:api[_-]?key|authorization|credential|pass(?:word)?|secret|token)", re.IGNORECASE)
MAX_TOPOLOGY_DEPTH = 8
MAX_TOPOLOGY_NODES = 256


class ProbeError(RuntimeError):
    pass


def _canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def _digest(value: object) -> str:
    return hashlib.sha256(_canonical(value)).hexdigest()


def _request(token: str) -> tuple[dict[str, Any], str]:
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
    return payload, hashlib.sha256(raw).hexdigest()


def _validate_provider_envelope(payload: object) -> None:
    """Accept only the explicit successful Cloudflare API envelope."""

    if not isinstance(payload, dict):
        raise ProbeError("capacity response must be a JSON object")
    if payload.get("success") is not True:
        raise ProbeError("capacity endpoint must report success=true")
    if payload.get("errors") != []:
        raise ProbeError("capacity endpoint must report an empty errors array")


def _source(payload: dict[str, Any]) -> dict[str, Any]:
    result = payload.get("result")
    if isinstance(result, dict):
        return result
    return payload


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
    raise ProbeError("capacity response contains a non-JSON value")


def _is_static_schema_key(key: str) -> bool:
    """Allow a small human-readable vocabulary; redact dynamic object keys."""

    return (
        SAFE_SCHEMA_KEY_RE.fullmatch(key) is not None
        and OPAQUE_IDENTIFIER_KEY_RE.fullmatch(key) is None
        and SENSITIVE_KEY_RE.search(key) is None
    )


def _json_pointer_segment(segment: str) -> str:
    return segment.replace("~", "~0").replace("/", "~1")


def _key_type_topology(payload: dict[str, Any]) -> dict[str, str]:
    """Return a bounded JSON key/type map without retaining any value.

    Arrays are intentionally terminal nodes. Their elements can contain
    account-specific identifiers, so nested array paths would add no capacity
    schema evidence while widening the retention surface.
    """

    topology: dict[str, str] = {}

    def visit(value: object, path: str, depth: int) -> None:
        if len(topology) >= MAX_TOPOLOGY_NODES:
            raise ProbeError("capacity topology exceeds the bounded node limit")
        topology[path] = _json_type(value)
        if isinstance(value, dict):
            if depth >= MAX_TOPOLOGY_DEPTH:
                raise ProbeError("capacity topology exceeds the bounded depth")
            dynamic_key_count = 0
            for key in sorted(value):
                if not isinstance(key, str) or not key:
                    raise ProbeError("capacity topology contains an invalid JSON key")
                if _is_static_schema_key(key):
                    segment = _json_pointer_segment(key)
                else:
                    dynamic_key_count += 1
                    # The ordinal distinguishes sibling fields without retaining
                    # an identifier, numeric key, or secret-bearing key name.
                    segment = f"~dynamic-key-{dynamic_key_count}"
                visit(value[key], f"{path}/{segment}", depth + 1)

    visit(payload, "", 0)
    return topology


def _number(source: dict[str, Any], *paths: tuple[str, ...]) -> float:
    for path in paths:
        value: Any = source
        for part in path:
            if not isinstance(value, dict):
                break
            value = value.get(part)
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            continue
        if not math.isfinite(value):
            raise ProbeError(f"capacity field {'/'.join(path)} is not finite")
        if value < 0:
            raise ProbeError(f"capacity field {'/'.join(path)} is negative")
        return float(value)
    raise ProbeError(f"capacity field is absent: {'/'.join(paths[0])}")


def _write(path: Path, receipt: dict[str, object]) -> None:
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ProbeError("receipt output must be a regular non-symlink file")
    path.parent.mkdir(parents=True, exist_ok=True)
    unsigned = dict(receipt)
    unsigned.pop("receipt_sha256", None)
    receipt["receipt_sha256"] = _digest(unsigned)
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
    """Capture provider schema metadata without retaining response values."""

    if not ACCOUNT_ID_RE.fullmatch(CANONICAL_ACCOUNT_ID):
        raise ProbeError("canonical account identity is malformed")
    payload, _response_sha256 = _request(token)
    _write(
        output,
        {
            "schema_version": 1,
            "schema": "corelink.issue-2044.capacity-key-type-topology.v1",
            "issue": 2044,
            "read_only": True,
            "captured_at": datetime.now(timezone.utc).isoformat(),
            "account_id_redacted": ACCOUNT_REDACTED,
            "endpoint": "GET /accounts/{account}/containers/me",
            "provider_api_version": "v4",
            "key_type_topology": _key_type_topology(payload),
        },
    )
    return 0


def run(token: str, output: Path) -> int:
    if not ACCOUNT_ID_RE.fullmatch(CANONICAL_ACCOUNT_ID):
        raise ProbeError("canonical account identity is malformed")
    payload, response_sha256 = _request(token)
    source = _source(payload)
    total_vcpu = _number(source, ("total_vcpu",), ("quota", "total_vcpu"), ("limits", "total_vcpu"))
    vcpu_per_deployment = _number(
        source, ("vcpu_per_deployment",), ("quota", "vcpu_per_deployment"), ("limits", "vcpu_per_deployment")
    )
    total_memory_mib = _number(source, ("total_memory_mib",), ("quota", "total_memory_mib"), ("limits", "total_memory_mib"))
    used_vcpu = _number(
        source,
        ("usage", "used_vcpu"),
        ("usage", "vcpu"),
        ("usage", "vcpu", "used"),
        ("usage", "used"),
        ("usage", "current_vcpu"),
        ("used_vcpu",),
    )
    if total_vcpu <= 0 or vcpu_per_deployment <= 0 or total_memory_mib <= 0 or used_vcpu > total_vcpu:
        raise ProbeError("capacity values are invalid or exceed the account quota")
    receipt: dict[str, object] = {
        "schema_version": 1,
        "schema": "corelink.issue-1656.capacity-read-only.v1",
        "issue": 1656,
        "read_only": True,
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "account_id_redacted": ACCOUNT_REDACTED,
        "endpoint": "GET /accounts/{account}/containers/me",
        "response_sha256": response_sha256,
        "total_vcpu": total_vcpu,
        "vcpu_per_deployment": vcpu_per_deployment,
        "total_memory_mib": total_memory_mib,
        "usage_vcpu": used_vcpu,
        "headroom_vcpu": total_vcpu - used_vcpu,
        "quota": {
            "total_vcpu": total_vcpu,
            "vcpu_per_deployment": vcpu_per_deployment,
            "total_memory_mib": total_memory_mib,
        },
        "usage": {"used_vcpu": used_vcpu},
        "headroom": {"provider_vcpu": total_vcpu - used_vcpu},
    }
    _write(output, receipt)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    outputs = parser.add_mutually_exclusive_group(required=True)
    outputs.add_argument("--output", type=Path)
    outputs.add_argument("--topology-output", type=Path)
    args = parser.parse_args()
    try:
        if args.topology_output is not None:
            return run_topology(os.environ.get("CLOUDFLARE_CAPACITY_READ_TOKEN", ""), args.topology_output)
        return run(os.environ.get("CLOUDFLARE_CAPACITY_READ_TOKEN", ""), args.output)
    except ProbeError as exc:
        print(f"issue-1656 capacity readback: FAIL-CLOSED: {exc}", flush=True)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
